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
    /// A `propose_view` argument that doesn't suit the chosen data or way of
    /// drawing it, such as `show_as` `line` for data drawn only as bars
    /// (M6-B204). It is reported as [`Self::BadArgument`]; the answer to the
    /// model adds the words that suit, or the argument to use instead, all
    /// from KassenSturz's own lists.
    NotForChart {
        /// The argument's name, from the function's fixed list.
        argument: &'static str,
        /// The words of the argument that suit, if a list says it.
        allowed: Vec<&'static str>,
        /// The argument to use instead, such as `formula` for categories
        /// on a key figure.
        use_instead: Option<&'static str>,
        /// The ways of drawing (`show_as`) the argument applies to, of
        /// those that suit the data, when the chosen one takes no such
        /// argument at all: `compare` on a bar chart, say.
        only_for_show_as: Vec<&'static str>,
    },
    /// An object of a `propose_changes` call, such as an item of
    /// `new_budgets`, has a key it doesn't take (M6-B201). The model's key
    /// is never repeated back to it (X-8); the answer names the object and
    /// lists the keys it does take, all from KassenSturz's own fixed lists.
    UnknownField {
        /// The object's name, such as `new_budgets` or `arguments`.
        argument: &'static str,
        /// The keys the object takes.
        allowed: &'static [&'static str],
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
    /// No chart saved under Statistics has that ID (D-681).
    UnknownView,
    /// A Rule or category can't be changed that way: a Rule with sub-Rules
    /// can't be deleted, only a top-level Rule moves, a Rule that books a
    /// Transfer or has conditions a model can't write keeps them. The
    /// report names the argument and why (M6-B191, D-626).
    NotChangeable {
        /// The argument that asked for it, from the function's fixed list.
        argument: &'static str,
        /// Why not, one of the fixed words in `docs/ERROR_CODES.md`.
        reason: &'static str,
    },
    /// The conditions of a Rule are unusable. The report counts the
    /// condition from 1 in the Rule's `conditions` (not for the list as a
    /// whole) and gives a fixed reason, never the model's text (M6-B191,
    /// D-626).
    BadCondition {
        /// Its place in the list, from 1, if it is about one condition.
        condition: Option<usize>,
        /// Why, one of the fixed words in `docs/ERROR_CODES.md`.
        reason: &'static str,
    },
    /// A formula the AI wrote for a chart can't be used (D-650). The report
    /// gives the argument, a fixed reason and the character it is at,
    /// counted from 1, never the model's text or the book's names.
    BadFormula {
        /// `formula` or `target`.
        argument: &'static str,
        /// Where, in characters from 1, if the mistake is at one place.
        position: Option<usize>,
        /// Why, one of the fixed words in `docs/ERROR_CODES.md`.
        reason: &'static str,
    },
    /// The error `cause` came from one item of a list of the call, e.g. the
    /// third of `new_rules`. The report is the cause's, plus the list and
    /// the item's place, so the model knows which one to fix (M6-B191,
    /// D-626).
    InItem {
        /// The list's name, from [`PROPOSE_CHANGES`](crate::PROPOSE_CHANGES)'s fixed list.
        list: &'static str,
        /// The item's place in the list, from 1.
        item: usize,
        /// What was wrong with it.
        cause: Box<ToolError>,
    },
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
    /// The book would refuse the budget the call prepares, for the reason
    /// the store gives a budget made by hand (D-358): the model hears the
    /// store's own code, as the user would.
    BudgetRefused(StoreError),
    /// The current period's figures of one budget couldn't be counted:
    /// `list_budgets` shows this in that budget's row and lists the others
    /// as usual (M6-B198).
    BudgetFigures(kassensturz_report::ReportError),
}

impl ToolError {
    /// The error as the item `index` (from 0) of the call's list `list`
    /// raised it. An error that is not about one item, or already names
    /// one, stays as it is.
    #[must_use]
    pub fn at(self, list: &'static str, index: usize) -> Self {
        match self {
            Self::Store(_)
            | Self::TooManyChanges
            | Self::NotAllowed { .. }
            | Self::InItem { .. } => self,
            cause => Self::InItem {
                list,
                item: index + 1,
                cause: Box::new(cause),
            },
        }
    }

