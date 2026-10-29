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

mod catalog;
mod error;
mod filter;
mod metrics;
mod monthly;
mod period;
mod source;
mod spending;
mod text;

pub use catalog::{CATALOG, DatasetInfo, dataset};
pub use error::ReportError;
pub use filter::Filter;
pub use metrics::{
    Change, Compare, IncomeExpenseChange, Percent, compare_income_expense, comparison_span,
    savings_rate,
};
pub use monthly::{
    INCOME_EXPENSE_BY_MONTH, IncomeExpenseByMonth, MonthRow, income_expense_by_month,
};
pub use period::{MAX_MONTHS, Period};
pub use source::Source;
pub use spending::{
    CategoryRow, Level, SPENDING_BY_CATEGORY, SpendingByCategory, spending_by_category,
};
pub use text::{TextFormat, TextTable};

#[cfg(test)]
mod tests;
