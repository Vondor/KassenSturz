// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sorting the transactions already in the book with the user's own Rules,
//! prepared by the AI (US2.9, #572, D-682).
//!
//! It is the run of "Apply all" and "Apply now" on the Rules page (D-630,
//! D-631): [`Book::preview_apply_rules`] makes what the user reviews, and
//! [`Book::apply_rules`] sorts on the user's confirm, as part of the one
//! `apply_ai_changes` entry that one Undo takes back. The model only names
//! which Rule, or all of them, and whether what Rules sorted before is
//! sorted again; which transaction goes where is the Rules' decision, never
//! the model's. What the user sorted or changed by hand is never touched
//! (D-031).
//!
//! The review shows the run as it is when the set is prepared. When the
//! user confirms, the run must still be exactly that one, on transactions
//! that are exactly as reviewed; otherwise nothing of the set is applied
//! and the user is told the book changed since (`ai-changes.out-of-date`).
//! A sort goes in a call of its own: other changes of the same set, a new
//! Rule or a category for one transaction, would make the run differ from
//! the one reviewed.

use kassensturz_core::ai_switch::{Field, Permissions};
use kassensturz_core::{RuleId, Transaction};
use kassensturz_store::{ApplyRules, Book, RuleChange, RuleRun, StoreError};
use serde_json::Value;

use super::{ChangeError, Object};
use crate::error::ToolError;

/// The list of `propose_changes` that holds the sort.
pub(super) const LIST: &str = "sort_with_rules";

/// The keys an item of [`LIST`] takes.
const KEYS: &[&str] = &["rule_id", "sort_again"];

/// The transactions already in the book sorted with the user's Rules, as
/// the user reviews it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SortWithRules {
    /// The one Rule whose transactions are sorted, with the whole list of
    /// Rules deciding in its order; `None` for every Rule (D-630).
    pub rule: Option<RuleId>,
    /// That Rule's name.
    pub rule_name: Option<String>,
    /// Whether what Rules sorted before and the user didn't change since is
    /// sorted again too (D-631). Without it only transactions without a
    /// category are sorted.
    pub sort_again: bool,
    /// The transactions the Rules change, oldest first, as the run reports
    /// them.
    pub changes: Vec<RuleChange>,
    /// How many a Rule fits that stay as they are because they look like
    /// one side of a Transfer, which the user decides first.
    pub transfers: usize,
    /// How many stay because the book has no exchange rate for their day.
    pub no_rate: usize,
    /// How many stay because an account they would go to, or come from, is
    /// closed.
    pub closed: usize,
    /// How many stay for another reason.
    pub other: usize,
    /// Whether the run books some transaction as a Transfer, into another
    /// of the user's accounts.
    pub books_transfer: bool,
    /// The changed transactions as the user reviewed them, in the order of
    /// `changes`.
    reviewed: Vec<Transaction>,
}

impl SortWithRules {
    /// How many transactions a Rule fits that stay as they are.
    pub fn staying(&self) -> usize {
        self.transfers + self.no_rate + self.closed + self.other
    }

    /// Whether the run gives some transaction the name a Rule sets.
    pub fn sets_payee(&self) -> bool {
        self.changes.iter().any(|change| change.payee.is_some())
    }

    /// Whether the user allows it under `allowed`: sorting transactions,
    /// with the category field not locked, and neither the payee nor the
    /// account locked when the Rules set one (D-252, D-257).
    pub(super) fn allowed(&self, allowed: Permissions) -> bool {
        allowed.sorts()
            && !(self.sets_payee() && allowed.locked(Field::Payee))
            && !(self.books_transfer && allowed.locked(Field::Account))
    }

    fn apply_rules(&self) -> ApplyRules {
        ApplyRules {
            rule: self.rule,
            resort: self.sort_again,
        }
    }
}

/// Checks one item of [`LIST`] against `book` and runs the Rules as a
/// preview: what the user reviews. Nothing in the book changes.
pub(super) fn sort_with_rules(
    book: &Book,
    value: &Value,
    allowed: Permissions,
) -> Result<SortWithRules, ToolError> {
    let item = Object::read(value, KEYS, LIST)?;
    let rule = item.uuid("rule_id")?.map(RuleId::from_bytes);
    let rule_name = match rule {
        Some(id) => Some(match book.rule(id) {
            Ok(rule) => rule.name,
            Err(StoreError::UnknownRule(_)) => return Err(ToolError::UnknownRule),
            Err(error) => return Err(error.into()),
        }),
        None => None,
    };
    let sort_again = item.boolean("sort_again")?.unwrap_or(false);
    let apply = ApplyRules {
        rule,
        resort: sort_again,
    };
    let run = book.preview_apply_rules(apply)?;
    if run.changed.is_empty() {
        return Err(ToolError::NotChangeable {
            argument: LIST,
            reason: "nothing-to-sort",
        });
    }
    let reviewed = run
        .changed
        .iter()
        .map(|change| book.transaction(change.transaction))
        .collect::<Result<_, _>>()?;
    let mut books_transfer = false;
    for change in &run.changed {
        if let Some(to) = change.booked_to {
            books_transfer |= book.account(to)?.kind.is_account();
        }
    }
    let sort = SortWithRules {
        rule,
        rule_name,
        sort_again,
        books_transfer,
        reviewed,
        ..counted(run)
    };
    if sort.sets_payee() && allowed.locked(Field::Payee) {
        return Err(ToolError::FieldLocked { field: "payee" });
    }
    if sort.books_transfer && allowed.locked(Field::Account) {
        return Err(ToolError::FieldLocked { field: "account" });
    }
    Ok(sort)
}

/// The run's changes and why the others stay, as the Rules page's dialog
/// counts them.
fn counted(run: RuleRun) -> SortWithRules {
    let mut sort = SortWithRules {
        rule: None,
        rule_name: None,
        sort_again: false,
        changes: run.changed,
        transfers: 0,
        no_rate: 0,
        closed: 0,
        other: 0,
        books_transfer: false,
        reviewed: Vec::new(),
    };
    for skip in run.skipped {
        match skip.reason {
            StoreError::TransferSuggested(_) | StoreError::FitsMonthlyMove(_) => {
                sort.transfers += 1;
            }
            StoreError::NoRate { .. } => sort.no_rate += 1,
            StoreError::AccountClosed(_) => sort.closed += 1,
            _ => sort.other += 1,
        }
    }
    sort
}

/// Sorts what the user reviewed, inside the set's user action: the same
/// run, on the same transactions, or nothing of the set at all.
pub(super) fn apply(book: &mut Book, sort: &SortWithRules) -> Result<(), ChangeError> {
    for reviewed in &sort.reviewed {
        match book.transaction(reviewed.id()) {
            Ok(stored) if stored == *reviewed => {}
            Ok(_) | Err(StoreError::UnknownTransaction(_)) => return Err(ChangeError::OutOfDate),
            Err(error) => return Err(error.into()),
        }
    }
    let run = match book.apply_rules(sort.apply_rules()) {
        Err(StoreError::UnknownRule(_)) => return Err(ChangeError::OutOfDate),
        other => other?,
    };
    // The set's action takes back what the run wrote when this fails.
    if run.changed != sort.changes {
        return Err(ChangeError::OutOfDate);
    }
    Ok(())
}
