// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! MT940, the SWIFT customer statement message: the plain-text statement
//! many banks, especially German ones, still deliver for an account's day or
//! month (AGENTS.md §5). What the importer takes from the file, and how it
//! decides, is D-096 in `docs/DECISIONS.md`.
//!
//! A file is a list of messages, each made of fields that start with a tag
//! on a line of their own (`:25:` the account, `:60F:` the opening balance,
//! `:61:` an entry, `:86:` what the bank says about it, `:62F:` the closing
//! balance). One message is one [`Statement`]. The bank's own notes about an
//! entry, `:86:`, come in two forms: the structured one German banks use,
//! whose `?NN` sub-fields hold the other party's name and account and the
//! purpose, and free text.
//!
//! The file is untrusted (AGENTS.md §8): it is read line by line, the
//! limits on entries and text are kept while reading, and an amount or date
//! that isn't written exactly as the standard says is refused, never
//! guessed.

use kassensturz_core::{Currency, CurrencyCode, Date, Money};

use crate::amount::{DecimalMark, currency, parse_amount};
use crate::csv::decode;
use crate::error::ImportError;
use crate::importer::{ImportKind, Imported, Importer, ImporterInfo, Recognition};
use crate::limits::Limits;
use crate::statement::{Balance, Entry, EntryStatus, Statement, StatementAccount};

/// What banks write where the standard wants a reference and they have
/// none.
const NO_REFERENCE: [&str; 2] = ["NONREF", "NOTPROVIDED"];

/// The tags German banks put in front of the parts of a SEPA payment's
/// purpose (DK's "Verwendungszweck" rules, e.g. `SVWZ+` before the text the
/// payer wrote).
const PURPOSE_TAGS: [&str; 12] = [
    "EREF+", "KREF+", "MREF+", "CRED+", "DEBT+", "COAM+", "OAMT+", "SVWZ+", "ABWA+", "ABWE+",
    "IBAN+", "BIC+",
];

/// The tag that holds the text the payer wrote.
const PURPOSE_TEXT: &str = "SVWZ+";

/// The tag that holds the other party's IBAN, where the account sub-field
/// doesn't.
const PURPOSE_IBAN: &str = "IBAN+";

/// The importer for MT940, with the ID `mt940`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Mt940;

impl Importer for Mt940 {
    fn info(&self) -> ImporterInfo {
        ImporterInfo {
            id: "mt940",
            kind: ImportKind::Statements,
            extensions: &["sta", "mt940", "940", "swi", "txt"],
        }
    }

    fn recognize(&self, start: &[u8]) -> Recognition {
        let start = start.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(start);
        let mut reference = false;
        let mut balance = false;
        for line in start.split(|&byte| byte == b'\n') {
            reference |= line.starts_with(b":20:");
            balance |= line.starts_with(b":60F:") || line.starts_with(b":60M:");
        }
        match (reference, balance) {
            (true, true) => Recognition::Yes,
            (false, false) => Recognition::No,
            // A statement's reference or opening balance alone: the other
            // may lie beyond the bytes looked at.
            _ => Recognition::Maybe,
        }
    }

    fn read(&self, input: &[u8], limits: &Limits) -> Result<Imported, ImportError> {
        // The standard's characters are ASCII; German banks add umlauts in
        // UTF-8 or Windows-1252, which `decode` tells apart. There is no
        // encoding to choose, so a byte neither has is a damaged file.
        let text = decode(input, None).map_err(|error| match error {
            ImportError::WrongEncoding { line, .. } => ImportError::Malformed { line },
            other => other,
        })?;
        let messages = split_messages(&text, limits)?;
        if messages.is_empty() {
            return Err(ImportError::Malformed { line: Some(1) });
        }
        let mut statements = Vec::with_capacity(messages.len());
        for fields in &messages {
            statements.push(statement_from(fields, limits)?);
        }
        Ok(Imported::Statements(statements))
    }
}

