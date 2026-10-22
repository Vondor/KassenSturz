// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a book keeps besides accounts and transactions: the banks accounts
//! are at, who the user pays, and tags (D-132). KassenSturz only adds and
//! lists them so far; changing and removing them comes with their screens.

use kassensturz_core::{Institution, InstitutionId, Payee, PayeeId, PayeeRecognition, Tag, TagId};
use rusqlite::{OptionalExtension, params};

use crate::error::{Corruption, StoreError};
use crate::{Action, Book};

/// What a payee's `recognition` column says.
const RECOGNITIONS: [(&str, &str); 3] = [
    ("never", "never"),
    ("name", "name"),
    ("key_words", "key_words"),
];

impl Book {
    /// Adds a bank or other institution. Its name can't be empty; the rest
    /// is kept as given.
    pub fn add_institution(&mut self, institution: &Institution) -> Result<(), StoreError> {
        if institution.name.trim().is_empty() {
            return Err(StoreError::EmptyName);
        }
        self.as_one_action(Action::AddInstitution, |book| {
            book.conn.execute(
                "INSERT INTO institutions (id, name, bank_code, contact, address, phone)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    institution.id.to_bytes(),
                    institution.name,
                    institution.bank_code,
                    institution.contact,
                    institution.address,
                    institution.phone,
                ],
            )?;
            Ok(())
        })
    }

    /// The institution with this ID.
    pub fn institution(&self, id: InstitutionId) -> Result<Institution, StoreError> {
        self.conn
            .query_row(
                "SELECT id, name, bank_code, contact, address, phone FROM institutions
                 WHERE id = ?1",
                [id.to_bytes()],
                institution_row,
            )
            .optional()?
            .ok_or(StoreError::UnknownInstitution(id))
    }

    /// Every institution, sorted by name, then by ID.
    pub fn institutions(&self) -> Result<Vec<Institution>, StoreError> {
        let mut statement = self.conn.prepare(
            "SELECT id, name, bank_code, contact, address, phone FROM institutions ORDER BY id",
        )?;
        let mut rows = statement
            .query_map([], institution_row)?
            .collect::<Result<Vec<_>, _>>()?;
        rows.sort_by_cached_key(|institution| (institution.name.to_lowercase(), institution.id));
        Ok(rows)
    }

    /// Adds a payee. The name can't be empty, and no two payees share one,
    /// ignoring capitals and spaces around it, so the user can tell them
    /// apart; key words, if the payee is recognized by them, can't be
    /// empty either.
    pub fn add_payee(&mut self, payee: &Payee) -> Result<(), StoreError> {
        if payee.name.trim().is_empty() {
            return Err(StoreError::EmptyName);
        }
        let wanted = payee.name.trim().to_lowercase();
        if let Some(existing) = self
            .payees()?
            .into_iter()
            .find(|other| other.id != payee.id && other.name.trim().to_lowercase() == wanted)
        {
            return Err(StoreError::DuplicatePayeeName {
                existing: existing.name,
            });
        }
        let (recognition, key_words) = match &payee.recognition {
            PayeeRecognition::Never => ("never", None),
            PayeeRecognition::ByName => ("name", None),
            PayeeRecognition::ByKeyWords(words) => {
                if words.is_empty() || words.iter().any(|word| word.trim().is_empty()) {
                    return Err(StoreError::EmptyName);
                }
                ("key_words", Some(words.join("\n")))
            }
        };
        self.as_one_action(Action::AddPayee, |book| {
            book.conn.execute(
                "INSERT INTO payees
                     (id, name, address, email, phone, reference,
                      recognition, key_words, recognition_ignores_case)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    payee.id.to_bytes(),
                    payee.name,
                    payee.address,
                    payee.email,
                    payee.phone,
                    payee.reference,
                    recognition,
                    key_words,
                    payee.recognition_ignores_case,
                ],
            )?;
            Ok(())
        })
    }

    /// Every payee, sorted by name, then by ID.
    pub fn payees(&self) -> Result<Vec<Payee>, StoreError> {
        let mut statement = self.conn.prepare(
            "SELECT id, name, address, email, phone, reference,
                    recognition, key_words, recognition_ignores_case
             FROM payees ORDER BY id",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    PayeeId::from_bytes(row.get(0)?),
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, i64>(8)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut payees = Vec::with_capacity(rows.len());
        for (id, name, address, email, phone, reference, recognition, key_words, ignores_case) in
            rows
        {
            let bad = || StoreError::Corrupt(Corruption::BadPayee(id));
            let recognition = match (
                RECOGNITIONS
                    .iter()
                    .find(|(stored, _)| *stored == recognition)
                    .map(|(_, word)| *word),
                key_words,
            ) {
                (Some("never"), None) => PayeeRecognition::Never,
                (Some("name"), None) => PayeeRecognition::ByName,
                (Some("key_words"), Some(words)) => {
                    PayeeRecognition::ByKeyWords(words.lines().map(str::to_owned).collect())
                }
                _ => return Err(bad()),
            };
            payees.push(Payee {
                id,
                name,
                address,
                email,
                phone,
                reference,
                recognition,
                recognition_ignores_case: ignores_case != 0,
            });
        }
        payees.sort_by_cached_key(|payee| (payee.name.to_lowercase(), payee.id));
        Ok(payees)
    }

    /// Adds a tag. The name can't be empty, and no two tags share one,
    /// ignoring capitals and spaces around it.
    pub fn add_tag(&mut self, tag: &Tag) -> Result<(), StoreError> {
        if tag.name.trim().is_empty() {
            return Err(StoreError::EmptyName);
        }
        let wanted = tag.name.trim().to_lowercase();
        if let Some(existing) = self
            .tags()?
            .into_iter()
            .find(|other| other.id != tag.id && other.name.trim().to_lowercase() == wanted)
        {
            return Err(StoreError::DuplicateTagName {
                existing: existing.name,
            });
        }
        self.as_one_action(Action::AddTag, |book| {
            book.conn.execute(
                "INSERT INTO tags (id, name) VALUES (?1, ?2)",
                params![tag.id.to_bytes(), tag.name],
            )?;
            Ok(())
        })
    }

    /// Every tag, sorted by name, then by ID.
    pub fn tags(&self) -> Result<Vec<Tag>, StoreError> {
        let mut statement = self.conn.prepare("SELECT id, name FROM tags ORDER BY id")?;
        let mut tags = statement
            .query_map([], |row| {
                Ok(Tag {
                    id: TagId::from_bytes(row.get(0)?),
                    name: row.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        tags.sort_by_cached_key(|tag| (tag.name.to_lowercase(), tag.id));
        Ok(tags)
    }
}

fn institution_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Institution> {
    Ok(Institution {
        id: InstitutionId::from_bytes(row.get(0)?),
        name: row.get(1)?,
        bank_code: row.get(2)?,
        contact: row.get(3)?,
        address: row.get(4)?,
        phone: row.get(5)?,
    })
}

#[cfg(test)]
mod tests;
