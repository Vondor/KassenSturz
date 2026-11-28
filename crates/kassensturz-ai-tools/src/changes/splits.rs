// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A transaction broken into parts by the AI, or its parts joined back
//! into one (US5.1, D-737, `docs/AI_SPLITS.md` §1).
//!
//! Behind its own cell, *Parts of transactions › Change*, off by default.
//! The model names a transaction by the ID `search_transactions` gives and
//! writes every part: a category (or none, which books it to *Not sorted
//! yet*), an exact amount as seen from the account, and an optional name,
//! which is the part's note (D-132). One part joins a split back into one.
//!
//! The money rule: the part in the user's account never changes, not its
//! amount, day, payee or what a bank delivered for it (D-067), and the new
//! parts add up to its amount exactly, in integer minor units with checked
//! arithmetic. Nothing is ever made up to fill a gap: parts that don't add
//! up are refused with the total, their sum and the difference, so the
//! model corrects them. [`Transaction::new`] checks the balance once more.
//!
//! The parts it replaces must hold nothing the user would lose: no cheque
//! number, imported bank details, foreign amount, rate or amount from a
//! replaced currency, and on an already-split transaction no per-part tags
//! (B-472, D-1011). Category-split `status` (`Cleared`/`Reconciled` imported
//! from KMyMoney) is not shown or edited on categories in KassenSturz, while
//! the bank account split keeps its own status intact. On an unsplit
//! transaction whose sole category split carries tags, those tags are
//! promoted onto the whole transaction (deduplicated with any existing
//! transaction tags, within `MAX_TAGS_ON_TRANSACTION`), so every new part
//! keeps them under D-713 and Undo restores the exact original transaction.
//! Likewise, if an unsplit transaction has no whole-transaction note
//! (`stored.note()` is `None`) while its sole replaced category split carries
//! a non-empty `split.note` (such as an imported KMyMoney category memo), that
//! memo is preserved onto the whole transaction's `note` (D-1015), without
//! ever overwriting an existing `stored.note()`, and Undo restores the exact
//! original transaction.
//! Any other transaction with part details is the user's to split on the
//! Parts page. A tag on the whole transaction stays and counts for every
//! new part (D-713).
//!
//! On confirm the transaction must be exactly as the user reviewed it,
//! otherwise nothing of the set applies (`ai-changes.out-of-date`); the
//! split is part of the set's one `apply_ai_changes` entry, so one Undo
//! takes the whole set back.

use kassensturz_core::ai_switch::{Field, Permissions};
use kassensturz_core::{
    AccountId, Date, MAX_TAGS_ON_TRANSACTION, Money, Split, TagId, Transaction, TransactionId,
};
use kassensturz_store::{Book, StoreError};
use serde_json::Value;

use super::transactions::{free_text, parse_amount};
use super::{ChangeError, Known, Object};
use crate::error::ToolError;
use crate::writer::decimal_text;

/// The list of `propose_changes` that holds the splits.
pub(super) const LIST: &str = "split_transactions";

/// The most transactions one set may split, so the user can still review
/// them.
pub(super) const MAX_SPLIT: usize = 100;

/// The most parts one transaction may get from the AI, besides the
/// account's own.
const MAX_PARTS: usize = 50;

/// The keys an item of [`LIST`] takes.
const KEYS: &[&str] = &["transaction_id", "parts"];

/// The keys a part takes.
const PART_KEYS: &[&str] = &["category_id", "new_category", "amount", "name"];

/// One new part of a split, as the user reviews it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitPart {
    /// Its category, by its path of names; `None` for *Not sorted yet*.
    pub category: Option<Vec<String>>,
    /// Its amount as seen from the account: money out is negative.
    pub amount: Money,
    /// Its name, if any.
    pub name: Option<String>,
}

/// A transaction broken into parts, or joined back into one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitTransaction {
    /// The transaction.
    pub transaction: TransactionId,
    /// The day its account shows it with.
    pub date: Date,
    /// Who was paid or paid, if anyone.
    pub payee: Option<String>,
    /// The user's account the money moved in or out of, by its name.
    pub account: String,
    /// What it did to that account, which stays: the parts add up to it.
    pub total: Money,
    /// The parts it has now besides the account's, as the review shows
    /// them.
    pub before: Vec<SplitPart>,
    /// The parts it gets, in the model's order.
    pub parts: Vec<SplitPart>,
    /// The transaction as the user reviewed it.
    reviewed: Transaction,
    /// The transaction with its new parts.
    edited: Transaction,
}

impl SplitTransaction {
    /// Whether one of the parts it makes books into `id`.
    pub(super) fn books_into(&self, id: AccountId) -> bool {
        self.edited
            .splits()
            .iter()
            .any(|part| part.account == Some(id))
    }
}

