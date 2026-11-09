// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Charts made from the datasets of `kassensturz-report`.

use kassensturz_core::{CurrencyCode, Money};
use kassensturz_report::{
    Average, CATEGORY_OVER_TIME, CategoryOverTime, INCOME_EXPENSE_BY_MONTH, IncomeExpenseByMonth,
    Per, Percent, SPENDING_BY_CATEGORY, SpendingByCategory, TextFormat, TextTable,
};

use crate::ticks::ticks;
use crate::{
    ChartError, ChartKind, ChartModel, HiddenNote, MAX_SLICES, NAMED_FROM_TENTHS, Point, Series,
    Slices,
};

pub(crate) fn point(money: Money, format: &impl TextFormat) -> Point {
    Point {
        minor_units: money.minor_units(),
        label: format.amount(money),
    }
}

/// One major unit of the currency in minor units, the closest a mark on the
/// value axis may come to the next one. The ISO table's decimal places; a
/// book's own setting changes only how fine the marks are, not any amount.
pub(crate) fn major_unit(currency: CurrencyCode) -> i64 {
    10_i64.pow(u32::from(currency.iso_minor_digits().unwrap_or(2)))
}

/// The lowest and the highest value the axis has to reach.
pub(crate) fn extent(series: &[Series], stacked: bool) -> (i64, i64) {
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

/// The note about hidden categories, `None` when nothing is hidden. The
/// largest hidden category is named; the screen writes the sentence. The
/// amount is the exact one or its `average`.
fn hidden_note(
    data: &SpendingByCategory,
    format: &impl TextFormat,
    average: Option<&Average>,
) -> Option<HiddenNote> {
    let first = data.hidden.first()?;
    Some(HiddenNote {
        first: first.path.join(" › "),
        more: data.hidden.len() - 1,
        total: format.amount(shown_as(data.hidden_total, average)),
    })
}

/// `money` as a chart shows it: the exact amount, or its average.
fn shown_as(money: Money, average: Option<&Average>) -> Money {
    average.map_or(money, |average| average.of(money))
}

/// The fixed name of the amount column of a spending table: `total`, or
/// the average it shows (D-648), which the UI translates.
fn amount_column(average: Option<&Average>) -> &'static str {
    match average.map(|average| average.per) {
        None => "total",
        Some(Per::Month) => "per_month",
        Some(Per::Quarter) => "per_quarter",
        Some(Per::Year) => "per_year",
    }
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
    spending_by_category_as(data, kind, format, None)
}

/// [`spending_by_category`] with every amount shown as its average
/// (US1.13, D-648): each row, "Other", the total and the hidden amount are
/// divided once from their exact amounts, never added up from rounded
/// averages. The shares of a round chart come from the exact amounts, so
/// they stay what the total shows; the table's amount column is named for
/// the average (`per_month`, `per_quarter`, `per_year`).
pub fn spending_by_category_as(
    data: &SpendingByCategory,
    kind: ChartKind,
    format: &impl TextFormat,
    average: Option<&Average>,
) -> Result<ChartModel, ChartError> {
    if !matches!(
        kind,
        ChartKind::Bar | ChartKind::StackedBar | ChartKind::Pie | ChartKind::Donut
    ) {
        return Err(ChartError::KindNotSupported(kind));
    }
    // One bar split into the categories is a pie as a bar (D-666): the same
    // slices, so the same rows are left out and put into "Other".
    let parts = kind.is_round() || kind == ChartKind::StackedBar;
    let mut shown = if parts {
        round_rows(data, format, average)?
    } else {
        Shown::plain(data, format, average)
    };
    // The points are drawn and the axis is marked in what the labels say;
    // the shares were worked out before, from the exact amounts.
    if let Some(average) = average {
        for row in &mut shown.rows {
            row.minor_units = average.of_minor(row.minor_units);
        }
    }
    let table = TextTable {
        currency: data.currency.to_string(),
        columns: vec!["category", amount_column(average)],
        rows: shown
            .rows
            .iter()
            .map(|row| vec![row.name.clone(), row.label.clone()])
            .collect(),
    };
    if kind == ChartKind::StackedBar {
        return one_split_bar(data, shown, table, format, average);
    }
    let categories = shown.rows.iter().map(|row| row.name.clone()).collect();
    let unsorted = shown.rows.iter().position(|row| row.unsorted);
    let series = vec![Series {
        key: "spending",
        name: None,
        color: 0,
        points: shown
            .rows
            .iter()
            .map(|row| Point {
                minor_units: row.minor_units,
                label: row.label.clone(),
            })
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
        total: format.amount(shown_as(data.total, average)),
        table,
        slices: shown.slices,
        hidden: hidden_note(data, format, average),
        overlays: Vec::new(),
        extra_colors: Vec::new(),
        slice_colors: Vec::new(),
    })
}

