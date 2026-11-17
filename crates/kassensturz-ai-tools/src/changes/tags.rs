// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tags put on whole transactions or taken off them, prepared by the AI
//! (US5.2 step b, D-744).
//!
//! Behind its own cell, *Tags on transactions › Change*, off by default. The
//! model names a transaction by the ID `search_transactions` gives and tags
//! by the IDs `list_tags` gives. A new tag (`new_tags`, D-830) needs a cell
//! of its own besides, *Tags on transactions › Add*: it is made only
//! together with a transaction of the same set that gets it, never with
//! the name of a tag the book has, and never more than [`MAX_NEW_TAGS`] at
//! once, so the model can't leave unused words behind or make a second
//! tag for one the user has. Only the tags on the whole
//! transaction change, as the simple form sets them (Q-493): a part's own
//! tags stay as they are, so a tag the user put on one item of a bill is
//! never moved or removed.
//!
//! On confirm the transaction must be exactly as the user reviewed it;
//! otherwise nothing of the set is applied (`ai-changes.out-of-date`). The
//! change is part of the set's one `apply_ai_changes` entry, so one Undo
//! takes the whole set back, a new tag with every use of it.

use std::collections::{HashMap, HashSet};

use kassensturz_core::{
    MAX_TAGS_IN_BOOK, MAX_TAGS_ON_TRANSACTION, Money, Tag, TagId, Transaction, TransactionId,
};
use kassensturz_store::{Book, StoreError};
use serde_json::Value;

use super::{ChangeError, MAX_KEY_CHARS, Object, plain_text};
use crate::error::ToolError;

/// The list of `propose_changes` that holds the changes.
pub(super) const LIST: &str = "tag_transactions";

/// The list of `propose_changes` that holds the new tags (D-830).
pub(super) const NEW_LIST: &str = "new_tags";

/// The most tags one set may make, so the model can't fill the user's list
/// of tags with words of its own (D-830). The schema states it too.
pub(super) const MAX_NEW_TAGS: usize = 5;

/// A new tag, as the user reviews it (D-830).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewTag {
    /// The ID it gets.
    pub id: TagId,
    /// Its name, as the book will keep it.
    pub name: String,
    /// How many transactions of the set get it; never 0.
    pub uses: usize,
}

/// The new tags of a call by the model's keys, while its tag changes are
/// read.
#[derive(Default)]
pub(super) struct NewTags {
    /// The tag each key names.
    keys: HashMap<String, TagId>,
    /// The names of the new tags.
    names: HashMap<TagId, String>,
    /// How many tag changes put each one on.
    uses: HashMap<TagId, usize>,
}

impl NewTags {
    /// How many tag changes put the new tag `id` on.
    pub(super) fn uses(&self, id: TagId) -> usize {
        self.uses.get(&id).copied().unwrap_or(0)
    }
}

/// Checks one item of [`NEW_LIST`] against the book's tags `known` and the
/// new tags before it: a name the book could keep for a tag made by hand (not empty, at
/// most [`kassensturz_core::MAX_TAG_NAME_CHARS`], one line) and that hides
/// nothing (as other names the AI writes), no tag of the book or of the call
/// with a name that looks the same, a key of its own, and room in the book.
/// The name is read before the key, so a call that repeats a tag of the
/// book hears which one to use (`ai-tools.tag-name-taken`).
pub(super) fn new_tag(
    value: &Value,
    known: &[Tag],
    new: &mut NewTags,
) -> Result<NewTag, ToolError> {
    let item = Object::read(value, &["key", "name"], NEW_LIST)?;
    let name = plain_text(item.required_string("name")?, "name")?;
    let name = kassensturz_store::checked_tag_name(&name).map_err(ToolError::TagRefused)?;
    // Names that look the same are the same name, as for a tag made by hand
    // (B-272, D-782).
    let wanted = kassensturz_store::name_key(&name);
    if let Some(existing) = known
        .iter()
        .find(|tag| kassensturz_store::name_key(&tag.name) == wanted)
    {
        return Err(ToolError::TagNameTaken { tag: existing.id });
    }
    if new
        .names
        .values()
        .any(|other| kassensturz_store::name_key(other) == wanted)
    {
        return Err(ToolError::NameTaken);
    }
    let key = item.required_string("key")?.trim();
    if key.is_empty() || key.chars().count() > MAX_KEY_CHARS || new.keys.contains_key(key) {
        return Err(ToolError::BadArgument { argument: "key" });
    }
    if known.len() + new.names.len() >= MAX_TAGS_IN_BOOK {
        return Err(ToolError::TagRefused(StoreError::TooManyTags {
            limit: "book",
            max: MAX_TAGS_IN_BOOK,
        }));
    }
    let id = TagId::generate();
    new.keys.insert(key.to_owned(), id);
    new.names.insert(id, name.clone());
    Ok(NewTag { id, name, uses: 0 })
}