impl Known {
    /// Checks one item of [`LIST`] against `book` under the locks of
    /// `allowed`, and works out the transaction it makes.
    pub(super) fn split_transaction(
        &self,
        book: &Book,
        value: &Value,
        allowed: Permissions,
    ) -> Result<SplitTransaction, ToolError> {
        let item = Object::read(value, KEYS, LIST)?;
        let id = item
            .uuid("transaction_id")?
            .map(TransactionId::from_bytes)
            .ok_or(ToolError::BadArgument {
                argument: "transaction_id",
            })?;
        // Splitting replaces category parts while cloning the account part
        // untouched and verifying the parts add up to it in exact minor units,
        // so Amount may stay locked to protect whole-transaction totals
        // (D-737, D-1010); only a locked Category refuses splitting.
        if allowed.locked(Field::Category) {
            return Err(ToolError::FieldLocked {
                field: Field::Category.name(),
            });
        }
        let stored = match book.transaction(id) {
            Err(StoreError::UnknownTransaction(_)) => return Err(ToolError::UnknownTransaction),
            other => other?,
        };
        let (own, carried_tags, carried_note) = self.own_part(&stored)?;
        let own_split = &stored.splits()[own];
        let currency = stored.currency();
        let digits = book.currency_minor_digits(currency)?;

        let given = item.array("parts")?;
        if given.is_empty() {
            return Err(ToolError::BadArgument { argument: "parts" });
        }
        if given.len() > MAX_PARTS {
            return Err(ToolError::NotChangeable {
                argument: "parts",
                reason: "too-many-parts",
            });
        }
        let mut parts = Vec::with_capacity(given.len());
        let mut splits = Vec::with_capacity(given.len() + 1);
        splits.push(own_split.clone());
        for value in given {
            let part = Object::read(value, PART_KEYS, "parts")?;
            let category = self.part_category(&part)?;
            if let Some(category) = category
                && self.accounts[&category].currency != currency
            {
                return Err(ToolError::OtherCurrency);
            }
            let amount = parse_amount(part.required_string("amount")?, currency, digits)?;
            let name = part
                .string("name")?
                .map(|text| free_text(text, "name"))
                .transpose()?
                .flatten();
            if name.is_some() && allowed.locked(Field::Note) {
                return Err(ToolError::FieldLocked {
                    field: Field::Note.name(),
                });
            }
            // The part's amount as the transaction holds it: the opposite
            // of what it did to the account.
            let booked = amount
                .checked_neg()
                .map_err(|_| ToolError::BadArgument { argument: "amount" })?;
            let mut split = match category {
                Some(category) => Split::new(category, booked),
                None => Split::uncategorized(booked),
            };
            split.note.clone_from(&name);
            splits.push(split);
            parts.push(SplitPart {
                category: category.map(|id| self.path(id)),
                amount,
                name,
            });
        }

        // The money rule, in integer minor units: the parts add up to what
        // the transaction did to the account, exactly.
        let total = own_split.amount;
        let sum = Money::checked_sum(currency, parts.iter().map(|part| part.amount))
            .map_err(|_| ToolError::BadArgument { argument: "amount" })?;
        if sum != total {
            let difference = total
                .checked_sub(sum)
                .map_err(|_| ToolError::BadArgument { argument: "amount" })?;
            return Err(ToolError::PartsDontAddUp {
                total: decimal_text(total.minor_units(), digits),
                sum: decimal_text(sum.minor_units(), digits),
                difference: decimal_text(difference.minor_units(), digits),
            });
        }
        let before: Vec<SplitPart> = stored
            .splits()
            .iter()
            .enumerate()
            .filter(|&(at, _)| at != own)
            .map(|(_, split)| {
                Ok(SplitPart {
                    category: split.account.map(|id| self.path(id)),
                    amount: split
                        .amount
                        .checked_neg()
                        .map_err(|_| ToolError::BadArgument { argument: "parts" })?,
                    name: split.note.clone(),
                })
            })
            .collect::<Result<_, ToolError>>()?;
        if before == parts {
            return Err(ToolError::BadArgument { argument: "parts" });
        }
        let mut edited = Transaction::new(
            id,
            stored.booking_date(),
            stored.payee().map(str::to_owned),
            splits,
        )
        .map_err(|_| ToolError::BadArgument { argument: "parts" })?
        .with_details_of(&stored);
        if !carried_tags.is_empty() {
            let mut tags = stored.tags().to_vec();
            tags.extend(carried_tags);
            edited = edited.with_tags(tags);
        }
        if let Some(note) = carried_note {
            edited = edited.with_note(Some(note));
        }
        let account = own_split
            .account
            .and_then(|id| self.accounts.get(&id))
            .map(|account| account.name.clone())
            .unwrap_or_default();
        Ok(SplitTransaction {
            transaction: id,
            date: stored.date_of(own_split),
            payee: stored.payee_of(own_split).map(str::to_owned),
            account,
            total,
            before,
            parts,
            edited,
            reviewed: stored,
        })
    }

