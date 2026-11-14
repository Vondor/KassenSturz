// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! How much a book may hold in one currency (B-269, #965).
//!
//! Each amount is checked on its own when it is entered, but several large
//! ones could add up past what 64 bits hold: the balances, the register,
//! the Overview and the forecast then failed to read the book. So every
//! change that writes parts of transactions is checked as a whole before
//! it is kept, whichever way it came in (the window, the command line, an
//! import, a bank fetch, the AI, a complete export): per currency, the
//! amounts **without their signs** must add up to at most
//! [`MAX_CURRENCY_TOTAL`]. Every balance, running balance and total of any
//! accounts or categories in that currency is then at most that, in any
//! order, and the forecast still has room for the planned payments
//! ([`MAX_PLAN_AMOUNT`]).

use kassensturz_core::{MAX_DUE_YEARS, MAX_SCHEDULES};
use rusqlite::{Connection, ErrorCode};

use crate::cached::Cached;
use crate::error::StoreError;

/// The most the amounts of a book in one currency may add up to without
/// their signs, in minor units: a quarter of what 64 bits hold, about 23
/// million billion euros, so a forecast can add the planned payments to a
/// balance and still fit.
pub const MAX_CURRENCY_TOTAL: i64 = i64::MAX / 4;

/// The largest amount one planned payment may have, in minor units: so
/// large that the most plans a book holds, each due every week for the
/// years a forecast looks at, still add up to at most
/// [`MAX_CURRENCY_TOTAL`] (about 435 billion euros).
pub const MAX_PLAN_AMOUNT: i64 =
    MAX_CURRENCY_TOTAL / (MAX_SCHEDULES as i64 * (MAX_DUE_YEARS as i64 * 53));

/// Fails with [`StoreError::AmountOverflow`] if the amounts of a currency,
/// in the transactions' own currency, in the currency of an account in
/// another one (D-043) and in a withdrawn currency they were converted
/// from (D-077), add up to more than [`MAX_CURRENCY_TOTAL`] without their
/// signs.
pub(crate) fn check_totals(conn: &Connection) -> Result<(), StoreError> {
    let largest: Result<Option<i64>, rusqlite::Error> = conn.query_row_cached(
        "SELECT max(total) FROM (
             SELECT sum(part) AS total FROM (
                 SELECT t.currency AS code, abs(s.amount) AS part
                 FROM main.splits s JOIN main.transactions t ON t.id = s.transaction_id
                 UNION ALL
                 SELECT foreign_currency, abs(foreign_amount) FROM main.splits
                 WHERE foreign_currency IS NOT NULL
                 UNION ALL
                 SELECT original_currency, abs(original_amount) FROM main.splits
                 WHERE original_currency IS NOT NULL
             ) GROUP BY code
         )",
        [],
        |row| row.get(0),
    );
    match largest {
        Ok(Some(total)) if total > MAX_CURRENCY_TOTAL => Err(StoreError::AmountOverflow),
        Ok(_) => Ok(()),
        // SQLite's sum() and abs() refuse a result past 64 bits.
        Err(error) if is_integer_overflow(&error) => Err(StoreError::AmountOverflow),
        Err(error) => Err(error.into()),
    }
}

/// Whether SQLite stopped because a sum or `abs` left 64 bits.
fn is_integer_overflow(error: &rusqlite::Error) -> bool {
    match error {
        rusqlite::Error::SqliteFailure(failure, message) => {
            failure.code == ErrorCode::Unknown
                && message
                    .as_deref()
                    .is_some_and(|message| message.contains("integer overflow"))
        }
        _ => false,
    }
}

/// Whether the user action under way wrote parts of transactions, so
/// [`check_totals`] has something to look at.
pub(crate) fn splits_changed(conn: &Connection) -> Result<bool, StoreError> {
    Ok(conn.query_row_cached(
        "SELECT EXISTS (SELECT 1 FROM temp.staged_splits)",
        [],
        |row| row.get(0),
    )?)
}
