// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Why a read function couldn't answer (D-034).
//!
//! The chat hands the code to the model as the function's answer, and the
//! tool-call log shows it (M5.4). No report echoes what the model sent:
//! a call is untrusted text (X-8), so a bad argument is named by its fixed
//! name only.

use kassensturz_core::{ErrorParam, ErrorReport, Report};
use kassensturz_store::StoreError;

/// An error of a read function.
#[derive(Debug)]
pub enum ToolError {
    /// The model asked for a function that isn't in the list.
    UnknownFunction,
    /// An argument is missing, of the wrong kind or out of range, or the
    /// call has one the function doesn't take (`arguments`).
    BadArgument {
        /// The argument's name, from the function's fixed list.
        argument: &'static str,
    },
    /// No account of the user's has that ID.
    UnknownAccount,
    /// No category has that ID.
    UnknownCategory,
    /// The period ends before it starts.
    BadPeriod,
    /// No transaction has that ID.
    UnknownTransaction,
    /// A change set would hold more than [`crate::MAX_CHANGES`] changes.
    TooManyChanges,
    /// A new category or Rule would have the name of one that is there
    /// already, or of another new one.
    NameTaken,
    /// The transaction can't simply get a category: it is a transfer, a
    /// starting balance, or split over several categories.
    NotCategorizable,
    /// The category is in another currency than the transaction.
    OtherCurrency,
    /// The book couldn't be read.
    Store(StoreError),
}

impl From<StoreError> for ToolError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl core::fmt::Display for ToolError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.report().code)
    }
}

impl std::error::Error for ToolError {}

impl Report for ToolError {
    fn report(&self) -> ErrorReport {
        match self {
            Self::UnknownFunction => ErrorReport::new("ai-tools.unknown-function"),
            Self::BadArgument { argument } => ErrorReport::new("ai-tools.bad-argument")
                .with("argument", ErrorParam::Keyword(argument)),
            Self::UnknownAccount => ErrorReport::new("ai-tools.unknown-account"),
            Self::UnknownCategory => ErrorReport::new("ai-tools.unknown-category"),
            Self::BadPeriod => ErrorReport::new("ai-tools.bad-period"),
            Self::UnknownTransaction => ErrorReport::new("ai-tools.unknown-transaction"),
            Self::TooManyChanges => ErrorReport::new("ai-tools.too-many-changes"),
            Self::NameTaken => ErrorReport::new("ai-tools.name-taken"),
            Self::NotCategorizable => ErrorReport::new("ai-tools.not-categorizable"),
            Self::OtherCurrency => ErrorReport::new("ai-tools.other-currency"),
            Self::Store(error) => {
                ErrorReport::new("ai-tools.book-unreadable").caused_by(error.report())
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
    use crate::changes::{ARGUMENTS as CHANGE_ARGUMENTS, ChangeError};
    use crate::functions::ARGUMENTS;

    /// One error of each kind. A new error needs an example here.
    fn examples() -> Vec<ToolError> {
        let mut examples = vec![
            ToolError::UnknownFunction,
            ToolError::UnknownAccount,
            ToolError::UnknownCategory,
            ToolError::BadPeriod,
            ToolError::UnknownTransaction,
            ToolError::TooManyChanges,
            ToolError::NameTaken,
            ToolError::NotCategorizable,
            ToolError::OtherCurrency,
            ToolError::Store(StoreError::AmountOverflow),
        ];
        examples.extend(
            ARGUMENTS
                .iter()
                .chain(CHANGE_ARGUMENTS)
                .map(|&argument| ToolError::BadArgument { argument }),
        );
        examples
    }

    #[test]
    fn every_code_is_listed_with_its_parameters() {
        let rows = documented();
        let mut seen = HashSet::new();
        for error in examples() {
            let report = error.report();
            assert!(report.code.starts_with("ai-tools."), "{}", report.code);
            check(&report, &rows, &mut seen);
        }
        let unused = unused(&rows, &seen, &["ai-tools"]);
        assert!(
            unused.is_empty(),
            "listed in docs/ERROR_CODES.md but no error has them: {unused:?}"
        );
    }

    #[test]
    fn every_change_set_code_is_listed() {
        let rows = documented();
        let mut seen = HashSet::new();
        for error in [
            ChangeError::OutOfDate,
            ChangeError::NotApplied(StoreError::AmountOverflow),
        ] {
            let report = error.report();
            assert!(report.code.starts_with("ai-changes."), "{}", report.code);
            check(&report, &rows, &mut seen);
        }
        let unused = unused(&rows, &seen, &["ai-changes"]);
        assert!(unused.is_empty(), "{unused:?}");
    }
}
