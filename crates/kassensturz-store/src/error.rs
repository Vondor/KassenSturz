// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Errors. They are structured codes with parameters; the UI turns them into
//! translated messages (D-034). [`Report`] gives each one's code and
//! parameters, listed in `docs/ERROR_CODES.md`.

use std::io::ErrorKind;

use kassensturz_core::{
    AccountId, AccountKind, AccountType, CurrencyCode, Date, DatedRateId, ErrorParam, ErrorReport,
    Money, Report, RuleError, RuleId, SplitId, TransactionError, TransactionId, TransferError,
};

use crate::Lock;
use crate::layout;

/// Why a book couldn't be opened, read or changed.
#[derive(Debug)]
pub enum StoreError {
    /// Creating a book failed because something is at the path already. It was
    /// left untouched.
    AlreadyExists {
        /// The path of the file, made absolute.
        file: String,
    },
    /// Opening a book failed because there's no file at the path.
    NotFound,
    /// Only a new, empty book can be filled in one go
    /// ([`Book::fill_new`](crate::Book::fill_new)), e.g. with a complete
    /// export, and this one holds something already. It wasn't changed.
    NotEmpty,
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
    /// The currency was withdrawn: another one replaced it at a fixed rate,
    /// as the euro replaced the German mark, so no new account can be in it
    /// (D-104). Its amounts are kept only as parts' original amounts.
    WithdrawnCurrency(CurrencyCode),
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
    /// Only a category can sit inside another one; an account can't.
    NotACategory(AccountId),
    /// The category to put something inside isn't a category (it is an
    /// account or an internal account).
    ParentNotACategory(AccountId),
    /// Income can only sit inside income, and spending inside spending.
    ParentKindMismatch {
        /// The category to sit inside.
        parent: AccountId,
        /// Its kind.
        parent_kind: AccountKind,
        /// The kind of the category that was to sit inside it.
        kind: AccountKind,
    },
    /// A category sits in the same currency as the category it is inside, so
    /// the totals of a category add up.
    ParentCurrencyMismatch {
        /// The category to sit inside.
        parent: AccountId,
        /// Its currency.
        parent_currency: CurrencyCode,
        /// The currency of the category that was to sit inside it.
        currency: CurrencyCode,
    },
    /// A category can't sit inside itself or inside one of its own
    /// subcategories.
    CategoryInsideItself(AccountId),
    /// There's no transaction with this ID.
    UnknownTransaction(TransactionId),
    /// A transaction with this ID is in the book already.
    TransactionIdInUse(TransactionId),
    /// A part with this ID belongs to another transaction already.
    SplitIdInUse(SplitId),
    /// A part is booked to an account in another currency than the part's:
    /// the transaction's currency, or the one of its foreign amount. A part
    /// booked to an account in another currency than the transaction's
    /// needs its amount in that currency as well (D-043).
    SplitCurrencyMismatch {
        /// The account the part is booked to.
        account: AccountId,
        /// The account's currency.
        account_currency: CurrencyCode,
        /// The part's currency.
        split_currency: CurrencyCode,
    },
    /// A change would alter a part that a bank or file delivered: its
    /// account, its amount or what was delivered with it, or it would add
    /// or remove such a part (D-067). The user can change the other parts.
    ImportedSplitChanged(SplitId),
    /// A change would alter a part converted from a withdrawn currency: its
    /// original amount, the fixed rate or the amount it was converted into,
    /// or it would remove such a part, give a part an original amount, or
    /// book it to an account in another currency (D-104). Its account can
    /// change to one in the same currency.
    OriginalChanged(SplitId),
    /// The two transactions can't be joined into a Transfer (D-087).
    Transfer(TransferError),
    /// There's no Rule with this ID.
    UnknownRule(RuleId),
    /// A Rule with this ID is in the book already.
    RuleIdInUse(RuleId),
    /// The Rule's name is empty.
    EmptyRuleName,
    /// Another Rule in the same place, the top level or inside the same
    /// Rule, already has this name, ignoring capitals.
    DuplicateRuleName {
        /// The Rule that has it.
        existing: RuleId,
    },
    /// A Rule can't sit inside itself or inside one of its own sub-Rules.
    RuleInsideItself(RuleId),
    /// A Rule books transactions to a category, but this is an account or
    /// an internal account. To move money into one of the user's accounts,
    /// a Rule makes a Transfer instead (D-031).
    RuleNeedsCategory(AccountId),
    /// The Rule can't be used as it is (D-014).
    Rule(RuleError),
    /// Running the Rules left this transaction alone, since it looks like
    /// one side of a Transfer, which the user decides on first (D-030,
    /// D-088).
    TransferSuggested(TransactionId),
    /// There's no exchange rate with this ID.
    UnknownRate(DatedRateId),
    /// An exchange rate from a currency into the same currency.
    RateWithinOneCurrency(CurrencyCode),
    /// A fixed rate a withdrawn currency was replaced at
    /// ([`RateSource::Changeover`](kassensturz_core::RateSource::Changeover))
    /// belongs to the parts converted with it, not to the price table
    /// (D-104).
    ChangeoverRate,
    /// The book has no exchange rate between the two currencies on or before
    /// the date, in either direction. The user can enter one (D-028).
    NoRate {
        /// The currency of the amount to convert.
        from: CurrencyCode,
        /// The currency to convert it into.
        to: CurrencyCode,
        /// The date of the conversion.
        date: Date,
    },
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
    /// A category whose place in the tree is wrong: it sits inside something
    /// that isn't a category of the same kind and currency, or inside
    /// itself.
    BadCategoryTree(AccountId),
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
    /// A part whose columns don't fit together: a foreign amount without
    /// its currency or the other way round, or a rate for a part that isn't
    /// in another currency.
    BadSplit(SplitId),
    /// A transaction's parts don't form a valid transaction, e.g. they don't
    /// add up to zero.
    InvalidTransaction {
        /// The transaction.
        transaction: TransactionId,
        /// What's wrong with it.
        error: TransactionError,
    },
    /// Something refers to a row that doesn't exist, e.g. a part to a missing
    /// account. Checked when the book's layout is updated, and for the
    /// accounts of a transaction when it is read.
    BrokenReferences,
    /// The change history names an action that doesn't exist.
    BadAction(String),
    /// A time in the change history that isn't a valid ISO 8601 time in UTC,
    /// in whole seconds.
    BadTimestamp(String),
    /// How long the book keeps its change history is out of range: a number
    /// of days below 0 or above what fits in 32 bits.
    BadHistoryRetention(i64),
    /// The book doesn't hold what the change history says a change left in
    /// it, or writing the change's values back would break the book's rules,
    /// e.g. because another program changed the book (D-066). The change
    /// can't be undone or redone; nothing was changed.
    HistoryMismatch,
    /// A Rule or one of its conditions has values that don't fit together,
    /// e.g. a field it can't compare in this way.
    BadRule(RuleId),
    /// A Rule sits in the wrong place in the tree of Rules: inside a Rule
    /// that doesn't exist, or inside itself.
    BadRuleTree(RuleId),
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

impl Report for StoreError {
    fn report(&self) -> ErrorReport {
        use ErrorParam as P;
        let new = ErrorReport::new;
        match self {
            Self::AlreadyExists { file } => {
                new("store.already-exists").with("file", P::Text(file.clone()))
            }
            Self::NotFound => new("store.not-found"),
            Self::NotEmpty => new("store.book-not-empty"),
            Self::NotABook => new("store.not-a-book"),
            Self::UnsupportedLayout { found } => new("store.unsupported-layout")
                .with("found", P::Number((*found).into()))
                .with("current", P::Number(layout::VERSION.into())),
            Self::Locked(lock) => {
                let mut report = new("store.locked");
                if let Some(computer) = &lock.computer {
                    report = report.with("computer", P::Text(computer.clone()));
                }
                if let Some(since) = lock.since {
                    report = report.with("since", P::Timestamp(since));
                }
                report
            }
            Self::BackupFailed(cause) => new("store.backup-failed").caused_by(cause.report()),
            Self::JournalModeRefused { mode } => {
                new("store.journal-mode-refused").with("mode", P::Text(mode.clone()))
            }
            Self::UnlistedCurrency(currency) => {
                new("store.unlisted-currency").with("currency", P::Currency(*currency))
            }
            Self::WithdrawnCurrency(currency) => {
                new("store.withdrawn-currency").with("currency", P::Currency(*currency))
            }
            Self::UnknownCurrency(currency) => {
                new("store.unknown-currency").with("currency", P::Currency(*currency))
            }
            Self::UnknownAccount(account) => {
                new("store.unknown-account").with("account", P::Account(*account))
            }
            Self::EmptyAccountName => new("store.empty-account-name"),
            Self::DuplicateAccountName { existing } => {
                new("store.duplicate-account-name").with("existing", P::Account(*existing))
            }
            Self::AccountTypeMismatch { account_type, kind } => new("store.account-type-mismatch")
                .with("account_type", P::AccountType(*account_type))
                .with("kind", P::AccountKind(*kind)),
            Self::NotAnAccount(account) => {
                new("store.not-an-account").with("account", P::Account(*account))
            }
            Self::AccountClosed(account) => {
                new("store.account-closed").with("account", P::Account(*account))
            }
            Self::AccountNotEmpty { account, balance } => new("store.account-not-empty")
                .with("account", P::Account(*account))
                .with("balance", P::Money(*balance)),
            Self::NotACategory(account) => {
                new("store.not-a-category").with("account", P::Account(*account))
            }
            Self::ParentNotACategory(parent) => {
                new("store.parent-not-a-category").with("parent", P::Account(*parent))
            }
            Self::ParentKindMismatch {
                parent,
                parent_kind,
                kind,
            } => new("store.parent-kind-mismatch")
                .with("parent", P::Account(*parent))
                .with("parent_kind", P::AccountKind(*parent_kind))
                .with("kind", P::AccountKind(*kind)),
            Self::ParentCurrencyMismatch {
                parent,
                parent_currency,
                currency,
            } => new("store.parent-currency-mismatch")
                .with("parent", P::Account(*parent))
                .with("parent_currency", P::Currency(*parent_currency))
                .with("currency", P::Currency(*currency)),
            Self::CategoryInsideItself(account) => {
                new("store.category-inside-itself").with("account", P::Account(*account))
            }
            Self::UnknownTransaction(transaction) => {
                new("store.unknown-transaction").with("transaction", P::Transaction(*transaction))
            }
            Self::TransactionIdInUse(transaction) => {
                new("store.transaction-id-in-use").with("transaction", P::Transaction(*transaction))
            }
            Self::SplitIdInUse(split) => {
                new("store.split-id-in-use").with("split", P::Split(*split))
            }
            Self::SplitCurrencyMismatch {
                account,
                account_currency,
                split_currency,
            } => new("store.split-currency-mismatch")
                .with("account", P::Account(*account))
                .with("account_currency", P::Currency(*account_currency))
                .with("split_currency", P::Currency(*split_currency)),
            Self::ImportedSplitChanged(split) => {
                new("store.imported-split-changed").with("split", P::Split(*split))
            }
            Self::OriginalChanged(split) => {
                new("store.original-changed").with("split", P::Split(*split))
            }
            // The transfer's own error says what doesn't fit.
            Self::Transfer(error) => error.report(),
            Self::UnknownRule(rule) => new("store.unknown-rule").with("rule", P::Rule(*rule)),
            Self::RuleIdInUse(rule) => new("store.rule-id-in-use").with("rule", P::Rule(*rule)),
            Self::EmptyRuleName => new("store.empty-rule-name"),
            Self::DuplicateRuleName { existing } => {
                new("store.duplicate-rule-name").with("existing", P::Rule(*existing))
            }
            Self::RuleInsideItself(rule) => {
                new("store.rule-inside-itself").with("rule", P::Rule(*rule))
            }
            Self::RuleNeedsCategory(account) => {
                new("store.rule-needs-category").with("account", P::Account(*account))
            }
            // The Rule's own error says what is wrong with it.
            Self::Rule(error) => error.report(),
            Self::TransferSuggested(transaction) => {
                new("store.transfer-suggested").with("transaction", P::Transaction(*transaction))
            }
            Self::UnknownRate(rate) => new("store.unknown-rate").with("rate", P::Rate(*rate)),
            Self::RateWithinOneCurrency(currency) => {
                new("store.rate-within-one-currency").with("currency", P::Currency(*currency))
            }
            Self::ChangeoverRate => new("store.changeover-rate"),
            Self::NoRate { from, to, date } => new("store.no-rate")
                .with("from", P::Currency(*from))
                .with("to", P::Currency(*to))
                .with("date", P::Date(*date)),
            Self::DuplicateRate { existing } => {
                new("store.duplicate-rate").with("existing", P::Rate(*existing))
            }
            Self::DateOutOfRange(date) => {
                new("store.date-out-of-range").with("date", P::Date(*date))
            }
            Self::AmountOverflow => new("store.amount-overflow"),
            Self::Corrupt(corruption) => corruption.report(),
            Self::Io(kind) => new("store.io").with("kind", P::Keyword(io_keyword(*kind))),
            Self::Database(_) => new("store.database"),
        }
    }
}

impl Report for Corruption {
    fn report(&self) -> ErrorReport {
        use ErrorParam as P;
        let new = ErrorReport::new;
        let found = |code, text: &String| new(code).with("found", P::Text(text.clone()));
        match self {
            Self::BadAccountKind(text) => found("store.corrupt.bad-account-kind", text),
            Self::BadAccountType(text) => found("store.corrupt.bad-account-type", text),
            Self::AccountTypeMismatch(account) => {
                new("store.corrupt.account-type-mismatch").with("account", P::Account(*account))
            }
            Self::BadCategoryTree(account) => {
                new("store.corrupt.bad-category-tree").with("account", P::Account(*account))
            }
            Self::BadCurrencyCode(text) => found("store.corrupt.bad-currency-code", text),
            Self::BadDate(text) => found("store.corrupt.bad-date", text),
            Self::BadMinorDigits(digits) => {
                new("store.corrupt.bad-minor-digits").with("found", P::Number(*digits))
            }
            Self::BadRate(rate) => new("store.corrupt.bad-rate").with("rate", P::Rate(*rate)),
            Self::BadRateSource(text) => found("store.corrupt.bad-rate-source", text),
            Self::MissingSettings => new("store.corrupt.missing-settings"),
            Self::BadRoundingMode(text) => found("store.corrupt.bad-rounding-mode", text),
            Self::CurrencyMismatch { transaction } => new("store.corrupt.currency-mismatch")
                .with("transaction", P::Transaction(*transaction)),
            Self::BadSplit(split) => new("store.corrupt.bad-split").with("split", P::Split(*split)),
            Self::InvalidTransaction { transaction, error } => {
                new("store.corrupt.invalid-transaction")
                    .with("transaction", P::Transaction(*transaction))
                    .caused_by(error.report())
            }
            Self::BrokenReferences => new("store.corrupt.broken-references"),
            Self::BadAction(text) => found("store.corrupt.bad-action", text),
            Self::BadTimestamp(text) => found("store.corrupt.bad-timestamp", text),
            Self::BadHistoryRetention(days) => {
                new("store.corrupt.bad-history-retention").with("found", P::Number(*days))
            }
            Self::HistoryMismatch => new("store.corrupt.history-mismatch"),
            Self::BadRule(rule) => new("store.corrupt.bad-rule").with("rule", P::Rule(*rule)),
            Self::BadRuleTree(rule) => {
                new("store.corrupt.bad-rule-tree").with("rule", P::Rule(*rule))
            }
        }
    }
}

/// What went wrong in the file system, as one of the keywords of `store.io`
/// in `docs/ERROR_CODES.md`.
fn io_keyword(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::NotFound => "not-found",
        ErrorKind::PermissionDenied => "permission-denied",
        ErrorKind::AlreadyExists => "already-exists",
        ErrorKind::ReadOnlyFilesystem => "read-only",
        ErrorKind::StorageFull => "storage-full",
        ErrorKind::QuotaExceeded => "quota-exceeded",
        ErrorKind::FileTooLarge => "file-too-large",
        _ => "other",
    }
}

#[cfg(test)]
mod list;
#[cfg(test)]
mod tests;
