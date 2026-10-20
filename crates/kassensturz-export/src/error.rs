// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Why data can't be exported, or a complete export can't be read back, as
//! codes with parameters (D-034, D-084).

use std::io::ErrorKind;

use kassensturz_core::{CurrencyCode, ErrorParam, ErrorReport, Report, SplitId, TransactionError};

/// Why data can't be exported, or a complete export can't be read back.
///
/// Every variant turns into an [`ErrorReport`] whose code starts with
/// `export.` (`docs/ERROR_CODES.md`). A failed export writes no file, or
/// removes the one it began. Errors in reading name the place in the file
/// as a path, e.g. `transactions[3].parts[0].amount`, counting from 0: text
/// for people who look into the file, passed on as data (D-084).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportError {
    /// The file couldn't be written.
    Io(ErrorKind),
    /// A file with that name exists already. An export never overwrites a
    /// file.
    AlreadyExists,
    /// The settings for writing a file contradict each other.
    BadSettings,
    /// A row's amount is in another currency than its account.
    CurrencyMismatch {
        /// The account's currency.
        expected: CurrencyCode,
        /// The amount's currency.
        found: CurrencyCode,
    },
    /// The decimal places given for a currency are more than amounts of
    /// 64-bit integers can have.
    BadDecimalPlaces {
        /// The currency.
        currency: CurrencyCode,
        /// The decimal places.
        digits: u8,
    },
    /// The exporter was given data of another kind than it writes, e.g. a
    /// whole book for CSV.
    WrongKind,
    /// An amount is in a currency the book's data doesn't list, so its
    /// decimal places aren't known.
    UnlistedCurrency {
        /// The currency.
        currency: CurrencyCode,
        /// Where the amount is, when reading a file.
        path: Option<String>,
    },
    /// A part is marked as booked by a Rule, but no transaction has it.
    MarkWithoutPart(SplitId),
    /// Rules sit inside each other deeper than the complete export allows.
    RulesTooDeep {
        /// The deepest a Rule may sit; a top-level Rule is at 1.
        max: usize,
    },
    /// The file is larger than a complete export can be.
    TooLarge {
        /// The largest size, in bytes.
        max: u64,
    },
    /// The file isn't JSON, e.g. because it is cut off.
    NotJson {
        /// The line where reading stopped, from 1.
        line: usize,
        /// The column there, from 1.
        column: usize,
    },
    /// The file is JSON, but not a complete export of a KassenSturz book.
    NotABook,
    /// The file was written by a newer KassenSturz, in a version of the
    /// format this one doesn't know.
    NewerVersion {
        /// The file's version.
        version: u64,
    },
    /// A value the format needs is missing.
    MissingValue {
        /// Where it should be.
        path: String,
    },
    /// The file has a key the format doesn't have, so reading it would leave
    /// something out.
    UnknownValue {
        /// Where it is.
        path: String,
    },
    /// A value isn't what the format needs there.
    BadValue {
        /// Where it is.
        path: String,
        /// What the format needs there.
        expected: Expected,
    },
    /// Two entries have the same ID, or two currencies the same code.
    Duplicate {
        /// Where the second one is.
        path: String,
    },
    /// A transaction doesn't add up or contradicts itself.
    InvalidTransaction {
        /// Where it is.
        path: String,
        /// What is wrong with it.
        error: TransactionError,
    },
}

/// What the format needs where a value is wrong ([`ExportError::BadValue`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expected {
    /// An object, in braces.
    Object,
    /// A list, in brackets.
    List,
    /// Text, in quotes.
    Text,
    /// A whole number in the range the value allows.
    Number,
    /// `true` or `false`.
    YesOrNo,
    /// `null`, because another value rules this one out: a Rule books to a
    /// category or an account, never both.
    Null,
    /// An ID: a UUID with hyphens.
    Id,
    /// A date: `YYYY-MM-DD`.
    Date,
    /// An amount: digits with a point and at most as many decimal places as
    /// the currency has, a minus in front for money out.
    Amount,
    /// A currency code: three capital letters.
    Currency,
    /// An exchange rate: digits with a point, above zero.
    Rate,
    /// One of the words the format lists for this value.
    Keyword,
}

