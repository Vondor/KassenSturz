// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The reporting layer of the report designer (M6a.1, D-277).
//!
//! Charts and cards never read the book themselves: they ask for a dataset
//! here. A dataset is a read-only function of the book, a [`Period`] and a
//! [`Filter`]. Every figure is added up from the exact amounts in minor
//! units with overflow checks, never from rounded display values, and only
//! within one currency: a book with several currencies gives one result per
//! currency. Money moved between the user's own accounts and starting
//! balances count as neither income nor spending (D-021, D-089).
//!
//! The datasets keep exact [`Money`](kassensturz_core::Money); a result turns
//! into text for QML only through [`TextFormat`], which the app implements
//! with the user's locale (D-065). The crate has no UI, AI or network code.
//!
//! The dashboard grid and the editor's session with its own undo are in
//! [`grid`] and [`Editor`] (M6b.1, D-287, D-288). My groups and the hide list
//! change how the datasets by category put figures together, never a figure
//! of the book ([`Shown`], [`category_over_time`], D-291).

mod catalog;
mod component;
mod default_overview;
mod document;
mod editor;
mod error;
mod filter;
mod formula;
pub mod grid;
mod groups;
mod metrics;
mod monthly;
mod over_time;
mod period;
mod schema;
mod source;
mod spending;
mod text;

pub use catalog::{CATALOG, DatasetInfo, GROUPS_GROUPING, dataset};
pub use component::Component;
pub use default_overview::{DEFAULT_OVERVIEW, OVERVIEW_NAME};
pub use document::{
    Document, Entry, FORMAT, Frame, Item, MAX_BYTES, MAX_COMPONENTS, MAX_DEPTH, MAX_NAME_CHARS,
    PERIODS, VERSION,
};
pub use editor::{Edit, Editor, MAX_UNDO};
pub use error::ReportError;
pub use filter::Filter;
pub use formula::{
    Counted, Formula, FormulaResult, FormulaValue, MAX_FORMULA_ARGUMENTS, MAX_FORMULA_CHARS,
    MAX_FORMULA_DEPTH, MAX_FORMULA_NAMES, MAX_FORMULA_PERIODS, MAX_FORMULA_TOKENS, NumberForm,
};
pub use groups::{Overlap, Shown};
pub use metrics::{
    Change, Compare, IncomeExpenseChange, Percent, compare_income_expense, comparison_span,
    savings_rate,
};
pub use monthly::{
    INCOME_EXPENSE_BY_MONTH, IncomeExpenseByMonth, MonthRow, income_expense_by_month,
};
pub use over_time::{
    CATEGORY_OVER_TIME, CategoryOverTime, ChosenSeries, MonthTotals, category_over_time,
};
pub use period::{MAX_MONTHS, Period};
pub use schema::{
    ComponentType, Kind, MAX_IDS, MAX_TEXT_CHARS, Place, Property, Setting, dataset_for,
};
pub use source::Source;
pub use spending::{
    CategoryRow, Level, SPENDING_BY_CATEGORY, SpendingByCategory, spending_by_category,
    spending_by_category_shown,
};
pub use text::{TextFormat, TextTable};

#[cfg(test)]
mod component_tests;
#[cfg(test)]
mod dashboard_tests;
#[cfg(test)]
mod document_tests;
#[cfg(test)]
mod formula_tests;
#[cfg(test)]
mod grid_tests;
#[cfg(test)]
mod groups_tests;
#[cfg(test)]
mod tests;
