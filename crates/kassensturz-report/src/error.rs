// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Why a dataset couldn't be made (D-034). No report repeats text from the
//! book.

use kassensturz_core::{ErrorReport, Report};
use kassensturz_store::StoreError;

/// An error of the reporting layer.
#[derive(Debug)]
pub enum ReportError {
    /// The period ends before it starts, or has a date outside the calendar.
    BadPeriod,
    /// The period spans more than [`MAX_MONTHS`](crate::MAX_MONTHS) months.
    PeriodTooLong,
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
