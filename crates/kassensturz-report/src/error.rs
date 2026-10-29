// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Why a dataset couldn't be made (D-034). No report repeats text from the
//! book.

use kassensturz_core::{ErrorParam, ErrorReport, Report};
use kassensturz_store::StoreError;

/// An error of the reporting layer.
#[derive(Debug)]
pub enum ReportError {
    /// The period ends before it starts, or has a date outside the calendar.
    BadPeriod,
    /// The period spans more than [`MAX_MONTHS`](crate::MAX_MONTHS) months.
    PeriodTooLong,
    /// A dataset was asked for days the `Source` was not read for, which
    /// would give figures that silently leave out transactions.
    PeriodNotRead,
    /// A component of a dashboard or view doesn't follow the property
    /// schema (D-281). `path` is where, as text; it is data, never part of a
    /// message.
    DocumentInvalid {
        /// What is wrong, by a fixed word.
        reason: &'static str,
        /// The key or dotted path of the value.
        path: String,
    },
    /// A document is bigger than the book keeps (D-281).
    DocumentTooBig {
        /// The most bytes a document may have.
        max_bytes: usize,
    },
    /// A document was written by a newer KassenSturz (D-278).
    UnsupportedDocumentVersion {
        /// The version the document says it has.
        version: i64,
    },
    /// The grid editor refused an edit, which changed nothing (D-296).
    GridEditRefused {
        /// Why, by a fixed word.
        reason: &'static str,
    },
    /// The book couldn't be read, or a total doesn't fit.
    Store(StoreError),
}

impl From<StoreError> for ReportError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl core::fmt::Display for ReportError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.report().code)
    }
}

impl std::error::Error for ReportError {}

impl Report for ReportError {
    fn report(&self) -> ErrorReport {
        match self {
            Self::BadPeriod => ErrorReport::new("report.bad-period"),
            Self::PeriodTooLong => ErrorReport::new("report.period-too-long"),
            Self::PeriodNotRead => ErrorReport::new("report.period-not-read"),
            Self::DocumentInvalid { reason, path } => ErrorReport::new("report.document-invalid")
                .with("reason", ErrorParam::Keyword(reason))
                .with("path", ErrorParam::Text(path.clone())),
            Self::DocumentTooBig { max_bytes } => ErrorReport::new("report.document-too-big")
                .with("max_bytes", ErrorParam::count(*max_bytes)),
            Self::UnsupportedDocumentVersion { version } => {
                ErrorReport::new("report.unsupported-document-version")
                    .with("version", ErrorParam::Number(*version))
            }
            Self::GridEditRefused { reason } => ErrorReport::new("report.grid-edit-refused")
                .with("reason", ErrorParam::Keyword(reason)),
            Self::Store(error) => {
                ErrorReport::new("report.book-unreadable").caused_by(error.report())
            }
        }
    }
}

// The list of error codes lives in `kassensturz-store`'s source, so that the
// crates that must not depend on each other all check their codes the same
// way.
#[cfg(test)]
#[path = "../../kassensturz-store/src/error/list.rs"]
mod list;
#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::list::{check, documented, unused};
    use super::*;

    #[test]
    fn every_code_is_listed_with_its_parameters() {
        let rows = documented();
        let mut seen = HashSet::new();
        for error in [
            ReportError::BadPeriod,
            ReportError::PeriodTooLong,
            ReportError::PeriodNotRead,
            ReportError::DocumentInvalid {
                reason: "bad-choice",
                path: "params.grouping".into(),
            },
            ReportError::DocumentTooBig { max_bytes: 1 },
            ReportError::UnsupportedDocumentVersion { version: 2 },
            ReportError::GridEditRefused { reason: "no-room" },
            ReportError::Store(StoreError::AmountOverflow),
        ] {
            let report = error.report();
            assert!(report.code.starts_with("report."), "{}", report.code);
            check(&report, &rows, &mut seen);
        }
        let unused = unused(&rows, &seen, &["report"]);
        assert!(
            unused.is_empty(),
            "listed in docs/ERROR_CODES.md but no error has them: {unused:?}"
        );
    }
}
