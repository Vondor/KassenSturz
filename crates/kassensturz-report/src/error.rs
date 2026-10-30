// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Why a dataset couldn't be made (D-034). Text from the book is only ever
//! a parameter, as data the UI shows as plain text, never part of a message.

use kassensturz_core::{CurrencyCode, ErrorParam, ErrorReport, Report};
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
    /// A chart of parts of a whole would count a category twice, because two
    /// of its groups or series hold it (D-291).
    GroupsOverlap {
        /// The category's path, joined with " › ".
        category: String,
        /// The first group's name or category's path.
        first: String,
        /// The second one.
        second: String,
    },
    /// A formula isn't written the way formulas are (D-292). Every
    /// formula error has the position of the character it is at, counted
    /// from 1.
    FormulaSyntax {
        /// Where, in characters from 1.
        position: usize,
        /// What was expected or is wrong, by a fixed word.
        reason: &'static str,
    },
    /// A formula's name is no category and no group.
    FormulaUnknownName {
        /// Where.
        position: usize,
        /// The name as the formula writes it.
        name: String,
    },
    /// A formula's name fits more than one category or group (T-42).
    FormulaAmbiguousName {
        /// Where.
        position: usize,
        /// The name as the formula writes it.
        name: String,
        /// The ones it fits, each written as a formula names it, in quotes.
        paths: String,
    },
    /// A formula calls a function there is none of.
    FormulaUnknownFunction {
        /// Where.
        position: usize,
        /// The name as the formula writes it.
        name: String,
    },
    /// A function was given the wrong arguments.
    FormulaArguments {
        /// Where.
        position: usize,
        /// The function's name.
        function: &'static str,
        /// What is wrong, by a fixed word.
        reason: &'static str,
    },
    /// Two parts of a formula don't go together, such as two amounts
    /// multiplied.
    FormulaKinds {
        /// Where.
        position: usize,
        /// Which rule, by a fixed word.
        reason: &'static str,
    },
    /// A formula's names have money in two currencies.
    FormulaMixedCurrencies {
        /// Where the second currency is.
        position: usize,
        /// The name with the first currency.
        first: String,
        /// That currency.
        first_currency: CurrencyCode,
        /// The name with the second currency.
        second: String,
        /// That currency.
        second_currency: CurrencyCode,
    },
    /// A formula divides by zero, so it has no value.
    FormulaNoValue {
        /// Where the division is.
        position: usize,
    },
    /// A figure of a formula is too large, or too precise, to work out
    /// exactly.
    FormulaTooLarge {
        /// Where.
        position: usize,
    },
    /// A formula is over one of its limits (D-292).
    FormulaTooBig {
        /// Which limit, by a fixed word.
        limit: &'static str,
    },
    /// A progress bar's formula cannot be measured against its target
    /// (M6b.7, D-322).
    FormulaTarget {
        /// Which rule, by a fixed word.
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
            Self::GroupsOverlap {
                category,
                first,
                second,
            } => ErrorReport::new("report.groups-overlap")
                .with("category", ErrorParam::Text(category.clone()))
                .with("first", ErrorParam::Text(first.clone()))
                .with("second", ErrorParam::Text(second.clone())),
            Self::FormulaSyntax { position, reason } => ErrorReport::new("report.formula-syntax")
                .with("position", ErrorParam::count(*position))
                .with("reason", ErrorParam::Keyword(reason)),
            Self::FormulaUnknownName { position, name } => {
                ErrorReport::new("report.formula-unknown-name")
                    .with("position", ErrorParam::count(*position))
                    .with("name", ErrorParam::Text(name.clone()))
            }
            Self::FormulaAmbiguousName {
                position,
                name,
                paths,
            } => ErrorReport::new("report.formula-ambiguous-name")
                .with("position", ErrorParam::count(*position))
                .with("name", ErrorParam::Text(name.clone()))
                .with("paths", ErrorParam::Text(paths.clone())),
            Self::FormulaUnknownFunction { position, name } => {
                ErrorReport::new("report.formula-unknown-function")
                    .with("position", ErrorParam::count(*position))
                    .with("name", ErrorParam::Text(name.clone()))
            }
            Self::FormulaArguments {
                position,
                function,
                reason,
            } => ErrorReport::new("report.formula-arguments")
                .with("position", ErrorParam::count(*position))
                .with("function", ErrorParam::Text((*function).to_owned()))
                .with("reason", ErrorParam::Keyword(reason)),
            Self::FormulaKinds { position, reason } => ErrorReport::new("report.formula-kinds")
                .with("position", ErrorParam::count(*position))
                .with("reason", ErrorParam::Keyword(reason)),
            Self::FormulaMixedCurrencies {
                position,
                first,
                first_currency,
                second,
                second_currency,
            } => ErrorReport::new("report.formula-mixed-currencies")
                .with("position", ErrorParam::count(*position))
                .with("first", ErrorParam::Text(first.clone()))
                .with("first_currency", ErrorParam::Currency(*first_currency))
                .with("second", ErrorParam::Text(second.clone()))
                .with("second_currency", ErrorParam::Currency(*second_currency)),
            Self::FormulaNoValue { position } => ErrorReport::new("report.formula-no-value")
                .with("position", ErrorParam::count(*position)),
            Self::FormulaTooLarge { position } => ErrorReport::new("report.formula-too-large")
                .with("position", ErrorParam::count(*position)),
            Self::FormulaTooBig { limit } => {
                ErrorReport::new("report.formula-too-big").with("limit", ErrorParam::Keyword(limit))
            }
            Self::FormulaTarget { reason } => ErrorReport::new("report.formula-target")
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
            ReportError::GroupsOverlap {
                category: "Phone".into(),
                first: "Fixed costs".into(),
                second: "Household".into(),
            },
            ReportError::FormulaSyntax {
                position: 5,
                reason: "missing-close",
            },
            ReportError::FormulaUnknownName {
                position: 1,
                name: "Travel".into(),
            },
            ReportError::FormulaAmbiguousName {
                position: 1,
                name: "Other".into(),
                paths: "'Food > Other', 'Car > Other'".into(),
            },
            ReportError::FormulaUnknownFunction {
                position: 2,
                name: "SUMME".into(),
            },
            ReportError::FormulaArguments {
                position: 2,
                function: "ROUND",
                reason: "places",
            },
            ReportError::FormulaKinds {
                position: 6,
                reason: "amount-times-amount",
            },
            ReportError::FormulaMixedCurrencies {
                position: 8,
                first: "Pets".into(),
                first_currency: kassensturz_core::CurrencyCode::new("EUR").expect("a code"),
                second: "Travel".into(),
                second_currency: kassensturz_core::CurrencyCode::new("USD").expect("a code"),
            },
            ReportError::FormulaNoValue { position: 6 },
            ReportError::FormulaTooLarge { position: 1 },
            ReportError::FormulaTooBig { limit: "depth" },
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
