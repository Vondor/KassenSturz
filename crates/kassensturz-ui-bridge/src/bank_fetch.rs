// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The statements fetched from the banks, waiting for the user to look at
//! them (M4.7, D-153).
//!
//! The bank worker thread fetches and converts (`bank_connection.rs`); the
//! import screens (`statement_import_model.rs`) run on the GUI thread. A
//! fetched statement goes from the first to the second through here, as a
//! [`Statement`] with exact amounts, never as text or numbers. Nothing here
//! is written anywhere: the statements live in memory until the user has
//! imported them, a new fetch for the account replaces (or, if it fails,
//! drops) the old one, or the app ends. They are keyed by the account's ID,
//! so a statement left behind in a book that was closed is never offered in
//! another.

use std::sync::{Mutex, PoisonError};

use kassensturz_core::{AccountId, Date};
use kassensturz_import::Statement;

/// A statement fetched for one account of the book.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fetch {
    /// The account in the book it was fetched for.
    pub account: AccountId,
    /// What the bank delivered, converted.
    pub statement: Statement,
    /// The day it was fetched, by this computer's clock: what the account's
    /// bank link notes when the user imports it.
    pub day: Date,
}

static WAITING: Mutex<Vec<Fetch>> = Mutex::new(Vec::new());

fn waiting() -> std::sync::MutexGuard<'static, Vec<Fetch>> {
    // A panic elsewhere must not lock the user out of the statements.
    WAITING.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Keeps `fetch` for its account, replacing one fetched earlier.
pub fn put(fetch: Fetch) {
    let mut waiting = waiting();
    waiting.retain(|other| other.account != fetch.account);
    waiting.push(fetch);
}

/// Hands over the statement waiting for `account`, if there is one. Once
/// taken it is gone: the user imports it or fetches again.
pub fn take(account: AccountId) -> Option<Fetch> {
    let mut waiting = waiting();
    let at = waiting.iter().position(|fetch| fetch.account == account)?;
    Some(waiting.remove(at))
}

#[cfg(test)]
mod tests {
    use kassensturz_core::{CurrencyCode, Date};
    use kassensturz_import::{Statement, StatementAccount};

    use super::*;

    fn statement() -> Statement {
        let eur = kassensturz_import::currency(CurrencyCode::new("EUR").expect("EUR"))
            .expect("a currency");
        Statement {
            account: StatementAccount::default(),
            currency: eur,
            opening_balance: None,
            closing_balance: None,
            entries: Vec::new(),
        }
    }

    fn fetch(account: AccountId, day: i8) -> Fetch {
        Fetch {
            account,
            statement: statement(),
            day: Date::new(2026, 10, day).expect("a date"),
        }
    }

    // The statements are shared by every test in the process, so the test
    // uses accounts of its own and never clears the lot.
    #[test]
    fn a_fetch_waits_until_it_is_taken_and_a_new_one_replaces_it() {
        let (first, second) = (AccountId::generate(), AccountId::generate());
        put(fetch(first, 1));
        put(fetch(second, 2));
        put(fetch(first, 3));
        assert_eq!(take(first).map(|fetch| fetch.day.day()), Some(3));
        assert_eq!(take(first), None, "taken once");
        assert_eq!(take(second).map(|fetch| fetch.day.day()), Some(2));
    }
}
