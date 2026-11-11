// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Transfers between the user's own accounts in a book: which transactions
//! to suggest joining, which ones the user may pick to join, and joining two
//! of them into one Transfer once the user confirms it (D-021, D-030,
//! D-087, D-691).

use std::collections::{HashMap, HashSet};

use kassensturz_core::{
    AccountId, MAX_BUSINESS_DAYS, RuleId, SplitId, Transaction, TransactionId, TransferMatch,
    TransferSide, days_between, find_transfers, join_transfer,
};
use rusqlite::params;

use crate::error::StoreError;
use crate::originals::keeping_originals;
use crate::transactions::{insert_attachments, insert_splits};
use crate::{Action, Book};

/// How many transactions [`Book::transfer_partners`] lists at most.
pub const MAX_TRANSFER_PARTNERS: usize = 20;

/// Who put a side's other part where it is (D-691, as D-672 reads it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideSorting {
    /// The other part has no category yet.
    Open,
    /// A Rule put it in its category, and the user didn't change it since
    /// (`split_rules`, D-088): the Rule, or `None` if it was removed since.
    ByRule(Option<RuleId>),
    /// It is in a category, and no Rule mark says a Rule put it there: the
    /// user sorted it, changed it after a Rule, or another program's book
    /// brought it in sorted.
    ByHand,
}

/// A transaction that could be the other side of a Transfer the user
/// chose to join ([`Book::transfer_partners`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferPartner {
    /// The side.
    pub side: TransferSide,
    /// Who sorted its other part.
    pub sorting: SideSorting,
    /// How many business days it is from the chosen side.
    pub business_days: u32,
    /// Whether it is within the days a suggestion allows
    /// ([`MAX_BUSINESS_DAYS`]), so likely the one.
    pub likely: bool,
}

