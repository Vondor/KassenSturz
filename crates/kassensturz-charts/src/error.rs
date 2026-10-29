// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Why a chart couldn't be made (D-034).

use kassensturz_core::{ErrorParam, ErrorReport, Report};

use crate::ChartKind;

/// An error of the chart model.
#[derive(Debug, PartialEq, Eq)]
pub enum ChartError {
    /// The dataset can't be shown as this kind of chart.
    KindNotSupported(ChartKind),
    /// An amount is too large to be placed on a chart without losing
    /// digits (more than 2^53 minor units).
    ValueTooLarge,
    /// A total or a tick doesn't fit into a 64-bit amount.
    Overflow,
}

impl core::fmt::Display for ChartError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.report().code)
    }
}

impl std::error::Error for ChartError {}

impl Report for ChartError {
    fn report(&self) -> ErrorReport {
        match self {
            Self::KindNotSupported(kind) => ErrorReport::new("charts.kind-not-supported")
                .with("kind", ErrorParam::Keyword(kind.key())),
            Self::ValueTooLarge => ErrorReport::new("charts.value-too-large"),
            Self::Overflow => ErrorReport::new("charts.amount-overflow"),
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
            ChartError::KindNotSupported(ChartKind::Pie),
            ChartError::ValueTooLarge,
            ChartError::Overflow,
        ] {
            let report = error.report();
            assert!(report.code.starts_with("charts."), "{}", report.code);
            check(&report, &rows, &mut seen);
        }
        let unused = unused(&rows, &seen, &["charts"]);
        assert!(
            unused.is_empty(),
            "listed in docs/ERROR_CODES.md but no error has them: {unused:?}"
        );
    }
}
