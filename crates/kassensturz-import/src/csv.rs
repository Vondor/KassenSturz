// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! CSV files from online banking: the format most households can download,
//! and the one that says least about itself.
//!
//! A CSV file names neither its text encoding, nor its separator, nor how
//! it writes amounts and dates, nor which column holds what, and every bank
//! writes it differently. So the importer reads nothing by guesswork: the
//! user's [`CsvSettings`] say all of it, chosen once per bank (D-095). What
//! the importer does is read exactly what they describe, and refuse a file
//! that doesn't fit them rather than read it wrong: an amount that isn't
//! written with the chosen decimal mark, a date not in the chosen order, a
//! named column the file doesn't have.
//!
//! CSV files carry no bank IDs, so the import pipeline's check for similar
//! entries is what recognizes one imported twice (D-092). Nor do they carry
//! balances, so there is no balance check.
//!
//! The file is untrusted (AGENTS.md §8): rows are read one at a time, with
//! at most [`MAX_COLUMNS`] fields each, and the limits on entries and text
//! are kept while reading.

mod settings;
mod text;
mod values;

use std::collections::HashMap;

use kassensturz_core::{Currency, CurrencyCode, Money};

use crate::error::ImportError;
use crate::importer::{
    ImportKind, Imported, Importer, ImporterInfo, RECOGNITION_BYTES, Recognition,
};
use crate::limits::Limits;
use crate::statement::{Entry, EntryStatus, Statement, StatementAccount};

pub use settings::{
    AmountColumns, Column, CsvSettings, DateOrder, MAX_COLUMNS, StatusColumn, TextEncoding,
};
use text::{Row, Rows, decode, without_first_lines, without_last_lines};
use values::{parse_csv_amount, parse_date};

/// The importer for CSV files, with the ID `csv`: one bank's [`CsvSettings`]
/// and the currency of the account the file is for.
///
/// The currency comes from the account, since most CSV files don't name
/// it; with a currency column in the settings, every row must be in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Csv {
    settings: CsvSettings,
    currency: Currency,
}

impl Csv {
    /// An importer reading files as `settings` describe, in `currency`.
    /// Fails with `import.bad-settings` if the settings can't be used
    /// ([`CsvSettings::check`]).
    pub fn new(settings: CsvSettings, currency: Currency) -> Result<Self, ImportError> {
        settings.check()?;
        Ok(Self { settings, currency })
    }

    /// The settings the importer reads files with.
    pub fn settings(&self) -> &CsvSettings {
        &self.settings
    }
}

impl Importer for Csv {
    fn info(&self) -> ImporterInfo {
        ImporterInfo {
            id: "csv",
            kind: ImportKind::Statements,
            extensions: &["csv", "txt"],
        }
    }

    /// Yes if the header row has every column the settings name; no if it
    /// lacks one, or if the start isn't text in the settings' encoding;
    /// maybe if its first row has the settings' separator.
    fn recognize(&self, start: &[u8]) -> Recognition {
        // The last line of a cut-off start may be cut in the middle of a
        // character.
        let start = if start.len() >= RECOGNITION_BYTES {
            start
                .iter()
                .rposition(|&byte| byte == b'\n')
                .map_or(&start[..0], |at| &start[..=at])
        } else {
            start
        };
        let Ok(text) = decode(start, self.settings.encoding) else {
            return Recognition::No;
        };
        let body = without_first_lines(&text, self.settings.skip_lines);
        let mut rows = Rows::new(body, self.settings.delimiter, 1);
        let first = loop {
            match rows.next_row() {
                Some(Ok(row)) if row.is_empty() => {}
                Some(Ok(row)) => break row,
                Some(Err(_)) => return Recognition::No,
                None => return Recognition::Maybe,
            }
        };
        if self.settings.header && self.names().next().is_some() {
            match Header::read(&first, &self.settings) {
                Ok(_) => Recognition::Yes,
                Err(_) => Recognition::No,
            }
        } else if first.fields.len() > 1 {
            Recognition::Maybe
        } else {
            Recognition::No
        }
    }

    fn read(&self, input: &[u8], limits: &Limits) -> Result<Imported, ImportError> {
        let text = decode(input, self.settings.encoding)?;
        let body = without_first_lines(&text, self.settings.skip_lines);
        let body = without_last_lines(body, self.settings.skip_last_lines);
        let first_line = u64::try_from(self.settings.skip_lines)
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        let mut rows = Rows::new(body, self.settings.delimiter, first_line);
        let mut reader = Reader {
            importer: self,
            limits,
            header: None,
            statements: Vec::new(),
            by_account: HashMap::new(),
            entries: 0,
        };
        if self.settings.account.is_none() {
            reader
                .statements
                .push(self.statement(StatementAccount::default()));
        }
        while let Some(row) = rows.next_row() {
            let row = row?;
            if row.is_empty() {
                continue;
            }
            if self.settings.header && reader.header.is_none() {
                reader.header = Some(Header::read(&row, &self.settings)?);
                continue;
            }
            reader.row(&row).map_err(|error| error.at_line(row.line))?;
        }
        if self.settings.header && reader.header.is_none() {
            // Not even the header row: an empty file, or all of it skipped.
            return Err(ImportError::Malformed { line: None });
        }
        Ok(Imported::Statements(reader.statements))
    }
}

