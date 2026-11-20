// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a book's content must satisfy before KassenSturz shows a figure
//! from it (B-291). SQLite's own `integrity_check` says the file is
//! well-formed, not that its numbers are right: one changed byte inside an
//! amount passes it, and a book whose transactions don't add up to zero
//! would show a wrong balance without a word. Both checks run when a book
//! is opened, before anything is read, changed or backed up over; a book
//! that fails them is not opened, so nothing is ever saved over it.

use kassensturz_core::{Currency, ExchangeRate, Money, RoundingMode};
use rusqlite::Connection;

use crate::error::{Corruption, StoreError};

/// Whether the file is well-formed and its indexes agree with its tables:
/// `PRAGMA integrity_check`. `quick_check` skips the indexes, and a changed
/// byte in one of them hides parts from every query that uses it, so a
/// balance comes out wrong while the file looks fine (B-291, found by
/// changing single bytes of a book). The check reads each page once and each
/// index entry once; a book of 80 000 parts (16 MB) takes some 40 ms, and a
/// page SQLite can't parse, which would otherwise come up only at the next
/// save as a generic database error, is found here.
///
/// A file cut short by a few bytes can pass it when its last page holds
/// little, e.g. an empty table's root, since SQLite reads the missing end
/// as zeros. Books are kept without a write-ahead log, so the file always
/// holds every page its header counts; one shorter than that is damaged.
pub(crate) fn check_file(conn: &Connection) -> Result<(), StoreError> {
    if let Some(path) = conn.path().filter(|path| !path.is_empty())
        && let Ok(metadata) = std::fs::metadata(path)
    {
        let pages = conn.query_row("PRAGMA page_count", [], |row| row.get::<_, i64>(0));
        let size = conn.query_row("PRAGMA page_size", [], |row| row.get::<_, i64>(0));
        let needed = |pages: i64, size: i64| {
            u64::try_from(pages)
                .unwrap_or(0)
                .saturating_mul(u64::try_from(size).unwrap_or(0))
        };
        match (pages, size) {
            (Ok(pages), Ok(size)) if metadata.len() < needed(pages, size) => {
                return Err(StoreError::Corrupt(Corruption::DamagedFile));
            }
            (Err(error), _) | (_, Err(error)) if is_damage(&error) => {
                return Err(StoreError::Corrupt(Corruption::DamagedFile));
            }
            _ => {}
        }
    }
    let first = conn.query_row("PRAGMA integrity_check(1)", [], |row| {
        row.get::<_, String>(0)
    });
    match first {
        Ok(text) if text == "ok" => Ok(()),
        // "malformed", "not a database" and the findings themselves.
        Ok(_) => Err(StoreError::Corrupt(Corruption::DamagedFile)),
        Err(error) if is_damage(&error) => Err(StoreError::Corrupt(Corruption::DamagedFile)),
        Err(error) => Err(error.into()),
    }
}

/// Whether every transaction's splits add up to zero, as each one did when
/// it was saved (D-043: a split's `amount` is in the transaction's
/// currency). One pass over the splits, grouped by transaction; no row is
/// built for a balanced transaction.
pub(crate) fn check_balances(conn: &Connection) -> Result<(), StoreError> {
    let unbalanced = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM splits GROUP BY transaction_id HAVING sum(amount) <> 0)",
        [],
        |row| row.get::<_, bool>(0),
    );
    match unbalanced {
        Ok(false) => Ok(()),
        Ok(true) => Err(StoreError::Corrupt(Corruption::UnbalancedTransaction)),
        // A sum that overflows is no balanced transaction either.
        Err(error) if is_overflow(&error) || is_damage(&error) => {
            Err(StoreError::Corrupt(Corruption::UnbalancedTransaction))
        }
        Err(error) => Err(error.into()),
    }
}

/// Whether the amounts of each currency stay within what every change to
/// the book keeps them to (B-269): a book that holds more was not written by
/// KassenSturz, and its balances, register and Overview couldn't be read, or
/// a sum in a report could be wrong. It reads as a book whose entries don't
/// add up, since a changed number is what it takes (B-416).
pub(crate) fn check_totals(conn: &Connection) -> Result<(), StoreError> {
    match crate::totals::check_totals(conn) {
        Err(StoreError::AmountOverflow) => {
            Err(StoreError::Corrupt(Corruption::UnbalancedTransaction))
        }
        other => other,
    }
}

/// `error` as the damaged book it is, if SQLite reports the file as
/// malformed while the book is being set up (a file cut short fails at the
/// first read of its header); any other error as it was.
pub(crate) fn damaged_if_malformed(error: StoreError) -> StoreError {
    match error {
        StoreError::Database(ref database) if is_damage(&database.0) => {
            StoreError::Corrupt(Corruption::DamagedFile)
        }
        other => other,
    }
}

/// Whether everything in the book that refers to something else, such as a
/// part to its account or its transaction, refers to something that is
/// there. SQLite checks this only when a row is written, so a changed byte
/// in a part's account leaves a part that counts in no balance, and nothing
/// says so (B-291, found by changing single bytes of a book).
pub(crate) fn check_references(conn: &Connection) -> Result<(), StoreError> {
    let broken = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM pragma_foreign_key_check)",
        [],
        |row| row.get::<_, bool>(0),
    );
    match broken {
        Ok(false) => Ok(()),
        Ok(true) => Err(StoreError::Corrupt(Corruption::BrokenReferences)),
        Err(error) if is_damage(&error) => Err(StoreError::Corrupt(Corruption::DamagedFile)),
        Err(error) => Err(error.into()),
    }
}