impl Expected {
    /// The keyword for it, from the list with `export.bad-value` in
    /// `docs/ERROR_CODES.md`.
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Object => "object",
            Self::List => "list",
            Self::Text => "text",
            Self::Number => "number",
            Self::YesOrNo => "yes-or-no",
            Self::Null => "null",
            Self::Id => "id",
            Self::Date => "date",
            Self::Amount => "amount",
            Self::Currency => "currency",
            Self::Rate => "rate",
            Self::Keyword => "keyword",
        }
    }
}

impl From<std::io::Error> for ExportError {
    fn from(error: std::io::Error) -> Self {
        match error.kind() {
            ErrorKind::AlreadyExists => Self::AlreadyExists,
            kind => Self::Io(kind),
        }
    }
}

impl Report for ExportError {
    fn report(&self) -> ErrorReport {
        match self {
            Self::Io(kind) => {
                ErrorReport::new("export.io").with("kind", ErrorParam::Keyword(io_keyword(*kind)))
            }
            Self::AlreadyExists => ErrorReport::new("export.already-exists"),
            Self::BadSettings => ErrorReport::new("export.bad-settings"),
            Self::CurrencyMismatch { expected, found } => {
                ErrorReport::new("export.currency-mismatch")
                    .with("expected", ErrorParam::Currency(*expected))
                    .with("found", ErrorParam::Currency(*found))
            }
            Self::BadDecimalPlaces { currency, digits } => {
                ErrorReport::new("export.bad-decimal-places")
                    .with("currency", ErrorParam::Currency(*currency))
                    .with("digits", ErrorParam::Number((*digits).into()))
            }
            Self::WrongKind => ErrorReport::new("export.wrong-kind"),
            Self::UnlistedCurrency { currency, path } => {
                let report = ErrorReport::new("export.unlisted-currency")
                    .with("currency", ErrorParam::Currency(*currency));
                match path {
                    Some(path) => report.with("path", ErrorParam::Text(path.clone())),
                    None => report,
                }
            }
            Self::MarkWithoutPart(split) => ErrorReport::new("export.mark-without-part")
                .with("split", ErrorParam::Split(*split)),
            Self::RulesTooDeep { max } => {
                ErrorReport::new("export.rules-too-deep").with("max", ErrorParam::count(*max))
            }
            Self::TooLarge { max } => ErrorReport::new("export.too-large").with(
                "max",
                ErrorParam::Number(i64::try_from(*max).unwrap_or(i64::MAX)),
            ),
            Self::NotJson { line, column } => ErrorReport::new("export.not-json")
                .with("line", ErrorParam::count(*line))
                .with("column", ErrorParam::count(*column)),
            Self::NotABook => ErrorReport::new("export.not-a-book"),
            Self::NewerVersion { version } => ErrorReport::new("export.newer-version").with(
                "version",
                ErrorParam::Number(i64::try_from(*version).unwrap_or(i64::MAX)),
            ),
            Self::MissingValue { path } => ErrorReport::new("export.missing-value")
                .with("path", ErrorParam::Text(path.clone())),
            Self::UnknownValue { path } => ErrorReport::new("export.unknown-value")
                .with("path", ErrorParam::Text(path.clone())),
            Self::BadValue { path, expected } => ErrorReport::new("export.bad-value")
                .with("path", ErrorParam::Text(path.clone()))
                .with("expected", ErrorParam::Keyword(expected.keyword())),
            Self::Duplicate { path } => {
                ErrorReport::new("export.duplicate").with("path", ErrorParam::Text(path.clone()))
            }
            Self::InvalidTransaction { path, error } => {
                ErrorReport::new("export.invalid-transaction")
                    .with("path", ErrorParam::Text(path.clone()))
                    .caused_by(error.report())
            }
        }
    }
}

/// The keyword for why a file couldn't be written, from the list with
/// `export.io` in `docs/ERROR_CODES.md`.
fn io_keyword(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::NotFound => "not-found",
        ErrorKind::PermissionDenied => "permission-denied",
        ErrorKind::IsADirectory => "is-a-directory",
        ErrorKind::StorageFull => "disk-full",
        _ => "other",
    }
}

// The reader of `docs/ERROR_CODES.md` the other crates' tests use, so all
// check their codes the same way.
#[cfg(test)]
#[path = "../../kassensturz-store/src/error/list.rs"]
mod list;
#[cfg(test)]
mod tests;
