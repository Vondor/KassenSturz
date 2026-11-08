// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A dataset as the flat table a chart description sees (M6e.2, D-503): one
//! row per entry of the dataset's result, one cell per field of
//! [`SpecDataset`](super::SpecDataset), in the order of the design table.
//!
//! The renderer (`kassensturz-charts`, which draws) reads these tables and
//! nothing else, so it can't reach the book. Every amount stays an exact
//! [`Money`]; a table holds the money of one currency only, because amounts
//! of different currencies are never added (design section 2).

use kassensturz_core::{BudgetEvery, CurrencyCode, Date, Money, RoundingMode};

use crate::{
    AccountBalance, BudgetUtilization, CashFlowByMonth, CategoryOverTime, CurrencyBalances,
    CurrencyWorth, IncomeExpenseByMonth, Percent, SpendingByCategory,
};

use super::fields::{FieldValue, SpecDataset, spec_dataset};

/// The separator of a category's path in a text field, as everywhere else.
const PATH_SEPARATOR: &str = " › ";

/// One value of a flat table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cell {
    /// Text; empty for money not sorted into a category yet.
    Text(String),
    /// A calendar day.
    Day(Date),
    /// An exact amount.
    Amount(Money),
    /// A percentage in tenths, or nothing when there was nothing to
    /// compare with.
    Percent(Option<Percent>),
    /// Yes or no.
    YesNo(bool),
}

impl Cell {
    /// Whether the cell holds what `value` says.
    #[must_use]
    pub fn fits(&self, value: FieldValue) -> bool {
        matches!(
            (self, value),
            (Cell::Text(_), FieldValue::Text)
                | (Cell::Day(_), FieldValue::Day)
                | (Cell::Amount(_), FieldValue::Amount)
                | (Cell::Percent(_), FieldValue::Percent)
                | (Cell::YesNo(_), FieldValue::YesNo)
        )
    }
}

/// Why rows can't make a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowsError {
    /// A row doesn't have one cell per field, or a cell isn't what its
    /// field holds.
    WrongShape,
    /// An amount is in another currency than the table's.
    MixedCurrencies,
}

/// A dataset as a flat table of one currency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatTable {
    dataset: &'static SpecDataset,
    currency: CurrencyCode,
    decimals: u8,
    rounding: RoundingMode,
    rows: Vec<Vec<Cell>>,
}

impl FlatTable {
    /// Checks that every row fits the dataset's fields and that its money is
    /// in `currency`. The decimal places are those of ISO 4217 and the
    /// rounding is the default; the app sets the book's own with
    /// [`FlatTable::with_book_settings`] (D-027).
    ///
    /// # Errors
    ///
    /// [`RowsError`] for a row that doesn't fit.
    pub fn new(
        dataset: &'static SpecDataset,
        currency: CurrencyCode,
        rows: Vec<Vec<Cell>>,
    ) -> Result<Self, RowsError> {
        for row in &rows {
            if row.len() != dataset.fields().len() {
                return Err(RowsError::WrongShape);
            }
            for (cell, field) in row.iter().zip(dataset.fields()) {
                if !cell.fits(field.value()) {
                    return Err(RowsError::WrongShape);
                }
                if let Cell::Amount(money) = cell
                    && money.currency() != currency
                {
                    return Err(RowsError::MixedCurrencies);
                }
            }
        }
        Ok(Self {
            dataset,
            currency,
            decimals: currency.iso_minor_digits().unwrap_or(2),
            rounding: RoundingMode::default(),
            rows,
        })
    }

    /// The book's decimal places for the currency and its rounding mode,
    /// which decide how an amount is compared with a number in a filter and
    /// how a mean is rounded (D-027).
    #[must_use]
    pub const fn with_book_settings(mut self, decimals: u8, rounding: RoundingMode) -> Self {
        self.decimals = decimals;
        self.rounding = rounding;
        self
    }

