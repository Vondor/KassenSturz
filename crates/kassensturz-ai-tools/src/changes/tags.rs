// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tags put on whole transactions or taken off them, prepared by the AI
//! (US5.2 step b, D-744).
//!
//! Behind its own cell, *Tags on transactions › Change*, off by default. The
//! model names a transaction by the ID `search_transactions` gives and tags
//! by the IDs `list_tags` gives; it never makes a new tag, so a word the
//! model invented can't enter the book. Only the tags on the whole
//! transaction change, as the simple form sets them (Q-493): a part's own
//! tags stay as they are, so a tag the user put on one item of a bill is
//! never moved or removed.
//!
//! On confirm the transaction must be exactly as the user reviewed it;
//! otherwise nothing of the set is applied (`ai-changes.out-of-date`). The
//! change is part of the set's one `apply_ai_changes` entry, so one Undo
//! takes the whole set back.

use std::collections::HashSet;

use kassensturz_core::{MAX_TAGS_ON_TRANSACTION, Money, TagId, Transaction, TransactionId};
use kassensturz_store::{Book, StoreError};
use serde_json::Value;

use super::{ChangeError, Object};
use crate::error::ToolError;

/// The list of `propose_changes` that holds the changes.
pub(super) const LIST: &str = "tag_transactions";

/// The most transactions one set may tag, so the user can still review
/// them.
pub(super) const MAX_TAGGED: usize = 100;

/// The keys an item of [`LIST`] takes.
const KEYS: &[&str] = &["transaction_id", "add_tag_ids", "remove_tag_ids"];

/// One transaction's tags changed, as the user reviews it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagTransaction {
    /// The transaction.
    pub transaction: TransactionId,
    /// Its day.
    pub date: kassensturz_core::Date,
    /// Who was paid or paid, if anyone.
    pub payee: Option<String>,
    /// The first of the user's own accounts it is in, if any.
    pub account: Option<String>,
    /// What it did to that account, or the first part's amount.
    pub amount: Money,
    /// The names of the tags put on it, in the model's order.
    pub added: Vec<String>,
    /// The names of the tags taken off it.
    pub removed: Vec<String>,
    /// The transaction as the user reviewed it.
    reviewed: Transaction,
    /// The transaction with its new tags.
    edited: Transaction,
}

/// The tag IDs of the list `name` of `item`, each a tag of the book, each
/// once.
fn tag_ids(
    item: &Object<'_>,
    name: &'static str,
    known: &[kassensturz_core::Tag],
) -> Result<Vec<TagId>, ToolError> {
    let mut ids = Vec::new();
    for value in item.array(name)? {
        let id = value
            .as_str()
            .and_then(|text| uuid::Uuid::try_parse(text).ok())
            .map(|id| TagId::from_bytes(id.into_bytes()))
            .ok_or(ToolError::BadArgument { argument: name })?;
        if !known.iter().any(|tag| tag.id == id) {
            return Err(ToolError::UnknownTag);
        }
        if ids.contains(&id) {
            return Err(ToolError::BadArgument { argument: name });
        }
        ids.push(id);
    }
    Ok(ids)
}

/// Checks one item of [`LIST`] against `book`: a transaction of the book,
/// tags of the book, at least one change, nothing added that it already
/// has on the whole, nothing removed that it doesn't, and at most
/// [`MAX_TAGS_ON_TRANSACTION`] tags after it. `seen` holds the transactions
/// other changes of the set already touch; a transaction in two changes is
/// refused (`also-changed`).
pub(super) fn tag_transaction(
    book: &Book,
    value: &Value,
    seen: &mut HashSet<TransactionId>,
) -> Result<TagTransaction, ToolError> {
    let item = Object::read(value, KEYS, LIST)?;
    let id = item
        .uuid("transaction_id")?
        .map(TransactionId::from_bytes)
        .ok_or(ToolError::BadArgument {
            argument: "transaction_id",
        })?;
    let stored = match book.transaction(id) {
        Err(StoreError::UnknownTransaction(_)) => return Err(ToolError::UnknownTransaction),
        other => other?,
    };
    let known = book.tags()?;
    let add = tag_ids(&item, "add_tag_ids", &known)?;
    let remove = tag_ids(&item, "remove_tag_ids", &known)?;
    if add.is_empty() && remove.is_empty() {
        return Err(ToolError::BadArgument {
            argument: "add_tag_ids",
        });
    }
    if add
        .iter()
        .any(|tag| stored.tags().contains(tag) || remove.contains(tag))
    {
        return Err(ToolError::BadArgument {
            argument: "add_tag_ids",
        });
    }
    if remove.iter().any(|tag| !stored.tags().contains(tag)) {
        return Err(ToolError::BadArgument {
            argument: "remove_tag_ids",
        });
    }
    let mut tags: Vec<TagId> = stored
        .tags()
        .iter()
        .copied()
        .filter(|tag| !remove.contains(tag))
        .collect();
    tags.extend(&add);
    if tags.len() > MAX_TAGS_ON_TRANSACTION {
        return Err(ToolError::NotChangeable {
            argument: "add_tag_ids",
            reason: "too-many-tags",
        });
    }
    if !seen.insert(id) {
        return Err(ToolError::NotChangeable {
            argument: "transaction_id",
            reason: "also-changed",
        });
    }
    let name = |id: &TagId| {
        known
            .iter()
            .find(|tag| tag.id == *id)
            .map(|tag| tag.name.clone())
            .unwrap_or_default()
    };
    let accounts = book.accounts(true)?;
    let own = stored.splits().iter().find_map(|split| {
        let account = accounts
            .iter()
            .find(|account| Some(account.id) == split.account)?;
        Some((split, account.name.clone()))
    });
    let (amount, account) = match own {
        Some((split, name)) => (split.account_amount(), Some(name)),
        None => (
            stored
                .splits()
                .first()
                .map_or_else(|| Money::zero(stored.currency()), |split| split.amount),
            None,
        ),
    };
    Ok(TagTransaction {
        transaction: id,
        date: stored.booking_date(),
        payee: stored.payee().map(str::to_owned),
        account,
        amount,
        added: add.iter().map(name).collect(),
        removed: remove.iter().map(name).collect(),
        edited: stored.clone().with_tags(tags),
        reviewed: stored,
    })
}

/// Writes what the user reviewed, inside the set's user action: the
/// transaction exactly as reviewed, or nothing of the set at all.
pub(super) fn apply(book: &mut Book, change: &TagTransaction) -> Result<(), ChangeError> {
    match book.transaction(change.transaction) {
        Ok(stored) if stored == change.reviewed => {}
        Ok(_) | Err(StoreError::UnknownTransaction(_)) => return Err(ChangeError::OutOfDate),
        Err(error) => return Err(error.into()),
    }
    book.update_transaction(&change.edited)?;
    Ok(())
}