    /// The report as the model hears it: the cause's own code and
    /// parameters, plus the list and the place of the item it came from, so
    /// that one answer names the item and the reason (M6-B191, D-626).
    /// [`Report::report`] keeps them apart, as a code of its own with the
    /// cause behind it.
    pub fn model_report(&self) -> ErrorReport {
        match self {
            Self::InItem { list, item, cause } => cause
                .model_report()
                .with("list", ErrorParam::Keyword(list))
                .with("item", ErrorParam::count(*item)),
            other => other.report(),
        }
    }

    /// The keys the object takes, when the error is an unknown key in it
    /// (M6-B201), or the words that suit the chart, when a `propose_view`
    /// argument doesn't (M6-B204). The answer to the model lists them, so
    /// it can fix its call; they come from KassenSturz's fixed lists, never
    /// from the call.
    pub fn allowed_fields(&self) -> Option<&[&'static str]> {
        match self.kind() {
            Self::UnknownField { allowed, .. } => Some(allowed),
            Self::NotForChart { allowed, .. } if !allowed.is_empty() => Some(allowed),
            _ => None,
        }
    }

    /// The ways of drawing the data that take the argument, when the chosen
    /// one takes none (M6-B204).
    pub fn only_for_show_as(&self) -> Option<&[&'static str]> {
        match self.kind() {
            Self::NotForChart {
                only_for_show_as, ..
            } if !only_for_show_as.is_empty() => Some(only_for_show_as),
            _ => None,
        }
    }