impl Csv {
    /// The column names the settings use.
    fn names(&self) -> impl Iterator<Item = &str> {
        self.settings.columns().filter_map(|column| match column {
            Column::Name(name) => Some(name.as_str()),
            Column::Number(_) => None,
        })
    }

    fn statement(&self, account: StatementAccount) -> Statement {
        Statement {
            account,
            currency: self.currency,
            opening_balance: None,
            closing_balance: None,
            entries: Vec::new(),
        }
    }
}

/// The header row: where each named column is.
struct Header {
    /// The index of each name the settings use, counted from 0.
    columns: HashMap<String, usize>,
}

impl Header {
    /// Finds every column the settings name in `row`. A name the row
    /// doesn't have, or has twice, fails, so a bank's changed file is never
    /// read with the wrong columns.
    fn read(row: &Row<'_>, settings: &CsvSettings) -> Result<Self, ImportError> {
        let mut columns = HashMap::new();
        for column in settings.columns() {
            let Column::Name(name) = column else {
                continue;
            };
            let wanted = name.trim();
            let mut found = row
                .fields
                .iter()
                .enumerate()
                .filter(|(_, field)| field.trim() == wanted)
                .map(|(index, _)| index);
            let index = match (found.next(), found.next()) {
                (Some(index), None) => index,
                (None, _) => {
                    return Err(ImportError::ColumnNotFound {
                        column: name.clone(),
                        line: Some(row.line),
                    });
                }
                (Some(_), Some(_)) => {
                    return Err(ImportError::ColumnNotUnique {
                        column: name.clone(),
                        line: Some(row.line),
                    });
                }
            };
            columns.insert(name.clone(), index);
        }
        Ok(Self { columns })
    }
}

/// What has been read of a file so far.
struct Reader<'a> {
    importer: &'a Csv,
    limits: &'a Limits,
    header: Option<Header>,
    statements: Vec<Statement>,
    /// Each account's statement, by the account column's text.
    by_account: HashMap<String, usize>,
    entries: usize,
}

