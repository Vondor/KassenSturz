// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! CAMT.053, the ISO 20022 bank-to-customer statement (AGENTS.md §5): the
//! XML file most European banks deliver for an account's day or month.
//!
//! The importer reads the versions `camt.053.001.01` to `camt.053.001.99`
//! by the element names they share, so the differences between them (the
//! entry's status as `<Sts>BOOK</Sts>` or `<Sts><Cd>BOOK</Cd></Sts>`, a
//! party's name as `Dbtr/Nm` or `Dbtr/Pty/Nm`) cost nothing. What it takes
//! from the file, and how it decides, is D-093 in `docs/DECISIONS.md`.
//!
//! The file is untrusted (AGENTS.md §8): it is read as a stream of events,
//! never as a tree, so the memory it needs doesn't grow with the nesting;
//! a document type definition is refused, so no entity is ever expanded or
//! fetched; and the limits on nesting, entries and text are kept while
//! reading.

use kassensturz_core::{Currency, CurrencyCode, Date, Money};
use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::{Namespace, ResolveResult};
use quick_xml::reader::NsReader;

use crate::amount::{DecimalMark, currency, parse_amount};
use crate::error::ImportError;
use crate::importer::{ImportKind, Imported, Importer, ImporterInfo, Recognition};
use crate::limits::Limits;
use crate::statement::{Balance, Entry, EntryStatus, Statement, StatementAccount};

/// What every CAMT.053 namespace starts with; a version number follows.
const NAMESPACE: &str = "urn:iso:std:iso:20022:tech:xsd:camt.053.001.";

/// Reasons banks give for "no reference" where the schema wants a value.
const NO_REFERENCE: [&str; 2] = ["NONREF", "NOTPROVIDED"];

/// The importer for CAMT.053, with the ID `camt053`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Camt053;

impl Importer for Camt053 {
    fn info(&self) -> ImporterInfo {
        ImporterInfo {
            id: "camt053",
            kind: ImportKind::Statements,
            extensions: &["xml", "camt"],
        }
    }

    fn recognize(&self, start: &[u8]) -> Recognition {
        if find(start, NAMESPACE.as_bytes()) {
            Recognition::Yes
        } else {
            Recognition::No
        }
    }

    fn read(&self, input: &[u8], limits: &Limits) -> Result<Imported, ImportError> {
        let text = std::str::from_utf8(input).map_err(|error| ImportError::Malformed {
            line: Some(line_at(input, error.valid_up_to())),
        })?;
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        Parser::new(text, limits).run()
    }
}

/// Whether `needle` occurs in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// The line, counted from 1, of the byte at `position`.
fn line_at(input: &[u8], position: usize) -> u64 {
    let before = &input[..position.min(input.len())];
    let breaks = before.iter().filter(|&&byte| byte == b'\n').count();
    u64::try_from(breaks).unwrap_or(u64::MAX).saturating_add(1)
}

/// Whether `namespace` is one of CAMT.053's: the prefix and then digits.
fn is_camt053(namespace: &ResolveResult<'_>) -> bool {
    let ResolveResult::Bound(Namespace(namespace)) = namespace else {
        return false;
    };
    namespace.strip_prefix(NAMESPACE).is_some_and(|version| {
        !version.is_empty() && version.bytes().all(|byte| byte.is_ascii_digit())
    })
}

// The data of the file before it is read as numbers and dates. Amounts and
// dates wait for the end of their statement, since the statement's currency
// may come after the first amount, and each keeps where it was in the file,
// for the error report.

/// A text with where it ended, as a byte offset.
#[derive(Clone, Debug)]
struct Raw {
    text: String,
    position: usize,
}

#[derive(Clone, Debug)]
struct RawAmount {
    value: Raw,
    /// The `Ccy` attribute.
    currency: Option<String>,
}

#[derive(Debug, Default)]
struct RawStatement {
    iban: Option<String>,
    number: Option<String>,
    currency: Option<Raw>,
    balances: Vec<RawBalance>,
    entries: Vec<RawEntry>,
    /// Where the statement ended.
    position: usize,
}

#[derive(Debug, Default)]
struct RawBalance {
    /// `OPBD`, `CLBD`, ...
    kind: Option<String>,
    amount: Option<RawAmount>,
    indicator: Option<Raw>,
    date: Option<Raw>,
}