/// One field of a message: its tag (`61`, `60F`, ...) and its lines, the
/// first without the tag. Only the lines the importer reads are kept
/// (`split_messages`).
#[derive(Debug)]
struct Field<'a> {
    /// The line of the tag, counted from 1.
    line: u64,
    tag: &'a str,
    lines: Vec<&'a str>,
    /// The bytes of all the lines, for a `:86:` field.
    size: usize,
}

impl<'a> Field<'a> {
    /// The field's text if it has a single line, as the balances and the
    /// account have.
    fn single_line(&self) -> Result<&'a str, ImportError> {
        match self.lines.as_slice() {
            [only] => Ok(*only),
            _ => Err(ImportError::Malformed {
                line: Some(self.line),
            }),
        }
    }
}

/// The tag of a field that starts on `line`, and the rest of the line:
/// `:`, two digits, perhaps a capital letter, `:`.
fn field_start(line: &str) -> Option<(&str, &str)> {
    let bytes = line.as_bytes();
    if bytes.first() != Some(&b':')
        || !bytes.get(1).is_some_and(u8::is_ascii_digit)
        || !bytes.get(2).is_some_and(u8::is_ascii_digit)
    {
        return None;
    }
    let end = if bytes.get(3).is_some_and(u8::is_ascii_uppercase) {
        4
    } else {
        3
    };
    (bytes.get(end) == Some(&b':')).then(|| (&line[1..end], &line[end + 1..]))
}

/// The tags of the fields the importer reads. Any other field is skipped
/// while the file is split, so a file full of them costs no memory.
const READ_TAGS: [&str; 7] = ["25", "60F", "60M", "61", "62F", "62M", "86"];

/// The most fields besides entries and their notes a message may have: its
/// account and its balances, which a real message has once each.
const MAX_OTHER_FIELDS: usize = 8;

/// The messages of a file, with the fields the importer reads. A message
/// ends with a line that is only `-` (or `-}`, where a SWIFT block wraps
/// it), or where the next one starts with `:20:`. Text before a message's
/// first tag is ignored if it is empty or a SWIFT block header
/// (`{1:...}{4:`), and refused otherwise.
///
/// The file is untrusted, so what a message may hold is limited while it is
/// split, before anything is built from it: the entries of all messages
/// (D-091), the text of a note, the lines of each field that has a fixed
/// form, and the other fields of a message. A `:86:` note is kept only
/// right after its `:61:` entry, and a second one for the same entry is
/// refused here.
fn split_messages<'a>(text: &'a str, limits: &Limits) -> Result<Vec<Vec<Field<'a>>>, ImportError> {
    let mut messages = Vec::new();
    let mut current: Vec<Field<'a>> = Vec::new();
    // Whether the current message has a tag yet, and where it starts.
    let mut message_line = None;
    // Whether the lines below the last tag belong to a field that is kept.
    let mut appending = false;
    // After an entry: whether its note has come.
    let mut after_entry: Option<bool> = None;
    let mut entries = 0;
    let mut others = 0;
    for (index, line) in text.split('\n').enumerate() {
        let number = u64::try_from(index).unwrap_or(u64::MAX).saturating_add(1);
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some((tag, rest)) = field_start(line) {
            if tag == "20"
                && let Some(start) = message_line.take()
            {
                close_message(&mut messages, &mut current, start)?;
                after_entry = None;
                others = 0;
            }
            message_line.get_or_insert(number);
            let previous = after_entry.take();
            appending = false;
            let field = Field {
                line: number,
                tag,
                lines: vec![rest],
                size: rest.len(),
            };
            match tag {
                "61" => {
                    entries += 1;
                    limits.check_entries(entries)?;
                    current.push(field);
                    after_entry = Some(false);
                }
                // A note belongs to the entry right above it; one about
                // the whole statement is skipped.
                "86" => match previous {
                    Some(false) => {
                        if field.size > limits.max_text_bytes {
                            return Err(too_long(limits, number));
                        }
                        current.push(field);
                        after_entry = Some(true);
                        appending = true;
                    }
                    Some(true) => return Err(ImportError::Malformed { line: Some(number) }),
                    None => {}
                },
                tag if READ_TAGS.contains(&tag) => {
                    others += 1;
                    if others > MAX_OTHER_FIELDS {
                        return Err(ImportError::Malformed { line: Some(number) });
                    }
                    current.push(field);
                    appending = true;
                }
                _ => {}
            }
        } else if line == "-" || line == "-}" {
            if let Some(start) = message_line.take() {
                close_message(&mut messages, &mut current, start)?;
                after_entry = None;
                others = 0;
                appending = false;
            }
        } else if line.trim().is_empty() {
            // SWIFT has no empty lines; some programs add them.
        } else if message_line.is_some() {
            // A line below a tag. Of the kept fields, a note keeps all its
            // lines (up to the limit on text) and the account and balances
            // two, enough to see that they have more than one; an entry's
            // second line is not read.
            if let (true, Some(field)) = (appending, current.last_mut()) {
                if field.tag == "86" {
                    field.size = field.size.saturating_add(line.len());
                    if field.size > limits.max_text_bytes {
                        return Err(too_long(limits, field.line));
                    }
                    field.lines.push(line);
                } else if field.lines.len() < 2 {
                    field.lines.push(line);
                }
            }
        } else if !line.starts_with(['{', '}']) {
            return Err(ImportError::Malformed { line: Some(number) });
        }
    }
    if let Some(start) = message_line {
        close_message(&mut messages, &mut current, start)?;
    }
    Ok(messages)
}

