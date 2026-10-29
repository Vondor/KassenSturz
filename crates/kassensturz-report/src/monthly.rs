// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `income_expense_by_month`: money in, money out and what is left, per
//! calendar month.

use std::collections::HashMap;

use kassensturz_core::{CurrencyCode, Date, Money};
use kassensturz_store::StoreError;

use crate::source::Kind;
use crate::{Filter, Period, ReportError, Source, TextFormat, TextTable};

/// The dataset's ID, as dashboards and the AI name it (D-277).
pub const INCOME_EXPENSE_BY_MONTH: &str = "income_expense_by_month";

/// One calendar month.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonthRow {
    /// The month's first day. A first or last month the period covers only
    /// in part counts only the days inside the period.
    pub month: Date,
    /// What came in.
    pub income: Money,
    /// What went out, less what came back.
    pub expenses: Money,
    /// `income` less `expenses`, negative when more went out than came in.
    pub net: Money,
}

/// The months of one currency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IncomeExpenseByMonth {
    /// The currency of every amount.
    pub currency: CurrencyCode,
    /// Every month the period touches, oldest first, also months in which
    /// nothing happened.
    pub rows: Vec<MonthRow>,
    /// All income, added up from the exact amounts.
    pub income: Money,
    /// All expenses.
    pub expenses: Money,
    /// All income less all expenses.
    pub net: Money,
}

/// What came in and what went out in each month of `period`, one result per
/// currency in the order of their codes. Only currencies with money in or
/// out are listed. Money moved between the user's own accounts and
/// starting balances count as neither.
pub fn income_expense_by_month(
    source: &Source,
    period: Period,
    filter: &Filter,
) -> Result<Vec<IncomeExpenseByMonth>, ReportError> {
    let overflow = |_| ReportError::Store(StoreError::AmountOverflow);
    let months = period.months();
    let position: HashMap<Date, usize> = months
        .iter()
        .enumerate()
        .map(|(position, &month)| (month, position))
        .collect();
    // Per currency, per month: income and expenses in minor units.
    let mut sums: HashMap<CurrencyCode, Vec<(Money, Money)>> = HashMap::new();
    for part in source.parts(period, filter)? {
        let Some(&at) = position.get(&part.day.first_of_month()) else {
            continue;
        };
        let month = &mut sums.entry(part.currency).or_insert_with(|| {
            vec![(Money::zero(part.currency), Money::zero(part.currency)); months.len()]
        })[at];
        let sum = match part.kind {
            Kind::Income => &mut month.0,
            Kind::Spending => &mut month.1,
        };
        *sum = sum.checked_add(part.amount).map_err(overflow)?;
    }
    let mut currencies: Vec<CurrencyCode> = sums.keys().copied().collect();
    currencies.sort_unstable();

    let mut results = Vec::new();
    for currency in currencies {
        let mut rows = Vec::with_capacity(months.len());
        let (mut income, mut expenses) = (Money::zero(currency), Money::zero(currency));
        for (&month, &(month_income, month_expenses)) in months.iter().zip(&sums[&currency]) {
            income = income.checked_add(month_income).map_err(overflow)?;
            expenses = expenses.checked_add(month_expenses).map_err(overflow)?;
            rows.push(MonthRow {
                month,
                income: month_income,
                expenses: month_expenses,
                net: month_income.checked_sub(month_expenses).map_err(overflow)?,
            });
        }
        results.push(IncomeExpenseByMonth {
            currency,
            rows,
            income,
            expenses,
            net: income.checked_sub(expenses).map_err(overflow)?,
        });
    }
    Ok(results)
}

impl IncomeExpenseByMonth {
    /// The result as a table of text with the columns `month`, `income`,
    /// `expenses` and `net`.
    pub fn to_text(&self, format: &impl TextFormat) -> TextTable {
        TextTable {
            currency: self.currency.to_string(),
            columns: vec!["month", "income", "expenses", "net"],
            rows: self
                .rows
                .iter()
                .map(|row| {
                    vec![
                        format.month(row.month),
                        format.amount(row.income),
                        format.amount(row.expenses),
                        format.amount(row.net),
                    ]
                })
                .collect(),
        }
    }
}
