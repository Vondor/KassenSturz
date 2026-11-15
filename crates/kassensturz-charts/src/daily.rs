// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The daily balance of an account with its averages and MACD as one or two
//! panels (US7, #1036, D-794): the balance as a stepped line with the two
//! averages over it, and under it MACD, its signal line and the histogram,
//! each panel with an axis of its own. Every figure comes from
//! [`kassensturz_report::daily_balance`]; this only lays them out.

use kassensturz_core::Money;
use kassensturz_report::{DAILY_BALANCE, DailyBalance, DailyParts, DailyRow, TextFormat};

use crate::ChartError;
use crate::datasets::{major_unit, point};
use crate::model::{ChartKind, ChartModel, DailyLook, Series, Slices};
use crate::ticks::{ticks, ticks_between};

/// The key of the average of `days` days, which the screen turns into
/// "Average of the last 12 days". Every length offered has one; any other
/// (a long average set one day longer than the short one) is the plain
/// `daily_average`.
fn average_key(days: u32) -> &'static str {
    match days {
        5 => "daily_average_5",
        7 => "daily_average_7",
        9 => "daily_average_9",
        10 => "daily_average_10",
        12 => "daily_average_12",
        14 => "daily_average_14",
        20 => "daily_average_20",
        26 => "daily_average_26",
        30 => "daily_average_30",
        50 => "daily_average_50",
        90 => "daily_average_90",
        100 => "daily_average_100",
        200 => "daily_average_200",
        _ => "daily_average",
    }
}

/// The places along the axis with a name: the first day of each month; in
/// a period with fewer than two of them, every seventh day from the first.
fn axis_names(rows: &[DailyRow], format: &impl TextFormat) -> Vec<(usize, String)> {
    let months: Vec<(usize, String)> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.day.day() == 1)
        .map(|(at, row)| (at, format.month(row.day)))
        .collect();
    if months.len() >= 2 {
        return months;
    }
    rows.iter()
        .enumerate()
        .step_by(7)
        .map(|(at, row)| (at, format.day(row.day)))
        .collect()
}

/// The panels of `data` that `parts` asks for: the balance panel, the MACD
/// panel, or both, in this order. Each panel's text of a day names every
/// figure of the chart (the other panel's too, not drawn), and each has the
/// whole table.
pub fn daily_balance(
    data: &DailyBalance,
    parts: DailyParts,
    kind: ChartKind,
    format: &impl TextFormat,
) -> Result<Vec<ChartModel>, ChartError> {
    if !matches!(kind, ChartKind::Line | ChartKind::Area | ChartKind::Bar) {
        return Err(ChartError::KindNotSupported(kind));
    }
    let table = data.to_text(format);
    let categories: Vec<String> = table.rows.iter().map(|row| row[0].clone()).collect();
    let axis = axis_names(&data.rows, format);
    let series = |key, color, pick: fn(&DailyRow) -> Money| Series {
        key,
        name: None,
        color,
        points: data
            .rows
            .iter()
            .map(|row| point(pick(row), format))
            .collect(),
    };
    let balance = || series("balance", 0, |row| row.balance);
    let fast = || series(average_key(data.ema.fast), 1, |row| row.fast);
    let slow = || series(average_key(data.ema.slow), 2, |row| row.slow);
    let gap = || series("daily_gap", 3, |row| row.gap);
    let macd = || series("daily_macd", 4, |row| row.macd);
    let signal = || series("daily_signal", 5, |row| row.signal);

    let mut panels = Vec::new();
    if parts.balance_panel() {
        let mut drawn = vec![(balance(), "step")];
        if parts.averages() {
            drawn.push((fast(), "line"));
            drawn.push((slow(), "dashed"));
        }
        let (low, high) = range(&drawn);
        let ticks = ticks_between(low, high, major_unit(data.currency), data.currency, format)?;
        if parts.macd_panel() {
            drawn.push((macd(), "none"));
            drawn.push((signal(), "none"));
            drawn.push((gap(), "none"));
        }
        panels.push(panel(
            data,
            "balance",
            drawn,
            ticks,
            &categories,
            &axis,
            &table,
        ));
    }
    if parts.macd_panel() {
        let mut drawn = vec![(gap(), "bars"), (macd(), "line"), (signal(), "dashed")];
        let (low, high) = range(&drawn);
        // MACD swings around zero, so its axis always shows zero.
        let ticks = ticks(low, high, major_unit(data.currency), data.currency, format)?;
        drawn.push((balance(), "none"));
        drawn.push((fast(), "none"));
        drawn.push((slow(), "none"));
        panels.push(panel(
            data,
            "macd",
            drawn,
            ticks,
            &categories,
            &axis,
            &table,
        ));
    }
    Ok(panels)
}

/// The lowest and highest value the drawn series reach.
fn range(drawn: &[(Series, &'static str)]) -> (i64, i64) {
    let mut values = drawn
        .iter()
        .flat_map(|(series, _)| series.points.iter().map(|point| point.minor_units));
    let Some(first) = values.next() else {
        return (0, 0);
    };
    values.fold((first, first), |(low, high), value| {
        (low.min(value), high.max(value))
    })
}

fn panel(
    data: &DailyBalance,
    name: &'static str,
    drawn: Vec<(Series, &'static str)>,
    ticks: Vec<crate::model::Tick>,
    categories: &[String],
    axis: &[(usize, String)],
    table: &kassensturz_report::TextTable,
) -> ChartModel {
    let (series, draws): (Vec<Series>, Vec<&'static str>) = drawn.into_iter().unzip();
    ChartModel {
        kind: ChartKind::Line,
        dataset: DAILY_BALANCE,
        currency: data.currency.to_string(),
        categories: categories.to_vec(),
        unsorted: None,
        series,
        ticks,
        // A total of balances or averages means nothing, so there is none.
        total: String::new(),
        table: table.clone(),
        slices: Slices::default(),
        hidden: None,
        overlays: Vec::new(),
        extra_colors: Vec::new(),
        slice_colors: Vec::new(),
        daily: Some(DailyLook {
            panel: name,
            draws,
            axis: axis.to_vec(),
            starts_late: data.starts_late,
        }),
        compared: None,
    }
}
