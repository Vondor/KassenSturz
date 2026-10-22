// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Transfers between the user's own accounts in a book: which transactions
//! to suggest joining, and joining two of them into one Transfer once the
//! user confirms it (D-021, D-030, D-087).

use std::collections::HashSet;

use kassensturz_core::{
    AccountId, TransactionId, TransferMatch, TransferSide, find_transfers, join_transfer,
};
use rusqlite::params;

use crate::error::StoreError;
use crate::transactions::insert_splits;
use crate::{Action, Book};

impl Book {
    /// The Transfers to suggest to the user: pairs of transactions where one
    /// takes money out of one of the user's accounts and the other puts the
    /// same amount into another, in the same currency, at most three
    /// business days apart, and whose other parts have no category yet
    /// (D-021, D-030). See [`find_transfers`] for which pair wins where
    /// several fit.
    ///
    /// They are only suggestions: nothing is changed until the user confirms
    /// one with [`Book::join_transfer`]. One the user turns down drops out
    /// once its parts get categories.
    pub fn transfer_suggestions(&self) -> Result<Vec<TransferMatch>, StoreError> {
        let accounts = self.account_ids()?;
        // Only transactions with two parts, one of them without a category,
        // can be sides; they are read together, in full, as a book's whole
        // list is, not one by one (#84).
        let candidates = self.transactions_where(
            "WHERE t.id IN (SELECT transaction_id FROM splits
                            GROUP BY transaction_id
                            HAVING count(*) = 2 AND count(account_id) = 1
                                   AND count(foreign_amount) = 0)",
            &[],
        )?;
        let mut sides = Vec::new();
        for transaction in &candidates {
            if let Ok(side) = TransferSide::of(transaction, |id| accounts.contains(&id)) {
                sides.push(side);
            }
        }
        Ok(find_transfers(&sides))
    }

    /// Joins the transactions `a` and `b`, the two sides of a Transfer the
    /// user confirmed, into one transaction, and returns its ID (D-087).
    ///
    /// The transaction the money left from stays, with its date and payee,
    /// and gets the part that books the money into the other account, with
    /// what that account's bank delivered (D-083) and, as its own, the date
    /// and payee that account showed (D-108); the other transaction is
    /// removed. Both accounts' balances and registers stay as they were, and
    /// the two parts without a category go. It is one user action, which
    /// undo takes back as a whole.
    ///
    /// Any two sides can be joined, however far apart their dates are, since
    /// the user decides; they must be in different accounts and take out and
    /// put in the same amount ([`StoreError::Transfer`]).
    pub fn join_transfer(
        &mut self,
        a: TransactionId,
        b: TransactionId,
    ) -> Result<TransactionId, StoreError> {
        let first = self.transaction(a)?;
        let second = self.transaction(b)?;
        let accounts = self.account_ids()?;
        let joined = join_transfer(&first, &second, |id| accounts.contains(&id))
            .map_err(StoreError::Transfer)?;
        let removed = if joined.id() == a { b } else { a };
        let id = joined.id().to_bytes();
        self.as_one_action(Action::JoinTransfer, |book| {
            // Its parts, with what their bank delivered, go with it; the one
            // booked to its account comes back in the joined transaction.
            book.conn.execute(
                "DELETE FROM transactions WHERE id = ?1",
                [removed.to_bytes()],
            )?;
            book.conn.execute(
                "UPDATE transactions SET payee = ?2 WHERE id = ?1",
                params![id, joined.payee()],
            )?;
            book.conn
                .execute("DELETE FROM splits WHERE transaction_id = ?1", [id])?;
            insert_splits(&book.conn, &joined)
        })?;
        Ok(joined.id())
    }

    /// The IDs of the user's accounts, closed ones included, as opposed to
    /// categories and internal accounts.
    fn account_ids(&self) -> Result<HashSet<AccountId>, StoreError> {
        Ok(self
            .accounts(true)?
            .into_iter()
            .map(|account| account.id)
            .collect())
    }
}

#[cfg(test)]
mod tests;