    /// The argument to use instead, when a `propose_view` argument can't be
    /// used with the chosen way of drawing (M6-B204).
    pub fn use_instead(&self) -> Option<&'static str> {
        match self.kind() {
            Self::NotForChart { use_instead, .. } => *use_instead,
            _ => None,
        }
    }

    /// The error itself, without the place of the item it came from.
    pub fn kind(&self) -> &Self {
        match self {
            Self::InItem { cause, .. } => cause,
            other => other,
        }
    }

    /// The error without the place of the item it came from, for code that
    /// tells the kinds of error apart.
    #[must_use]
    pub fn without_item(self) -> Self {
        match self {
            Self::InItem { cause, .. } => *cause,
            other => other,
        }
    }
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
            Self::BadArgument { argument } | Self::NotForChart { argument, .. } => {
                ErrorReport::new("ai-tools.bad-argument")
                    .with("argument", ErrorParam::Keyword(argument))
            }
            Self::UnknownField { argument, .. } => ErrorReport::new("ai-tools.unknown-field")
                .with("argument", ErrorParam::Keyword(argument)),
            Self::CallTooBig { max_bytes } => ErrorReport::new("ai-tools.call-too-big")
                .with("max_bytes", ErrorParam::count(*max_bytes)),
            Self::RepeatedKey => ErrorReport::new("ai-tools.repeated-key"),
            Self::UnknownArgument => ErrorReport::new("ai-tools.unknown-argument"),
            Self::UnknownAccount => ErrorReport::new("ai-tools.unknown-account"),
            Self::UnknownCategory => ErrorReport::new("ai-tools.unknown-category"),
            // The argument names only, never the days the model sent
            // (X-8, #593, D-670).
            Self::BadPeriod => ErrorReport::new("ai-tools.bad-period")
                .with("argument", ErrorParam::Keyword("to"))
                .with("not_before", ErrorParam::Keyword("from")),
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
            Self::UnknownView => ErrorReport::new("ai-tools.unknown-view"),
            Self::NotChangeable { argument, reason } => ErrorReport::new("ai-tools.not-changeable")
                .with("argument", ErrorParam::Keyword(argument))
                .with("reason", ErrorParam::Keyword(reason)),
            Self::BadCondition { condition, reason } => {
                let report = ErrorReport::new("ai-tools.bad-condition");
                let report = match condition {
                    Some(condition) => report.with("condition", ErrorParam::count(*condition)),
                    None => report,
                };
                report.with("reason", ErrorParam::Keyword(reason))
            }
            Self::BadFormula {
                argument,
                position,
                reason,
            } => {
                let report = ErrorReport::new("ai-tools.bad-formula")
                    .with("argument", ErrorParam::Keyword(argument));
                let report = match position {
                    Some(position) => report.with("position", ErrorParam::count(*position)),
                    None => report,
                };
                report.with("reason", ErrorParam::Keyword(reason))
            }
            Self::InItem { list, item, cause } => ErrorReport::new("ai-tools.in-item")
                .with("list", ErrorParam::Keyword(list))
                .with("item", ErrorParam::count(*item))
                .caused_by(cause.report()),
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
            Self::BudgetRefused(error) => error.report(),
            Self::BudgetFigures(error) => {
                ErrorReport::new("ai-tools.budget-figures-unreadable").caused_by(error.report())
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
            ToolError::UnknownView,
            ToolError::NotChangeable {
                argument: "conditions",
                reason: "conditions-hidden",
            },
            ToolError::NotChangeable {
                argument: "rule_id",
                reason: "has-sub-rules",
            },
            ToolError::NotChangeable {
                argument: "rule_id",
                reason: "is-sub-rule",
            },
            ToolError::NotChangeable {
                argument: "category_id",
                reason: "makes-transfer",
            },
            ToolError::NotChangeable {
                argument: "rule_id",
                reason: "also-deleted",
            },
            // Sorting with the user's Rules (US2.9, D-682).
            ToolError::NotChangeable {
                argument: "sort_with_rules",
                reason: "nothing-to-sort",
            },
            ToolError::NotChangeable {
                argument: "sort_with_rules",
                reason: "not-alone",
            },
            ToolError::BadCondition {
                condition: None,
                reason: "none",
            },
            ToolError::BadFormula {
                argument: "formula",
                position: Some(3),
                reason: "syntax",
            },
            ToolError::BadCondition {
                condition: None,
                reason: "too-many",
            },
            ToolError::BadCondition {
                condition: Some(1),
                reason: "shape",
            },
            ToolError::BadCondition {
                condition: Some(2),
                reason: "field",
            },
            ToolError::BadCondition {
                condition: Some(3),
                reason: "test",
            },
            ToolError::BadCondition {
                condition: Some(4),
                reason: "text",
            },
            ToolError::BadCondition {
                condition: Some(5),
                reason: "bad-pattern",
            },
            ToolError::BadCondition {
                condition: Some(6),
                reason: "invalid",
            },
            ToolError::BadCondition {
                condition: Some(7),
                reason: "not-there",
            },
            ToolError::BadCondition {
                condition: Some(1),
                reason: "duplicate",
            },
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
            ToolError::BudgetFigures(kassensturz_report::ReportError::PeriodTooLong),
        ];
        examples.extend(
            crate::changes::SECTIONS
                .iter()
                .map(|&(argument, _)| ToolError::NotAllowed { argument }),
        );
        examples.push(ToolError::NotAllowed { argument: "page" });
        // The place of the item a refusal came from (M6-B191).
        examples.extend(
            crate::changes::SECTIONS
                .iter()
                .map(|&(list, _)| ToolError::NameTaken.at(list, 2)),
        );
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
        // A chart setting that doesn't suit the chart (M6-B204).
        examples.push(ToolError::NotForChart {
            argument: "show_as",
            allowed: vec!["auto", "bar"],
            use_instead: None,
            only_for_show_as: Vec::new(),
        });
        // A key an object of the call doesn't take (M6-B201).
        examples.extend(
            [
                "arguments",
                "new_categories",
                "edit_categories",
                "new_rules",
                "edit_rules",
                "move_rules",
                "delete_rules",
                "conditions",
                "categorize",
                "delete_categories",
                "new_accounts",
                "edit_accounts",
                "edit_transactions",
                "new_transactions",
                "delete_transactions",
                "new_views",
                "new_budgets",
                "edit_budgets",
                "sort_with_rules",
            ]
            .map(|argument| ToolError::UnknownField {
                argument,
                allowed: &["name"],
            }),
        );
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
