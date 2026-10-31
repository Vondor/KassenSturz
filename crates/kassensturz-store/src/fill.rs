// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Filling a new book in one go, e.g. from a complete export (D-029, D-046):
//! as part of making the book, so the change history starts empty.

use kassensturz_core::{Currency, RuleId, SplitId};
use rusqlite::params;

use crate::error::StoreError;
use crate::{Action, Book, HistoryRetention};

impl Book {
    /// Whether nothing is in the book yet: no currency, account, rate,
    /// Rule, transaction, institution, payee, tag, view, dashboard, category
    /// group, hidden category or budget, and no change in its history. Only
    /// such a book can be filled ([`Book::fill_new`]); one after the app's first start
    /// has the household's currency and categories, so it is not empty.
    pub fn is_empty(&self) -> Result<bool, StoreError> {
        self.conn
            .query_row(
                "SELECT NOT (
                     EXISTS (SELECT 1 FROM main.currencies)
                     OR EXISTS (SELECT 1 FROM main.accounts)
                     OR EXISTS (SELECT 1 FROM main.prices)
                     OR EXISTS (SELECT 1 FROM main.rules)
                     OR EXISTS (SELECT 1 FROM main.transactions)
                     OR EXISTS (SELECT 1 FROM main.institutions)
                     OR EXISTS (SELECT 1 FROM main.payees)
                     OR EXISTS (SELECT 1 FROM main.tags)
                     OR EXISTS (SELECT 1 FROM main.views)
                     OR EXISTS (SELECT 1 FROM main.report_groups)
                     OR EXISTS (SELECT 1 FROM main.report_hidden)
                     OR EXISTS (SELECT 1 FROM main.budgets)
                     OR EXISTS (SELECT 1 FROM main.changes)
                     OR EXISTS (SELECT 1 FROM temp.changes))",
                [],
                |row| row.get(0),
            )
            .map_err(StoreError::from)
    }

    /// Fills a new, empty book in one go, e.g. with a complete export read
    /// back (D-029, D-046), and sets how long it keeps its change history
    /// (D-066).
    ///
    /// `fill` adds what the book holds through the usual functions, e.g.
    /// [`Book::add_account`] and [`Book::add_transaction`], and every one of
    /// their checks applies. What it does is part of making the book, like
    /// the creation of the book itself: it isn't recorded in the change
    /// history, which stays empty, since undoing it would only leave an empty
    /// book. While it runs, [`Book::restore_currency`] and
    /// [`Book::restore_rule_mark`] put back what a book can't get otherwise.
    ///
    /// If `fill` fails, everything it did is taken back and the book stays
    /// empty. Fails with [`StoreError::NotEmpty`] if the book holds anything
    /// already: a currency, account, rate, Rule or transaction, or a change
    /// in its history.
    ///
    /// # Panics
    ///
    /// Inside [`Book::as_one_action`] or another call of this function.
    pub fn fill_new<T, E>(
        &mut self,
        keep: HistoryRetention,
        fill: impl FnOnce(&mut Self) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<StoreError>,
    {
        assert!(
            !self.recording && !self.filling,
            "filling a new book can't be part of a user action"
        );
        let empty = self.is_empty()?;
        if !empty {
            return Err(StoreError::NotEmpty.into());
        }
        let days = match keep {
            HistoryRetention::Days(days) => Some(i64::from(days)),
            HistoryRetention::Forever => None,
        };
        self.in_savepoint(|book| {
            // The history is empty, so there is nothing to move between the
            // book and memory, as `set_history_retention` would.
            book.conn
                .execute(
                    "UPDATE main.history_settings SET keep_days = ?1 WHERE id = 1",
                    [days],
                )
                .map_err(StoreError::from)?;
            // Changes are allowed, but not recorded, as when undo writes rows
            // back.
            book.conn
                .execute("INSERT INTO temp.history_state (recording) VALUES (0)", [])
                .map_err(StoreError::from)?;
            book.recording = true;
            book.filling = true;
            let filled = fill(book);
            book.recording = false;
            book.filling = false;
            let value = filled?;
            book.conn
                .execute("DELETE FROM temp.history_state", [])
                .map_err(StoreError::from)?;
            Ok(value)
        })
    }

    /// Starts using `currency` with exactly its decimal places, as the book a
    /// complete export was made from stored them (D-027). Unlike
    /// [`Book::use_currency`], KassenSturz's ISO 4217 table isn't asked, so a
    /// currency keeps its decimal places even if a newer table changed them
    /// or no longer lists it. Each currency can be restored once.
    ///
    /// # Panics
    ///
    /// Outside [`Book::fill_new`].
    pub fn restore_currency(&mut self, currency: Currency) -> Result<(), StoreError> {
        assert!(
            self.filling,
            "only a new book being filled takes currencies this way"
        );
        self.as_one_action(Action::UseCurrency, |book| {
            book.conn.execute(
                "INSERT INTO currencies (code, minor_digits) VALUES (?1, ?2)",
                params![currency.code().as_str(), currency.minor_digits()],
            )?;
            Ok(())
        })
    }

    /// Marks the part `split` as booked by `rule`, or by a Rule removed
    /// since if `rule` is `None`, as the book a complete export was made from
    /// had it marked (D-088, [`RuleMark`](crate::RuleMark)). The part and the
    /// Rule must be in the book.
    ///
    /// # Panics
    ///
    /// Outside [`Book::fill_new`].
    pub fn restore_rule_mark(
        &mut self,
        split: SplitId,
        rule: Option<RuleId>,
    ) -> Result<(), StoreError> {
        assert!(
            self.filling,
            "only a new book being filled takes marks this way"
        );
        if let Some(rule) = rule {
            self.rule(rule)?;
        }
        self.as_one_action(Action::RunRules, |book| {
            book.conn.execute(
                "INSERT INTO split_rules (split_id, rule_id) VALUES (?1, ?2)",
                params![split.to_bytes(), rule.map(RuleId::to_bytes)],
            )?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests;
