// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a bank or file delivered stays as it was delivered (D-067, D-375).
//!
//! The original is the row of `split_imports` with the account and the
//! amount of its part. The user's own changes (payee, date, note, category)
//! live in other columns, on top of it. Layout 23's triggers refuse to
//! change an original at all and refuse to delete one unless the guard
//! table `originals_unlocked` holds its row. This module is the only place
//! that writes that row: [`unlocked`] around removing whole transactions the
//! user removes and around undo and redo, and [`keeping_originals`] around
//! the edits that write a transaction's parts again and must give every
//! original back unchanged. Both delete the row again before they return,
//! inside the same database transaction, so it is never in a saved book.

use kassensturz_core::{SplitId, TransactionId};
use rusqlite::types::Value;
use rusqlite::{Connection, params};

use crate::cached::Cached;
use crate::error::StoreError;

/// Runs `change` with layout 23's guard open, so the originals it deletes
/// may go. Only for the user removing whole transactions and for undo and
/// redo, which write back what the history kept.
pub(crate) fn unlocked<T>(
    conn: &Connection,
    change: impl FnOnce() -> Result<T, StoreError>,
) -> Result<T, StoreError> {
    conn.execute_cached("INSERT INTO main.originals_unlocked (id) VALUES (1)", [])?;
    let result = change();
    let locked = conn.execute_cached("DELETE FROM main.originals_unlocked", []);
    let value = result?;
    locked?;
    Ok(value)
}

/// Runs `change`, which deletes and writes again the parts of
/// `transactions`, with layout 23's guard open, then checks that every
/// original those transactions held is back exactly as it was: the same
/// delivered details, on a part with the same ID, account and amount in the
/// account's currency, in whichever transaction (a joined Transfer moves
/// one). Otherwise it fails with [`StoreError::ImportedSplitChanged`], and
/// the caller's savepoint takes everything back.
pub(crate) fn keeping_originals<T>(
    conn: &Connection,
    transactions: &[TransactionId],
    change: impl FnOnce() -> Result<T, StoreError>,
) -> Result<T, StoreError> {
    let mut before = Vec::new();
    for id in transactions {
        before.extend(originals_of(conn, *id)?);
    }
    let value = unlocked(conn, change)?;
    for (split, delivered) in before {
        if original(conn, split)?.as_ref() != Some(&delivered) {
            return Err(StoreError::ImportedSplitChanged(split));
        }
    }
    Ok(value)
}

/// Everything that makes up an original, after the part's ID: what was
/// delivered, the part's account, and its amount and currency in that
/// account.
const ORIGINAL: &str = "
    SELECT i.split_id, i.bank_id, i.booking_date, i.value_date, i.other_party_name,
           i.other_party_iban, i.remittance_information, s.account_id,
           coalesce(s.foreign_amount, s.amount), coalesce(s.foreign_currency, t.currency)
    FROM split_imports AS i
    JOIN splits AS s ON s.id = i.split_id
    JOIN transactions AS t ON t.id = s.transaction_id";

/// The originals of the transaction with `id`, by part.
fn originals_of(
    conn: &Connection,
    id: TransactionId,
) -> Result<Vec<(SplitId, Vec<Value>)>, StoreError> {
    let mut statement = conn.prepare_cached(&format!("{ORIGINAL} WHERE s.transaction_id = ?1"))?;
    let rows = statement
        .query_map(params![id.to_bytes()], read)?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

/// The original of the part with `split`, if the book has it.
fn original(conn: &Connection, split: SplitId) -> Result<Option<Vec<Value>>, StoreError> {
    let mut statement = conn.prepare_cached(&format!("{ORIGINAL} WHERE i.split_id = ?1"))?;
    let mut rows = statement.query_map(params![split.to_bytes()], read)?;
    Ok(rows.next().transpose()?.map(|(_, values)| values))
}

fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<(SplitId, Vec<Value>)> {
    let id = SplitId::from_bytes(row.get(0)?);
    let values = (1..10)
        .map(|index| row.get::<_, Value>(index))
        .collect::<rusqlite::Result<_>>()?;
    Ok((id, values))
}

#[cfg(test)]
mod tests;