/// Adds the message that started on line `start`, made of `current`, to
/// `messages`. One without a single field the importer reads has no
/// balance and so no currency; it is refused here rather than kept.
fn close_message<'a>(
    messages: &mut Vec<Vec<Field<'a>>>,
    current: &mut Vec<Field<'a>>,
    start: u64,
) -> Result<(), ImportError> {
    if current.is_empty() {
        return Err(ImportError::Malformed { line: Some(start) });
    }
    messages.push(std::mem::take(current));
    Ok(())
}

fn too_long(limits: &Limits, line: u64) -> ImportError {
    ImportError::TextTooLong {
        max_bytes: limits.max_text_bytes,
        line: Some(line),
    }
}

/// A balance field (`:60F:`, `:62F:`, ...) before it is read as numbers:
/// the mark, the date, the currency and the amount.
struct RawBalance<'a> {
    line: u64,
    negative: bool,
    date: Date,
    currency: &'a str,
    amount: &'a str,
}

impl<'a> RawBalance<'a> {
    fn new(field: &Field<'a>) -> Result<Self, ImportError> {
        let text = field.single_line()?;
        let malformed = || ImportError::Malformed {
            line: Some(field.line),
        };
        if !text.is_ascii() || text.len() < 11 {
            return Err(malformed());
        }
        let negative = match &text[..1] {
            "C" => false,
            "D" => true,
            _ => return Err(malformed()),
        };
        Ok(Self {
            line: field.line,
            negative,
            date: parse_date(&text[1..7]).map_err(|error| error.at_line(field.line))?,
            currency: &text[7..10],
            amount: &text[10..],
        })
    }
}

/// A `:61:` field read as far as it needs no currency: the dates, the
/// direction, the amount as written, the bank's reference.
struct RawEntry<'a> {
    line: u64,
    value_date: Date,
    booking_date: Date,
    negative: bool,
    amount: &'a str,
    bank_reference: Option<&'a str>,
}

