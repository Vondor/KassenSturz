// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Why a file can't be imported, as codes with parameters (D-034, D-084).

use std::io::ErrorKind;

use kassensturz_core::{CurrencyCode, ErrorParam, ErrorReport, Report};

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
            | Self::TooManyDecimalPlaces { line, .. }
            | Self::AmountTooLarge { line, .. }
            | Self::UnknownCurrency { line, .. } => *line = Some(at),
            Self::Io(_)
            | Self::FileTooLarge { .. }
            | Self::TooManyEntries { .. }
            | Self::CurrencyMismatch { .. } => {}
        }
        self
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