#[derive(Debug, Default)]
struct RawEntry {
    amount: Option<RawAmount>,
    indicator: Option<Raw>,
    reversal: Option<Raw>,
    status: Option<String>,
    booking_date: Option<Raw>,
    value_date: Option<Raw>,
    reference: Option<String>,
    transactions: Vec<RawTransaction>,
    position: usize,
}

/// One `TxDtls`: what an entry holds for each payment in it.
#[derive(Debug, Default)]
struct RawTransaction {
    reference: Option<String>,
    debtor: RawParty,
    creditor: RawParty,
    remittance: Vec<String>,
}

#[derive(Debug, Default)]
struct RawParty {
    name: Option<String>,
    iban: Option<String>,
    other: Option<String>,
}

impl RawParty {
    /// The name and the account number the file gives, if it gives either.
    fn identity(&self) -> Option<(Option<&str>, Option<&str>)> {
        let account = self.iban.as_deref().or(self.other.as_deref());
        let name = self.name.as_deref();
        (name.is_some() || account.is_some()).then_some((name, account))
    }
}

#[derive(Clone, Copy, Debug)]
enum Side {
    Debtor,
    Creditor,
}

#[derive(Clone, Copy, Debug)]
enum PartyField {
    Name,
    Iban,
    Other,
}

/// The elements whose text the importer keeps.
#[derive(Clone, Copy, Debug)]
enum Field {
    AccountIban,
    AccountNumber,
    AccountCurrency,
    BalanceKind,
    BalanceAmount,
    BalanceIndicator,
    BalanceDate,
    EntryAmount,
    EntryIndicator,
    EntryReversal,
    EntryStatus,
    /// The status as a proprietary code, which is never "booked".
    EntryStatusProprietary,
    EntryBookingDate,
    EntryValueDate,
    EntryReference,
    TransactionReference,
    Party(Side, PartyField),
    /// Unstructured remittance information, or a creditor reference.
    Remittance,
}

/// Which element of a statement the path leads to, if it is one the
/// importer keeps. `path` starts below `Stmt`.
fn classify(path: &[&str]) -> Option<Field> {
    use Field::{
        AccountCurrency, AccountIban, AccountNumber, BalanceAmount, BalanceDate, BalanceIndicator,
        BalanceKind, EntryAmount, EntryBookingDate, EntryIndicator, EntryReference, EntryReversal,
        EntryStatus, EntryStatusProprietary, EntryValueDate, Party, Remittance,
        TransactionReference,
    };
    let party = |side, rest: &[&str]| match rest {
        ["Nm"] | ["Pty", "Nm"] => Some(Party(side, PartyField::Name)),
        _ => None,
    };
    let account = |side, rest: &[&str]| match rest {
        ["Id", "IBAN"] => Some(Party(side, PartyField::Iban)),
        ["Id", "Othr", "Id"] => Some(Party(side, PartyField::Other)),
        _ => None,
    };
    match path {
        ["Acct", "Id", "IBAN"] => Some(AccountIban),
        ["Acct", "Id", "Othr", "Id"] => Some(AccountNumber),
        ["Acct", "Ccy"] => Some(AccountCurrency),
        ["Bal", "Tp", "CdOrPrtry", "Cd"] => Some(BalanceKind),
        ["Bal", "Amt"] => Some(BalanceAmount),
        ["Bal", "CdtDbtInd"] => Some(BalanceIndicator),
        ["Bal", "Dt", "Dt" | "DtTm"] => Some(BalanceDate),
        ["Ntry", "Amt"] => Some(EntryAmount),
        ["Ntry", "CdtDbtInd"] => Some(EntryIndicator),
        ["Ntry", "RvslInd"] => Some(EntryReversal),
        ["Ntry", "Sts"] | ["Ntry", "Sts", "Cd"] => Some(EntryStatus),
        ["Ntry", "Sts", "Prtry"] => Some(EntryStatusProprietary),
        ["Ntry", "BookgDt", "Dt" | "DtTm"] => Some(EntryBookingDate),
        ["Ntry", "ValDt", "Dt" | "DtTm"] => Some(EntryValueDate),
        ["Ntry", "AcctSvcrRef"] => Some(EntryReference),
        ["Ntry", "NtryDtls", "TxDtls", rest @ ..] => match rest {
            ["Refs", "AcctSvcrRef"] => Some(TransactionReference),
            ["RmtInf", "Ustrd"] | ["RmtInf", "Strd", "CdtrRefInf", "Ref"] => Some(Remittance),
            ["RltdPties", "Dbtr", rest @ ..] => party(Side::Debtor, rest),
            ["RltdPties", "Cdtr", rest @ ..] => party(Side::Creditor, rest),
            ["RltdPties", "DbtrAcct", rest @ ..] => account(Side::Debtor, rest),
            ["RltdPties", "CdtrAcct", rest @ ..] => account(Side::Creditor, rest),
            _ => None,
        },
        _ => None,
    }
}

