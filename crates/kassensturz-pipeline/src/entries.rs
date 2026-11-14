// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which entries go into the book: only booked ones, and none the account
//! already holds (D-030, Q-039, D-092).

use std::collections::{HashMap, HashSet};

use kassensturz_core::{
    Date, MAX_BUSINESS_DAYS, Money, SplitId, TransactionId, business_days_apart,
};
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
///
/// A part the account holds stands for at most one entry (B-277): an entry
/// with its bank ID, or one entry that looks like it. Two entries that look
/// like one transaction in the book can't both be it, so only the closer one
/// is left out for the user to decide, and the other comes in. Which part an
/// entry looks like depends on the statements and the book only, never on
/// what the user chose, so a preview and the import agree.
pub(crate) fn decide(
    statements: &[Statement],
    held: &[AccountPart],
    choices: &Choices,
) -> Vec<(EntryRef, Decision)> {
    let mut in_book: HashMap<&str, Vec<&AccountPart>> = HashMap::new();
    let mut by_amount: HashMap<Money, Vec<&AccountPart>> = HashMap::new();
    for part in held {
        if let Some(id) = part_bank_id(part) {
            in_book.entry(id).or_default().push(part);
        }
        by_amount.entry(part.amount).or_default().push(part);
        // An entry of a statement in a withdrawn currency compares with
        // what a part had in it (D-105).
        if let Some(original) = part.original {
            by_amount.entry(original).or_default().push(part);
        }
    }
    // The parts an entry already stands for.
    let mut claimed: HashSet<SplitId> = HashSet::new();
    let mut in_statements: HashMap<&str, Vec<(EntryRef, Money)>> = HashMap::new();
    // Each entry, and what is decided about it so far: `None` while it may
    // still look like a part of the book.
    let mut decided: Vec<(EntryRef, &Entry, Option<Decision>)> = Vec::new();
    for (statement_index, statement) in statements.iter().enumerate() {
        for (entry_index, entry) in statement.entries.iter().enumerate() {
            let at = EntryRef {
                statement: statement_index,
                entry: entry_index,
            };
            let decision = 'decided: {
                if entry.status == EntryStatus::Pending {
                    break 'decided Some(Decision::LeaveOut(EntryOutcome::Pending));
                }
                if let Some(id) = bank_id(entry.bank_id.as_deref()) {
                    if let Some(parts) = in_book.get(id) {
                        if let Some(part) = parts.iter().find(|part| same_amount(entry, part)) {
                            claimed.insert(part.split);
                            break 'decided Some(Decision::LeaveOut(EntryOutcome::Duplicate(
                                Duplicate::InBook(part.transaction),
                            )));
                        }
                        let part = parts[0];
                        claimed.insert(part.split);
                        // The bank delivers what it delivered before with
                        // another amount, e.g. corrected: never skipped
                        // without a word, the user decides.
                        break 'decided Some(similar(at, vec![part.transaction], choices));
                    }
                    // The same entry twice, e.g. in two overlapping
                    // statements. One with the ID but another amount is
                    // another entry (B-279): it never goes missing.
                    let earlier = in_statements.entry(id).or_default();
                    if let Some(&(first, _)) =
                        earlier.iter().find(|(_, amount)| *amount == entry.amount)
                    {
                        break 'decided Some(Decision::LeaveOut(EntryOutcome::Duplicate(
                            Duplicate::InStatements(first),
                        )));
                    }
                    earlier.push((at, entry.amount));
                }
                None
            };
            decided.push((at, entry, decision));
        }
    }
    // The closest pairs first: an entry on the very day of a part takes it
    // before one a business day away, and so on; on the same distance, the
    // earlier entry of the statements.
    for distance in 0..=MAX_BUSINESS_DAYS {
        for (at, entry, decision) in &mut decided {
            if decision.is_some() {
                continue;
            }
            let found = similar_to(entry, &by_amount)
                .into_iter()
                .find(|(part, apart)| *apart == distance && !claimed.contains(&part.split));
            if let Some((part, _)) = found {
                claimed.insert(part.split);
                *decision = Some(similar(*at, vec![part.transaction], choices));
            }
        }
    }
    decided
        .into_iter()
        .map(|(at, _, decision)| {
            (
                at,
                decision.unwrap_or(Decision::Import {
                    similar: Vec::new(),
                }),
            )
        })
        .collect()
}

/// An entry that looks like the transactions `like`: left out for the user
/// to decide, unless they chose to import it anyway.
fn similar(at: EntryRef, like: Vec<TransactionId>, choices: &Choices) -> Decision {
    if choices.import_similar.contains(&at) {
        Decision::Import { similar: like }
    } else {
        Decision::LeaveOut(EntryOutcome::Similar(like))
    }
}

/// Whether `entry` has the amount `part` has, or had in a withdrawn
/// currency (D-105).
fn same_amount(entry: &Entry, part: &AccountPart) -> bool {
    entry.amount == part.amount || part.original == Some(entry.amount)
}

/// The parts the account already has that `entry` looks like, each with
/// how many business days apart they are, oldest first: a part with the
/// same amount, or the same original amount in a withdrawn currency, at
/// most [`MAX_BUSINESS_DAYS`] apart, unless both have bank IDs and they
/// differ (D-092). Entries of the same import are never compared with each
/// other: two coffees of the same price on one day are two coffees.
fn similar_to<'a>(
    entry: &Entry,
    by_amount: &HashMap<Money, Vec<&'a AccountPart>>,
) -> Vec<(&'a AccountPart, u32)> {
    let entry_id = bank_id(entry.bank_id.as_deref());
    let mut similar: Vec<(&AccountPart, u32)> = Vec::new();
    for &part in by_amount.get(&entry.amount).into_iter().flatten() {
        let ids_differ = matches!((entry_id, part_bank_id(part)), (Some(a), Some(b)) if a != b);
        let apart = business_days_apart(entry.booking_date, bank_date(part), MAX_BUSINESS_DAYS);
        if let Some(apart) = apart
            && !ids_differ
            && !similar.iter().any(|(seen, _)| seen.split == part.split)
        {
            similar.push((part, apart));
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
