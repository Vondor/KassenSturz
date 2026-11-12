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

use crate::read::MIN_REFERENCE_CHARS;

/// The words that say what an argument of a change list has to be
/// (US3.9). They are the same for every list, so a small model learns one
/// set; the answer's `expected_shape` is one of them.
pub(crate) mod shape {
    /// An object (`{...}`), as the call itself and every item of a list is.
    pub(crate) const OBJECT: &str = "an object";
    /// A list whose items are objects, as every list of changes is.
    pub(crate) const LIST_OF_OBJECTS: &str = "a list of objects";
    /// Text, in quotation marks.
    pub(crate) const TEXT: &str = "text";
    /// `true` or `false`, without quotation marks.
    pub(crate) const TRUE_OR_FALSE: &str = "true or false";
    /// Text that is an ID, as the read functions give it.
    pub(crate) const ID: &str = "text that is an ID, as the read functions give it";
}

/// What [`ToolError::hint`] tells the model when `measure` is refused on a
/// chart (R2-16).
pub const READY_MADE_TILE_HINT: &str = "measure is only the one number of a key figure \
(show_as \"figure\"); no chart takes it. The ready-made tiles of the Overview, such as \
\"Where your money came from\", net worth and the warnings, are not charts and can't be \
drawn as one. Call propose_view again without measure to draw this data as a chart, or tell \
the user plainly that the ready-made tile stays as it is and that they can make a chart of \
their own in the Statistic Builder, or change a chart there with \"Change step by step in the \
Statistic Builder\" in its properties.";

/// An error of a read function.
#[derive(Debug)]
pub enum ToolError {
    /// The model asked for a function that isn't in the list.
    UnknownFunction,
    /// A custom chart's description the AI wrote can't be used (D-729):
    /// too big, not readable, hiding characters, a part that isn't
    /// supported, or data it may not name. Every field is one of
    /// KassenSturz's own words or a count, never the model's text (X-8).
    BadDescription {
        /// What is wrong, by a fixed word (`docs/ERROR_CODES.md`).
        reason: &'static str,
        /// Why, for `too-big` the limit and for `not-supported` the
        /// reason word of the "not supported here" list (D-507).
        why: Option<&'static str>,
        /// The part of the description, by a word of the subset.
        part: Option<&'static str>,
        /// The layer it is in, counted from 1.
        layer: Option<usize>,
        /// How many parts are not supported.
        count: Option<usize>,
    },
    /// An argument is missing, of the wrong kind or out of range, or the
    /// call has one the function doesn't take (`arguments`).
    BadArgument {
        /// The argument's name, from the function's fixed list.
        argument: &'static str,
    },
    /// An argument of a `propose_changes` call is of the wrong kind, such as
    /// a list that is an object or an item that is a text (US3.9). It is
    /// reported as [`Self::BadArgument`]; the answer to the model adds the
    /// shape that was expected, from KassenSturz's own fixed words, never
    /// the model's text (X-8).
    NotShaped {
        /// The argument's name, from the function's fixed list.
        argument: &'static str,
        /// What the argument has to be: one of the fixed words of
        /// [`shape`].
        expected: &'static str,
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
    /// No tag of the book has that ID (US5.2, D-744).
    UnknownTag,
    /// A `reference` to search for has fewer than four letters or digits,
    /// so it would match half the book (US5.12, D-738).
    ReferenceTooShort,
    /// No bill folder is chosen on this computer, so no bill can be
    /// attached (US5.13, D-739).
    NoBillFolder,
    /// A bill to attach is not inside the bill folder once `..` and links
    /// are resolved. The path is not repeated back.
    BillOutsideFolder,
    /// A bill to attach is not a PDF, PNG or JPEG file by its name.
    BillType,
    /// A bill to attach is not a file in the bill folder.
    BillNotFound,
    /// The same bill is attached to the same transaction twice.
    Duplicate,
    /// The parts of a split the AI prepared don't add up to what the
    /// transaction did to the account (US5.1, D-737). The amounts are exact
    /// decimal text in the transaction's currency, worked out by
    /// KassenSturz from the book and the model's parts, so the model can
    /// correct its split; nothing is ever made up to fill the difference.
    PartsDontAddUp {
        /// What the transaction did to the account.
        total: String,
        /// What the parts add up to.
        sum: String,
        /// `total` minus `sum`: what a part is missing (positive: money in).
        difference: String,
    },
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
    /// `reason` says why not, so the model can tell the user (M6-B215).
    NotEditable {
        /// `split`, `transfer`, `other-currency`, `bank-delivered`,
        /// `own-date` or `own-payee`.
        reason: &'static str,
    },
    /// A new budget for a category that already has one for these days and
    /// accounts, in the book or as another new budget of the same set: the
    /// model can change the one in the book with `edit_budgets` instead, or
    /// keep only one of the new ones (M6-B216, #651). The store's own `store.budget-overlap` names the category
    /// by its ID, which the answer to a model leaves out.
    HasBudget,
    /// A new top-level category or a new account without `currency`, while
    /// the user's open accounts use more than one currency or there is none
    /// yet, so there is no currency to take by default. The schema can't
    /// say so itself: it is the same for every book, and AI tools read it
    /// once (#652).
    CurrencyNeeded,
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
    /// The name an item gave next to an ID is not the name of what the ID
    /// names: the ID was probably copied wrongly (D-675). `argument` is the
    /// ID's key.
    NameMismatch {
        /// The ID's key, such as `category_id`.
        argument: &'static str,
    },
    /// A budget amount `based_on` past spending can't be computed: nothing
    /// was spent in the periods counted, or the book doesn't reach back to
    /// their start (M6-B210). The days are the periods looked at, so the
    /// model can ask the user for an amount instead.
    NothingToBaseOn {
        /// `nothing-spent` or `book-too-short`.
        reason: &'static str,
        /// The first day of the periods looked at.
        from: kassensturz_core::Date,
        /// Their last day.
        to: kassensturz_core::Date,
    },
    /// [`ToolError::NothingToBaseOn`] for a model the user doesn't let
    /// read what categories spent: one refusal for either reason, with no
    /// days, so it tells nothing about the spending (M6-B222, D-704). The
    /// model can ask the user for an amount.
    SpendingNotShown,
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
    /// A period word in a formula the AI wrote has a number outside its
    /// range, e.g. `"last 367 days"` (US5.6, #759 point 3). Reported as
    /// `ai-tools.bad-formula` with reason `period-count`, the form's `unit`
    /// and the largest number `most` it takes, so the model can correct it.
    BadFormulaPeriodCount {
        /// `formula` or `target`.
        argument: &'static str,
        /// Where the period starts, in characters from 1.
        position: usize,
        /// `days`, `weeks`, `months` or `years`.
        unit: &'static str,
        /// The largest number the form takes; the smallest is 1.
        most: u16,
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
    /// The two transactions of a pair can't be joined as one Transfer, for
    /// the reason the store gives a join by hand (D-691, D-693): the model
    /// hears the store's own code, as the user would.
    TransferRefused(kassensturz_core::TransferError),
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
            Self::BadDescription {
                reason: "data-not-offered",
                ..
            } => Some(&crate::changes::DESCRIPTION_DATA),
            _ => None,
        }
    }