/// Whether every part that was converted with a stored rate still agrees
/// with it: its amount and its foreign amount are the one calculated from
/// the other with that rate (D-043, AGENTS.md §5). One changed byte in
/// either of them gives a book whose balances are wrong although it adds up
/// (B-291). A part with no stored rate has none to check against, and one
/// with an original amount in a withdrawn currency keeps its rate for that,
/// not for the foreign amount.
///
/// The calculation is exact, within what the rate's last digit leaves open
/// and whichever rounding mode was used, and either of the two amounts may
/// have been the one the rate was applied to. Only a pair that fits none of
/// these is refused: a book must never be locked out over a rounding.
pub(crate) fn check_conversions(conn: &Connection) -> Result<(), StoreError> {
    let mut statement = conn.prepare(
        "SELECT s.amount, t.currency, ct.minor_digits, s.foreign_amount, s.foreign_currency,
                cf.minor_digits, r.from_currency, r.rate_digits, r.rate_decimal_places
         FROM splits AS s
         JOIN split_rates AS r ON r.split_id = s.id
         JOIN transactions AS t ON t.id = s.transaction_id
         JOIN currencies AS ct ON ct.code = t.currency
         JOIN currencies AS cf ON cf.code = s.foreign_currency
         WHERE s.foreign_amount IS NOT NULL AND s.original_amount IS NULL",
    )?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let part = Conversion {
            amount: row.get(0)?,
            currency: (row.get(1)?, row.get(2)?),
            foreign: row.get(3)?,
            foreign_currency: (row.get(4)?, row.get(5)?),
            from: row.get(6)?,
            digits: row.get(7)?,
            places: row.get(8)?,
        };
        if part.disagrees().unwrap_or(false) {
            return Err(StoreError::Corrupt(Corruption::UnbalancedTransaction));
        }
    }
    Ok(())
}

/// A converted part as the book stores it.
struct Conversion {
    amount: i64,
    currency: (String, i64),
    foreign: i64,
    foreign_currency: (String, i64),
    /// The currency the stored rate converts from.
    from: String,
    digits: i64,
    places: i64,
}

impl Conversion {
    /// `Some(true)` if the amount and the foreign amount can't be one the
    /// other's conversion; `None` if the row can't be judged (it is left to
    /// the checks that read it).
    fn disagrees(&self) -> Option<bool> {
        let own = Currency::new(
            crate::currency_from_text(&self.currency.0).ok()?,
            u8::try_from(self.currency.1).ok()?,
        )?;
        let foreign = Currency::new(
            crate::currency_from_text(&self.foreign_currency.0).ok()?,
            u8::try_from(self.foreign_currency.1).ok()?,
        )?;
        let rate = ExchangeRate::from_digits(self.digits, u8::try_from(self.places).ok()?).ok()?;
        let from = crate::currency_from_text(&self.from).ok()?;
        let amount = Money::from_minor_units(self.amount, own.code());
        let foreign_amount = Money::from_minor_units(self.foreign, foreign.code());
        // The rate runs from one of the two currencies into the other.
        let forward = if from == own.code() {
            true
        } else if from == foreign.code() {
            false
        } else {
            return None;
        };
        // A rate keeps only so many digits, so a calculation with it is
        // right within what the rate's last digit leaves open (a rate typed
        // as 0.92116 may stand for anything from 0.921155 to 0.921165).
        // Only an amount outside that range is refused.
        let (low, high) = match (
            self.digits.checked_mul(10).and_then(|d| d.checked_sub(5)),
            self.digits.checked_mul(10).and_then(|d| d.checked_add(5)),
            u8::try_from(self.places + 1).ok(),
        ) {
            (Some(low), Some(high), Some(places)) => (
                ExchangeRate::from_digits(low, places).unwrap_or(rate),
                ExchangeRate::from_digits(high, places).unwrap_or(rate),
            ),
            _ => (rate, rate),
        };
        let fits = |ones: Money,
                    ones_currency: Currency,
                    other: Money,
                    other_currency: Currency,
                    multiply: bool| {
            let convert = |rate: ExchangeRate, rounding: RoundingMode| {
                if multiply {
                    ones.checked_mul_rate(ones_currency, rate, other_currency, rounding)
                } else {
                    ones.checked_div_rate(ones_currency, rate, other_currency, rounding)
                }
            };
            // Which end of the rate gives the smaller result depends on
            // the direction and on the amount's sign (a larger rate makes
            // a negative amount more negative), so both ends are tried
            // with both roundings (B-349).
            // Too big to judge here: the other checks decide.
            let mut lowest = i64::MAX;
            let mut highest = i64::MIN;
            for rate in [low, high] {
                lowest = lowest.min(convert(rate, RoundingMode::Floor).ok()?.minor_units());
                highest = highest.max(convert(rate, RoundingMode::Ceiling).ok()?.minor_units());
            }
            Some((lowest..=highest).contains(&other.minor_units()))
        };
        let one_way = fits(amount, own, foreign_amount, foreign, forward)?;
        let other_way = fits(foreign_amount, foreign, amount, own, !forward)?;
        Some(!(one_way || other_way))
    }
}

fn is_damage(error: &rusqlite::Error) -> bool {
    matches!(
        error.sqlite_error_code(),
        Some(rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase)
    )
}

fn is_overflow(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(_, Some(message)) if message.contains("integer overflow")
    )
}

#[cfg(test)]
mod tests;
