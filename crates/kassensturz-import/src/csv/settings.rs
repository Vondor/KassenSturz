// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! How a bank's CSV file is to be read, and the text form that keeps it.

use std::fmt::Write as _;

use crate::amount::DecimalMark;
use crate::error::ImportError;

/// The most columns a row may have, and the highest column number settings
/// may name. Bank files have a few dozen; the limit keeps a damaged file
/// with a long row of separators from using memory out of proportion.
pub const MAX_COLUMNS: usize = 1024;

/// How to read one bank's CSV files: the text encoding, the separator,
/// which rows to skip, how amounts and dates are written and which column
/// holds what (D-095).
///
/// CSV files say none of this themselves, so the user chooses it once per
/// bank: in the command-line tool as a settings file
/// ([`CsvSettings::from_text`]), from M3 on a screen. Everything is
/// explicit; nothing is guessed from the file except the text encoding
/// when [`CsvSettings::encoding`] is `None`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CsvSettings {
    /// The file's text encoding, or `None` to take UTF-8 if the file is
    /// valid UTF-8 and Windows-1252 otherwise. A UTF-8 byte order mark
    /// always means UTF-8.
    pub encoding: Option<TextEncoding>,
    /// The character between two columns, such as `;` or `,`.
    pub delimiter: char,
    /// How many lines at the start to skip before the header or the first
    /// row, e.g. lines with the account's details.
    pub skip_lines: usize,
    /// How many lines at the end to skip, not counting empty ones, e.g. a
    /// line with the final balance.
    pub skip_last_lines: usize,
    /// Whether the first row (after the skipped lines) names the columns.
    pub header: bool,
    /// The character between whole units and decimal places.
    pub decimal_mark: DecimalMark,
    /// The order of day, month and year in dates.
    pub date_order: DateOrder,
    /// The date the bank booked an entry.
    pub booking_date: Column,
    /// The value date, if the file has one.
    pub value_date: Option<Column>,
    /// Where the amount is and how its direction is written.
    pub amount: AmountColumns,
    /// The other party's name.
    pub other_party_name: Option<Column>,
    /// The other party's IBAN or account number.
    pub other_party_iban: Option<Column>,
    /// The columns of the remittance information, joined in this order
    /// with a line break, as for CAMT.053 (Q-052). Empty ones are left out.
    pub remittance_information: Vec<Column>,
    /// A column saying whether the bank has booked an entry, and the values
    /// that mean booked; any other value means not booked yet (D-030).
    /// Without it, every row counts as booked.
    pub status: Option<StatusColumn>,
    /// A column with each row's currency code. Every row must then be in
    /// the account's currency, so an amount in another currency is never
    /// taken for one in the account's.
    pub currency: Option<Column>,
    /// A column naming the account a row belongs to, for files with
    /// several accounts: each account becomes a statement of its own.
    pub account: Option<Column>,
}

/// The text encodings KassenSturz reads CSV files in: the ones German and
/// other European banks use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextEncoding {
    /// UTF-8, with or without a byte order mark.
    Utf8,
    /// Windows-1252, which Windows calls "ANSI" in Western Europe. Files
    /// described as ISO-8859-1 are read with it too: it has every character
    /// ISO-8859-1 has, at the same place.
    Windows1252,
    /// ISO-8859-15 (Latin-9), ISO-8859-1 with the euro sign.
    Iso8859_15,
}

impl TextEncoding {
    /// Every encoding, for lists to choose from.
    pub const ALL: [Self; 3] = [Self::Utf8, Self::Windows1252, Self::Iso8859_15];

    /// The encoding's name in settings and in error reports.
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Utf8 => "utf-8",
            Self::Windows1252 => "windows-1252",
            Self::Iso8859_15 => "iso-8859-15",
        }
    }
}

/// The order of day, month and year in a file's dates. Day and month may
/// have one or two digits, the year two or four; the parts are separated
/// by `.`, `-` or `/`, or not at all when all eight digits are there
/// (`20261005`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DateOrder {
    /// `05.10.2026`, as in Germany.
    DayMonthYear,
    /// `10/05/2026`, as in the United States.
    MonthDayYear,
    /// `2026-10-05`, as in ISO 8601.
    YearMonthDay,
}

impl DateOrder {
    const ALL: [Self; 3] = [Self::DayMonthYear, Self::MonthDayYear, Self::YearMonthDay];

    const fn keyword(self) -> &'static str {
        match self {
            Self::DayMonthYear => "day-month-year",
            Self::MonthDayYear => "month-day-year",
            Self::YearMonthDay => "year-month-day",
        }
    }
}

