// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Two sides of a move between the user's own accounts joined as one
//! Transfer, prepared by the AI (US3.3, #571, D-693).
//!
//! The model only names the two transactions, by the IDs
//! `search_transactions` gives (it marks such entries `possible_transfer`,
//! D-672). Everything else comes from the book: which side the money left
//! from and which one stays, the accounts, dates and amounts the review
//! shows, and the join itself, which is the store's own
//! [`Book::join_transfer`] (D-691), so a pair is checked exactly as a join
//! by hand is. A side the user sorted by hand may be named too: the user's
//! confirm in the review is the pick D-031 asks for, and the review says
//! who sorted each side.
//!
//! On confirm both transactions must be exactly as the user reviewed them;
//! otherwise nothing of the set is applied (`ai-changes.out-of-date`). The
//! join is part of the set's one `apply_ai_changes` entry, so one Undo
//! takes the whole set back, the categories that went included.

use std::collections::HashSet;

use kassensturz_core::{
    Transaction, TransactionId, TransferMatch, TransferSide, days_between, join_transfer,
};
use kassensturz_store::{Book, SideSorting, StoreError};
use serde_json::Value;

use super::{ChangeError, Object};
use crate::error::ToolError;

/// The list of `propose_changes` that holds the pairs.
pub(super) const LIST: &str = "join_transfers";

/// The most pairs one set may join, so the user can still review them.
pub(super) const MAX_JOINS: usize = 50;

/// The keys an item of [`LIST`] takes.
const KEYS: &[&str] = &["transaction_id", "other_transaction_id"];

/// Two transactions joined as one Transfer, as the user reviews them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JoinTransfer {
    /// The two sides, the one the money left from first, and how many
    /// business days lie between them. The outgoing transaction stays and
    /// takes the incoming account's part; the incoming one is removed
    /// (D-691).
    pub transfer: TransferMatch,
    /// Who sorted the outgoing side's other part, when it was prepared.
    pub outgoing_sorting: SideSorting,
    /// Who sorted the incoming side's other part, when it was prepared.
    pub incoming_sorting: SideSorting,
    /// The outgoing and the incoming transaction as the user reviewed them.
    reviewed: [Transaction; 2],
}

/// Checks one item of [`LIST`] against `book`: two sides, in different
/// accounts, that cancel in one currency, and nothing on a removed part
/// that would be lost (D-691). `seen` holds the transactions other changes
/// of the set already touch; a transaction in two changes is refused
/// (`also-changed`), and both of this pair are added.
pub(super) fn join_transfers(
    book: &Book,
    value: &Value,
    seen: &mut HashSet<TransactionId>,
) -> Result<JoinTransfer, ToolError> {
    let item = Object::read(value, KEYS, LIST)?;
    let id = |name: &'static str| {
        item.uuid(name)?
            .map(TransactionId::from_bytes)
            .ok_or(ToolError::BadArgument { argument: name })
    };
    let first = id("transaction_id")?;
    let second = id("other_transaction_id")?;
    let stored = |id: TransactionId| match book.transaction(id) {
        Err(StoreError::UnknownTransaction(_)) => Err(ToolError::UnknownTransaction),
        other => other.map_err(ToolError::from),
    };
    let first = stored(first)?;
    let second = stored(second)?;
    let accounts: HashSet<_> = book
        .accounts(true)?
        .into_iter()
        .map(|account| account.id)
        .collect();
    let is_account = |id| accounts.contains(&id);
    // The store's own check, without changing anything: what it refuses
    // here, it would refuse on confirm.
    join_transfer(&first, &second, is_account).map_err(ToolError::TransferRefused)?;
    let side = |transaction: &Transaction| {
        TransferSide::of(transaction, is_account).map_err(ToolError::TransferRefused)
    };
    let (first_side, second_side) = (side(&first)?, side(&second)?);
    let ((outgoing, outgoing_side), (incoming, incoming_side)) = if first_side.amount.is_negative()
    {
        ((first, first_side), (second, second_side))
    } else {
        ((second, second_side), (first, first_side))
    };
    for id in [outgoing.id(), incoming.id()] {
        if !seen.insert(id) {
            return Err(ToolError::NotChangeable {
                argument: "transaction_id",
                reason: "also-changed",
            });
        }
    }
    let (business_days, _) = days_between(outgoing_side.booking_date, incoming_side.booking_date);
    Ok(JoinTransfer {
        transfer: TransferMatch {
            outgoing: outgoing_side,
            incoming: incoming_side,
            business_days,
        },
        outgoing_sorting: book.side_sorting(&outgoing_side)?,
        incoming_sorting: book.side_sorting(&incoming_side)?,
        reviewed: [outgoing, incoming],
    })
}

/// Joins what the user reviewed, inside the set's user action: both
/// transactions exactly as reviewed, or nothing of the set at all.
pub(super) fn apply(book: &mut Book, join: &JoinTransfer) -> Result<(), ChangeError> {
    for reviewed in &join.reviewed {
        match book.transaction(reviewed.id()) {
            Ok(stored) if stored == *reviewed => {}
            Ok(_) | Err(StoreError::UnknownTransaction(_)) => return Err(ChangeError::OutOfDate),
            Err(error) => return Err(error.into()),
        }
    }
    book.join_transfer(
        join.transfer.outgoing.transaction,
        join.transfer.incoming.transaction,
    )?;
    Ok(())
}