/// Spending as one bar split into its categories (D-666): a series for each
/// slice a pie would draw, largest first, each with one point, so the bar is
/// as tall as the drawn spending. "Other" is the series `other`, which the
/// screen names; every other slice carries its category's name (or the
/// words for money not sorted yet). A refund larger than its spending is
/// in the table only, as it is for a pie. The one place on the axis has no
/// name.
fn one_split_bar(
    data: &SpendingByCategory,
    shown: Shown,
    table: TextTable,
    format: &impl TextFormat,
    average: Option<&Average>,
) -> Result<ChartModel, ChartError> {
    let series: Vec<Series> = shown
        .rows
        .iter()
        .enumerate()
        .filter(|(index, _)| shown.slices.undrawn.get(*index) != Some(&true))
        .map(|(index, row)| {
            let other = shown.slices.other == Some(index);
            Series {
                key: if other { "other" } else { "series" },
                name: (!other).then(|| row.name.clone()),
                color: ChartModel::color_of(index),
                points: vec![Point {
                    minor_units: row.minor_units,
                    label: row.label.clone(),
                }],
            }
        })
        .collect();
    let (low, high) = extent(&series, true);
    let ticks = ticks(low, high, major_unit(data.currency), data.currency, format)?;
    Ok(ChartModel {
        kind: ChartKind::StackedBar,
        dataset: SPENDING_BY_CATEGORY,
        currency: data.currency.to_string(),
        categories: vec![String::new()],
        unsorted: None,
        series,
        ticks,
        total: format.amount(shown_as(data.total, average)),
        table,
        slices: shown.slices,
        hidden: hidden_note(data, format, average),
        overlays: Vec::new(),
        extra_colors: Vec::new(),
        slice_colors: Vec::new(),
    })
}

/// A row of a spending chart as shown: its name (empty for money not sorted
/// yet), its amount in minor units with the text it is shown as, and whether
/// it is the money not sorted yet.
pub(crate) struct ShownRow {
    pub(crate) name: String,
    pub(crate) minor_units: i64,
    pub(crate) label: String,
    pub(crate) unsorted: bool,
}

/// The rows of a spending chart and, for a round one, what it says about
/// its slices.
pub(crate) struct Shown {
    pub(crate) rows: Vec<ShownRow>,
    pub(crate) slices: Slices,
}

impl Shown {
    /// Every row of the dataset with its exact amount, labelled with the
    /// amount or its `average`.
    fn plain(
        data: &SpendingByCategory,
        format: &impl TextFormat,
        average: Option<&Average>,
    ) -> Self {
        let rows = data
            .rows
            .iter()
            .map(|row| ShownRow {
                name: if row.is_unsorted() {
                    format.unsorted()
                } else {
                    row.path.join(" › ")
                },
                minor_units: row.total.minor_units(),
                label: format.amount(shown_as(row.total, average)),
                unsorted: row.is_unsorted(),
            })
            .collect();
        Self {
            rows,
            slices: Slices::default(),
        }
    }
}

/// The rows of a pie, donut or ring of a spending result (D-291).
fn round_rows(
    data: &SpendingByCategory,
    format: &impl TextFormat,
    average: Option<&Average>,
) -> Result<Shown, ChartError> {
    let plain = Shown::plain(data, format, average);
    round_slices(plain.rows, format, |minor_units| {
        format.amount(shown_as(
            Money::from_minor_units(minor_units, data.currency),
            average,
        ))
    })
}

