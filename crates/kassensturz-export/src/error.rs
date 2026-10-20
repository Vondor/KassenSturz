// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Why data can't be exported, as codes with parameters (D-034, D-084).

use std::io::ErrorKind;

use kassensturz_core::{CurrencyCode, ErrorParam, ErrorReport, Report};

/// Why data can't be exported.
///
/// Every variant turns into an [`ErrorReport`] whose code starts with
/// `export.` (`docs/ERROR_CODES.md`). A failed export writes no file, or
/// removes the one it began.
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
