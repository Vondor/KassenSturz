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
    /// A call of `propose_view` is longer than its limit (D-353).
    CallTooBig {
        /// The most bytes a call may have.
        max_bytes: usize,
    },
    /// An object of a `propose_view` call has the same key twice, compared
    /// after decoding escapes (D-353).
    RepeatedKey,
    /// A `propose_view` call has an argument the function doesn't take. The
    /// model's key is not repeated back to it (D-353).
    UnknownArgument,
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
    /// The transaction can't be edited that way: its amount or account only
    /// when it has two parts in one currency and is no transfer (D-257).
    NotEditable,
    /// The account isn't a cash account: the AI adds transactions only to
    /// those (D-258).
    NotCashAccount,
    /// The transaction isn't one the AI may delete: only one the user
    /// entered by hand in a cash account, with one category or none (D-258).
    NotDeletable,
    /// The field is locked against the AI in the AI settings (D-252, D-257).
    FieldLocked {
        /// The field's fixed name, such as `amount`.
        field: &'static str,
    },
    /// The category is in another currency than the transaction.
    OtherCurrency,
    /// No Rule has that ID.
    UnknownRule,
    /// A Rule or category can't be changed that way: a Rule with sub-Rules
    /// can't be deleted, only a top-level Rule moves, a Rule that books a
    /// Transfer or has conditions a model can't write keeps them.
    NotChangeable,
    /// A category can't be deleted because something uses it
    /// (`used_by`: `subcategories`, `transactions` or `rules`, D-253).
    CategoryInUse {
        /// What uses it.
        used_by: &'static str,
    },
    /// An account can't be closed while it holds money (D-255).
    AccountNotEmpty,
    /// The call has a list of a kind of change the user doesn't allow, or
    /// calls a function that reads what the user doesn't allow reading
    /// (`argument` names the list or the function, D-239, D-252).
    NotAllowed {
        /// The list's name, from [`PROPOSE_CHANGES`](crate::PROPOSE_CHANGES)'s fixed list.
        argument: &'static str,
    },
    /// The debug log can't be read: AI or the log is off (D-173).
    #[cfg(feature = "debug-log")]
    DebugLogOff,
    /// An issue draft's words hold text from the book, a figure or an
    /// e-mail address (D-228).
    IssueBookData {
        /// The field it is in, from the draft's fixed list.
        field: &'static str,
    },
    /// An issue draft's words hold something shaped like a secret (D-228).
    IssueSecret {
        /// The field it is in, from the draft's fixed list.
        field: &'static str,
    },
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
            Self::CallTooBig { max_bytes } => ErrorReport::new("ai-tools.call-too-big")
                .with("max_bytes", ErrorParam::count(*max_bytes)),
            Self::RepeatedKey => ErrorReport::new("ai-tools.repeated-key"),
            Self::UnknownArgument => ErrorReport::new("ai-tools.unknown-argument"),
            Self::UnknownAccount => ErrorReport::new("ai-tools.unknown-account"),
            Self::UnknownCategory => ErrorReport::new("ai-tools.unknown-category"),
            Self::BadPeriod => ErrorReport::new("ai-tools.bad-period"),
            Self::UnknownTransaction => ErrorReport::new("ai-tools.unknown-transaction"),
            Self::TooManyChanges => ErrorReport::new("ai-tools.too-many-changes"),
            Self::NameTaken => ErrorReport::new("ai-tools.name-taken"),
            Self::NotCategorizable => ErrorReport::new("ai-tools.not-categorizable"),
            Self::NotEditable => ErrorReport::new("ai-tools.not-editable"),
            Self::NotCashAccount => ErrorReport::new("ai-tools.not-cash-account"),
            Self::NotDeletable => ErrorReport::new("ai-tools.not-deletable"),
            Self::FieldLocked { field } => {
                ErrorReport::new("ai-tools.field-locked").with("field", ErrorParam::Keyword(field))
            }
            Self::OtherCurrency => ErrorReport::new("ai-tools.other-currency"),
            Self::UnknownRule => ErrorReport::new("ai-tools.unknown-rule"),
            Self::NotChangeable => ErrorReport::new("ai-tools.not-changeable"),
            Self::CategoryInUse { used_by } => ErrorReport::new("ai-tools.category-in-use")
                .with("used_by", ErrorParam::Keyword(used_by)),
            Self::AccountNotEmpty => ErrorReport::new("ai-tools.account-not-empty"),
            Self::NotAllowed { argument } => ErrorReport::new("ai-tools.not-allowed")
                .with("argument", ErrorParam::Keyword(argument)),
            #[cfg(feature = "debug-log")]
            Self::DebugLogOff => ErrorReport::new("ai-tools.debug-log-off"),
            Self::IssueBookData { field } => ErrorReport::new("ai-tools.issue-book-data")
                .with("field", ErrorParam::Keyword(field)),
            Self::IssueSecret { field } => {
                ErrorReport::new("ai-tools.issue-secret").with("field", ErrorParam::Keyword(field))
            }
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
    use crate::changes::{ARGUMENTS as CHANGE_ARGUMENTS, ChangeError, EditError};
    use crate::functions::ARGUMENTS;

    /// One error of each kind. A new error needs an example here.
    fn examples() -> Vec<ToolError> {
        let mut examples = vec![
            ToolError::UnknownFunction,
            ToolError::CallTooBig {
                max_bytes: crate::changes::MAX_VIEW_CALL_BYTES,
            },
            ToolError::RepeatedKey,
            ToolError::UnknownArgument,
            ToolError::UnknownAccount,
            ToolError::UnknownCategory,
            ToolError::BadPeriod,
            ToolError::UnknownTransaction,
            ToolError::TooManyChanges,
            ToolError::NameTaken,
            ToolError::NotCategorizable,
            ToolError::NotEditable,
            ToolError::NotCashAccount,
            ToolError::NotDeletable,
            ToolError::OtherCurrency,
            ToolError::UnknownRule,
            ToolError::NotChangeable,
            ToolError::AccountNotEmpty,
            ToolError::CategoryInUse {
                used_by: "subcategories",
            },
            ToolError::CategoryInUse {
                used_by: "transactions",
            },
            ToolError::CategoryInUse { used_by: "rules" },
            #[cfg(feature = "debug-log")]
            ToolError::DebugLogOff,
            ToolError::Store(StoreError::AmountOverflow),
        ];
        examples.extend(
            crate::changes::SECTIONS
                .iter()
                .map(|&(argument, _)| ToolError::NotAllowed { argument }),
        );
        examples.push(ToolError::NotAllowed { argument: "page" });
        // The chart builder's ids and the function itself (D-353).
        for argument in ["categories", "accounts", crate::changes::PROPOSE_VIEW_NAME] {
            examples.push(ToolError::NotAllowed { argument });
        }
        examples.extend(kassensturz_core::ai_switch::Field::ALL.iter().map(|field| {
            ToolError::FieldLocked {
                field: field.name(),
            }
        }));
        examples.extend(
            crate::FUNCTIONS
                .iter()
                .map(|function| ToolError::NotAllowed {
                    argument: function.name,
                }),
        );
        examples.extend(crate::issue::FIELDS.iter().flat_map(|&field| {
            [
                ToolError::IssueBookData { field },
                ToolError::IssueSecret { field },
            ]
        }));
        examples.extend(
            ARGUMENTS
                .iter()
                .chain(CHANGE_ARGUMENTS)
                .chain(crate::changes::VIEW_ARGUMENTS)
                .chain(crate::issue::FIELDS)
                .chain(crate::open_page::FIELDS)
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
        #[cfg_attr(feature = "debug-log", allow(unused_mut))]
        let mut unused = unused(&rows, &seen, &["ai-tools"]);
        // The debug log's code exists only in a build with it (D-227).
        #[cfg(not(feature = "debug-log"))]
        unused.retain(|code| code != "ai-tools.debug-log-off");
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
            ChangeError::NotAllowed,
        ] {
            let report = error.report();
            assert!(report.code.starts_with("ai-changes."), "{}", report.code);
            check(&report, &rows, &mut seen);
        }
        // The user's corrections of a set before confirming it (D-297).
        for error in [
            EditError::NotEditable,
            EditError::Text,
            EditError::NameTaken,
            EditError::InUse,
            EditError::NotAllowed,
            EditError::Refused(ToolError::UnknownCategory),
        ] {
            let report = error.report();
            assert!(report.code.starts_with("ai-changes."), "{}", report.code);
            check(&report, &rows, &mut seen);
        }
        let unused = unused(&rows, &seen, &["ai-changes"]);
        assert!(unused.is_empty(), "{unused:?}");
    }
}
