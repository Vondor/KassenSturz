// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Why a file can't be imported, as codes with parameters (D-034, D-084).

use std::io::ErrorKind;

use kassensturz_core::{AccountId, CurrencyCode, ErrorParam, ErrorReport, Report};

use crate::csv::TextEncoding;

/// Why a file can't be imported.
///
/// Every variant turns into an [`ErrorReport`] whose code starts with
/// `import.` (`docs/ERROR_CODES.md`). Text found in the file stays text in
/// the report, shown as data, never as part of a message (AGENTS.md §8).
/// Where a variant has a `line`, it is the line of the file the problem is
/// on, counted from 1, if the importer knows it ([`ImportError::at_line`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportError {
    /// The file couldn't be read.
    Io(ErrorKind),
    /// The file is larger than [`crate::Limits::max_file_bytes`].
    FileTooLarge {
        /// The limit, in bytes.
        max_bytes: u64,
    },
    /// The file nests deeper than [`crate::Limits::max_depth`].
    TooDeeplyNested {
        /// The limit.
        max: usize,
        /// Where, if known.
        line: Option<u64>,
    },
    /// The file has more entries than [`crate::Limits::max_entries`].
    TooManyEntries {
        /// The limit.
        max: usize,
    },
    /// A text in the file is longer than [`crate::Limits::max_text_bytes`].
    TextTooLong {
        /// The limit, in bytes.
        max_bytes: usize,
        /// Where, if known.
        line: Option<u64>,
    },
    /// The file isn't in the importer's format, or is damaged.
    Malformed {
        /// Where, if known.
        line: Option<u64>,
    },
    /// An amount in the file isn't a number the importer can read.
    BadAmount {
        /// The amount as the file wrote it.
        found: String,
        /// Where, if known.
        line: Option<u64>,
    },
    /// A date in the file isn't one the importer can read, e.g. not in the
    /// order of day, month and year chosen for a CSV file.
    BadDate {
        /// The date as the file wrote it.
        found: String,
        /// Where, if known.
        line: Option<u64>,
    },
    /// The file has a byte the chosen text encoding has no character for,
    /// so it is in another one.
    WrongEncoding {
        /// The encoding the file was read in.
        encoding: TextEncoding,
        /// Where, if known.
        line: Option<u64>,
    },
    /// The file's header row has no column of the name the settings give.
    ColumnNotFound {
        /// The name.
        column: String,
        /// The header row's line, if known.
        line: Option<u64>,
    },
    /// The file's header row has more than one column of the name the
    /// settings give, so it isn't clear which one is meant.
    ColumnNotUnique {
        /// The name.
        column: String,
        /// The header row's line, if known.
        line: Option<u64>,
    },
    /// The settings for reading a file, such as a CSV file's columns, are
    /// incomplete or contradict each other.
    BadSettings {
        /// The line of the settings' text form, if they were read from one.
        line: Option<u64>,
    },
    /// An amount has more decimal places than its currency, so it can't be
    /// stored without rounding.
    TooManyDecimalPlaces {
        /// The amount as the file wrote it.
        found: String,
        /// Its currency.
        currency: CurrencyCode,
        /// Where, if known.
        line: Option<u64>,
    },
    /// An amount is too large for KassenSturz to work with.
    AmountTooLarge {
        /// The amount as the file wrote it.
        found: String,
        /// Where, if known.
        line: Option<u64>,
    },
    /// The file uses a currency KassenSturz doesn't know.
    UnknownCurrency {
        /// The currency's code.
        currency: CurrencyCode,
        /// Where, if known.
        line: Option<u64>,
    },
    /// A statement has an amount in another currency than its own.
    CurrencyMismatch {
        /// The statement's currency.
        expected: CurrencyCode,
        /// The amount's currency.
        found: CurrencyCode,
    },
    /// A statement is in another currency than the account it is to go
    /// into. The import pipeline refuses it, also for a withdrawn currency
    /// such as the Mark, until converting one comes (M2.9, D-077, D-092).
    AccountCurrencyDiffers {
        /// The account.
        account: AccountId,
        /// The account's currency.
        account_currency: CurrencyCode,
        /// The statement's currency.
        statement_currency: CurrencyCode,
    },
    /// The file counts a currency in other decimal places than the book
    /// does (D-027), so its amounts would be read with the decimal point in
    /// another place.
    DecimalPlacesDiffer {
        /// The currency.
        currency: CurrencyCode,
        /// The decimal places the book stores for it.
        book: u8,
        /// The decimal places the file's amounts are counted in.
        file: u8,
    },
}

impl ImportError {
    /// The error, saying it is on `line` of the file, counted from 1. An
    /// error that isn't about one place in the file stays as it is.
    #[must_use]
    pub fn at_line(mut self, at: u64) -> Self {
        match &mut self {
            Self::TooDeeplyNested { line, .. }
            | Self::TextTooLong { line, .. }
            | Self::Malformed { line }
            | Self::BadAmount { line, .. }
            | Self::BadDate { line, .. }
            | Self::WrongEncoding { line, .. }
            | Self::ColumnNotFound { line, .. }
            | Self::ColumnNotUnique { line, .. }
            | Self::BadSettings { line }
            | Self::TooManyDecimalPlaces { line, .. }
            | Self::AmountTooLarge { line, .. }
            | Self::UnknownCurrency { line, .. } => *line = Some(at),
            Self::Io(_)
            | Self::FileTooLarge { .. }
            | Self::TooManyEntries { .. }
            | Self::CurrencyMismatch { .. }
            | Self::AccountCurrencyDiffers { .. }
            | Self::DecimalPlacesDiffer { .. } => {}
        }
        self
    }

