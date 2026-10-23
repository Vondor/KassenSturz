// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Bank links: which bank account an account is filled from, and the day it
//! was last fetched (Q-086, D-148; layout 17). The book keeps no login data
//! (D-139): the link names the bank account by IBAN, or by bank code and
//! account number, so that it finds it again in any login that lists it,
//! on this computer or another.

use kassensturz_core::{AccountId, BankLink, Date};
use rusqlite::{OptionalExtension, params};

use crate::error::StoreError;
use crate::{Action, Book, date_from_text, date_to_text};

/// The columns [`read_link`] expects, in its order.
const COLUMNS: &str = "account_id, iban, bank_code, number, last_fetched_on";

fn read_link(row: &rusqlite::Row<'_>) -> rusqlite::Result<(BankLink, Option<String>)> {
    Ok((
        BankLink {
            account: AccountId::from_bytes(row.get(0)?),
            iban: row.get(1)?,
            bank_code: row.get(2)?,
            number: row.get(3)?,
            last_fetched_on: None,
        },
        row.get(4)?,
    ))
}

fn with_date((mut link, on): (BankLink, Option<String>)) -> Result<BankLink, StoreError> {
    link.last_fetched_on = on.as_deref().map(date_from_text).transpose()?;
    Ok(link)
}

/// The text without spaces around it, or `None` when nothing is left.
fn trimmed(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

impl Book {
    /// The bank link of the account `id`, if it has one.
    pub fn bank_link(&self, id: AccountId) -> Result<Option<BankLink>, StoreError> {
        // The account must exist, linked or not.
        self.account(id)?;
        self.conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM bank_links WHERE account_id = ?1"),
                [id.to_bytes()],
                read_link,
            )
            .optional()?
            .map(with_date)
            .transpose()
    }

    /// Every bank link in the book, sorted by account ID.
    pub fn bank_links(&self) -> Result<Vec<BankLink>, StoreError> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM bank_links ORDER BY account_id"
        ))?;
        let rows = statement
            .query_map([], read_link)?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter().map(with_date).collect()
    }

    /// Links the account `link.account` to the bank account the link names,
    /// or changes its link; the user action [`Action::LinkBankAccount`].
    ///
    /// The link must name a bank account: an IBAN, or a bank code with an
    /// account number ([`StoreError::IncompleteBankLink`]). The IBAN is kept
    /// without spaces and in capitals, the bank code and the number without
    /// spaces around them. Only an account the user sees can be linked,
    /// never a category ([`StoreError::NotAnAccount`]), and no two accounts
    /// to the same bank account ([`StoreError::BankAccountAlreadyLinked`]).
    /// The day of the last fetch is kept as given: `None` for a new link,
    /// or the old link's day when only its numbers are corrected.
    pub fn link_bank_account(&mut self, link: &BankLink) -> Result<(), StoreError> {
        let id = link.account;
        let account = self.account(id)?;
        if !account.kind.is_account() {
            return Err(StoreError::NotAnAccount(id));
        }
        let link = BankLink {
            account: id,
            iban: link.iban.as_deref().map(|iban| {
                iban.chars()
                    .filter(|c| !c.is_whitespace())
                    .map(|c| c.to_ascii_uppercase())
                    .collect()
            }),
            bank_code: trimmed(link.bank_code.as_deref()),
            number: trimmed(link.number.as_deref()),
            last_fetched_on: link.last_fetched_on,
        };
        if !link.names_a_bank_account() {
            return Err(StoreError::IncompleteBankLink(id));
        }
        let taken: Option<[u8; 16]> = self
            .conn
            .query_row(
                "SELECT account_id FROM bank_links
                 WHERE account_id <> ?1
                 AND ((?2 IS NOT NULL AND iban = ?2)
                      OR (?3 IS NOT NULL AND ?4 IS NOT NULL
                          AND bank_code = ?3 AND number = ?4))
                 ORDER BY account_id LIMIT 1",
                params![id.to_bytes(), link.iban, link.bank_code, link.number],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(existing) = taken {
            return Err(StoreError::BankAccountAlreadyLinked {
                existing: AccountId::from_bytes(existing),
            });
        }
        let on = link.last_fetched_on.map(date_to_text).transpose()?;
        self.as_one_action(Action::LinkBankAccount, |book| {
            book.conn.execute(
                "INSERT INTO bank_links (account_id, iban, bank_code, number, last_fetched_on)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (account_id) DO UPDATE SET
                     iban = excluded.iban, bank_code = excluded.bank_code,
                     number = excluded.number, last_fetched_on = excluded.last_fetched_on",
                params![id.to_bytes(), link.iban, link.bank_code, link.number, on],
            )?;
            Ok(())
        })
    }

    /// Removes the bank link of the account `id`; the user action
    /// [`Action::UnlinkBankAccount`]. The account keeps everything else,
    /// its transactions included. An account without a link is left as it
    /// is, and nothing is recorded.
    pub fn unlink_bank_account(&mut self, id: AccountId) -> Result<(), StoreError> {
        self.account(id)?;
        self.as_one_action(Action::UnlinkBankAccount, |book| {
            book.conn.execute(
                "DELETE FROM bank_links WHERE account_id = ?1",
                [id.to_bytes()],
            )?;
            Ok(())
        })
    }

    /// Notes `on` as the day the account `id` was last fetched from its
    /// bank. On its own this is the user action [`Action::FetchStatement`];
    /// a fetch that also imports what it got notes the day within that
    /// action, so one undo takes back both. The account must be linked
    /// ([`StoreError::NotLinked`]).
    pub fn note_bank_fetch(&mut self, id: AccountId, on: Date) -> Result<(), StoreError> {
        if self.bank_link(id)?.is_none() {
            return Err(StoreError::NotLinked(id));
        }
        let on = date_to_text(on)?;
        self.as_one_action(Action::FetchStatement, |book| {
            book.conn.execute(
                "UPDATE bank_links SET last_fetched_on = ?2 WHERE account_id = ?1",
                params![id.to_bytes(), on],
            )?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests;