/// The rows of a pie, donut or ring (D-291): at most [`MAX_SLICES`] slices,
/// the seven largest and "Other" when there would be more, each with its
/// share; then the rows that cannot be drawn (a refund larger than the
/// spending, or nothing), listed with a note and never as slices. The
/// rows come largest first, so the first seven are the largest. `label`
/// writes the amount of "Other".
pub(crate) fn round_slices(
    plain: Vec<ShownRow>,
    format: &impl TextFormat,
    label: impl Fn(i64) -> String,
) -> Result<Shown, ChartError> {
    let (drawn, undrawn): (Vec<ShownRow>, Vec<ShownRow>) =
        plain.into_iter().partition(|row| row.minor_units > 0);
    let mut rows = Vec::new();
    let mut holds = Vec::new();
    let mut other = None;
    if drawn.len() > MAX_SLICES {
        let mut rest = drawn;
        let kept: Vec<ShownRow> = rest.drain(..MAX_SLICES - 1).collect();
        rows.extend(kept);
        let sum = rest
            .iter()
            .try_fold(0_i64, |sum, row| sum.checked_add(row.minor_units))
            .ok_or(ChartError::Overflow)?;
        holds = rest
            .iter()
            .map(|row| (row.name.clone(), row.label.clone()))
            .collect();
        other = Some(rows.len());
        rows.push(ShownRow {
            name: String::new(),
            minor_units: sum,
            label: label(sum),
            unsorted: false,
        });
    } else {
        rows.extend(drawn);
    }
    let whole = rows
        .iter()
        .try_fold(0_i64, |sum, row| sum.checked_add(row.minor_units))
        .ok_or(ChartError::Overflow)?;
    let drawn_count = rows.len();
    rows.extend(undrawn);
    let mut shares = Vec::with_capacity(rows.len());
    let mut named = Vec::with_capacity(rows.len());
    let mut flags = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        if index >= drawn_count {
            shares.push(String::new());
            named.push(false);
            flags.push(true);
            continue;
        }
        let tenths = share_tenths(row.minor_units, whole);
        shares.push(format.percent(Percent::from_tenths(tenths)));
        named.push(tenths >= NAMED_FROM_TENTHS);
        flags.push(false);
    }
    Ok(Shown {
        rows,
        slices: Slices {
            other,
            holds,
            shares,
            named,
            undrawn: flags,
        },
    })
}

/// `part` as tenths of a percent of `whole`, rounded half away from zero.
/// Both are positive here; the wide type keeps the product from overflowing.
fn share_tenths(part: i64, whole: i64) -> i64 {
    if whole <= 0 {
        return 0;
    }
    let (part, whole) = (i128::from(part), i128::from(whole));
    i64::try_from((part * 1000 + whole / 2) / whole).unwrap_or(1000)
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
            name: None,
            color: 0,
            points: data
                .rows
                .iter()
                .map(|row| point(row.income, format))
                .collect(),
        },
        Series {
            key: "expenses",
            name: None,
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
        slices: Slices::default(),
        hidden: None,
        overlays: Vec::new(),
        extra_colors: Vec::new(),
        slice_colors: Vec::new(),
    })
}

/// The chosen categories and groups month by month as a line, area, bar or
/// stacked-bar chart (bars and the table since D-666): one series for each, in the order they were chosen,
/// one point per month. The total is what the series add up to for the whole
/// period, which for series that share a category is more than was spent
/// (the book refuses that for stacked bars, D-291).
pub fn category_over_time(
    data: &CategoryOverTime,
    kind: ChartKind,
    format: &impl TextFormat,
) -> Result<ChartModel, ChartError> {
    if !matches!(
        kind,
        ChartKind::Line | ChartKind::Area | ChartKind::Bar | ChartKind::StackedBar
    ) {
        return Err(ChartError::KindNotSupported(kind));
    }
    let table = data.to_text(format);
    let categories = data
        .rows
        .iter()
        .map(|row| format.month(row.month))
        .collect();
    let series: Vec<Series> = data
        .series
        .iter()
        .enumerate()
        .map(|(index, chosen)| Series {
            key: "series",
            name: Some(chosen.path.join(" › ")),
            color: ChartModel::color_of(index),
            points: data
                .rows
                .iter()
                .map(|row| point(row.totals[index], format))
                .collect(),
        })
        .collect();
    let (low, high) = extent(&series, kind == ChartKind::StackedBar);
    let ticks = ticks(low, high, major_unit(data.currency), data.currency, format)?;
    let total = Money::checked_sum(data.currency, data.totals.iter().copied())
        .map_err(|_| ChartError::Overflow)?;
    Ok(ChartModel {
        kind,
        dataset: CATEGORY_OVER_TIME,
        currency: data.currency.to_string(),
        categories,
        unsorted: None,
        series,
        ticks,
        total: format.amount(total),
        table,
        slices: Slices::default(),
        hidden: None,
        overlays: Vec::new(),
        extra_colors: Vec::new(),
        slice_colors: Vec::new(),
    })
}