impl<'a> RawEntry<'a> {
    /// Reads `:61:`, e.g. `2610011001D12,50NMSCNONREF//BANKREF`: the value
    /// date `YYMMDD`, the booking date `MMDD` if different, `C`, `D`, `RC`
    /// or `RD`, perhaps a funds code letter, the amount, `N`, `F` or `S`
    /// and three characters for the kind of booking, the customer's
    /// reference, `//` and the bank's. The line below it (supplementary
    /// details) is not read.
    fn new(field: &Field<'a>) -> Result<Self, ImportError> {
        let text = field.lines[0];
        let at = field.line;
        let malformed = || ImportError::Malformed { line: Some(at) };
        let bytes = text.as_bytes();
        let digits = |from: usize, count: usize| {
            bytes
                .get(from..from + count)
                .is_some_and(|part| part.iter().all(u8::is_ascii_digit))
        };
        if !digits(0, 6) {
            return Err(malformed());
        }
        let value_date = parse_date(&text[..6]).map_err(|error| error.at_line(at))?;
        let mut position = 6;
        let booking_date = if digits(position, 4) {
            let date = booking_date(value_date, &text[position..position + 4])
                .map_err(|error| error.at_line(at))?;
            position += 4;
            date
        } else {
            value_date
        };
        // `RC` is the reversal of a credit, which takes money out of the
        // account; `RD` the reversal of a debit, which puts it back.
        let (negative, length) = if bytes[position..].starts_with(b"RC") {
            (true, 2)
        } else if bytes[position..].starts_with(b"RD") {
            (false, 2)
        } else if bytes[position..].starts_with(b"D") {
            (true, 1)
        } else if bytes[position..].starts_with(b"C") {
            (false, 1)
        } else {
            return Err(malformed());
        };
        position += length;
        // The funds code, the third letter of the currency, is not used.
        if bytes.get(position).is_some_and(u8::is_ascii_uppercase) {
            position += 1;
        }
        let amount_end = bytes[position..]
            .iter()
            // Whatever could be part of an amount, so a point or a sign
            // is reported as the bad amount it is.
            .position(|byte| !matches!(byte, b'0'..=b'9' | b',' | b'.' | b'+' | b'-'))
            .map_or(text.len(), |length| position + length);
        let amount = &text[position..amount_end];
        let rest = &text[amount_end..];
        let mut bank_reference = None;
        if !rest.is_empty() {
            // The kind of booking: `N`, `F` or `S` and three characters.
            let kind = rest.as_bytes();
            if !(matches!(kind[0], b'N' | b'F' | b'S')
                && kind.len() >= 4
                && kind[1..4].iter().all(u8::is_ascii_alphanumeric))
            {
                return Err(malformed());
            }
            // `//` separates the customer's reference from the bank's.
            bank_reference = rest[4..].split_once("//").map(|(_, bank)| bank);
        }
        Ok(Self {
            line: at,
            value_date,
            booking_date,
            negative,
            amount,
            bank_reference,
        })
    }
}

/// The `YYMMDD` date `text` (six ASCII digits). Years 69 to 99 are 1969 to
/// 1999, 00 to 68 are 2000 to 2068, as POSIX `%y` has it (D-095). The error
/// has no line yet.
fn parse_date(text: &str) -> Result<Date, ImportError> {
    let bad = || ImportError::BadDate {
        found: text.to_owned(),
        line: None,
    };
    if text.len() != 6 || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(bad());
    }
    let number = |range: std::ops::Range<usize>| text[range].parse::<i16>().map_err(|_| bad());
    let short_year = number(0..2)?;
    let year = short_year + if short_year >= 69 { 1900 } else { 2000 };
    let month = i8::try_from(number(2..4)?).map_err(|_| bad())?;
    let day = i8::try_from(number(4..6)?).map_err(|_| bad())?;
    Date::new(year, month, day).map_err(|_| bad())
}

/// The booking date from its `MMDD`, which has no year: the one of the
/// value date's year, the year before or the year after that puts it
/// nearest to the value date, so a Christmas payment booked in January
/// lands in the right year.
fn booking_date(value_date: Date, text: &str) -> Result<Date, ImportError> {
    let bad = || ImportError::BadDate {
        found: text.to_owned(),
        line: None,
    };
    let month = i8::try_from(text[..2].parse::<i16>().map_err(|_| bad())?).map_err(|_| bad())?;
    let day = i8::try_from(text[2..].parse::<i16>().map_err(|_| bad())?).map_err(|_| bad())?;
    // A day count good enough to tell a year from the next: months of 31
    // days.
    let count = |date: Date| {
        i32::from(date.year()) * 372 + i32::from(date.month()) * 31 + i32::from(date.day())
    };
    (-1..=1)
        .filter_map(|shift| Date::new(value_date.year() + shift, month, day).ok())
        .min_by_key(|date| (count(*date) - count(value_date)).abs())
        .ok_or_else(bad)
}

