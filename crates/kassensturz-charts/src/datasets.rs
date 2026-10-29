// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Charts made from the datasets of `kassensturz-report`.

use kassensturz_core::{CurrencyCode, Money};
use kassensturz_report::{
    INCOME_EXPENSE_BY_MONTH, IncomeExpenseByMonth, SPENDING_BY_CATEGORY, SpendingByCategory,
    TextFormat,
};

use crate::ticks::ticks;
use crate::{ChartError, ChartKind, ChartModel, Point, Series};

fn point(money: Money, format: &impl TextFormat) -> Point {
    Point {
        minor_units: money.minor_units(),
        label: format.amount(money),
    }
}

/// One major unit of the currency in minor units, the closest a mark on the
/// value axis may come to the next one. The ISO table's decimal places; a
/// book's own setting changes only how fine the marks are, not any amount.
fn major_unit(currency: CurrencyCode) -> i64 {
    10_i64.pow(u32::from(currency.iso_minor_digits().unwrap_or(2)))
}

/// The lowest and the highest value the axis has to reach.
fn extent(series: &[Series], stacked: bool) -> (i64, i64) {
    let count = series.first().map_or(0, |series| series.points.len());
    let (mut low, mut high) = (0_i64, 0_i64);
    for index in 0..count {
        let values = series.iter().map(|series| series.points[index].minor_units);
        if stacked {
            // Positive values pile up above zero, negative ones below. The
            // sums saturate: they only decide how far the axis reaches.
            let up = values
                .clone()
                .filter(|&v| v > 0)
                .fold(0_i64, i64::saturating_add);
            let down = values.filter(|&v| v < 0).fold(0_i64, i64::saturating_add);
            high = high.max(up);
            low = low.min(down);
        } else {
            for value in values {
                high = high.max(value);
                low = low.min(value);
            }
        }
    }
    (low, high)
}

/// Spending by category as a bar, pie or donut chart. Every category is one
/// point of the single series `spending`, largest first as the dataset
/// orders them. A round chart cannot show money that came back (a negative
/// total): such a slice is in the table and in the labels but has no size.
pub fn spending_by_category(
    data: &SpendingByCategory,
    kind: ChartKind,
    format: &impl TextFormat,
) -> Result<ChartModel, ChartError> {
    if !matches!(kind, ChartKind::Bar | ChartKind::Pie | ChartKind::Donut) {
        return Err(ChartError::KindNotSupported(kind));
    }
    let table = data.to_text(format);
    let categories = table.rows.iter().map(|row| row[0].clone()).collect();
    let unsorted = data.rows.iter().position(|row| row.is_unsorted());
    let series = vec![Series {
        key: "spending",
        color: 0,
        points: data
            .rows
            .iter()
            .map(|row| point(row.total, format))
            .collect(),
    }];
    let ticks = if kind.is_round() {
        Vec::new()
    } else {
        let (low, high) = extent(&series, false);
        ticks(low, high, major_unit(data.currency), data.currency, format)?
    };
    Ok(ChartModel {
        kind,
        dataset: SPENDING_BY_CATEGORY,
        currency: data.currency.to_string(),
        categories,
        unsorted,
        series,
        ticks,
        total: format.amount(data.total),
        table,
    })
}

/// Income and expenses by month as a line, bar, stacked bar or area chart:
/// the series `income` and `expenses`, one point per month, also for months
/// in which nothing happened. The total is what is left over for the whole
/// period.
pub fn income_expense_by_month(
    data: &IncomeExpenseByMonth,
    kind: ChartKind,
    format: &impl TextFormat,
) -> Result<ChartModel, ChartError> {
    if kind.is_round() {
        return Err(ChartError::KindNotSupported(kind));
    }
    let table = data.to_text(format);
    let categories = table.rows.iter().map(|row| row[0].clone()).collect();
    let series = vec![
        Series {
            key: "income",
            color: 0,
            points: data
                .rows
                .iter()
                .map(|row| point(row.income, format))
                .collect(),
        },
        Series {
            key: "expenses",
            color: 1,
            points: data
                .rows
                .iter()
                .map(|row| point(row.expenses, format))
                .collect(),
        },
    ];
    let (low, high) = extent(&series, kind == ChartKind::StackedBar);
    let ticks = ticks(low, high, major_unit(data.currency), data.currency, format)?;
    Ok(ChartModel {
        kind,
        dataset: INCOME_EXPENSE_BY_MONTH,
        currency: data.currency.to_string(),
        categories,
        unsorted: None,
        series,
        ticks,
        total: format.amount(data.net),
        table,
    })
}