    /// Where the user switches on what this refusal says is off: the cells
    /// and locks of "What the AI may do" (D-676). Only a refusal says it,
    /// never a list of everything that is off (D-239).
    pub fn setting(&self) -> Option<Setting> {
        match self.kind() {
            Self::NotAllowed { .. } | Self::FieldLocked { .. } => Some(AI_PERMISSIONS),
            _ => None,
        }
    }

    /// What the argument has to be, when it was of the wrong kind (US3.9):
    /// the answer to the model says it, so it can fix its call.
    pub fn expected_shape(&self) -> Option<&'static str> {
        match self.kind() {
            Self::NotShaped { expected, .. } => Some(expected),
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

    /// A plain hint for the model when `measure` is refused because only a
    /// key figure takes it (R2-16): models asked for a ready-made tile such
    /// as "Where your money came from" as a chart and got only the list of
    /// ways, with nothing to tell the user. Ready-made tiles aren't charts
    /// (Q-507, D-741), so the hint says so and points to the Statistic
    /// Builder. Fixed text, never the call's.
    pub fn hint(&self) -> Option<&'static str> {
        match self.kind() {
            Self::NotForChart {
                argument: "measure",
                only_for_show_as,
                ..
            } if only_for_show_as.contains(&"figure") => Some(READY_MADE_TILE_HINT),
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

    /// The amounts a refusal names for the model, as exact decimal text
    /// KassenSturz worked out: for a split whose parts don't add up, the
    /// transaction's total, the parts' sum and the difference (D-737).
    pub fn amounts(&self) -> Option<[(&'static str, &str); 3]> {
        match self.kind() {
            Self::PartsDontAddUp {
                total,
                sum,
                difference,
            } => Some([("total", total), ("sum", sum), ("difference", difference)]),
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

/// A place in KassenSturz's settings, as the app shows it in English and
/// in German, so the AI can point the user there in their language
/// (D-676).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Setting {
    /// The path in the English app.
    pub en: &'static str,
    /// The same path in the German app.
    pub de: &'static str,
}

impl Setting {
    /// The setting as a model reads it: `{"en": ..., "de": ...}`.
    pub fn json(self) -> serde_json::Value {
        serde_json::json!({"en": self.en, "de": self.de})
    }
}

/// The AI settings' grid of what the AI may read, add, change and delete,
/// with the field locks below it (D-252).
pub const AI_PERMISSIONS: Setting = Setting {
    en: "Settings › AI provider and privacy › What the AI may do",
    de: "Einstellungen › KI-Anbieter und Datenschutz › Was die KI darf",
};

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
            Self::BadDescription {
                reason,
                why,
                part,
                layer,
                count,
            } => {
                let mut report = ErrorReport::new("ai-tools.bad-description")
                    .with("argument", ErrorParam::Keyword("description"))
                    .with("reason", ErrorParam::Keyword(reason));
                if let Some(why) = why {
                    report = report.with("why", ErrorParam::Keyword(why));
                }
                if let Some(part) = part {
                    report = report.with("part", ErrorParam::Keyword(part));
                }
                if let Some(layer) = layer {
                    report = report.with("layer", ErrorParam::count(*layer));
                }
                if let Some(count) = count {
                    report = report.with("count", ErrorParam::count(*count));
                }
                report
            }
            Self::BadArgument { argument }
            | Self::NotShaped { argument, .. }
            | Self::NotForChart { argument, .. } => ErrorReport::new("ai-tools.bad-argument")
                .with("argument", ErrorParam::Keyword(argument)),
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
            Self::UnknownTag => ErrorReport::new("ai-tools.unknown-tag"),
            Self::ReferenceTooShort => ErrorReport::new("ai-tools.reference-too-short")
                .with("min_chars", ErrorParam::count(MIN_REFERENCE_CHARS)),
            Self::NoBillFolder => ErrorReport::new("ai-tools.no-bill-folder"),
            Self::BillOutsideFolder => ErrorReport::new("ai-tools.bill-outside-folder"),
            Self::BillType => ErrorReport::new("ai-tools.bill-type"),
            Self::BillNotFound => ErrorReport::new("ai-tools.bill-not-found"),
            Self::Duplicate => ErrorReport::new("ai-tools.duplicate"),
            // The amounts go to the model through [`Self::amounts`]: a
            // report carries keywords and counts, never text.
            Self::PartsDontAddUp { .. } => ErrorReport::new("ai-tools.parts-dont-add-up"),
            Self::TooManyChanges => ErrorReport::new("ai-tools.too-many-changes"),
            Self::NameTaken => ErrorReport::new("ai-tools.name-taken"),
            Self::NotCategorizable => ErrorReport::new("ai-tools.not-categorizable"),
            Self::NotEditable { reason } => ErrorReport::new("ai-tools.not-editable")
                .with("reason", ErrorParam::Keyword(reason)),
            Self::HasBudget => ErrorReport::new("ai-tools.has-budget")
                .with("use_instead", ErrorParam::Keyword("edit_budgets")),
            Self::CurrencyNeeded => ErrorReport::new("ai-tools.currency-needed"),
            Self::NotCashAccount => ErrorReport::new("ai-tools.not-cash-account"),
            Self::NotDeletable => ErrorReport::new("ai-tools.not-deletable"),
            Self::FieldLocked { field } => {
                ErrorReport::new("ai-tools.field-locked").with("field", ErrorParam::Keyword(field))
            }
            Self::OtherCurrency => ErrorReport::new("ai-tools.other-currency"),
            Self::UnknownRule => ErrorReport::new("ai-tools.unknown-rule"),
            Self::UnknownView => ErrorReport::new("ai-tools.unknown-view"),
            Self::NameMismatch { argument } => ErrorReport::new("ai-tools.name-mismatch")
                .with("argument", ErrorParam::Keyword(argument)),
            Self::NothingToBaseOn { reason, from, to } => {
                ErrorReport::new("ai-tools.nothing-to-base-on")
                    .with("reason", ErrorParam::Keyword(reason))
                    .with("from", ErrorParam::Date(*from))
                    .with("to", ErrorParam::Date(*to))
            }
            Self::SpendingNotShown => ErrorReport::new("ai-tools.nothing-to-base-on")
                .with("reason", ErrorParam::Keyword("spending-not-readable")),
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
            Self::BadFormulaPeriodCount {
                argument,
                position,
                unit,
                most,
            } => ErrorReport::new("ai-tools.bad-formula")
                .with("argument", ErrorParam::Keyword(argument))
                .with("position", ErrorParam::count(*position))
                .with("reason", ErrorParam::Keyword("period-count"))
                .with("unit", ErrorParam::Keyword(unit))
                .with("most", ErrorParam::count(usize::from(*most))),
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
            Self::TransferRefused(error) => error.report(),
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
            ToolError::BadDescription {
                reason: "not-supported",
                why: Some("transform"),
                part: Some("transform"),
                layer: Some(1),
                count: Some(2),
            },
            ToolError::CallTooBig {
                max_bytes: crate::changes::MAX_VIEW_CALL_BYTES,
            },
            ToolError::RepeatedKey,
            ToolError::UnknownArgument,
            ToolError::UnknownAccount,
            ToolError::UnknownCategory,
            ToolError::BadPeriod,
            ToolError::UnknownTransaction,
            ToolError::UnknownTag,
            ToolError::ReferenceTooShort,
            ToolError::NoBillFolder,
            ToolError::BillOutsideFolder,
            ToolError::BillType,
            ToolError::BillNotFound,
            ToolError::Duplicate,
            ToolError::PartsDontAddUp {
                total: "-19.99".into(),
                sum: "-19.98".into(),
                difference: "-0.01".into(),
            },
            ToolError::TooManyChanges,
            ToolError::NameTaken,
            ToolError::NotCategorizable,
            ToolError::HasBudget,
            ToolError::CurrencyNeeded,
            ToolError::NotEditable { reason: "split" },
            ToolError::NotEditable { reason: "transfer" },
            ToolError::NotEditable {
                reason: "other-currency",
            },
            ToolError::NotEditable {
                reason: "bank-delivered",
            },
            ToolError::NotEditable { reason: "own-date" },
            ToolError::NotEditable {
                reason: "own-payee",
            },
            ToolError::NotCashAccount,
            ToolError::NotDeletable,
            ToolError::OtherCurrency,
            ToolError::UnknownRule,
            ToolError::UnknownView,
            ToolError::NameMismatch {
                argument: "category_id",
            },
            ToolError::NothingToBaseOn {
                reason: "nothing-spent",
                from: kassensturz_core::Date::constant(2026, 8, 1),
                to: kassensturz_core::Date::constant(2026, 10, 31),
            },
            ToolError::NothingToBaseOn {
                reason: "book-too-short",
                from: kassensturz_core::Date::constant(2026, 8, 1),
                to: kassensturz_core::Date::constant(2026, 10, 31),
            },
            ToolError::SpendingNotShown,
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
            // Joining two sides as one Transfer (US3.3, D-693).
            ToolError::NotChangeable {
                argument: "transaction_id",
                reason: "also-changed",
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
            ToolError::BadFormulaPeriodCount {
                argument: "formula",
                position: 11,
                unit: "days",
                most: 366,
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
        for expected in [
            super::shape::OBJECT,
            super::shape::LIST_OF_OBJECTS,
            super::shape::TEXT,
            super::shape::TRUE_OR_FALSE,
            super::shape::ID,
        ] {
            examples.push(ToolError::NotShaped {
                argument: "arguments",
                expected,
            });
        }
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
                "join_transfers",
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
        // A name next to an ID that doesn't fit (D-675).
        for (id, _, _) in crate::changes::NAMED_IDS {
            examples.push(ToolError::NameMismatch { argument: id });
        }
        // The MCP server's own argument of the two tools that prepare (D-674).
        examples.push(ToolError::BadArgument {
            argument: crate::changes::CHECK_ONLY,
        });
        examples
    }

    #[test]
    fn the_setting_is_named_as_the_app_shows_it() {
        // The English path is the app's own words, and the German one is
        // their translation (D-676).
        // Git may check the file out with Windows line endings.
        let po = include_str!("../../../po/de/kassensturz.po").replace("\r\n", "\n");
        let english: Vec<&str> = AI_PERMISSIONS.en.split(" › ").collect();
        let german: Vec<&str> = AI_PERMISSIONS.de.split(" › ").collect();
        assert_eq!(english.len(), german.len());
        for (en, de) in english.iter().zip(&german) {
            let entry = format!("msgid \"{en}\"\nmsgstr \"{de}\"");
            // The button that opens the page ends in an ellipsis.
            let button = format!("msgid \"{en}…\"\nmsgstr \"{de}…\"");
            assert!(
                po.contains(&entry) || po.contains(&button),
                "{en} / {de} in po/de"
            );
        }
        assert_eq!(
            ToolError::NotAllowed {
                argument: "delete_rules"
            }
            .at("delete_rules", 0)
            .setting(),
            Some(AI_PERMISSIONS)
        );
        assert_eq!(ToolError::UnknownRule.setting(), None);
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