/// A column of the file: by its number, counted from 1 as a spreadsheet
/// shows them, or by its name in the header row.
///
/// A name is safer: if the bank reorders its columns, the name still finds
/// the right one, and if it renames one, the import fails instead of
/// reading the wrong column.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Column {
    /// The column's number, counted from 1.
    Number(usize),
    /// The column's name in the header row, compared without the spaces
    /// around it.
    Name(String),
}

/// Where a file has its amounts, and how it says whether money came in or
/// went out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AmountColumns {
    /// One column, negative for money out: `-12,50`.
    Signed(Column),
    /// One column for money out and one for money in, each row filling one
    /// of them; the other is empty or zero. Money out counts as out whether
    /// or not the file writes a minus sign; money in can't be negative.
    DebitCredit {
        /// The column for money out (German: Soll).
        debit: Column,
        /// The column for money in (German: Haben).
        credit: Column,
    },
    /// One column without a sign and one saying the direction, such as `S`
    /// and `H`.
    WithDirection {
        /// The amount, without a sign.
        amount: Column,
        /// The direction.
        direction: Column,
        /// The value of the direction column for money out.
        money_out: String,
        /// The value of the direction column for money in.
        money_in: String,
    },
}

/// A column with an entry's status, and its values that mean booked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusColumn {
    /// The column.
    pub column: Column,
    /// The values meaning booked, compared without the spaces around them,
    /// e.g. `Gebucht`.
    pub booked: Vec<String>,
}

impl CsvSettings {
    /// Settings with the given separator, decimal mark, date order, booking
    /// date and amount, a header row, no skipped lines, the encoding taken
    /// from the file and no other column.
    pub fn new(
        delimiter: char,
        decimal_mark: DecimalMark,
        date_order: DateOrder,
        booking_date: Column,
        amount: AmountColumns,
    ) -> Self {
        Self {
            encoding: None,
            delimiter,
            skip_lines: 0,
            skip_last_lines: 0,
            header: true,
            decimal_mark,
            date_order,
            booking_date,
            value_date: None,
            amount,
            other_party_name: None,
            other_party_iban: None,
            remittance_information: Vec::new(),
            status: None,
            currency: None,
            account: None,
        }
    }

    /// Checks that the settings can be used: a separator that can separate,
    /// column numbers from 1 to [`MAX_COLUMNS`], names only with a header
    /// row, at least one value meaning booked, and two different values for
    /// the directions.
    pub fn check(&self) -> Result<(), ImportError> {
        let bad = || ImportError::BadSettings { line: None };
        if matches!(self.delimiter, '"' | '\n' | '\r') {
            return Err(bad());
        }
        for column in self.columns() {
            match column {
                Column::Number(number) if !(1..=MAX_COLUMNS).contains(number) => return Err(bad()),
                Column::Name(name) if !self.header || name.trim().is_empty() => return Err(bad()),
                _ => {}
            }
        }
        if let Some(status) = &self.status
            && status.booked.is_empty()
        {
            return Err(bad());
        }
        if let AmountColumns::WithDirection {
            money_out,
            money_in,
            ..
        } = &self.amount
            && (money_out.trim() == money_in.trim()
                || money_out.trim().is_empty()
                || money_in.trim().is_empty())
        {
            return Err(bad());
        }
        Ok(())
    }

    /// Every column the settings name.
    pub(crate) fn columns(&self) -> impl Iterator<Item = &Column> {
        let amount: Vec<&Column> = match &self.amount {
            AmountColumns::Signed(column) => vec![column],
            AmountColumns::DebitCredit { debit, credit } => vec![debit, credit],
            AmountColumns::WithDirection {
                amount, direction, ..
            } => vec![amount, direction],
        };
        [&self.booking_date]
            .into_iter()
            .chain(amount)
            .chain(self.value_date.as_ref())
            .chain(self.other_party_name.as_ref())
            .chain(self.other_party_iban.as_ref())
            .chain(&self.remittance_information)
            .chain(self.status.as_ref().map(|status| &status.column))
            .chain(self.currency.as_ref())
            .chain(self.account.as_ref())
    }

