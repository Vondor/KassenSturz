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
//!
//! What the user has, owes and is worth and the money in and out of their
//! accounts come from the accounts' balances, which the [`Source`] reads too
//! ([`account_balances`], [`net_worth_over_time`], [`cash_flow_by_month`],
//! D-348, D-349).
//!
//! How much of each budget is spent and left comes from the budgets the
//! [`Source`] reads with the parts ([`budget_utilization`], D-346, D-347).
//!
//! What a component of a dashboard is drawn with, its period, accounts and
//! categories from its own settings, its period box or the page's filter
//! bar, is worked out here too, with the notes it shows ([`Asked`],
//! [`Document::asked`], D-350, D-351).

mod alarms;
mod average;
mod balances;
mod budget_utilization;
mod candles;
mod cash_flow;
mod catalog;
mod columns;
mod compared;
mod component;
mod dashboard_file;
mod dashboard_file_read;
mod default_overview;
mod document;
mod editor;
mod error;
mod filter;
mod formula;
pub mod grid;
mod groups;
mod listed;
mod metrics;
mod monthly;
pub mod notices;
mod over_time;
mod period;
mod schema;
mod source;
pub mod spec;
mod spending;
pub mod status;
pub mod templates;
mod text;

pub use alarms::{
    AlarmReason, AlarmResult, AlarmRule, AlarmState, BudgetLook, CantCheck, MAX_ALARMS,
    MAX_AMOUNT_CHARS, MAX_SHARE, MAX_TIMES, amount_money, amount_text, evaluate as evaluate_alarms,
};
pub use average::{AVERAGE_WORDS, Average, CountedEarly, Per, counted_early};
pub use balances::{
    ACCOUNT_BALANCES, AccountBalance, AccountBalances, Converted, CurrencyBalances, CurrencyWorth,
    NET_WORTH_OVER_TIME, NetWorthOverTime, Worth, WorthRow, account_balances, net_worth_over_time,
};
pub use budget_utilization::{
    BUDGET_UTILIZATION, BudgetGrouping, BudgetNote, BudgetPeriod, BudgetUtilization,
    budget_utilization,
};
pub use candles::{BalanceCandles, Candle, CandleLength, balance_candles};
pub use cash_flow::{CASH_FLOW_BY_MONTH, CashFlowByMonth, CashFlowRow, cash_flow_by_month};
pub use catalog::{CATALOG, DatasetInfo, GROUPS_GROUPING, dataset};
pub use columns::{
    Column, MAX_COLUMNS, SharedCategory, columns_over_time, columns_span, columns_spending,
    shared as shared_categories, unknown as unknown_column_members,
};
pub use compared::{
    COMPARE_WORDS, CompareWith, ComparedEarly, ComparedRow, SpendingCompared, compared_early,
    spending_compared,
};
pub use component::Component;
pub use dashboard_file::{
    DASHBOARD_FILE_EXTENSION, DASHBOARD_FILE_FORMAT, DASHBOARD_FILE_VERSION, DashboardFile,
    MAX_APPLICATION_CHARS, MAX_DASHBOARD_FILE_BYTES, MAX_PATH_NAMES, MAX_REFERENCE_NAME_CHARS,
    MAX_REFERENCES, Reference, dashboard_file_name, write_dashboard_file,
};
pub use dashboard_file_read::{
    BookNames, FormulaName, IncomingDashboardFile, MAX_DASHBOARD_FILE_DEPTH, MissingName, Resolved,
    ShowsMore, Widget, free_dashboard_name,
};
pub use default_overview::{DEFAULT_OVERVIEW, OVERVIEW_NAME};
pub use document::{
    DASHBOARD_TABS_ID, Document, Entry, FILTER_BAR_ID, FORMAT, Frame, Item, MAX_BYTES,
    MAX_COMPONENTS, MAX_DEPTH, MAX_NAME_CHARS, PERIOD_HEADER_ID, PERIODS, VERSION, ViewOrigin,
};
pub use editor::{Edit, Editor, MAX_UNDO};
pub use error::ReportError;
pub use filter::{
    Applied, Asked, Chosen, Dimension, Filter, FilterNote, Filters, MAX_FILTER_IDS, Origin,
    SERIES_FILTERS, SeriesAsked, SeriesFilter, Takes, is_series_filter, takes,
};
pub use formula::{
    Counted, FUNCTIONS, Formula, FormulaResult, FormulaValue, FunctionInfo, KnownName,
    MAX_FORMULA_ARGUMENTS, MAX_FORMULA_CHARS, MAX_FORMULA_DEPTH, MAX_FORMULA_NAMES,
    MAX_FORMULA_PERIODS, MAX_FORMULA_TOKENS, NumberForm, Term,
};
pub use groups::{Overlap, Shown};
/// The day of the week, for [`set_first_weekday`].
pub use jiff::civil::Weekday;
pub use listed::{
    LISTED_ROWS, Listed, ListedTransactions, MAX_LISTED, OtherSide, listed_transactions,
};
pub use metrics::{
    Change, Compare, IncomeExpenseChange, Percent, compare_income_expense, comparison_span,
    savings_rate,
};
pub use monthly::{
    INCOME_EXPENSE_BY_MONTH, IncomeExpenseByMonth, MonthRow, income_expense_by_month,
    income_expense_by_series,
};
pub use over_time::{
    CATEGORY_OVER_TIME, CategoryOverTime, ChosenSeries, MonthTotals, category_over_time,
};
pub use period::{
    MAX_FORMULA_WEEKS, MAX_FORMULA_YEARS, MAX_MONTHS, MAX_ROLLING_DAYS, MAX_ROLLING_MONTHS, Period,
    Rolling, first_weekday, is_period_word, range_word, read_range_word, set_first_weekday,
};
pub use schema::{
    ALARMS_KEY, CANDLE_WORDS, ColumnSetting, ComponentType, FORMULA_MEASURE, Kind, MAX_IDS,
    MAX_TEXT_CHARS, ORIENTATION_WORDS, Place, Property, Setting, dataset_for,
};
pub use source::Source;
pub use spending::{
    CategoryRow, Level, SPENDING_BY_CATEGORY, SpendingByCategory, spending_by_category,
    spending_by_category_shown,
};
pub use text::{TextFormat, TextTable};

#[cfg(test)]
mod alarms_tests;
#[cfg(test)]
mod budget_tests;
#[cfg(test)]
mod candles_tests;
#[cfg(test)]
mod columns_tests;
#[cfg(test)]
mod compared_tests;
#[cfg(test)]
mod component_tests;
#[cfg(test)]
mod custom_chart_tests;
#[cfg(test)]
mod dashboard_file_read_tests;
#[cfg(test)]
mod dashboard_file_tests;
#[cfg(test)]
mod dashboard_tests;
#[cfg(test)]
mod document_tests;
#[cfg(test)]
mod filter_tests;
#[cfg(test)]
mod financial_tests;
#[cfg(test)]
mod formula_tests;
#[cfg(test)]
mod grid_tests;
#[cfg(test)]
mod groups_tests;
#[cfg(test)]
mod page_widgets_tests;
#[cfg(test)]
mod tags_tests;
#[cfg(test)]
mod tests;
