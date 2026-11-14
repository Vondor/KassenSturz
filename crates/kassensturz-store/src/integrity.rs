// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a book's content must satisfy before KassenSturz shows a figure
//! from it (B-291). SQLite's own `integrity_check` says the file is
//! well-formed, not that its numbers are right: one changed byte inside an
//! amount passes it, and a book whose transactions don't add up to zero
//! would show a wrong balance without a word. Both checks run when a book
//! is opened, before anything is read, changed or backed up over; a book
//! that fails them is not opened, so nothing is ever saved over it.

use rusqlite::Connection;

use crate::error::{Corruption, StoreError};

/// Whether the file is well-formed as far as SQLite can tell without
/// reading every index entry: `PRAGMA quick_check`. It reads each page once
/// (a large book is a matter of milliseconds) and finds a page SQLite can't
/// parse, which would otherwise come up only at the next save as a generic
/// database error.
pub(crate) fn check_file(conn: &Connection) -> Result<(), StoreError> {
    let first = conn.query_row("PRAGMA quick_check(1)", [], |row| row.get::<_, String>(0));
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
