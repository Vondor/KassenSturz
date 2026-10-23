// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Statements that run on every change, kept prepared (Q-084): reading a
//! statement's text costs more than running it, and a change to the book
//! runs dozens. The statements are the same text every time, so SQLite's
//! own check against schema changes still applies.

use rusqlite::{Connection, Params, Row};

/// How many statements a book's connection keeps. The default of 16 is
/// fewer than one change uses, which would throw them out again.
pub(crate) const CAPACITY: usize = 128;

/// `execute` and `query_row` that keep the statement for the next time.
pub(crate) trait Cached {
    /// Like [`Connection::execute`].
    fn execute_cached<P: Params>(&self, sql: &str, params: P) -> rusqlite::Result<usize>;

    /// Like [`Connection::query_row`].
    fn query_row_cached<T, P, F>(&self, sql: &str, params: P, map: F) -> rusqlite::Result<T>
    where
        P: Params,
        F: FnOnce(&Row<'_>) -> rusqlite::Result<T>;
}

impl Cached for Connection {
    fn execute_cached<P: Params>(&self, sql: &str, params: P) -> rusqlite::Result<usize> {
        self.prepare_cached(sql)?.execute(params)
    }

    fn query_row_cached<T, P, F>(&self, sql: &str, params: P, map: F) -> rusqlite::Result<T>
    where
        P: Params,
        F: FnOnce(&Row<'_>) -> rusqlite::Result<T>,
    {
        self.prepare_cached(sql)?.query_row(params, map)
    }
}