    /// Reads settings from their text form, as [`CsvSettings::to_text`]
    /// writes it: one `key = value` per line; empty lines and lines starting
    /// with `#` are left out.
    ///
    /// ```text
    /// # Example Bank, giro account
    /// encoding = windows-1252
    /// delimiter = ";"
    /// skip-lines = 4
    /// decimal-mark = comma
    /// date-order = day-month-year
    /// booking-date = "Buchungstag"
    /// value-date = "Valuta"
    /// amount = "Betrag"
    /// other-party-name = "Name"
    /// remittance-information = "Verwendungszweck"
    /// ```
    ///
    /// A column is a number, counted from 1, or a name in double quotes;
    /// other text is in double quotes too, where `\"`, `\\`, `\t`, `\n` and
    /// `\r` stand for a quote, a backslash, a tab and the two line breaks.
    /// The keys are those of [`CsvSettings::to_text`];
    /// `remittance-information` and `booked` may be given more than once. An
    /// unknown key or a key given twice fails with `import.bad-settings` and
    /// the line, a missing one or settings that contradict each other
    /// without it.
    pub fn from_text(text: &str) -> Result<Self, ImportError> {
        let mut found = Found::default();
        for (number, line) in (1_u64..).zip(text.lines()) {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            found
                .take(line)
                .map_err(|()| ImportError::BadSettings { line: Some(number) })?;
        }
        let settings = found
            .finish()
            .ok_or(ImportError::BadSettings { line: None })?;
        settings.check()?;
        Ok(settings)
    }

    /// The settings as text, for keeping them in a file, one `key = value`
    /// per line. [`CsvSettings::from_text`] reads it back.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        let mut line = |key: &str, value: String| {
            let _ = writeln!(out, "{key} = {value}");
        };
        line(
            "encoding",
            self.encoding
                .map_or("auto", TextEncoding::keyword)
                .to_owned(),
        );
        line("delimiter", quoted(&self.delimiter.to_string()));
        line("skip-lines", self.skip_lines.to_string());
        line("skip-last-lines", self.skip_last_lines.to_string());
        line("header", if self.header { "yes" } else { "no" }.to_owned());
        line(
            "decimal-mark",
            match self.decimal_mark {
                DecimalMark::Point => "point",
                DecimalMark::Comma => "comma",
            }
            .to_owned(),
        );
        line("date-order", self.date_order.keyword().to_owned());
        line("booking-date", column_text(&self.booking_date));
        match &self.amount {
            AmountColumns::Signed(column) => line("amount", column_text(column)),
            AmountColumns::DebitCredit { debit, credit } => {
                line("debit", column_text(debit));
                line("credit", column_text(credit));
            }
            AmountColumns::WithDirection {
                amount,
                direction,
                money_out,
                money_in,
            } => {
                line("amount", column_text(amount));
                line("direction", column_text(direction));
                line("money-out", quoted(money_out));
                line("money-in", quoted(money_in));
            }
        }
        let optional = [
            ("value-date", &self.value_date),
            ("other-party-name", &self.other_party_name),
            ("other-party-iban", &self.other_party_iban),
            ("currency", &self.currency),
            ("account", &self.account),
        ];
        for (key, column) in optional {
            if let Some(column) = column {
                line(key, column_text(column));
            }
        }
        for column in &self.remittance_information {
            line("remittance-information", column_text(column));
        }
        if let Some(status) = &self.status {
            line("status", column_text(&status.column));
            for value in &status.booked {
                line("booked", quoted(value));
            }
        }
        out
    }
}

/// The values read from a settings text so far.
#[derive(Default)]
struct Found {
    encoding: Option<Option<TextEncoding>>,
    delimiter: Option<char>,
    skip_lines: Option<usize>,
    skip_last_lines: Option<usize>,
    header: Option<bool>,
    decimal_mark: Option<DecimalMark>,
    date_order: Option<DateOrder>,
    booking_date: Option<Column>,
    value_date: Option<Column>,
    amount: Option<Column>,
    debit: Option<Column>,
    credit: Option<Column>,
    direction: Option<Column>,
    money_out: Option<String>,
    money_in: Option<String>,
    other_party_name: Option<Column>,
    other_party_iban: Option<Column>,
    remittance_information: Vec<Column>,
    status: Option<Column>,
    booked: Vec<String>,
    currency: Option<Column>,
    account: Option<Column>,
}