    /// The dataset the rows are of.
    #[must_use]
    pub const fn dataset(&self) -> &'static SpecDataset {
        self.dataset
    }

    /// The currency of every amount.
    #[must_use]
    pub const fn currency(&self) -> CurrencyCode {
        self.currency
    }

    /// The decimal places of the currency in this book.
    #[must_use]
    pub const fn decimals(&self) -> u8 {
        self.decimals
    }

    /// The rounding mode of the book.
    #[must_use]
    pub const fn rounding(&self) -> RoundingMode {
        self.rounding
    }

    /// The rows.
    #[must_use]
    pub fn rows(&self) -> &[Vec<Cell>] {
        &self.rows
    }

    /// `spending_by_category`: the rows of the result, hidden categories
    /// left out as the result leaves them out.
    #[must_use]
    pub fn from_spending(data: &SpendingByCategory) -> Self {
        let rows = data
            .rows
            .iter()
            .map(|row| {
                vec![
                    Cell::Text(row.path.join(PATH_SEPARATOR)),
                    Cell::Text(row.path.first().cloned().unwrap_or_default()),
                    Cell::Amount(row.total),
                    currency_cell(data.currency),
                ]
            })
            .collect();
        build("spending_by_category", data.currency, rows)
    }

    /// `income_expense_by_month`: one row per month.
    #[must_use]
    pub fn from_monthly(data: &IncomeExpenseByMonth) -> Self {
        let rows = data
            .rows
            .iter()
            .map(|row| {
                vec![
                    Cell::Day(row.month),
                    Cell::Amount(row.income),
                    Cell::Amount(row.expenses),
                    Cell::Amount(row.net),
                    currency_cell(data.currency),
                ]
            })
            .collect();
        build("income_expense_by_month", data.currency, rows)
    }

    /// `account_balances`: one row per account. A debt is shown as a
    /// positive amount with `owed` set, as the built-in table has it (D-370).
    #[must_use]
    pub fn from_balances(data: &CurrencyBalances) -> Self {
        let rows = data
            .rows
            .iter()
            .map(|row: &AccountBalance| {
                let shown = if row.owed {
                    // A booked debt is negative; the checked negation can
                    // only fail at the smallest possible amount, which is
                    // kept as it is.
                    row.balance.checked_neg().unwrap_or(row.balance)
                } else {
                    row.balance
                };
                vec![
                    Cell::Text(row.name.clone()),
                    Cell::YesNo(row.owed),
                    Cell::YesNo(row.closed),
                    Cell::Amount(shown),
                    currency_cell(data.currency),
                ]
            })
            .collect();
        build("account_balances", data.currency, rows)
    }

    /// `net_worth_over_time`: one row per month end. The extra point of a
    /// period inside one month (what you were worth when it began) is not a
    /// month and is left out.
    #[must_use]
    pub fn from_worth(data: &CurrencyWorth) -> Self {
        let rows = data
            .rows
            .iter()
            .filter(|row| !row.opening)
            .map(|row| {
                vec![
                    Cell::Day(row.month),
                    Cell::Amount(row.worth.assets),
                    Cell::Amount(row.worth.liabilities),
                    Cell::Amount(row.worth.net_worth),
                    currency_cell(data.currency),
                ]
            })
            .collect();
        build("net_worth_over_time", data.currency, rows)
    }

    /// `cash_flow_by_month`: one row per month.
    #[must_use]
    pub fn from_cash_flow(data: &CashFlowByMonth) -> Self {
        let rows = data
            .rows
            .iter()
            .map(|row| {
                vec![
                    Cell::Day(row.month),
                    Cell::Amount(row.start),
                    Cell::Amount(row.money_in),
                    Cell::Amount(row.money_out),
                    Cell::Amount(row.net_change),
                    Cell::Amount(row.end),
                    currency_cell(data.currency),
                ]
            })
            .collect();
        build("cash_flow_by_month", data.currency, rows)
    }

    /// `category_over_time`: one row per month and chosen series.
    #[must_use]
    pub fn from_over_time(data: &CategoryOverTime) -> Self {
        let mut rows = Vec::with_capacity(data.rows.len() * data.series.len());
        for month in &data.rows {
            for (chosen, total) in data.series.iter().zip(&month.totals) {
                rows.push(vec![
                    Cell::Day(month.month),
                    Cell::Text(chosen.path.join(PATH_SEPARATOR)),
                    Cell::Amount(*total),
                    currency_cell(data.currency),
                ]);
            }
        }
        build("category_over_time", data.currency, rows)
    }

    /// `budget_utilization`: one row per budget and period of `currency`.
    /// The budgets of other currencies are not in it.
    #[must_use]
    pub fn from_budgets(currency: CurrencyCode, budgets: &[BudgetUtilization]) -> Self {
        let mut rows = Vec::new();
        for budget in budgets.iter().filter(|budget| budget.currency == currency) {
            for period in &budget.periods {
                rows.push(vec![
                    Cell::Text(budget.categories_text(PATH_SEPARATOR)),
                    Cell::Text(every_word(budget.every).to_owned()),
                    Cell::Day(period.first_day),
                    Cell::Day(period.last_day),
                    Cell::Amount(period.planned),
                    Cell::Amount(period.carried_in),
                    Cell::Amount(period.available),
                    Cell::Amount(period.spent),
                    Cell::Amount(period.left),
                    Cell::Percent(period.used),
                    Cell::YesNo(period.carries_over),
                    currency_cell(currency),
                ]);
            }
        }
        build("budget_utilization", currency, rows)
    }
}

/// The currencies the budgets are in, in the order of their codes, one table
/// each.
#[must_use]
pub fn budget_tables(budgets: &[BudgetUtilization]) -> Vec<FlatTable> {
    let mut currencies: Vec<CurrencyCode> = budgets.iter().map(|budget| budget.currency).collect();
    currencies.sort_by_key(ToString::to_string);
    currencies.dedup();
    currencies
        .into_iter()
        .map(|currency| FlatTable::from_budgets(currency, budgets))
        .collect()
}

fn currency_cell(currency: CurrencyCode) -> Cell {
    Cell::Text(currency.to_string())
}

/// The word of a budget's period in the field `every`, the same as the
/// book's and the AI's: a salary month is a month that starts on another
/// day (D-651), and its rows' days say which.
const fn every_word(every: BudgetEvery) -> &'static str {
    match every {
        BudgetEvery::Month | BudgetEvery::SalaryMonth { .. } => "month",
        BudgetEvery::Quarter => "quarter",
        BudgetEvery::Year => "year",
        BudgetEvery::Range => "range",
    }
}

/// Builds the table of a known dataset from rows made in the order of its
/// fields. A mismatch is a mistake in this file, which the tests of every
/// dataset catch, so it is not an error value.
fn build(id: &str, currency: CurrencyCode, rows: Vec<Vec<Cell>>) -> FlatTable {
    let dataset = spec_dataset(id).expect("the dataset is one of the seven");
    FlatTable::new(dataset, currency, rows).expect("the rows follow the dataset's fields")
}