    /// As [`ImportError::at_line`], for a place that may not be known: a
    /// file that isn't made of lines, such as a SQLite database, has none.
    #[must_use]
    pub(crate) fn at(self, line: Option<u64>) -> Self {
        match line {
            Some(line) => self.at_line(line),
            None => self,
        }
    }
}

impl Report for ImportError {
    fn report(&self) -> ErrorReport {
        let with_line = |report: ErrorReport, line: &Option<u64>| match *line {
            Some(line) => report.with(
                "line",
                ErrorParam::Number(i64::try_from(line).unwrap_or(i64::MAX)),
            ),
            None => report,
        };
        let text = |text: &str| ErrorParam::Text(text.to_owned());
        match self {
            Self::Io(kind) => {
                ErrorReport::new("import.io").with("kind", ErrorParam::Keyword(io_keyword(*kind)))
            }
            Self::FileTooLarge { max_bytes } => ErrorReport::new("import.file-too-large").with(
                "max_bytes",
                ErrorParam::Number(i64::try_from(*max_bytes).unwrap_or(i64::MAX)),
            ),
            Self::TooDeeplyNested { max, line } => with_line(
                ErrorReport::new("import.too-deeply-nested").with("max", ErrorParam::count(*max)),
                line,
            ),
            Self::TooManyEntries { max } => {
                ErrorReport::new("import.too-many-entries").with("max", ErrorParam::count(*max))
            }
            Self::TextTooLong { max_bytes, line } => with_line(
                ErrorReport::new("import.text-too-long")
                    .with("max_bytes", ErrorParam::count(*max_bytes)),
                line,
            ),
            Self::Malformed { line } => with_line(ErrorReport::new("import.malformed"), line),
            Self::BadAmount { found, line } => with_line(
                ErrorReport::new("import.bad-amount").with("found", text(found)),
                line,
            ),
            Self::BadDate { found, line } => with_line(
                ErrorReport::new("import.bad-date").with("found", text(found)),
                line,
            ),
            Self::WrongEncoding { encoding, line } => with_line(
                ErrorReport::new("import.wrong-encoding")
                    .with("encoding", ErrorParam::Keyword(encoding.keyword())),
                line,
            ),
            Self::ColumnNotFound { column, line } => with_line(
                ErrorReport::new("import.column-not-found").with("column", text(column)),
                line,
            ),
            Self::ColumnNotUnique { column, line } => with_line(
                ErrorReport::new("import.column-not-unique").with("column", text(column)),
                line,
            ),
            Self::BadSettings { line } => with_line(ErrorReport::new("import.bad-settings"), line),
            Self::TooManyDecimalPlaces {
                found,
                currency,
                line,
            } => with_line(
                ErrorReport::new("import.too-many-decimal-places")
                    .with("found", text(found))
                    .with("currency", ErrorParam::Currency(*currency)),
                line,
            ),
            Self::AmountTooLarge { found, line } => with_line(
                ErrorReport::new("import.amount-too-large").with("found", text(found)),
                line,
            ),
            Self::UnknownCurrency { currency, line } => with_line(
                ErrorReport::new("import.unknown-currency")
                    .with("currency", ErrorParam::Currency(*currency)),
                line,
            ),
            Self::CurrencyMismatch { expected, found } => {
                ErrorReport::new("import.currency-mismatch")
                    .with("expected", ErrorParam::Currency(*expected))
                    .with("found", ErrorParam::Currency(*found))
            }
            Self::AccountCurrencyDiffers {
                account,
                account_currency,
                statement_currency,
            } => ErrorReport::new("import.account-currency-differs")
                .with("account", ErrorParam::Account(*account))
                .with("account_currency", ErrorParam::Currency(*account_currency))
                .with(
                    "statement_currency",
                    ErrorParam::Currency(*statement_currency),
                ),
            Self::DecimalPlacesDiffer {
                currency,
                book,
                file,
            } => ErrorReport::new("import.decimal-places-differ")
                .with("currency", ErrorParam::Currency(*currency))
                .with("book", ErrorParam::Number((*book).into()))
                .with("file", ErrorParam::Number((*file).into())),
        }
    }
}

/// The keyword for why a file couldn't be read, from the list with
/// `import.io` in `docs/ERROR_CODES.md`.
fn io_keyword(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::NotFound => "not-found",
        ErrorKind::PermissionDenied => "permission-denied",
        ErrorKind::IsADirectory => "is-a-directory",
        _ => "other",
    }
}

// The reader of `docs/ERROR_CODES.md` the store's tests use, so both crates
// check their codes the same way.
#[cfg(test)]
#[path = "../../kassensturz-store/src/error/list.rs"]
mod list;
#[cfg(test)]
mod tests;
