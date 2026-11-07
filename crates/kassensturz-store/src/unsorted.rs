// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The transactions that have no category yet, across the whole book (US1.9,
//! #535). The Overview's note about them reads the count and the newest few.
//!
//! A transaction counts when at least one of its parts has no category and
//! it is not a Transfer, i.e. fewer than two of its parts are booked to the
//! user's own accounts. The user's own accounts are the `asset` and
//! `liability` ones; a part booked to a category or to the equity account of
//! a starting balance is never "without a category".

use kassensturz_core::{Transaction, TransactionId};

use crate::Book;
use crate::cached::Cached;
use crate::error::StoreError;

/// Which transactions are without a category: the `WHERE` clause over
/// `transactions AS t`, shared by the count and the list so the two always
/// agree.
const UNSORTED: &str = "
    EXISTS (SELECT 1 FROM splits AS s
            WHERE s.transaction_id = t.id AND s.account_id IS NULL)
    AND (SELECT count(*) FROM splits AS o
         JOIN accounts AS a ON a.id = o.account_id
         WHERE o.transaction_id = t.id
           AND a.kind IN ('asset', 'liability')) < 2";

impl Book {
    /// How many transactions in the whole book have a part without a
    /// category, Transfers between the user's own accounts left out.
    pub fn unsorted_count(&self) -> Result<usize, StoreError> {
        let count: i64 = self.conn.query_row_cached(
            &format!("SELECT count(*) FROM transactions AS t WHERE {UNSORTED}"),
            [],
            |row| row.get(0),
        )?;
        Ok(usize::try_from(count).unwrap_or(usize::MAX))
    }

    /// The newest `limit` of those transactions, newest first (booking date,
    /// then ID, descending), each checked as [`Book::transaction`] checks it.
    pub fn newest_unsorted(&self, limit: usize) -> Result<Vec<Transaction>, StoreError> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut statement = self.conn.prepare_cached(&format!(
            "SELECT t.id FROM transactions AS t WHERE {UNSORTED}
             ORDER BY t.booking_date DESC, t.id DESC LIMIT ?1"
        ))?;
        let ids: Vec<TransactionId> = statement
            .query_map([limit], |row| Ok(TransactionId::from_bytes(row.get(0)?)))?
            .collect::<Result<_, _>>()?;
        drop(statement);
        ids.into_iter().map(|id| self.transaction(id)).collect()
    }
}

#[cfg(test)]
mod tests {
    use kassensturz_core::{AccountKind, Split, Transaction, TransactionId};

    use crate::tests::{EUR, Sample, account, date, eur, sample};

    fn uncategorised(
        day: i8,
        payee: &str,
        account: kassensturz_core::AccountId,
        cents: i64,
    ) -> Transaction {
        Transaction::new(
            TransactionId::generate(),
            date(2026, 10, day),
            Some(payee.into()),
            vec![
                Split::new(account, eur(-cents)),
                Split::uncategorized(eur(cents)),
            ],
        )
        .expect("balanced")
    }

    #[test]
    fn only_transactions_with_a_part_without_a_category_count() {
        let Sample {
            mut book,
            checking,
            groceries,
            ..
        } = sample();
        assert_eq!(book.unsorted_count().expect("count"), 0);
        assert!(book.newest_unsorted(5).expect("list").is_empty());

        let sorted = Transaction::new(
            TransactionId::generate(),
            date(2026, 10, 1),
            Some("Corner Grocery".into()),
            vec![
                Split::new(checking, eur(-500)),
                Split::new(groceries, eur(500)),
            ],
        )
        .expect("balanced");
        book.add_transaction(&sorted).expect("added");
        let first = uncategorised(2, "Kiosk", checking, 300);
        let second = uncategorised(9, "Bakery", checking, 450);
        book.add_transaction(&first).expect("added");
        book.add_transaction(&second).expect("added");

        assert_eq!(book.unsorted_count().expect("count"), 2);
        let newest = book.newest_unsorted(5).expect("list");
        assert_eq!(
            newest.iter().map(Transaction::id).collect::<Vec<_>>(),
            [second.id(), first.id()],
            "newest first"
        );
        let one = book.newest_unsorted(1).expect("list");
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].id(), second.id(), "the limit keeps the newest");
    }

    #[test]
    fn a_joined_transfer_is_not_counted_but_the_sides_of_an_unjoined_one_are() {
        let Sample {
            mut book, checking, ..
        } = sample();
        let savings = account(&mut book, "Savings", AccountKind::Asset, EUR);
        let transfer = Transaction::new(
            TransactionId::generate(),
            date(2026, 10, 3),
            Some("To savings".into()),
            vec![
                Split::new(checking, eur(-1000)),
                Split::new(savings, eur(1000)),
            ],
        )
        .expect("balanced");
        book.add_transaction(&transfer).expect("added");
        // The two sides of a Transfer that is not joined yet are two
        // transactions without a category: they are still to be sorted.
        book.add_transaction(&uncategorised(4, "Out", checking, 700))
            .expect("added");
        book.add_transaction(&uncategorised(4, "In", savings, -700))
            .expect("added");
        assert_eq!(book.unsorted_count().expect("count"), 2);
    }
}