impl Book {
    /// The Transfers to suggest to the user: pairs of transactions where one
    /// takes money out of one of the user's accounts and the other puts the
    /// same amount into another, in the same currency, at most three
    /// business days apart, and whose other parts have no category yet or
    /// were put in one by a Rule (D-021, D-030, D-691). A side the user
    /// sorted by hand is never suggested (D-031). See [`find_transfers`]
    /// for which pair wins where several fit: sides without a category are
    /// paired first, so a side a Rule sorted never takes their place.
    ///
    /// They are only suggestions: nothing is changed until the user confirms
    /// one with [`Book::join_transfer`]. One the user turns down drops out
    /// once a side gets a category by hand.
    pub fn transfer_suggestions(&self) -> Result<Vec<TransferMatch>, StoreError> {
        let accounts = self.account_ids()?;
        let marks = self.marks()?;
        // Only transactions with two parts, none in another currency, and
        // either one part without a category or one a Rule booked, can be
        // sides; they are read together, in full, as a book's whole list
        // is, not one by one (#84).
        let candidates = self.transactions_where(
            "WHERE t.id IN (SELECT p.transaction_id FROM splits AS p
                            LEFT JOIN split_rules AS m ON m.split_id = p.id
                            GROUP BY p.transaction_id
                            HAVING count(*) = 2 AND count(p.foreign_amount) = 0
                                   AND (count(p.account_id) = 1
                                        OR count(m.split_id) = 1))",
            &[],
        )?;
        let sides: Vec<TransferSide> = candidates
            .iter()
            .filter_map(|transaction| {
                let side = TransferSide::of(transaction, |id| accounts.contains(&id)).ok()?;
                match sorting(transaction, &side, &marks) {
                    SideSorting::Open | SideSorting::ByRule(_) => Some(side),
                    SideSorting::ByHand => None,
                }
            })
            .collect();
        Ok(find_transfers(&sides))
    }

    /// Who sorted the other part of `side` ([`SideSorting`]).
    pub fn side_sorting(&self, side: &TransferSide) -> Result<SideSorting, StoreError> {
        let transaction = self.transaction(side.transaction)?;
        Ok(sorting(&transaction, side, &self.marks()?))
    }

    /// The transactions the user may pick as the other side of a Transfer
    /// with `transaction` (D-691, D-692): sides in another of the user's
    /// accounts that take out or put in exactly the opposite amount, in the
    /// same currency, whoever sorted them and however far apart, the
    /// closest dates first (fewest business days, then calendar days, then
    /// the earlier date, then the IDs), at most [`MAX_TRANSFER_PARTNERS`].
    ///
    /// [`StoreError::Transfer`] if `transaction` itself can't be a side.
    pub fn transfer_partners(
        &self,
        transaction: TransactionId,
    ) -> Result<Vec<TransferPartner>, StoreError> {
        let chosen = self.transaction(transaction)?;
        let accounts = self.account_ids()?;
        let side =
            TransferSide::of(&chosen, |id| accounts.contains(&id)).map_err(StoreError::Transfer)?;
        let opposite = side
            .amount
            .checked_neg()
            .map_err(|_| StoreError::AmountOverflow)?;
        let marks = self.marks()?;
        let candidates = self.transactions_where(
            "WHERE t.currency = ?1
               AND t.id IN (SELECT transaction_id FROM splits
                            WHERE amount = ?2 AND account_id IS NOT NULL
                                  AND account_id != ?3 AND foreign_amount IS NULL)
               AND t.id != ?4",
            &[
                &opposite.currency().as_str(),
                &opposite.minor_units(),
                &side.account.to_bytes(),
                &transaction.to_bytes(),
            ],
        )?;
        let mut partners: Vec<(u32, TransferPartner)> = candidates
            .iter()
            .filter_map(|other| {
                let found = TransferSide::of(other, |id| accounts.contains(&id)).ok()?;
                if found.account == side.account || found.amount != opposite {
                    return None;
                }
                let (business_days, calendar_days) =
                    days_between(side.booking_date, found.booking_date);
                Some((
                    calendar_days,
                    TransferPartner {
                        side: found,
                        sorting: sorting(other, &found, &marks),
                        business_days,
                        likely: business_days <= MAX_BUSINESS_DAYS,
                    },
                ))
            })
            .collect();
        partners.sort_by(|(a_days, a), (b_days, b)| {
            (
                a.business_days,
                *a_days,
                a.side.booking_date,
                a.side.transaction,
            )
                .cmp(&(
                    b.business_days,
                    *b_days,
                    b.side.booking_date,
                    b.side.transaction,
                ))
        });
        Ok(partners
            .into_iter()
            .take(MAX_TRANSFER_PARTNERS)
            .map(|(_, partner)| partner)
            .collect())
    }

    /// Joins the transactions `a` and `b`, the two sides of a Transfer the
    /// user confirmed, into one transaction, and returns its ID (D-087,
    /// D-691).
    ///
    /// The transaction the money left from stays, with its date and payee,
    /// and gets the part that books the money into the other account, with
    /// what that account's bank delivered (D-083) and, as its own, the date
    /// and payee that account showed (D-108); the other transaction is
    /// removed. Both accounts' balances and registers stay as they were, and
    /// the two other parts go, with their categories and the Rule marks on
    /// them; what the user wrote on them moves or refuses the join
    /// ([`join_transfer`]). Both transactions' notes and attachments stay,
    /// on the joined one (D-706). It is one user action, which undo takes back as
    /// a whole, categories and Rule marks included.
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
            // Every original of both sides must be back afterwards, on the
            // same part (D-375).
            keeping_originals(&book.conn, &[a, b], || {
                // Its parts, with what their bank delivered, go with it; the
                // one booked to its account comes back in the joined
                // transaction.
                book.conn.execute(
                    "DELETE FROM transactions WHERE id = ?1",
                    [removed.to_bytes()],
                )?;
                // Both sides' notes and attachments stay with it (D-706).
                book.conn.execute(
                    "UPDATE transactions SET payee = ?2, note = ?3 WHERE id = ?1",
                    params![id, joined.payee(), joined.note()],
                )?;
                book.conn
                    .execute("DELETE FROM splits WHERE transaction_id = ?1", [id])?;
                insert_splits(&book.conn, &joined)?;
                book.conn
                    .execute("DELETE FROM attachments WHERE transaction_id = ?1", [id])?;
                insert_attachments(&book.conn, &joined)
            })
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

/// Who sorted the other part of `side`, a side of `transaction`, by the
/// book's Rule `marks`.
fn sorting(
    transaction: &Transaction,
    side: &TransferSide,
    marks: &HashMap<SplitId, Option<RuleId>>,
) -> SideSorting {
    let Some(category) = side.category else {
        return SideSorting::Open;
    };
    transaction
        .splits()
        .iter()
        .find(|split| split.account == Some(category))
        .and_then(|split| marks.get(&split.id))
        .map_or(SideSorting::ByHand, |rule| SideSorting::ByRule(*rule))
}

#[cfg(test)]
mod tests;