    /// The place of the one part in an open account of the user, any tags
    /// carried by a sole non-account part to promote onto the whole
    /// transaction so splitting an unsplit entry loses no tag (B-472, D-1011,
    /// D-713), and any non-empty note carried by a sole non-account part when
    /// the whole transaction has no note yet (D-1015). Otherwise why not, as
    /// `ai-tools.not-editable`.
    fn own_part(
        &self,
        transaction: &Transaction,
    ) -> Result<(usize, Vec<TagId>, Option<String>), ToolError> {
        let not_editable = |reason| ToolError::NotEditable { reason };
        let account_of = |split: &Split| split.account.and_then(|id| self.accounts.get(&id));
        let mut own = None;
        let mut other_count = 0usize;
        let mut carried_tags: Vec<TagId> = Vec::new();
        let mut carried_note: Option<String> = None;
        for (at, split) in transaction.splits().iter().enumerate() {
            match account_of(split) {
                Some(account) if account.kind.is_account() => {
                    if own.replace(at).is_some() {
                        return Err(not_editable("transfer"));
                    }
                    if account.closed {
                        return Err(ToolError::UnknownAccount);
                    }
                    if account.currency != transaction.currency()
                        || split.foreign.is_some()
                        || split.original.is_some()
                    {
                        return Err(not_editable("other-currency"));
                    }
                }
                Some(account) if !account.kind.is_category() => {
                    return Err(not_editable("transfer"));
                }
                _ => {
                    other_count += 1;
                    if split.foreign.is_some() || split.original.is_some() {
                        return Err(not_editable("other-currency"));
                    }
                    // Category-split `status` (`Cleared`/`Reconciled`, e.g.
                    // from KMyMoney imports) is not shown or edited on
                    // categories in KassenSturz; the bank account split keeps
                    // its own status untouched. Only `number`, `imported`, and
                    // per-part `tags` on an already-split transaction block
                    // splitting (D-1011).
                    if split.number.is_some() || split.imported.is_some() {
                        return Err(not_editable("part-details"));
                    }
                    if !split.tags.is_empty() {
                        carried_tags.extend_from_slice(&split.tags);
                    }
                    if other_count == 1
                        && transaction.note().is_none()
                        && let Some(note) = split.note.as_deref()
                        && !note.trim().is_empty()
                    {
                        carried_note = Some(note.to_owned());
                    } else if other_count > 1 {
                        carried_note = None;
                    }
                }
            }
        }
        let own = own.ok_or(not_editable("split"))?;
        if !carried_tags.is_empty() {
            // Already-split transactions with per-part tags stay on the Parts
            // page so per-part tag distinctions are never flattened.
            if other_count != 1 {
                return Err(not_editable("part-details"));
            }
            let mut merged = transaction.tags().to_vec();
            for &tag in &carried_tags {
                if !merged.contains(&tag) {
                    merged.push(tag);
                }
            }
            if merged.len() > MAX_TAGS_ON_TRANSACTION {
                return Err(not_editable("part-details"));
            }
        }
        Ok((own, carried_tags, carried_note))
    }

    /// The category a part books to, by `category_id` (an open category of the
    /// book) or `new_category` (a key in `new_categories` of the same call,
    /// B-472, D-1011), or none for *Not sorted yet*. Giving both is refused
    /// like `categorize`.
    fn part_category(&self, part: &Object<'_>) -> Result<Option<AccountId>, ToolError> {
        match (part.id("category_id")?, part.string("new_category")?) {
            (None, None) => Ok(None),
            (Some(id), None) => self
                .accounts
                .get(&id)
                .filter(|category| category.kind.is_category() && !category.closed)
                .map(|_| Some(id))
                .ok_or(ToolError::UnknownCategory),
            (None, Some(key)) => {
                self.keys
                    .get(key)
                    .copied()
                    .map(Some)
                    .ok_or(ToolError::BadArgument {
                        argument: "new_category",
                    })
            }
            (Some(_), Some(_)) => Err(ToolError::BadArgument {
                argument: "category_id",
            }),
        }
    }
}

/// Writes what the user reviewed, inside the set's user action: the
/// transaction exactly as reviewed, or nothing of the set at all.
pub(super) fn apply(book: &mut Book, change: &SplitTransaction) -> Result<(), ChangeError> {
    match book.transaction(change.transaction) {
        Ok(stored) if stored == change.reviewed => {}
        Ok(_) | Err(StoreError::UnknownTransaction(_)) => return Err(ChangeError::OutOfDate),
        Err(error) => return Err(error.into()),
    }
    book.update_transaction(&change.edited)?;
    Ok(())
}
