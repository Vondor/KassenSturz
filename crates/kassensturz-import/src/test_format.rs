// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A made-up statement format, for tests and for trying out the import
//! with the command-line tool before a real format exists (D-092), and a
//! small example of an importer. No bank writes it, and the app never
//! offers it.
//!
//! The first line is `KSTEST`, then one record per line, its fields
//! separated by `;`, an empty field meaning none:
//!
//! ```text
//! KSTEST
//! account;IBAN;NUMBER;CURRENCY
//! opening;DATE;AMOUNT
//! entry;booked|pending;BOOKING DATE;VALUE DATE;AMOUNT;BANK ID;NAME;IBAN;PURPOSE
//! closing;DATE;AMOUNT
//! ```
//!
//! Each `account` line starts a statement. Amounts have a decimal point.

use kassensturz_core::{CurrencyCode, Date};

use crate::amount::{DecimalMark, currency, parse_amount};
use crate::error::ImportError;
use crate::importer::{ImportKind, Imported, Importer, ImporterInfo, Recognition};
use crate::limits::Limits;
use crate::statement::{Balance, Entry, EntryStatus, Statement, StatementAccount};

const MAGIC: &[u8] = b"KSTEST";

/// The importer for the made-up format, with the ID `kstest`.
#[derive(Clone, Copy, Debug, Default)]
pub struct TestFormat;

impl Importer for TestFormat {
    fn info(&self) -> ImporterInfo {
        ImporterInfo {
            id: "kstest",
            kind: ImportKind::Statements,
            extensions: &["kstest"],
        }
    }

    fn recognize(&self, start: &[u8]) -> Recognition {
        if start.starts_with(MAGIC) {
            Recognition::Yes
        } else {
            Recognition::No
        }
    }

    fn read(&self, input: &[u8], limits: &Limits) -> Result<Imported, ImportError> {
        let text = std::str::from_utf8(input).map_err(|_| ImportError::Malformed { line: None })?;
        let mut lines = (1_u64..).zip(text.lines());
        match lines.next() {
            Some((_, "KSTEST")) => {}
            _ => return Err(ImportError::Malformed { line: Some(1) }),
        }
        let mut statements: Vec<Statement> = Vec::new();
        let mut entries = 0;
        for (number, line) in lines {
            read_line(line, limits, &mut statements, &mut entries)
                .map_err(|error| error.at_line(number))?;
        }
        Ok(Imported::Statements(statements))
    }
}

/// Reads one record into the statements read so far.
fn read_line(
    line: &str,
    limits: &Limits,
    statements: &mut Vec<Statement>,
    entries: &mut usize,
) -> Result<(), ImportError> {
    let malformed = || ImportError::Malformed { line: None };
    let fields: Vec<&str> = line.split(';').collect();
    if let ["account", iban, number, code] = fields[..] {
        let code = CurrencyCode::new(code).map_err(|_| malformed())?;
        statements.push(Statement {
            account: StatementAccount {
                iban: optional(iban, limits)?,
                number: optional(number, limits)?,
            },
            currency: currency(code)?,
            opening_balance: None,
            closing_balance: None,
            entries: Vec::new(),
        });
        return Ok(());
    }
    let statement = statements.last_mut().ok_or_else(malformed)?;
    let currency = statement.currency;
    let amount = |text: &str| parse_amount(text, DecimalMark::Point, currency);
    match fields[..] {
        ["opening", day, value] => {
            statement.opening_balance = Some(Balance {
                amount: amount(value)?,
                date: date(day)?,
            });
        }
        ["closing", day, value] => {
            statement.closing_balance = Some(Balance {
                amount: amount(value)?,
                date: date(day)?,
            });
        }
        [
            "entry",
            status,
            booking,
            value_date,
            value,
            bank_id,
            name,
            iban,
            purpose,
        ] => {
            *entries += 1;
            limits.check_entries(*entries)?;
            let status = match status {
                "booked" => EntryStatus::Booked,
                "pending" => EntryStatus::Pending,
                _ => return Err(malformed()),
            };
            let entry = Entry {
                status,
                bank_id: optional(bank_id, limits)?,
                booking_date: date(booking)?,
                value_date: match value_date {
                    "" => None,
                    day => Some(date(day)?),
                },
                amount: amount(value)?,
                other_party_name: optional(name, limits)?,
                other_party_iban: optional(iban, limits)?,
                remittance_information: optional(purpose, limits)?,
            };
            statement.entries.push(entry);
        }
        _ => return Err(malformed()),
    }
    Ok(())
}

fn date(text: &str) -> Result<Date, ImportError> {
    text.parse()
        .map_err(|_| ImportError::Malformed { line: None })
}

/// A field as delivered, or none if it is empty.
fn optional(text: &str, limits: &Limits) -> Result<Option<String>, ImportError> {
    limits.check_text(text, None)?;
    Ok((!text.is_empty()).then(|| text.to_owned()))
}
