// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `category_over_time`: chosen categories and groups, month by month.

use std::collections::HashMap;

use kassensturz_core::{CurrencyCode, Date, GroupMember, Money};
use kassensturz_store::StoreError;

use crate::{Filter, Overlap, Period, ReportError, Source, TextFormat, TextTable};

/// The dataset's ID, as dashboards name it (D-277).
pub const CATEGORY_OVER_TIME: &str = "category_over_time";

/// One line, area or stack of the chart: a category or one of My groups.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChosenSeries {
    /// The category or group.
    pub of: GroupMember,
    /// The category's names from the top-level category down, or the
    /// group's name alone.
    pub path: Vec<String>,
}

/// One calendar month.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonthTotals {
    /// The month's first day. A first or last month the period covers only
    /// in part counts only the days inside the period.
    pub month: Date,
    /// One figure per series, in the order of the series.
    pub totals: Vec<Money>,
}

/// The chosen series of one currency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CategoryOverTime {
    /// The currency of every amount.
    pub currency: CurrencyCode,
    /// The series, in the order they were chosen.
    pub series: Vec<ChosenSeries>,
    /// Every month the period touches, oldest first, also months in which
    /// nothing happened.
    pub rows: Vec<MonthTotals>,
    /// Each series' figure for the whole period, added up from the exact
    /// amounts.
    pub totals: Vec<Money>,
}

/// The figure of each chosen category or group in each month of `period`,
/// one result per currency in the order of their codes; only currencies
/// with money in a series are listed, and each lists every series.
///
/// A figure is money spent for a spending category or group and money
/// received for an income one, both positive (a refund makes it smaller),
/// with everything inside a category and all members of a group, each
/// category once. A chosen series is counted even when its category is
/// hidden, as a formula counts what it names (D-291). A series the book
/// doesn't know is skipped ([`Source::unknown`] says which); one chosen
/// twice is shown once. Fails with `report.groups-overlap` when two series
/// share a category and `overlap` refuses it (stacked bars).
pub fn category_over_time(
    source: &Source,
    period: Period,
    filter: &Filter,
    series: &[GroupMember],
    overlap: Overlap,
) -> Result<Vec<CategoryOverTime>, ReportError> {
    let overflow = |_| ReportError::Store(StoreError::AmountOverflow);
    let series = source.resolve(series, overlap)?;
    let months = period.months();
    let position: HashMap<Date, usize> = months
        .iter()
        .enumerate()
        .map(|(position, &month)| (month, position))
        .collect();
    // Per currency, per month, per series.
    let mut sums: HashMap<CurrencyCode, Vec<Vec<Money>>> = HashMap::new();
    for part in source.parts(period, filter)? {
        let Some(category) = part.category else {
            continue;
        };
        let Some(&at) = position.get(&part.day.first_of_month()) else {
            continue;
        };
        for (index, chosen) in series.iter().enumerate() {
            if !chosen.categories.contains(&category) {
                continue;
            }
            let month = &mut sums.entry(part.currency).or_insert_with(|| {
                vec![vec![Money::zero(part.currency); series.len()]; months.len()]
            })[at];
            month[index] = month[index].checked_add(part.amount).map_err(overflow)?;
        }
    }
    let mut currencies: Vec<CurrencyCode> = sums.keys().copied().collect();
    currencies.sort_unstable();

    let mut results = Vec::new();
    for currency in currencies {
        let mut totals = vec![Money::zero(currency); series.len()];
        let mut rows = Vec::with_capacity(months.len());
        for (&month, figures) in months.iter().zip(&sums[&currency]) {
            for (total, &figure) in totals.iter_mut().zip(figures) {
                *total = total.checked_add(figure).map_err(overflow)?;
            }
            rows.push(MonthTotals {
                month,
                totals: figures.clone(),
            });
        }
        results.push(CategoryOverTime {
            currency,
            series: series
                .iter()
                .map(|chosen| ChosenSeries {
                    of: chosen.of,
                    path: chosen.path.clone(),
                })
                .collect(),
            rows,
            totals,
        });
    }
    Ok(results)
}

impl CategoryOverTime {
    /// The result as a table of text with the columns `month`, `category`
    /// and `total`: one row per month and series, the months oldest first
    /// and the series in their order. A category's path is joined with
    /// " › "; a group's row has the group's name.
    pub fn to_text(&self, format: &impl TextFormat) -> TextTable {
        TextTable {
            currency: self.currency.to_string(),
            columns: vec!["month", "category", "total"],
            rows: self
                .rows
                .iter()
                .flat_map(|row| {
                    let month = format.month(row.month);
                    self.series
                        .iter()
                        .zip(&row.totals)
                        .map(move |(series, &total)| {
                            vec![month.clone(), series.path.join(" › "), format.amount(total)]
                        })
                })
                .collect(),
        }
    }
}