/// An amount as MT940 writes it, digits with a decimal comma and no sign
/// (`12,50`, or `100,` for a whole number), as an amount of `currency` that
/// goes out of the account if `negative`. Everything else is refused, also
/// a decimal point: an amount is never guessed (AGENTS.md §3.1 rule 1).
fn parse_mt940_amount(
    text: &str,
    negative: bool,
    currency: Currency,
) -> Result<Money, ImportError> {
    let bad = || ImportError::BadAmount {
        found: text.to_owned(),
        line: None,
    };
    if !text.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        return Err(bad());
    }
    let Some((_, fraction)) = text.split_once(',') else {
        return Err(bad());
    };
    // `100,` is a whole number.
    let plain = if fraction.is_empty() {
        &text[..text.len() - 1]
    } else {
        text
    };
    let signed = if negative {
        format!("-{plain}")
    } else {
        plain.to_owned()
    };
    parse_amount(&signed, DecimalMark::Comma, currency).map_err(|error| match error {
        // The text as the file wrote it, without the sign this code added.
        ImportError::BadAmount { line, .. } => ImportError::BadAmount {
            found: text.to_owned(),
            line,
        },
        ImportError::TooManyDecimalPlaces { currency, line, .. } => {
            ImportError::TooManyDecimalPlaces {
                found: text.to_owned(),
                currency,
                line,
            }
        }
        ImportError::AmountTooLarge { line, .. } => ImportError::AmountTooLarge {
            found: text.to_owned(),
            line,
        },
        other => other,
    })
}