/// The most transactions one set may tag, so the user can still review
/// them.
pub(super) const MAX_TAGGED: usize = 100;

/// The keys an item of [`LIST`] takes.
const KEYS: &[&str] = &[
    "transaction_id",
    "add_tag_ids",
    "add_new_tags",
    "remove_tag_ids",
];

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
    /// The names of the book's tags put on it, in the model's order.
    pub added: Vec<String>,
    /// The names of the new tags of the set put on it, in the model's order
    /// (D-830): the review marks them as new.
    pub added_new: Vec<String>,
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

/// The new tags of the set the list `add_new_tags` of `item` names by
/// their keys, each once (D-830).
fn new_tag_ids(item: &Object<'_>, new: &NewTags) -> Result<Vec<TagId>, ToolError> {
    let refused = || ToolError::BadArgument {
        argument: "add_new_tags",
    };
    let mut ids = Vec::new();
    for value in item.array("add_new_tags")? {
        let id = value
            .as_str()
            .and_then(|key| new.keys.get(key.trim()))
            .copied()
            .ok_or_else(refused)?;
        if ids.contains(&id) {
            return Err(refused());
        }
        ids.push(id);
    }
    Ok(ids)
}

/// Checks one item of [`LIST`] against `book`: a transaction of the book,
/// tags of the book or new ones of the set (`new`, only when `may_add`),
/// at least one change, nothing added that it already has on the whole,
/// nothing removed that it doesn't, and at most [`MAX_TAGS_ON_TRANSACTION`]
/// tags after it. `seen` holds the transactions other changes of the set
/// already touch; a transaction in two changes is refused (`also-changed`).
pub(super) fn tag_transaction(
    book: &Book,
    value: &Value,
    seen: &mut HashSet<TransactionId>,
    new: &mut NewTags,
    may_add: bool,
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
    // Making tags is a cell of its own (D-830): without it the key list is
    // refused as not allowed, so the model hears where the user allows it.
    if !may_add && !item.array("add_new_tags")?.is_empty() {
        return Err(ToolError::NotAllowed {
            argument: "add_new_tags",
        });
    }
    let add_new = new_tag_ids(&item, new)?;
    if add.is_empty() && add_new.is_empty() && remove.is_empty() {
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
    tags.extend(&add_new);
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
    for tag in &add_new {
        *new.uses.entry(*tag).or_default() += 1;
    }
    let added_new = add_new
        .iter()
        .map(|tag| new.names.get(tag).cloned().unwrap_or_default())
        .collect();
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
        added_new,
        removed: remove.iter().map(name).collect(),
        edited: stored.clone().with_tags(tags),
        reviewed: stored,
    })
}

/// Makes the new tag the user reviewed, inside the set's user action, with
/// the name and limits the store checks for a tag made by hand: a tag of
/// the same name made since the review stops the whole set.
pub(super) fn apply_new(book: &mut Book, new: &NewTag) -> Result<(), ChangeError> {
    book.add_tag(&Tag {
        id: new.id,
        name: new.name.clone(),
    })?;
    Ok(())
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
