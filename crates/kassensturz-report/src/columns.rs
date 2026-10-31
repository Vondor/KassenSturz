// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Columns of the Statistic Builder (D-339): a name and the categories and
//! groups put under it. A column is one series of a chart or one row of a
//! table; its figure is the sum of what its members count.
//!
//! A column is a view on the book, never a change to it: the transactions
//! stay exactly as the bank delivered them, and a column only says how
//! their figures are put together. A category may be in several columns;
//! the figures then add up to more than was spent, which [`shared`] reports
//! so the screen can say so in plain words.

use std::cmp::Reverse;
use std::collections::HashSet;

use kassensturz_core::{
    AccountId, CategoryGroup, CurrencyCode, GroupId, GroupKind, GroupMember, Money,
};

use crate::over_time::{Line, MonthTotals, sum_lines};
use crate::{
    CategoryOverTime, CategoryRow, ChosenSeries, Filter, Formula, FormulaValue, Period,
    ReportError, Source, SpendingByCategory,
};

/// The most columns one chart or table can have.
pub const MAX_COLUMNS: usize = 12;

/// One column: a name and what is under it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Column {
    /// What the column is called; the series' name, and the name a formula
    /// uses for it.
    pub name: String,
    /// The categories (each with everything inside it) and groups put
    /// under it.
    pub members: Vec<GroupMember>,
    /// A formula in the form the book stores, whose amount is the column's
    /// figure (D-341); then `members` is empty. It may name categories,
    /// groups and the other columns (those with members), like any formula.
    pub formula: Option<String>,
}

/// A category that is counted in more than one column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedCategory {
    /// The category's names from the top-level category down.
    pub path: Vec<String>,
    /// The names of the columns that count it, in their order.
    pub columns: Vec<String>,
}

fn lines(source: &Source, columns: &[Column]) -> Vec<Line> {
    columns
        .iter()
        .map(|column| {
            let categories: HashSet<AccountId> = column
                .members
                .iter()
                .filter_map(|&member| source.counted_set(member))
                .flatten()
                .collect();
            Line {
                of: None,
                path: vec![column.name.clone()],
                categories,
            }
        })
        .collect()
}

/// Each column's figure in each month of `period`, like
/// [`crate::category_over_time`] with one series per column, in the order
/// of the columns. Overlap is allowed: a category in two columns counts in
/// both ([`shared`] names such categories). Members the book doesn't know
/// are skipped ([`unknown`] counts them).
pub fn columns_over_time(
    source: &Source,
    period: Period,
    filter: &Filter,
    columns: &[Column],
    fallback: CurrencyCode,
) -> Result<Vec<CategoryOverTime>, ReportError> {
    let mut results = sum_lines(source, period, filter, &lines(source, columns))?;
    if columns.iter().any(|column| column.formula.is_some()) {
        add_formulas(source, period, filter, columns, fallback, &mut results)?;
    }
    Ok(results)
}

/// The days a chart's columns need the book read for: `period`, and what
/// the formulas of the columns name beyond it (like [`Formula::span`]).
pub fn columns_span(columns: &[Column], period: Period) -> Result<Period, ReportError> {
    let mut span = period;
    for text in columns
        .iter()
        .filter_map(|column| column.formula.as_deref())
    {
        let wanted = Formula::parse(text)?.span(period)?;
        span = Period::new(
            span.first().min(wanted.first()),
            span.last().max(wanted.last()),
        )?;
    }
    Ok(span)
}

/// The id of the stand-in group for the column at `index`.
fn column_group(index: usize) -> GroupId {
    let mut bytes = [0xC0; 16];
    bytes[15] = u8::try_from(index).unwrap_or(u8::MAX);
    GroupId::from_bytes(bytes)
}

/// A column's figure, which must be an amount, from its formula in `period`.
fn formula_figure(
    source: &Source,
    column: &Column,
    text: &str,
    period: Period,
    filter: &Filter,
    fallback: CurrencyCode,
) -> Result<Money, ReportError> {
    match Formula::parse(text)?
        .evaluate(source, period, filter, fallback)?
        .value
    {
        FormulaValue::Amount(money) => Ok(money),
        _ => Err(ReportError::ColumnFormulaNotAmount {
            column: column.name.clone(),
        }),
    }
}