/// Whether `text` looks like an IBAN: two capital letters, two digits and
/// 11 to 30 capital letters or digits. The check digits are not tested; the
/// file's text is passed on as delivered (D-067).
pub(crate) fn looks_like_iban(text: &str) -> bool {
    let bytes = text.as_bytes();
    (15..=34).contains(&bytes.len())
        && bytes[..2].iter().all(u8::is_ascii_uppercase)
        && bytes[2..4].iter().all(u8::is_ascii_digit)
        && bytes[4..]
            .iter()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

/// `text`, or none if it is empty or only spaces (D-095).
pub(crate) fn some_text(text: &str) -> Option<String> {
    (!text.trim().is_empty()).then(|| text.to_owned())
}

/// A message as a statement.
fn statement_from(fields: &[Field<'_>], limits: &Limits) -> Result<Statement, ImportError> {
    let first_line = fields.first().map_or(1, |field| field.line);
    let mut account = StatementAccount::default();
    let mut has_account = false;
    let mut opening = None;
    let mut closing = None;
    for field in fields {
        let slot = match field.tag {
            "25" => {
                if std::mem::replace(&mut has_account, true) {
                    return Err(ImportError::Malformed {
                        line: Some(field.line),
                    });
                }
                let text = field.single_line()?.trim();
                limits.check_text(text, Some(field.line))?;
                if looks_like_iban(text) {
                    account.iban = Some(text.to_owned());
                } else if !text.is_empty() {
                    account.number = Some(text.to_owned());
                }
                continue;
            }
            "60F" | "60M" => &mut opening,
            "62F" | "62M" => &mut closing,
            _ => continue,
        };
        if slot.is_some() {
            return Err(ImportError::Malformed {
                line: Some(field.line),
            });
        }
        *slot = Some(RawBalance::new(field)?);
    }
    // The currency is the one of the balances; entries have none of their
    // own.
    let code = match (&opening, &closing) {
        (Some(first), Some(second)) => {
            if first.currency != second.currency {
                return Err(ImportError::CurrencyMismatch {
                    expected: currency_code(first)?,
                    found: currency_code(second)?,
                });
            }
            currency_code(first)?
        }
        (Some(only), None) | (None, Some(only)) => currency_code(only)?,
        (None, None) => {
            return Err(ImportError::Malformed {
                line: Some(first_line),
            });
        }
    };
    let code_line = opening
        .as_ref()
        .or(closing.as_ref())
        .map_or(first_line, |balance| balance.line);
    let currency = currency(code).map_err(|error| error.at_line(code_line))?;
    let balance = |raw: Option<RawBalance<'_>>| {
        raw.map(|raw| {
            parse_mt940_amount(raw.amount, raw.negative, currency)
                .map(|amount| Balance {
                    amount,
                    date: raw.date,
                })
                .map_err(|error| error.at_line(raw.line))
        })
        .transpose()
    };
    let opening_balance = balance(opening)?;
    let closing_balance = balance(closing)?;

    let mut entries: Vec<Entry> = Vec::new();
    for field in fields {
        match field.tag {
            "61" => entries.push(entry_from(&RawEntry::new(field)?, currency, limits)?),
            // The splitter keeps a note only right below its entry.
            "86" => {
                if let Some(entry) = entries.last_mut() {
                    add_details(entry, field, limits)?;
                }
            }
            _ => {}
        }
    }
    Ok(Statement {
        account,
        currency,
        opening_balance,
        closing_balance,
        entries,
    })
}

/// The currency code of a balance.
fn currency_code(balance: &RawBalance<'_>) -> Result<CurrencyCode, ImportError> {
    CurrencyCode::new(balance.currency).map_err(|_| ImportError::Malformed {
        line: Some(balance.line),
    })
}

/// A booked entry from a `:61:` line.
fn entry_from(
    raw: &RawEntry<'_>,
    currency: Currency,
    limits: &Limits,
) -> Result<Entry, ImportError> {
    let amount = parse_mt940_amount(raw.amount, raw.negative, currency)
        .map_err(|error| error.at_line(raw.line))?;
    let bank_id = raw
        .bank_reference
        .map(str::trim)
        .filter(|reference| {
            !reference.is_empty()
                && !NO_REFERENCE
                    .iter()
                    .any(|none| reference.eq_ignore_ascii_case(none))
        })
        .map(str::to_owned);
    if let Some(id) = &bank_id {
        limits.check_text(id, Some(raw.line))?;
    }
    Ok(Entry {
        status: EntryStatus::Booked,
        bank_id,
        booking_date: raw.booking_date,
        value_date: Some(raw.value_date),
        amount,
        other_party_name: None,
        other_party_iban: None,
        remittance_information: None,
    })
}

/// Puts what the `:86:` field says into `entry`.
fn add_details(entry: &mut Entry, field: &Field<'_>, limits: &Limits) -> Result<(), ImportError> {
    let details = Details::new(&field.lines);
    for text in [
        &details.name,
        &details.iban,
        &details.remittance_information,
    ]
    .into_iter()
    .flatten()
    {
        limits.check_text(text, Some(field.line))?;
    }
    entry.other_party_name = details.name;
    entry.other_party_iban = details.iban;
    entry.remittance_information = details.remittance_information;
    Ok(())
}

/// What a `:86:` field says about the other party and the purpose.
#[derive(Debug, Default)]
struct Details {
    name: Option<String>,
    iban: Option<String>,
    remittance_information: Option<String>,
}

impl Details {
    /// Reads the field's lines. A field that starts with the three digits of
    /// the booking key and `?NN` sub-fields is the structured form, whose
    /// physical lines only wrap the text and are joined without a
    /// separator; anything else is free text, whose lines are joined with a
    /// line break (Q-052).
    fn new(lines: &[&str]) -> Self {
        let joined = lines.concat();
        match subfields(&joined) {
            Some(subfields) => Self::structured(&subfields),
            None => Self {
                remittance_information: some_text(&lines.join("\n")),
                ..Self::default()
            },
        }
    }

    /// The structured form (DK's rules for the `:86:` field, as a public
    /// description of the format). Sub-field 00 is the booking text, 20 to
    /// 29 and 60 to 63 the purpose, 30 the other party's bank, 31 their
    /// account, 32 and 33 their name.
    fn structured(subfields: &[(u8, &str)]) -> Self {
        let text_of = |wanted: &dyn Fn(u8) -> bool| -> String {
            subfields
                .iter()
                .filter(|(id, _)| wanted(*id))
                .map(|(_, text)| *text)
                .collect()
        };
        let one = |id: u8| text_of(&|found| found == id);
        let name = text_of(&|id| id == 32 || id == 33);
        let bank = one(30);
        let account = one(31);

        // The purpose, from its sub-fields, with the offsets where each
        // starts: a SEPA tag starts a part there, or at the start, or after
        // a space.
        let mut purpose = String::new();
        let mut starts = Vec::new();
        for (id, text) in subfields {
            if (20..=29).contains(id) || (60..=63).contains(id) {
                starts.push(purpose.len());
                purpose.push_str(text);
            }
        }
        let mut tags: Vec<(usize, &str)> = Vec::new();
        for (at, _) in purpose.char_indices() {
            let boundary =
                at == 0 || starts.binary_search(&at).is_ok() || purpose[..at].ends_with(' ');
            if !boundary {
                continue;
            }
            if let Some(tag) = PURPOSE_TAGS
                .iter()
                .find(|tag| purpose[at..].starts_with(**tag))
            {
                tags.push((at, *tag));
            }
        }
        let value_of = |wanted: &str| -> String {
            tags.iter()
                .enumerate()
                .filter(|(_, (_, tag))| *tag == wanted)
                .map(|(index, (at, tag))| {
                    let value = |end: usize| &purpose[at + tag.len()..end];
                    match tags.get(index + 1) {
                        // The space that separates a part from the next
                        // tag is not part of it.
                        Some((next, _)) => value(*next).trim_end(),
                        None => value(purpose.len()),
                    }
                })
                .collect()
        };
        let remittance = if tags.is_empty() {
            purpose.clone()
        } else if tags.iter().any(|(_, tag)| *tag == PURPOSE_TEXT) {
            value_of(PURPOSE_TEXT)
        } else {
            // Tags but no text of the payer's: what stands before them.
            purpose[..tags[0].0].trim_end().to_owned()
        };
        // Without a purpose, the booking text ("Gutschrift", "Entgelt")
        // says at least what the entry is.
        let remittance = some_text(&remittance).or_else(|| some_text(&one(0)));

        let tagged_iban = value_of(PURPOSE_IBAN);
        let (bank, account, tagged_iban) = (bank.trim(), account.trim(), tagged_iban.trim());
        let iban = if looks_like_iban(account) {
            Some(account.to_owned())
        } else if !tagged_iban.is_empty() {
            Some(tagged_iban.to_owned())
        } else if account.is_empty() {
            None
        } else if !bank.is_empty() && bank.bytes().all(|byte| byte.is_ascii_digit()) {
            // A bank code and an account number, as in the statement's own
            // account (`37040044/0532013000`).
            Some(format!("{bank}/{account}"))
        } else {
            Some(account.to_owned())
        };
        Self {
            name: some_text(&name),
            iban,
            remittance_information: remittance,
        }
    }
}

/// The sub-fields of the structured `:86:` form, as their numbers and
/// texts, or none if `text` isn't that form. The text starts with the three
/// digits of the booking key (or directly with a sub-field), and each
/// sub-field starts with `?` and two digits; a `?` followed by anything
/// else is part of a text.
fn subfields(text: &str) -> Option<Vec<(u8, &str)>> {
    let bytes = text.as_bytes();
    let marker = |at: usize| {
        bytes.get(at) == Some(&b'?')
            && bytes.get(at + 1).is_some_and(u8::is_ascii_digit)
            && bytes.get(at + 2).is_some_and(u8::is_ascii_digit)
    };
    let markers: Vec<usize> = (0..bytes.len()).filter(|&at| marker(at)).collect();
    let first = *markers.first()?;
    if !(first == 0 || (first == 3 && bytes[..3].iter().all(u8::is_ascii_digit))) {
        return None;
    }
    Some(
        markers
            .iter()
            .enumerate()
            .map(|(index, &at)| {
                let id = (bytes[at + 1] - b'0') * 10 + (bytes[at + 2] - b'0');
                let end = markers.get(index + 1).copied().unwrap_or(bytes.len());
                (id, &text[at + 3..end])
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests;