impl Reader<'_> {
    /// Reads one row into its statement.
    fn row(&mut self, row: &Row<'_>) -> Result<(), ImportError> {
        let settings = &self.importer.settings;
        let currency = self.importer.currency;
        self.entries += 1;
        self.limits.check_entries(self.entries)?;
        if let Some(column) = &settings.currency {
            let found = CurrencyCode::new(self.field(row, column).trim())
                .map_err(|_| ImportError::Malformed { line: None })?;
            if found != currency.code() {
                return Err(ImportError::CurrencyMismatch {
                    expected: currency.code(),
                    found,
                });
            }
        }
        let status = match &settings.status {
            Some(status) => {
                let value = self.field(row, &status.column).trim();
                if status.booked.iter().any(|booked| booked.trim() == value) {
                    EntryStatus::Booked
                } else {
                    EntryStatus::Pending
                }
            }
            None => EntryStatus::Booked,
        };
        let value_date = match &settings.value_date {
            Some(column) => match self.field(row, column) {
                text if text.trim().is_empty() => None,
                text => Some(parse_date(text, settings.date_order)?),
            },
            None => None,
        };
        let remittance: Vec<&str> = settings
            .remittance_information
            .iter()
            .map(|column| self.field(row, column))
            .filter(|text| !text.trim().is_empty())
            .collect();
        let entry = Entry {
            status,
            bank_id: None,
            booking_date: parse_date(
                required(self.field(row, &settings.booking_date))?,
                settings.date_order,
            )?,
            value_date,
            amount: self.amount(row)?,
            other_party_name: self.text(row, settings.other_party_name.as_ref())?,
            other_party_iban: self.text(row, settings.other_party_iban.as_ref())?,
            remittance_information: if remittance.is_empty() {
                None
            } else {
                let joined = remittance.join("\n");
                self.limits.check_text(&joined, None)?;
                Some(joined)
            },
        };
        let statement = match &settings.account {
            Some(column) => {
                let text = self.field(row, column);
                self.limits.check_text(text, None)?;
                match self.by_account.get(text) {
                    Some(&index) => index,
                    None => {
                        let index = self.statements.len();
                        self.statements
                            .push(self.importer.statement(statement_account(text)));
                        self.by_account.insert(text.to_owned(), index);
                        index
                    }
                }
            }
            None => 0,
        };
        self.statements[statement].entries.push(entry);
        Ok(())
    }

    /// The amount of `row`, as the settings say it is written.
    fn amount(&self, row: &Row<'_>) -> Result<Money, ImportError> {
        let settings = &self.importer.settings;
        let read =
            |text: &str| parse_csv_amount(text, settings.decimal_mark, self.importer.currency);
        match &settings.amount {
            AmountColumns::Signed(column) => read(required(self.field(row, column))?),
            AmountColumns::DebitCredit { debit, credit } => {
                let read_optional = |column| {
                    let text = self.field(row, column);
                    if text.trim().is_empty() {
                        Ok(None)
                    } else {
                        read(text).map(|amount| Some((amount, text)))
                    }
                };
                let debit = read_optional(debit)?;
                let credit = read_optional(credit)?;
                let not_zero = |&(amount, _): &(Money, &str)| !amount.is_zero();
                match (debit.filter(not_zero), credit.filter(not_zero)) {
                    (Some(_), Some(_)) => Err(ImportError::Malformed { line: None }),
                    // Money out, whether or not the file writes a minus
                    // sign; negating a positive amount can't overflow.
                    (Some((amount, _)), None) if amount.is_negative() => Ok(amount),
                    (Some((amount, _)), None) => Ok(negated(amount)),
                    (None, Some((amount, text))) if amount.is_negative() => {
                        Err(ImportError::BadAmount {
                            found: text.to_owned(),
                            line: None,
                        })
                    }
                    (None, Some((amount, _))) => Ok(amount),
                    // Both zero, or one zero and the other empty.
                    (None, None) if debit.is_some() || credit.is_some() => {
                        Ok(Money::zero(self.importer.currency.code()))
                    }
                    (None, None) => Err(ImportError::Malformed { line: None }),
                }
            }
            AmountColumns::WithDirection {
                amount,
                direction,
                money_out,
                money_in,
            } => {
                let text = required(self.field(row, amount))?;
                if text.trim().starts_with(['-', '+']) {
                    return Err(ImportError::BadAmount {
                        found: text.to_owned(),
                        line: None,
                    });
                }
                let amount = read(text)?;
                let direction = self.field(row, direction).trim();
                if direction == money_out.trim() {
                    // It has no sign, so negating it can't overflow.
                    Ok(negated(amount))
                } else if direction == money_in.trim() {
                    Ok(amount)
                } else {
                    Err(ImportError::Malformed { line: None })
                }
            }
        }
    }

    /// The text of `column` in `row`, as delivered, or `None` if there is
    /// no such column or it is empty or spaces only.
    fn text(&self, row: &Row<'_>, column: Option<&Column>) -> Result<Option<String>, ImportError> {
        let Some(column) = column else {
            return Ok(None);
        };
        let text = self.field(row, column);
        if text.trim().is_empty() {
            return Ok(None);
        }
        self.limits.check_text(text, None)?;
        Ok(Some(text.to_owned()))
    }

    /// The field of `row` in `column`; empty if the row is shorter.
    fn field<'r>(&self, row: &'r Row<'_>, column: &Column) -> &'r str {
        let index = match column {
            Column::Number(number) => number - 1,
            Column::Name(name) => self
                .header
                .as_ref()
                .and_then(|header| header.columns.get(name))
                .copied()
                // check() allows names only with a header, and
                // Header::read found each of them.
                .unwrap_or(usize::MAX),
        };
        row.field(index)
    }
}

/// `text`, unless it is empty or spaces only, which for a field every row
/// needs, such as the date, means the row is damaged or cut off.
fn required(text: &str) -> Result<&str, ImportError> {
    if text.trim().is_empty() {
        Err(ImportError::Malformed { line: None })
    } else {
        Ok(text)
    }
}

/// `amount` with the other sign; only for amounts that aren't negative,
/// whose negation always fits.
fn negated(amount: Money) -> Money {
    Money::from_minor_units(-amount.minor_units(), amount.currency())
}

/// The account as the account column names it: an IBAN if it looks like
/// one (two letters, then two digits), any other number otherwise. It is
/// kept as delivered, like everything else (D-067).
fn statement_account(text: &str) -> StatementAccount {
    if text.trim().is_empty() {
        return StatementAccount::default();
    }
    let bytes = text.trim().as_bytes();
    let iban = bytes.len() >= 4
        && bytes[..2].iter().all(u8::is_ascii_alphabetic)
        && bytes[2..4].iter().all(u8::is_ascii_digit);
    if iban {
        StatementAccount {
            iban: Some(text.to_owned()),
            number: None,
        }
    } else {
        StatementAccount {
            iban: None,
            number: Some(text.to_owned()),
        }
    }
}

#[cfg(test)]
mod tests;
