// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which entries go into the book: only booked ones, and none the account
//! already holds (D-030, Q-039, D-092).

use std::collections::HashMap;
use std::collections::hash_map::Entry as Slot;

use kassensturz_core::{Date, MAX_BUSINESS_DAYS, Money, TransactionId, business_days_apart};
use kassensturz_import::{Entry, EntryStatus, Statement};
use kassensturz_store::AccountPart;

use crate::Choices;
use crate::report::{Duplicate, EntryOutcome, EntryRef};

/// What the pipeline does with one entry, decided before the book changes.
pub(crate) enum Decision {
    /// It becomes a new transaction. `similar` names the transactions it
    /// looks like, if the user chose to import it anyway.
    Import { similar: Vec<TransactionId> },
    /// It stays out of the book.
    LeaveOut(EntryOutcome),
}

/// What happens to each entry of `statements`, in their order, given the
/// parts the account `held` before the import.
pub(crate) fn decide(
    statements: &[Statement],
    held: &[AccountPart],
    choices: &Choices,
) -> Vec<(EntryRef, Decision)> {
    let mut in_book: HashMap<&str, TransactionId> = HashMap::new();
    let mut by_amount: HashMap<Money, Vec<&AccountPart>> = HashMap::new();
    for part in held {
        if let Some(id) = part_bank_id(part) {
            in_book.entry(id).or_insert(part.transaction);
        }
        by_amount.entry(part.amount).or_default().push(part);
        // An entry of a statement in a withdrawn currency compares with
        // what a part had in it (D-105).
        if let Some(original) = part.original {
            by_amount.entry(original).or_default().push(part);
        }
    }
    let mut in_statements: HashMap<&str, EntryRef> = HashMap::new();
    let mut decisions = Vec::new();
    for (statement_index, statement) in statements.iter().enumerate() {
        for (entry_index, entry) in statement.entries.iter().enumerate() {
            let at = EntryRef {
                statement: statement_index,
                entry: entry_index,
            };
            let decision = 'decided: {
                if entry.status == EntryStatus::Pending {
                    break 'decided Decision::LeaveOut(EntryOutcome::Pending);
                }
                if let Some(id) = bank_id(entry.bank_id.as_deref()) {
                    if let Some(&transaction) = in_book.get(id) {
                        break 'decided Decision::LeaveOut(EntryOutcome::Duplicate(
                            Duplicate::InBook(transaction),
                        ));
                    }
                    match in_statements.entry(id) {
                        Slot::Occupied(earlier) => {
                            break 'decided Decision::LeaveOut(EntryOutcome::Duplicate(
                                Duplicate::InStatements(*earlier.get()),
                            ));
                        }
                        Slot::Vacant(slot) => {
                            slot.insert(at);
                        }
                    }
                }
                let similar = similar_to(entry, &by_amount);
                if similar.is_empty() || choices.import_similar.contains(&at) {
                    Decision::Import { similar }
                } else {
                    Decision::LeaveOut(EntryOutcome::Similar(similar))
                }
            };
            decisions.push((at, decision));
        }
    }
    decisions
}

/// The transactions the account already has that `entry` looks like,
/// oldest first: a part with the same amount, or the same original amount
/// in a withdrawn currency, at most
/// [`MAX_BUSINESS_DAYS`] apart, unless both have bank IDs and they differ
/// (D-092). Entries of the same import are never compared with each other:
/// two coffees of the same price on one day are two coffees.
fn similar_to(entry: &Entry, by_amount: &HashMap<Money, Vec<&AccountPart>>) -> Vec<TransactionId> {
    let entry_id = bank_id(entry.bank_id.as_deref());
    let mut similar: Vec<TransactionId> = Vec::new();
    for part in by_amount.get(&entry.amount).into_iter().flatten() {
        let ids_differ = matches!((entry_id, part_bank_id(part)), (Some(a), Some(b)) if a != b);
        let close =
            business_days_apart(entry.booking_date, bank_date(part), MAX_BUSINESS_DAYS).is_some();
        if close && !ids_differ && !similar.contains(&part.transaction) {
            similar.push(part.transaction);
        }
    }
    similar
}

/// The day the bank booked a part, as it delivered it, or for a part typed
/// in by hand, its transaction's booking date.
pub(crate) fn bank_date(part: &AccountPart) -> Date {
    part.bank_date()
}

/// The bank ID a part was delivered with, if any.
fn part_bank_id(part: &AccountPart) -> Option<&str> {
    bank_id(part.imported.as_ref()?.bank_id.as_deref())
}

/// A bank ID, unless it is empty: an empty one identifies nothing, so it
/// counts as none.
fn bank_id(id: Option<&str>) -> Option<&str> {
    id.filter(|id| !id.trim().is_empty())
}