impl Found {
    /// Takes one `key = value` line.
    fn take(&mut self, line: &str) -> Result<(), ()> {
        let (key, value) = line.split_once('=').ok_or(())?;
        let value = value.trim();
        match key.trim() {
            "encoding" => set(
                &mut self.encoding,
                match value {
                    "auto" => None,
                    _ => Some(
                        TextEncoding::ALL
                            .into_iter()
                            .find(|encoding| encoding.keyword() == value)
                            .ok_or(())?,
                    ),
                },
            ),
            "delimiter" => {
                let text = unquoted(value)?;
                let mut chars = text.chars();
                match (chars.next(), chars.next()) {
                    (Some(delimiter), None) => set(&mut self.delimiter, delimiter),
                    _ => Err(()),
                }
            }
            "skip-lines" => set(&mut self.skip_lines, count(value)?),
            "skip-last-lines" => set(&mut self.skip_last_lines, count(value)?),
            "header" => set(
                &mut self.header,
                match value {
                    "yes" => true,
                    "no" => false,
                    _ => return Err(()),
                },
            ),
            "decimal-mark" => set(
                &mut self.decimal_mark,
                match value {
                    "point" => DecimalMark::Point,
                    "comma" => DecimalMark::Comma,
                    _ => return Err(()),
                },
            ),
            "date-order" => set(
                &mut self.date_order,
                DateOrder::ALL
                    .into_iter()
                    .find(|order| order.keyword() == value)
                    .ok_or(())?,
            ),
            "booking-date" => set(&mut self.booking_date, column(value)?),
            "value-date" => set(&mut self.value_date, column(value)?),
            "amount" => set(&mut self.amount, column(value)?),
            "debit" => set(&mut self.debit, column(value)?),
            "credit" => set(&mut self.credit, column(value)?),
            "direction" => set(&mut self.direction, column(value)?),
            "money-out" => set(&mut self.money_out, unquoted(value)?),
            "money-in" => set(&mut self.money_in, unquoted(value)?),
            "other-party-name" => set(&mut self.other_party_name, column(value)?),
            "other-party-iban" => set(&mut self.other_party_iban, column(value)?),
            "remittance-information" => {
                self.remittance_information.push(column(value)?);
                Ok(())
            }
            "status" => set(&mut self.status, column(value)?),
            "booked" => {
                self.booked.push(unquoted(value)?);
                Ok(())
            }
            "currency" => set(&mut self.currency, column(value)?),
            "account" => set(&mut self.account, column(value)?),
            _ => Err(()),
        }
    }

    /// The settings, if everything needed was given and nothing that
    /// contradicts it.
    fn finish(self) -> Option<CsvSettings> {
        let amount = match (
            self.amount,
            self.debit,
            self.credit,
            self.direction,
            self.money_out,
            self.money_in,
        ) {
            (Some(amount), None, None, None, None, None) => AmountColumns::Signed(amount),
            (None, Some(debit), Some(credit), None, None, None) => {
                AmountColumns::DebitCredit { debit, credit }
            }
            (Some(amount), None, None, Some(direction), Some(money_out), Some(money_in)) => {
                AmountColumns::WithDirection {
                    amount,
                    direction,
                    money_out,
                    money_in,
                }
            }
            _ => return None,
        };
        let status = match (self.status, self.booked.is_empty()) {
            (Some(column), false) => Some(StatusColumn {
                column,
                booked: self.booked,
            }),
            (None, true) => None,
            _ => return None,
        };
        Some(CsvSettings {
            encoding: self.encoding.flatten(),
            delimiter: self.delimiter?,
            skip_lines: self.skip_lines.unwrap_or(0),
            skip_last_lines: self.skip_last_lines.unwrap_or(0),
            header: self.header.unwrap_or(true),
            decimal_mark: self.decimal_mark?,
            date_order: self.date_order?,
            booking_date: self.booking_date?,
            value_date: self.value_date,
            amount,
            other_party_name: self.other_party_name,
            other_party_iban: self.other_party_iban,
            remittance_information: self.remittance_information,
            status,
            currency: self.currency,
            account: self.account,
        })
    }
}

/// Sets a value given once only.
fn set<T>(slot: &mut Option<T>, value: T) -> Result<(), ()> {
    if slot.is_some() {
        return Err(());
    }
    *slot = Some(value);
    Ok(())
}

fn count(value: &str) -> Result<usize, ()> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(());
    }
    value.parse().map_err(|_| ())
}

fn column(value: &str) -> Result<Column, ()> {
    if value.starts_with('"') {
        unquoted(value).map(Column::Name)
    } else {
        count(value).map(Column::Number)
    }
}

/// The text inside double quotes, with its escapes undone.
fn unquoted(value: &str) -> Result<String, ()> {
    let inner = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or(())?;
    let mut text = String::new();
    let mut chars = inner.chars();
    while let Some(char) = chars.next() {
        match char {
            '\\' => text.push(match chars.next() {
                Some('"') => '"',
                Some('\\') => '\\',
                Some('t') => '\t',
                Some('n') => '\n',
                Some('r') => '\r',
                _ => return Err(()),
            }),
            '"' => return Err(()),
            char => text.push(char),
        }
    }
    Ok(text)
}

/// `text` in double quotes, as [`unquoted`] reads it.
fn quoted(text: &str) -> String {
    let mut out = String::from('"');
    for char in text.chars() {
        match char {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            char => out.push(char),
        }
    }
    out.push('"');
    out
}

fn column_text(column: &Column) -> String {
    match column {
        Column::Number(number) => number.to_string(),
        Column::Name(name) => quoted(name),
    }
}
