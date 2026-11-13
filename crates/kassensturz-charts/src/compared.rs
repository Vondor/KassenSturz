// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Spending by category next to a comparison (US3.1a, #587, D-687): two
//! bars per category, this period's and the comparison's.

use kassensturz_report::{
    Average, Change, Compare, CompareWith, SPENDING_BY_CATEGORY, SpendingCompared, TextFormat,
    TextTable,
};

use crate::datasets::{amount_column, extent, hidden_note, major_unit, shown_as};
use crate::ticks::ticks;
use crate::{ChangeMark, ChartError, ChartKind, ChartModel, ComparedTable, Point, Series, Slices};

/// The fixed name of a comparison, as the key of its series and its table
/// column, which the screen names: `previous_period`, `same_last_year`,
/// `average_3`, `average_6` or `average_12`.
pub const fn compare_key(compare: CompareWith) -> &'static str {
    match compare {
        CompareWith::Earlier(Compare::PreviousPeriod) => "previous_period",
        CompareWith::Earlier(Compare::SameLastYear) => "same_last_year",
        CompareWith::Average(3) => "average_3",
        CompareWith::Average(6) => "average_6",
        CompareWith::Average(_) => "average_12",
    }
}

/// Spending by category as bars next to its comparison: series 0 is
/// `spending`, this period, and series 1 the comparison, keyed by
/// [`compare_key`], with the second palette colour. Each row's comparison
/// is its exact sum over the comparison's days divided once by `earlier`
/// ([`CompareWith::divisor`]); this period's figure is shown as its
/// `average`, if the chart has one (D-648). The total stays the period's
/// own. The table has the category, the amount, the comparison, and the
/// change in amount and in percent (`change`, `change_percent`), the
/// percent empty when the comparison is zero. Each row and the total row
/// carry a [`ChangeMark`] in [`ChartModel::compared`] (US6, #789, D-760):
/// less spent than the comparison is good news.
///
/// Only bars compare (a table is drawn from the same model): a stacked
/// bar is one bar split into its categories (D-666) and has no place for a
/// second bar.
pub fn spending_by_category_compared(
    data: &SpendingCompared,
    compare: CompareWith,
    kind: ChartKind,
    format: &impl TextFormat,
    average: Option<&Average>,
    earlier: &Average,
) -> Result<ChartModel, ChartError> {
    if kind != ChartKind::Bar {
        return Err(ChartError::KindNotSupported(kind));
    }
    let currency = data.now.currency;
    let mut now_points = Vec::new();
    let mut then_points = Vec::new();
    let mut categories = Vec::new();
    let mut rows = Vec::new();
    let mut marks = Vec::new();
    for compared in &data.rows {
        let name = if compared.row.is_unsorted() {
            format.unsorted()
        } else {
            compared.row.path.join(" › ")
        };
        let now = shown_as(compared.row.total, average);
        let then = earlier.of(compared.earlier);
        let change = Change::new(then, now).map_err(|_| ChartError::Overflow)?;
        marks.push(spending_mark(&change));
        now_points.push(Point {
            minor_units: now.minor_units(),
            label: format.amount(now),
        });
        then_points.push(Point {
            minor_units: then.minor_units(),
            label: format.amount(then),
        });
        rows.push(change_row(name.clone(), &change, format));
        categories.push(name);
    }
    // The total compares the period's own total with all the comparison
    // counts, each divided as its rows are.
    let total = Change::new(
        earlier.of(data.earlier_total),
        shown_as(data.now.total, average),
    )
    .map_err(|_| ChartError::Overflow)?;
    let unsorted = data.rows.iter().position(|row| row.row.is_unsorted());
    let key = compare_key(compare);
    let series = vec![
        Series {
            key: "spending",
            name: None,
            color: 0,
            points: now_points,
        },
        Series {
            key,
            name: None,
            color: 1,
            points: then_points,
        },
    ];
    let (low, high) = extent(&series, false);
    let ticks = ticks(low, high, major_unit(currency), currency, format)?;
    Ok(ChartModel {
        kind,
        dataset: SPENDING_BY_CATEGORY,
        currency: currency.to_string(),
        categories,
        unsorted,
        series,
        ticks,
        total: format.amount(total.current),
        table: TextTable {
            currency: currency.to_string(),
            columns: vec![
                "category",
                amount_column(average),
                key,
                "change",
                "change_percent",
            ],
            rows,
        },
        slices: Slices::default(),
        hidden: hidden_note(&data.now, format, average),
        overlays: Vec::new(),
        extra_colors: Vec::new(),
        slice_colors: Vec::new(),
        compared: Some(ComparedTable {
            marks,
            total: change_row(String::new(), &total, format),
            total_mark: spending_mark(&total),
        }),
    })
}

/// A row of the table: its name, the amount, the comparison, and the change
/// in amount and in percent, the percent empty when the comparison is zero.
fn change_row(name: String, change: &Change, format: &impl TextFormat) -> Vec<String> {
    vec![
        name,
        format.amount(change.current),
        format.amount(change.previous),
        format.amount(change.amount),
        change
            .percent
            .map_or_else(String::new, |percent| format.percent(percent)),
    ]
}

/// How spending changed: less than the comparison is good news (D-760).
pub(crate) fn spending_mark(change: &Change) -> ChangeMark {
    let (direction, tone) = match change.moved() {
        core::cmp::Ordering::Greater => ("up", "bad"),
        core::cmp::Ordering::Less => ("down", "good"),
        core::cmp::Ordering::Equal => ("same", "same"),
    };
    ChangeMark {
        direction,
        tone,
        reach: change.reach(),
    }
}