/// Reads the events of one file.
struct Parser<'a> {
    input: &'a str,
    reader: NsReader<&'a [u8]>,
    limits: &'a Limits,
    /// The local names of the open elements.
    path: Vec<String>,
    /// The element whose text is being collected, and the text so far.
    field: Option<Field>,
    text: String,
    /// The `Ccy` attribute of the amount element being read.
    attribute: Option<String>,
    statement: Option<RawStatement>,
    balance: Option<RawBalance>,
    entry: Option<RawEntry>,
    transaction: Option<RawTransaction>,
    entries: usize,
    statements: Vec<Statement>,
    root_closed: bool,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str, limits: &'a Limits) -> Self {
        Self {
            input,
            reader: NsReader::from_str(input),
            limits,
            path: Vec::new(),
            field: None,
            text: String::new(),
            attribute: None,
            statement: None,
            balance: None,
            entry: None,
            transaction: None,
            entries: 0,
            statements: Vec::new(),
            root_closed: false,
        }
    }

    /// The byte offset the reader has reached.
    fn position(&self) -> usize {
        usize::try_from(self.reader.buffer_position()).unwrap_or(usize::MAX)
    }

    /// The line of the file the reader has reached.
    fn line(&self) -> u64 {
        line_at(self.input.as_bytes(), self.position())
    }

    fn malformed(&self) -> ImportError {
        ImportError::Malformed {
            line: Some(self.line()),
        }
    }

    /// Puts `error` on the line of `position`, unless it knows its own.
    fn at(&self, position: usize, error: ImportError) -> ImportError {
        error.at_line(line_at(self.input.as_bytes(), position))
    }

    fn run(mut self) -> Result<Imported, ImportError> {
        loop {
            let read = self.reader.read_resolved_event();
            let (namespace, event) = match read {
                Ok(read) => read,
                Err(_) => return Err(self.malformed()),
            };
            let in_camt053 = is_camt053(&namespace);
            match event {
                Event::Start(start) => self.start(&start, in_camt053)?,
                // `<a/>` is a start and an end.
                Event::Empty(start) => {
                    self.start(&start, in_camt053)?;
                    self.end()?;
                }
                Event::End(_) => self.end()?,
                Event::Text(text) => self.collect(&text.xml10_content())?,
                Event::CData(text) => self.collect(&text.xml10_content())?,
                Event::GeneralRef(reference) => {
                    let mut character = String::new();
                    match reference.resolve_char_ref() {
                        Ok(Some(found)) => character.push(found),
                        Ok(None) => character.push_str(match &*reference {
                            "amp" => "&",
                            "lt" => "<",
                            "gt" => ">",
                            "quot" => "\"",
                            "apos" => "'",
                            // Any other entity needs a definition, and the
                            // file has none: it has no document type.
                            _ => return Err(self.malformed()),
                        }),
                        Err(_) => return Err(self.malformed()),
                    }
                    self.collect(&character)?;
                }
                Event::Decl(declaration) => {
                    let version = declaration.version().is_ok_and(|version| version == "1.0");
                    let encoding = match declaration.encoding() {
                        None => true,
                        // ISO 20022 files are UTF-8, and the bytes were
                        // already read as such (D-091).
                        Some(encoding) => {
                            encoding.is_ok_and(|encoding| encoding.eq_ignore_ascii_case("utf-8"))
                        }
                    };
                    if !(version && encoding) {
                        return Err(self.malformed());
                    }
                }
                Event::DocType(_) => return Err(self.malformed()),
                Event::Comment(_) | Event::PI(_) => {}
                Event::Eof => break,
            }
        }
        if !self.path.is_empty() || !self.root_closed || self.statements.is_empty() {
            return Err(self.malformed());
        }
        Ok(Imported::Statements(self.statements))
    }

    /// The path below `Stmt`, or none when the reader isn't inside one.
    fn below_statement(&self) -> Option<Vec<String>> {
        let [document, report, statement, rest @ ..] = self.path.as_slice() else {
            return None;
        };
        (document == "Document" && report == "BkToCstmrStmt" && statement == "Stmt")
            .then(|| rest.to_vec())
    }

    fn start(&mut self, start: &BytesStart<'_>, in_camt053: bool) -> Result<(), ImportError> {
        if self.root_closed {
            return Err(self.malformed());
        }
        self.limits
            .check_depth(self.path.len() + 1, Some(self.line()))?;
        let name = start.local_name().into_inner().to_owned();
        match self.path.len() {
            0 if name != "Document" || !in_camt053 => return Err(self.malformed()),
            1 if name != "BkToCstmrStmt" => return Err(self.malformed()),
            _ => {}
        }
        self.path.push(name);
        self.text.clear();
        self.attribute = None;
        self.field = None;
        if self.path.len() == 3 && self.path[2] == "Stmt" {
            self.statement = Some(RawStatement::default());
        }
        let Some(below) = self.below_statement() else {
            return Ok(());
        };
        let below: Vec<&str> = below.iter().map(String::as_str).collect();
        match below.as_slice() {
            ["Bal"] => self.balance = Some(RawBalance::default()),
            ["Ntry"] => {
                self.entries += 1;
                self.limits.check_entries(self.entries)?;
                self.entry = Some(RawEntry::default());
            }
            ["Ntry", "NtryDtls", "TxDtls"] => self.transaction = Some(RawTransaction::default()),
            _ => {}
        }
        self.field = classify(&below);
        if matches!(self.field, Some(Field::BalanceAmount | Field::EntryAmount)) {
            self.attribute = self.currency_attribute(start)?;
        }
        Ok(())
    }

    /// The `Ccy` attribute of an amount element.
    fn currency_attribute(&self, start: &BytesStart<'_>) -> Result<Option<String>, ImportError> {
        for attribute in start.attributes() {
            let attribute = attribute.map_err(|_| self.malformed())?;
            if attribute.key.local_name().into_inner() == "Ccy" {
                let value = attribute
                    .normalized_value(XmlVersion::Implicit1_0)
                    .map_err(|_| self.malformed())?
                    .into_owned();
                self.limits.check_text(&value, Some(self.line()))?;
                return Ok(Some(value));
            }
        }
        Ok(None)
    }

    /// Adds text to the element being read, if it is one the importer keeps.
    fn collect(&mut self, text: &str) -> Result<(), ImportError> {
        if self.field.is_some() {
            self.text.push_str(text);
            if self.text.len() > self.limits.max_text_bytes {
                return Err(ImportError::TextTooLong {
                    max_bytes: self.limits.max_text_bytes,
                    line: Some(self.line()),
                });
            }
        }
        Ok(())
    }

    fn end(&mut self) -> Result<(), ImportError> {
        let position = self.position();
        if let Some(field) = self.field.take() {
            let text = std::mem::take(&mut self.text);
            self.keep(field, text, position);
        }
        let below = self.below_statement();
        let below: Option<Vec<&str>> = below
            .as_ref()
            .map(|below| below.iter().map(String::as_str).collect());
        match below.as_deref() {
            Some(["Bal"]) => {
                if let (Some(balance), Some(statement)) =
                    (self.balance.take(), self.statement.as_mut())
                {
                    statement.balances.push(balance);
                }
            }
            Some(["Ntry"]) => {
                if let (Some(mut entry), Some(statement)) =
                    (self.entry.take(), self.statement.as_mut())
                {
                    entry.position = position;
                    statement.entries.push(entry);
                }
            }
            Some(["Ntry", "NtryDtls", "TxDtls"]) => {
                if let (Some(transaction), Some(entry)) =
                    (self.transaction.take(), self.entry.as_mut())
                {
                    entry.transactions.push(transaction);
                }
            }
            Some([]) => {
                if let Some(mut raw) = self.statement.take() {
                    raw.position = position;
                    let statement = self.statement_from(&raw)?;
                    self.statements.push(statement);
                }
            }
            _ => {}
        }
        self.path.pop();
        if self.path.is_empty() {
            self.root_closed = true;
        }
        Ok(())
    }

    /// Stores the text of a finished element where it belongs.
    fn keep(&mut self, field: Field, text: String, position: usize) {
        let raw = |text: String| Raw { text, position };
        // A text that is only whitespace, as pretty-printing leaves around
        // an empty element, is no text.
        let present = |text: String| (!text.trim().is_empty()).then_some(text);
        let amount = |text: String, currency: Option<String>| RawAmount {
            value: raw(text),
            currency,
        };
        let attribute = self.attribute.take();
        match field {
            Field::AccountIban => {
                if let Some(statement) = self.statement.as_mut() {
                    statement.iban = present(text);
                }
            }
            Field::AccountNumber => {
                if let Some(statement) = self.statement.as_mut() {
                    statement.number = present(text);
                }
            }
            Field::AccountCurrency => {
                if let Some(statement) = self.statement.as_mut() {
                    statement.currency = Some(raw(text));
                }
            }
            Field::BalanceKind => {
                if let Some(balance) = self.balance.as_mut() {
                    balance.kind = Some(text.trim().to_owned());
                }
            }
            Field::BalanceAmount => {
                if let Some(balance) = self.balance.as_mut() {
                    balance.amount = Some(amount(text, attribute));
                }
            }
            Field::BalanceIndicator => {
                if let Some(balance) = self.balance.as_mut() {
                    balance.indicator = Some(raw(text));
                }
            }
            Field::BalanceDate => {
                if let Some(balance) = self.balance.as_mut() {
                    balance.date = Some(raw(text));
                }
            }
            Field::EntryAmount => {
                if let Some(entry) = self.entry.as_mut() {
                    entry.amount = Some(amount(text, attribute));
                }
            }
            Field::EntryIndicator => {
                if let Some(entry) = self.entry.as_mut() {
                    entry.indicator = Some(raw(text));
                }
            }
            Field::EntryReversal => {
                if let Some(entry) = self.entry.as_mut() {
                    entry.reversal = Some(raw(text));
                }
            }
            Field::EntryStatus => {
                if let Some(entry) = self.entry.as_mut() {
                    entry.status = Some(text.trim().to_owned());
                }
            }
            Field::EntryStatusProprietary => {
                if let Some(entry) = self.entry.as_mut() {
                    entry.status = Some(String::new());
                }
            }
            Field::EntryBookingDate => {
                if let Some(entry) = self.entry.as_mut() {
                    entry.booking_date = Some(raw(text));
                }
            }
            Field::EntryValueDate => {
                if let Some(entry) = self.entry.as_mut() {
                    entry.value_date = Some(raw(text));
                }
            }
            Field::EntryReference => {
                if let Some(entry) = self.entry.as_mut() {
                    entry.reference = present(text);
                }
            }
            Field::TransactionReference => {
                if let Some(transaction) = self.transaction.as_mut() {
                    transaction.reference = present(text);
                }
            }
            Field::Party(side, part) => {
                if let Some(transaction) = self.transaction.as_mut() {
                    let party = match side {
                        Side::Debtor => &mut transaction.debtor,
                        Side::Creditor => &mut transaction.creditor,
                    };
                    let slot = match part {
                        PartyField::Name => &mut party.name,
                        PartyField::Iban => &mut party.iban,
                        PartyField::Other => &mut party.other,
                    };
                    *slot = present(text);
                }
            }
            Field::Remittance => {
                if let (Some(transaction), Some(text)) = (self.transaction.as_mut(), present(text))
                {
                    transaction.remittance.push(text);
                }
            }
        }
    }

    /// Turns what the file says about one statement into numbers and dates.
    fn statement_from(&self, raw: &RawStatement) -> Result<Statement, ImportError> {
        let currency = self.statement_currency(raw)?;
        let amount = |amount: &RawAmount, indicator: &Option<Raw>| {
            self.amount(amount, indicator, currency, raw.position)
        };
        let balance_of = |kinds: &[&str]| -> Result<Option<Balance>, ImportError> {
            let found = raw.balances.iter().find(|balance| {
                balance
                    .kind
                    .as_deref()
                    .is_some_and(|kind| kinds.contains(&kind))
            });
            let Some(balance) = found else {
                return Ok(None);
            };
            let (Some(value), Some(date)) = (&balance.amount, &balance.date) else {
                return Err(self.at(raw.position, ImportError::Malformed { line: None }));
            };
            Ok(Some(Balance {
                amount: amount(value, &balance.indicator)?,
                date: self.date(date)?,
            }))
        };
        // Some banks give the previous day's closing balance (`PRCD`)
        // instead of an opening balance (`OPBD`), or besides it.
        let opening_balance = match balance_of(&["OPBD"])? {
            Some(balance) => Some(balance),
            None => balance_of(&["PRCD"])?,
        };
        let closing_balance = balance_of(&["CLBD"])?;
        let mut entries = Vec::with_capacity(raw.entries.len());
        for entry in &raw.entries {
            entries.push(self.entry_from(entry, currency)?);
        }
        Ok(Statement {
            account: StatementAccount {
                iban: raw.iban.clone(),
                number: raw.number.clone(),
            },
            currency,
            opening_balance,
            closing_balance,
            entries,
        })
    }

    /// The statement's currency: the account's, otherwise the one its first
    /// balance or entry is in.
    fn statement_currency(&self, raw: &RawStatement) -> Result<Currency, ImportError> {
        let (code, position) = if let Some(code) = &raw.currency {
            (code.text.trim(), code.position)
        } else if let Some((code, position)) = raw
            .balances
            .iter()
            .filter_map(|balance| balance.amount.as_ref())
            .chain(raw.entries.iter().filter_map(|entry| entry.amount.as_ref()))
            .find_map(|amount| Some((amount.currency.as_deref()?, amount.value.position)))
        {
            (code, position)
        } else {
            return Err(self.at(raw.position, ImportError::Malformed { line: None }));
        };
        let code = CurrencyCode::new(code)
            .map_err(|_| self.at(position, ImportError::Malformed { line: None }))?;
        currency(code).map_err(|error| self.at(position, error))
    }

    /// An amount of the statement's `currency`, negative when the file says
    /// the account was debited.
    fn amount(
        &self,
        amount: &RawAmount,
        indicator: &Option<Raw>,
        currency: Currency,
        fallback: usize,
    ) -> Result<Money, ImportError> {
        let position = amount.value.position;
        let text = amount.value.text.trim();
        let malformed = |position: usize| self.at(position, ImportError::Malformed { line: None });
        // An amount in another currency can't be added to the statement's.
        let found = amount
            .currency
            .as_deref()
            .and_then(|code| CurrencyCode::new(code).ok())
            .ok_or_else(|| malformed(position))?;
        if found != currency.code() {
            return Err(ImportError::CurrencyMismatch {
                expected: currency.code(),
                found,
            });
        }
        let indicator = indicator.as_ref().ok_or_else(|| malformed(fallback))?;
        let negative = match indicator.text.trim() {
            "CRDT" => false,
            "DBIT" => true,
            _ => return Err(malformed(indicator.position)),
        };
        // The schema's amounts have no sign: the indicator says the direction.
        if text.starts_with(['-', '+']) {
            return Err(self.at(
                position,
                ImportError::BadAmount {
                    found: text.to_owned(),
                    line: None,
                },
            ));
        }
        let signed = if negative {
            format!("-{text}")
        } else {
            text.to_owned()
        };
        parse_amount(&signed, DecimalMark::Point, currency).map_err(|error| match error {
            // The text without the sign this code added.
            ImportError::BadAmount { line, .. } => self.at(
                position,
                ImportError::BadAmount {
                    found: text.to_owned(),
                    line,
                },
            ),
            ImportError::TooManyDecimalPlaces { currency, line, .. } => self.at(
                position,
                ImportError::TooManyDecimalPlaces {
                    found: text.to_owned(),
                    currency,
                    line,
                },
            ),
            ImportError::AmountTooLarge { line, .. } => self.at(
                position,
                ImportError::AmountTooLarge {
                    found: text.to_owned(),
                    line,
                },
            ),
            other => self.at(position, other),
        })
    }

    /// A date, from a `Dt` (`2026-10-01`, maybe with a time zone) or a
    /// `DtTm` (`2026-10-01T10:15:00`). Only the calendar day counts: a
    /// transaction date has no time zone (AGENTS.md §5, D-061), and the
    /// day is the one the bank wrote.
    fn date(&self, raw: &Raw) -> Result<Date, ImportError> {
        let malformed = || self.at(raw.position, ImportError::Malformed { line: None });
        let text = raw.text.trim();
        let day = text.get(..10).ok_or_else(malformed)?;
        let rest = &text[10..];
        if !(rest.is_empty() || rest.starts_with(['T', 'Z', '+', '-'])) {
            return Err(malformed());
        }
        let well_formed = day.bytes().enumerate().all(|(at, byte)| match at {
            4 | 7 => byte == b'-',
            _ => byte.is_ascii_digit(),
        });
        if !well_formed {
            return Err(malformed());
        }
        day.parse().map_err(|_| malformed())
    }

    fn entry_from(&self, raw: &RawEntry, currency: Currency) -> Result<Entry, ImportError> {
        let malformed = || self.at(raw.position, ImportError::Malformed { line: None });
        let amount = raw.amount.as_ref().ok_or_else(malformed)?;
        let amount_money = self.amount(amount, &raw.indicator, currency, raw.position)?;
        // `BOOK` is booked; anything else (`PDNG`, `INFO`, a code of the
        // bank's own) is not, and a file that doesn't say is not trusted.
        let status = match raw.status.as_deref() {
            Some("BOOK") => EntryStatus::Booked,
            Some(_) => EntryStatus::Pending,
            None => return Err(malformed()),
        };
        let value_date = raw
            .value_date
            .as_ref()
            .map(|date| self.date(date))
            .transpose()?;
        // The schema leaves the booking date optional; the value date then
        // stands in, as in MT940 (D-091).
        let booking_date = match &raw.booking_date {
            Some(date) => self.date(date)?,
            None => value_date.ok_or_else(malformed)?,
        };
        let reversal = match raw.reversal.as_ref().map(|r| r.text.trim()) {
            None | Some("false" | "0") => false,
            Some("true" | "1") => true,
            Some(_) => return Err(malformed()),
        };
        // Money coming in is from the debtor, money going out is to the
        // creditor; the other way round for a reversal, which lists the
        // parties of the payment it takes back.
        let credit = raw
            .indicator
            .as_ref()
            .is_some_and(|indicator| indicator.text.trim() == "CRDT");
        let from_debtor = credit != reversal;
        let (other_party_name, other_party_iban) = other_party(&raw.transactions, from_debtor);
        let bank_id = raw
            .reference
            .clone()
            .or_else(|| {
                // An entry with a single payment may have its bank reference
                // only there.
                match raw.transactions.as_slice() {
                    [only] => only.reference.clone(),
                    _ => None,
                }
            })
            .filter(|reference| {
                let reference = reference.trim();
                !NO_REFERENCE
                    .iter()
                    .any(|none| reference.eq_ignore_ascii_case(none))
            });
        let lines: Vec<&str> = raw
            .transactions
            .iter()
            .flat_map(|transaction| transaction.remittance.iter().map(String::as_str))
            .collect();
        let remittance_information = (!lines.is_empty()).then(|| lines.join("\n"));
        Ok(Entry {
            status,
            bank_id,
            booking_date,
            value_date,
            amount: amount_money,
            other_party_name,
            other_party_iban,
            remittance_information,
        })
    }
}

/// The other party of an entry, from its payments. Only a party all
/// payments agree on counts: a collective booking (one debit for many
/// payments) has no single other party.
fn other_party(
    transactions: &[RawTransaction],
    from_debtor: bool,
) -> (Option<String>, Option<String>) {
    let mut parties = transactions.iter().filter_map(|transaction| {
        let party = if from_debtor {
            &transaction.debtor
        } else {
            &transaction.creditor
        };
        party.identity()
    });
    let Some(first) = parties.next() else {
        return (None, None);
    };
    if parties.all(|party| party == first) {
        (first.0.map(str::to_owned), first.1.map(str::to_owned))
    } else {
        (None, None)
    }
}

#[cfg(test)]
mod tests;
