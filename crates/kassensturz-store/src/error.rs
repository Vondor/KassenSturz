// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Errors. They are structured codes with parameters; the UI turns them into
//! translated messages (D-034).

use std::io::ErrorKind;

use kassensturz_core::{
    AccountId, AccountKind, AccountType, CurrencyCode, Date, DatedRateId, Money, TransactionError,
    TransactionId,
};

use crate::Lock;

/// Why a book couldn't be opened, read or changed.
#[derive(Debug)]
pub enum StoreError {
    /// Creating a book failed because something is at the path already. It was
    /// left untouched.
    AlreadyExists,
    /// Opening a book failed because there's no file at the path.
    NotFound,
    /// The file isn't a KassenSturz book. It was left untouched.
    NotABook,
    /// The book's layout is a different version: it was made by a newer
    /// KassenSturz, or by a development build from before 0.1 (D-045).
    UnsupportedLayout {
        /// The layout version stored in the file.
        found: i32,
    },
    /// The book's lock file says it is open somewhere else: on another
    /// computer or in another running KassenSturz, or it wasn't closed
    /// properly there, e.g. after a crash (D-038). After warning the user, the
    /// UI can open it anyway with [`Book::open_anyway`](crate::Book::open_anyway).
    Locked(Lock),
    /// The automatic backup before an update of the book's layout failed, so
    /// the book wasn't changed (AGENTS.md §3.1 rule 8, D-033).
    BackupFailed(Box<StoreError>),
    /// SQLite wouldn't switch the book to the rollback journal (D-033).
    JournalModeRefused {
        /// The journal mode SQLite kept.
        mode: String,
    },
    /// The currency isn't in KassenSturz's ISO 4217 table, so its number of
    /// decimal places isn't known (D-062): it isn't a current currency, or
    /// ISO 4217 gives it no decimal places, as for gold (XAU).
    UnlistedCurrency(CurrencyCode),
    /// The currency isn't used in this book yet.
    UnknownCurrency(CurrencyCode),
    /// There's no account or category with this ID.
    UnknownAccount(AccountId),
    /// The account's name is empty.
    EmptyAccountName,
    /// Another account already has this name, ignoring capitals. The user
    /// can pick another name.
    DuplicateAccountName {
        /// The account that has it.
        existing: AccountId,
    },
    /// The account's type doesn't fit its kind, e.g. a credit card that
    /// isn't money the user owes.
    AccountTypeMismatch {
        /// The type.
        account_type: AccountType,
        /// The kind it was given.
        kind: AccountKind,
    },
    /// The ID belongs to a category or an internal account, not to an
    /// account the user sees.
    NotAnAccount(AccountId),
    /// A closed account takes no new transactions. The user can open it
    /// again.
    AccountClosed(AccountId),
    /// An account can only be closed when its balance is zero.
    AccountNotEmpty {
        /// The account.
        account: AccountId,
        /// What is left in it.
        balance: Money,
    },
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
    /// There's no exchange rate with this ID.
    UnknownRate(DatedRateId),
    /// An exchange rate from a currency into the same currency.
    RateWithinOneCurrency(CurrencyCode),
    /// The book already has a rate between these currencies, in this
    /// direction, on that date. The user can change or remove that one.
    DuplicateRate {
        /// The rate already in the book.
        existing: DatedRateId,
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
    /// An account type that doesn't exist.
    BadAccountType(String),
    /// An account whose type doesn't fit its kind.
    AccountTypeMismatch(AccountId),
    /// A currency code that isn't three capital letters.
    BadCurrencyCode(String),
    /// A date that isn't a valid ISO 8601 calendar date.
    BadDate(String),
    /// A currency's number of decimal places is out of range.
    BadMinorDigits(i64),
    /// An exchange rate whose digits or decimal places are out of range.
    BadRate(DatedRateId),
    /// An exchange rate's source that doesn't exist.
    BadRateSource(String),
    /// The book's settings are missing.
    MissingSettings,
    /// A rounding mode that doesn't exist.
    BadRoundingMode(String),
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
    /// Something refers to a row that doesn't exist, e.g. a part to a missing
    /// account. Checked when the book's layout is updated.
    BrokenReferences,
}

/// An error reported by SQLite. Its details are for logs, not for users.
#[derive(Debug)]
pub struct DatabaseError(
    #[expect(dead_code, reason = "only read through Debug, for logs")] rusqlite::Error,
);

impl From<std::io::Error> for StoreError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.kind())
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        match error.sqlite_error_code() {
            Some(rusqlite::ErrorCode::NotADatabase) => Self::NotABook,
            _ => Self::Database(DatabaseError(error)),
        }
    }
}
