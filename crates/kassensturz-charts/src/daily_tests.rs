// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the panels of a daily balance (US7, #1036, D-794): which
//! series each panel draws and how, that each day's text names every
//! figure, and the axes. Synthetic figures only.

use kassensturz_core::{Date, Money};
use kassensturz_report::{DailyBalance, DailyParts, DailyRow, Ema};

use crate::tests::{EUR, Words};
use crate::{ChartError, ChartKind, ChartModel, daily_balance};

fn day(month: i8, day: i8) -> Date {
    Date::new(2026, month, day).unwrap()
}

/// 61 days from 1 May 2026: a balance that climbs by 10.00 a day, with
/// averages and MACD that are made up but add up.
fn data() -> DailyBalance {
    let euros = |units: i64| Money::from_minor_units(units, EUR);
    let mut rows = Vec::new();
    let mut at = day(5, 1);
    for index in 0..61_i64 {
        let balance = 100_000 + index * 1_000;
        let (fast, slow) = (balance - 3_000, balance - 7_000);
        let (macd, signal) = (fast - slow, 3_500);
        rows.push(DailyRow {
            day: at,
            balance: euros(balance),
            fast: euros(fast),
            slow: euros(slow),
            macd: euros(macd),
            signal: euros(signal),
            gap: euros(macd - signal),
        });
        at = at.tomorrow().unwrap();
    }
    DailyBalance {
        currency: EUR,
        ema: Ema::default(),
        starts_late: false,
        rows,
    }
}

fn panels(parts: DailyParts) -> Vec<ChartModel> {
    daily_balance(&data(), parts, ChartKind::Line, &Words).unwrap()
}

fn keys_and_draws(model: &ChartModel) -> Vec<(&'static str, &'static str)> {
    let look = model.daily.as_ref().unwrap();
    model
        .series
        .iter()
        .map(|series| series.key)
        .zip(look.draws.iter().copied())
        .collect()
}

#[test]
fn all_parts_are_a_balance_panel_and_a_macd_panel() {
    let panels = panels(DailyParts::All);
    assert_eq!(panels.len(), 2);
    let names: Vec<_> = panels
        .iter()
        .map(|panel| panel.daily.as_ref().unwrap().panel)
        .collect();
    assert_eq!(names, ["balance", "macd"]);
    assert_eq!(
        keys_and_draws(&panels[0]),
        [
            ("balance", "step"),
            ("daily_average_12", "line"),
            ("daily_average_26", "dashed"),
            ("daily_macd", "none"),
            ("daily_signal", "none"),
            ("daily_gap", "none"),
        ]
    );
    assert_eq!(
        keys_and_draws(&panels[1]),
        [
            ("daily_gap", "bars"),
            ("daily_macd", "line"),
            ("daily_signal", "dashed"),
            ("balance", "none"),
            ("daily_average_12", "none"),
            ("daily_average_26", "none"),
        ]
    );
    for panel in &panels {
        assert_eq!(panel.categories.len(), 61);
        assert_eq!(panel.table.rows.len(), 61);
        assert_eq!(panel.table.columns.len(), 7);
        assert!(panel.total.is_empty());
        // Every series has a point on every day, so each day's text names
        // all six figures.
        assert!(panel.series.iter().all(|series| series.points.len() == 61));
        // A series keeps its color in both panels.
        let color = |key| panel.series.iter().find(|s| s.key == key).unwrap().color;
        assert_eq!(
            [color("balance"), color("daily_gap"), color("daily_signal")],
            [0, 3, 5]
        );
    }
}

#[test]
fn the_other_parts_leave_out_what_they_do_not_show() {
    let averages = panels(DailyParts::Averages);
    assert_eq!(averages.len(), 1);
    assert_eq!(averages[0].series.len(), 3);
    let balance = panels(DailyParts::Balance);
    assert_eq!(keys_and_draws(&balance[0]), [("balance", "step")]);
    let macd = panels(DailyParts::Macd);
    assert_eq!(macd.len(), 1);
    assert_eq!(macd[0].daily.as_ref().unwrap().panel, "macd");
    // The MACD panel alone still tells the balance and the averages.
    assert_eq!(macd[0].series.len(), 6);
}

#[test]
fn the_balance_axis_spans_the_balances_and_the_macd_axis_shows_zero() {
    let panels = panels(DailyParts::All);
    let (balance, macd) = (&panels[0], &panels[1]);
    let low = balance.ticks.first().unwrap().minor_units;
    let high = balance.ticks.last().unwrap().minor_units;
    // The balances run from 1,000.00 to 1,600.00 and the long average from
    // 930.00: the axis does not go down to zero.
    assert!(low > 0 && low <= 93_000, "{low}");
    assert!(high >= 160_000, "{high}");
    let low = macd.ticks.first().unwrap().minor_units;
    let high = macd.ticks.last().unwrap().minor_units;
    assert!(low <= 0 && high >= 4_000, "{low} {high}");
}

#[test]
fn the_axis_names_the_first_day_of_each_month() {
    let panels = panels(DailyParts::Balance);
    let axis = &panels[0].daily.as_ref().unwrap().axis;
    // May starts at the first day, June at day 31 of the rows.
    assert_eq!(
        axis,
        &[(0, "2026-05".to_owned()), (31, "2026-06".to_owned())]
    );
}

#[test]
fn round_and_stacked_kinds_are_refused() {
    for kind in [ChartKind::Pie, ChartKind::Donut, ChartKind::StackedBar] {
        assert!(matches!(
            daily_balance(&data(), DailyParts::All, kind, &Words),
            Err(ChartError::KindNotSupported(_))
        ));
    }
}

#[test]
fn no_days_make_empty_panels_not_an_error() {
    let mut empty = data();
    empty.rows.clear();
    let panels = daily_balance(&empty, DailyParts::All, ChartKind::Line, &Words).unwrap();
    assert_eq!(panels.len(), 2);
    assert!(panels.iter().all(|panel| panel.categories.is_empty()));
    assert!(panels.iter().all(|panel| panel.ticks.len() >= 2));
}