/// Puts `money` into the result of its currency, adding that result when the
/// columns had no money in the currency. `month` is the row, or `None` for
/// the figure of the whole period.
fn put(
    results: &mut Vec<CategoryOverTime>,
    columns: &[Column],
    months: &[kassensturz_core::Date],
    index: usize,
    month: Option<usize>,
    money: Money,
) {
    if money.is_zero() {
        return;
    }
    let currency = money.currency();
    let at = match results.binary_search_by_key(&currency, |result| result.currency) {
        Ok(at) => at,
        Err(at) => {
            results.insert(
                at,
                CategoryOverTime {
                    currency,
                    series: columns
                        .iter()
                        .map(|column| ChosenSeries {
                            of: None,
                            path: vec![column.name.clone()],
                        })
                        .collect(),
                    rows: months
                        .iter()
                        .map(|&month| MonthTotals {
                            month,
                            totals: vec![Money::zero(currency); columns.len()],
                        })
                        .collect(),
                    totals: vec![Money::zero(currency); columns.len()],
                },
            );
            at
        }
    };
    match month {
        Some(row) => results[at].rows[row].totals[index] = money,
        None => results[at].totals[index] = money,
    }
}

/// Works out the columns that are a formula (D-341), month by month and for
/// the whole of `period`. A formula names categories and groups as ever,
/// and also the columns that have members, by their names.
fn add_formulas(
    source: &Source,
    period: Period,
    filter: &Filter,
    columns: &[Column],
    fallback: CurrencyCode,
    results: &mut Vec<CategoryOverTime>,
) -> Result<(), ReportError> {
    let stand_ins = columns
        .iter()
        .enumerate()
        .filter(|(_, column)| column.formula.is_none())
        .map(|(index, column)| CategoryGroup {
            id: column_group(index),
            name: column.name.clone(),
            kind: GroupKind::Expense,
            position: u32::try_from(index).unwrap_or(u32::MAX),
            members: column.members.clone(),
        })
        .collect();
    let named = source.with_more_groups(stand_ins);
    let months = period.months();
    for (index, column) in columns.iter().enumerate() {
        let Some(text) = column.formula.as_deref() else {
            continue;
        };
        let whole = formula_figure(&named, column, text, period, filter, fallback)?;
        put(results, columns, &months, index, None, whole);
        for (row, &month) in months.iter().enumerate() {
            // A first or last month the period covers only in part counts
            // only the days inside it, as everywhere.
            let inside = Period::new(
                month.max(period.first()),
                month.last_of_month().min(period.last()),
            )?;
            let figure = formula_figure(&named, column, text, inside, filter, fallback)?;
            put(results, columns, &months, index, Some(row), figure);
        }
    }
    Ok(())
}

/// What each column added up to over the whole of `period`, as the rows of
/// a chart or table of spending (a pie, a ring, bars): one row per column
/// that has money, largest first, a tie keeping the order of the columns.
/// `total` is every part once; when columns share a category the rows add
/// up to more than it.
pub fn columns_spending(
    source: &Source,
    period: Period,
    filter: &Filter,
    columns: &[Column],
    fallback: CurrencyCode,
) -> Result<Vec<SpendingByCategory>, ReportError> {
    let lines = lines(source, columns);
    let mut results = Vec::new();
    for data in columns_over_time(source, period, filter, columns, fallback)? {
        let mut rows: Vec<CategoryRow> = data
            .series
            .iter()
            .zip(&data.totals)
            .filter(|(_, total)| !total.is_zero())
            .map(|(series, &total)| CategoryRow {
                category: None,
                group: None,
                path: series.path.clone(),
                total,
            })
            .collect();
        // Equal figures keep the order of the columns.
        rows.sort_by_key(|row| Reverse(row.total.minor_units()));
        // Each part once: one series that counts the categories of every
        // column.
        let all = Line {
            of: None,
            path: Vec::new(),
            categories: lines
                .iter()
                .flat_map(|line| line.categories.iter().copied())
                .collect(),
        };
        let once = sum_lines(source, period, filter, &[all])?
            .into_iter()
            .find(|each| each.currency == data.currency)
            .map_or(Money::zero(data.currency), |each| each.totals[0]);
        results.push(SpendingByCategory {
            currency: data.currency,
            rows,
            total: once,
            hidden: Vec::new(),
            hidden_total: Money::zero(data.currency),
        });
    }
    Ok(results)
}

/// The categories that more than one column counts, in the order of the
/// category tree, each with the names of its columns. Empty when no figure
/// is counted twice.
pub fn shared(source: &Source, columns: &[Column]) -> Vec<SharedCategory> {
    let lines = lines(source, columns);
    source
        .tree()
        .iter()
        .filter_map(|entry| {
            let names: Vec<String> = lines
                .iter()
                .zip(columns)
                .filter(|(line, _)| line.categories.contains(&entry.category.id))
                .map(|(_, column)| column.name.clone())
                .collect();
            (names.len() > 1).then(|| SharedCategory {
                path: entry.path.clone(),
                columns: names,
            })
        })
        .collect()
}

/// How many members of `columns` the book doesn't know (any more), so the
/// screen can say "something in this view no longer exists" and not show
/// less without a word.
pub fn unknown(source: &Source, columns: &[Column]) -> usize {
    columns
        .iter()
        .map(|column| source.unknown(&column.members).len())
        .sum()
}
