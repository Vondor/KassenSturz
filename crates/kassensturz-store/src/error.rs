// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Errors. They are structured codes with parameters; the UI turns them into
//! translated messages (D-034).

use std::io::ErrorKind;

use kassensturz_core::{AccountId, CurrencyCode, Date, TransactionError, TransactionId};

/// Why a book couldn't be opened, read or changed.
#[derive(Debug)]
pub enum StoreError {
    /// Creating a book failed because something is at the path already. It was
    /// left untouched.
    AlreadyExists,
    /// Opening a book failed because there's no file at the path.
    NotFound,
    /// The file isn't a KassenSturz book.
    NotABook,
    /// The book's layout is a different version: it was made by a newer
    /// KassenSturz, or by a development build from before 0.1 (D-045).
    UnsupportedLayout {
        /// The layout version stored in the file.
        found: i32,
    },
    /// SQLite wouldn't switch the book to the rollback journal (D-033).
    JournalModeRefused {
        /// The journal mode SQLite kept.
        mode: String,
    },
    /// A currency can have at most 18 decimal places.
    InvalidMinorDigits(u8),
    /// The currency isn't used in this book yet.
    UnknownCurrency(CurrencyCode),
    /// The currency is already used with other decimal places, and those stay
    /// fixed (D-027).
    CurrencyDigitsConflict {
        /// The currency.
        currency: CurrencyCode,
        /// The decimal places stored in the book.
        stored: u8,
        /// The decimal places that were asked for.
        requested: u8,
    },
    /// There's no account or category with this ID.
    UnknownAccount(AccountId),
    /// There's no transaction with this ID.
    UnknownTransaction(TransactionId),
    /// A part is booked to an account whose currency differs from the
    /// transaction's. Parts in other currencies come with D-043.
    SplitCurrencyMismatch {
        /// The account the part is booked to.
        account: AccountId,
        /// The account's currency.
        account_currency: CurrencyCode,
        /// The transaction's currency.
        transaction_currency: CurrencyCode,
    },
    /// Dates before the year 1 can't be stored.
    DateOutOfRange(Date),
    /// Adding up amounts gave more than 64 bits can hold.
    AmountOverflow,
    /// Data in the book breaks one of KassenSturz's rules, for example because
    /// it was changed outside KassenSturz (D-046).
    Corrupt(Corruption),
    /// The file system reported an error.
    Io(ErrorKind),
    /// SQLite reported an error.
    Database(DatabaseError),
}

/// What's wrong with data read from a book.
#[derive(Debug)]
pub enum Corruption {
    /// An account kind that doesn't exist.
    BadAccountKind(String),
    /// A currency code that isn't three capital letters.
    BadCurrencyCode(String),
    /// A date that isn't a valid ISO 8601 calendar date.
    BadDate(String),
    /// A currency's number of decimal places is out of range.
    BadMinorDigits(i64),
    /// A part's currency differs from its account's.
    CurrencyMismatch {
        /// The transaction the part belongs to.
        transaction: TransactionId,
    },
    /// A transaction's parts don't form a valid transaction, e.g. they don't
    /// add up to zero.
    InvalidTransaction {
        /// The transaction.
        transaction: TransactionId,
        /// What's wrong with it.
        error: TransactionError,
    },
}

/// An error reported by SQLite. Its details are for logs, not for users.
#[derive(Debug)]
pub struct DatabaseError(
    #[expect(dead_code, reason = "only read through Debug, for logs")] rusqlite::Error,
);

impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        match error.sqlite_error_code() {
            Some(rusqlite::ErrorCode::NotADatabase) => Self::NotABook,
            _ => Self::Database(DatabaseError(error)),
        }
    }
}
