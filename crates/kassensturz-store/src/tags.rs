// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tags on transactions and their parts (D-132, D-712 to D-714): their
//! names and limits, and renaming, removing and merging them.
//!
//! A tag never moves money and never changes a sum or a balance: changing
//! one only changes which rows of `split_tags` and `transaction_tags` name
//! it, and every change is one action in the change history, so one Undo
//! takes it back (D-086).

use std::collections::HashMap;

use kassensturz_core::{MAX_TAG_NAME_CHARS, MAX_TAGS_IN_BOOK, TagId};
use rusqlite::{OptionalExtension, params};

use crate::cached::Cached;
use crate::error::StoreError;
use crate::{Action, Book};

/// How many transactions and parts carry one tag, for the list of tags
/// (D-712).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TagUse {
    /// Transactions that carry it on the whole of them.
    pub transactions: usize,
    /// Parts that carry it as one of their own tags.
    pub parts: usize,
}

/// A tag's name as the book keeps it (D-712): without spaces around it. An
/// empty name is refused ([`StoreError::EmptyName`]), and so is one longer
/// than [`MAX_TAG_NAME_CHARS`] or with a line break or another control
/// character ([`StoreError::TagName`]); a name is never shortened.
pub fn checked_tag_name(name: &str) -> Result<String, StoreError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(StoreError::EmptyName);
    }
    let refused = |reason| StoreError::TagName {
        reason,
        max_chars: MAX_TAG_NAME_CHARS,
    };
    if name.chars().any(char::is_control) {
        return Err(refused("line-break"));
    }
    if name.chars().count() > MAX_TAG_NAME_CHARS {
        return Err(refused("too-long"));
    }
    Ok(name.to_owned())
}

impl Book {
    /// Refuses `tag` if the book has no tag with its ID.
    pub(crate) fn check_tag_known(&self, tag: TagId) -> Result<(), StoreError> {
        let known: Option<i64> = self
            .conn
            .query_row_cached(
                "SELECT 1 FROM tags WHERE id = ?1",
                [tag.to_bytes()],
                |row| row.get(0),
            )
            .optional()?;
        known.map(|_| ()).ok_or(StoreError::UnknownTag(tag))
    }

    /// Refuses `name` (already checked) if another tag than `own` has it,
    /// ignoring capitals.
    pub(crate) fn check_tag_name_free(
        &self,
        name: &str,
        own: Option<TagId>,
    ) -> Result<(), StoreError> {
        let wanted = name.to_lowercase();
        match self
            .tags()?
            .into_iter()
            .find(|other| Some(other.id) != own && other.name.trim().to_lowercase() == wanted)
        {
            Some(existing) => Err(StoreError::DuplicateTagName {
                existing: existing.name,
            }),
            None => Ok(()),
        }
    }

    /// Refuses a new tag if the book has [`MAX_TAGS_IN_BOOK`] already.
    pub(crate) fn check_room_for_a_tag(&self) -> Result<(), StoreError> {
        let count: i64 = self
            .conn
            .query_row_cached("SELECT count(*) FROM tags", [], |row| row.get(0))?;
        if usize::try_from(count).unwrap_or(usize::MAX) >= MAX_TAGS_IN_BOOK {
            return Err(StoreError::TooManyTags {
                limit: "book",
                max: MAX_TAGS_IN_BOOK,
            });
        }
        Ok(())
    }

    /// Gives `tag` the name `name`, checked as for a new tag
    /// ([`checked_tag_name`], unique ignoring capitals). Everything tagged
    /// with it stays tagged: only the name changes.
    pub fn rename_tag(&mut self, tag: TagId, name: &str) -> Result<(), StoreError> {
        self.check_tag_known(tag)?;
        let name = checked_tag_name(name)?;
        self.check_tag_name_free(&name, Some(tag))?;
        self.as_one_action(Action::RenameTag, |book| {
            book.conn.execute(
                "UPDATE tags SET name = ?2 WHERE id = ?1",
                params![tag.to_bytes(), name],
            )?;
            Ok(())
        })
    }

    /// Removes `tag` from every transaction and part that carries it, and
    /// from the book. No amount, category or other tag changes, and one Undo
    /// brings it back with every use (D-712).
    pub fn remove_tag(&mut self, tag: TagId) -> Result<(), StoreError> {
        self.check_tag_known(tag)?;
        self.as_one_action(Action::RemoveTag, |book| {
            let id = tag.to_bytes();
            book.conn
                .execute("DELETE FROM split_tags WHERE tag_id = ?1", [id])?;
            book.conn
                .execute("DELETE FROM transaction_tags WHERE tag_id = ?1", [id])?;
            book.conn.execute("DELETE FROM tags WHERE id = ?1", [id])?;
            Ok(())
        })
    }

    /// Gives every transaction and part that carries `from` the tag `into`
    /// instead, in `from`'s place, and removes `from` (D-712). Where one
    /// already carries `into`, it keeps it once. One Undo takes it back.
    pub fn merge_tags(&mut self, from: TagId, into: TagId) -> Result<(), StoreError> {
        if from == into {
            return Err(StoreError::TagMergedIntoItself(from));
        }
        self.check_tag_known(from)?;
        self.check_tag_known(into)?;
        self.as_one_action(Action::MergeTags, |book| {
            let (from, into) = (from.to_bytes(), into.to_bytes());
            for (table, owner) in [
                ("split_tags", "split_id"),
                ("transaction_tags", "transaction_id"),
            ] {
                book.conn.execute(
                    &format!(
                        "DELETE FROM {table} WHERE tag_id = ?1 AND {owner} IN
                             (SELECT {owner} FROM {table} WHERE tag_id = ?2)"
                    ),
                    params![from, into],
                )?;
                book.conn.execute(
                    &format!("UPDATE {table} SET tag_id = ?2 WHERE tag_id = ?1"),
                    params![from, into],
                )?;
            }
            book.conn
                .execute("DELETE FROM tags WHERE id = ?1", [from])?;
            Ok(())
        })
    }

    /// How many transactions and parts carry each tag that is used; a tag
    /// nobody uses isn't in the map (D-712).
    pub fn tag_uses(&self) -> Result<HashMap<TagId, TagUse>, StoreError> {
        let mut uses: HashMap<TagId, TagUse> = HashMap::new();
        for (table, parts) in [("transaction_tags", false), ("split_tags", true)] {
            let mut statement = self.conn.prepare_cached(&format!(
                "SELECT tag_id, count(*) FROM {table} GROUP BY tag_id"
            ))?;
            let mut rows = statement.query([])?;
            while let Some(row) = rows.next()? {
                let tag = TagId::from_bytes(row.get(0)?);
                let count = usize::try_from(row.get::<_, i64>(1)?).unwrap_or(usize::MAX);
                let entry = uses.entry(tag).or_default();
                if parts {
                    entry.parts = count;
                } else {
                    entry.transactions = count;
                }
            }
        }
        Ok(uses)
    }
}

#[cfg(test)]
mod tests;
