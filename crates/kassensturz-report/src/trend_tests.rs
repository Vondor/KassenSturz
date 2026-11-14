// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the moving averages and MACD (US7, #875, D-777): the exact
//! sums, the one rounding, the months read before the period, and the
//! properties that an average stays between the smallest and the largest
//! month it spans and that MACD's gap adds up. Synthetic names only.

use kassensturz_core::{Account, AccountId, AccountKind, Split, Transaction, TransactionId};
use kassensturz_store::CategoryEntry;
use proptest::prelude::*;

use super::*;
use crate::tests::{EUR, date, money, period};

/// A book with one own account, a salary and a rent category, and one
/// booking each month of 2026 from January on: `income[i]` came in and
/// `spent[i]` went out in month `i + 1`.
fn source_of(income: &[i64], spent: &[i64]) -> Source {
    let make = |name: &str, kind| Account {
        id: AccountId::generate(),
        name: name.into(),
        kind,
        currency: EUR,
        account_type: None,
        parent: None,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    };
    let own = make("Current account", AccountKind::Asset);
    let salary = make("Salary", AccountKind::Income);
    let rent = make("Rent", AccountKind::Expense);
    let tree = vec![
        CategoryEntry {
            path: vec!["Salary".into()],
            category: salary.clone(),
        },
        CategoryEntry {
            path: vec!["Rent".into()],
            category: rent.clone(),
        },
    ];
    let mut bookings = Vec::new();
    let mut book = |month: i8, category: AccountId, amount: i64| {
        if amount != 0 {
            bookings.push(
                Transaction::new(
                    TransactionId::generate(),
                    date(2026, month, 10),
                    None,
                    vec![
                        Split::new(own.id, money(amount)),
                        Split::new(category, money(-amount)),
                    ],
                )
                .expect("balanced"),
            );
        }
    };
    for (at, amount) in income.iter().enumerate() {
        book(at as i8 + 1, salary.id, *amount);
    }
    for (at, amount) in spent.iter().enumerate() {
        book(at as i8 + 1, rent.id, -*amount);
    }
    let whole = period(date(2025, 1, 1), date(2026, 12, 31));
    Source::from_parts(whole, vec![own, salary, rent], tree, bookings)
}

fn minors(rows: &[AverageRow]) -> Vec<(i64, i64)> {
    rows.iter()
        .map(|row| (row.value.minor_units(), row.average.minor_units()))
        .collect()
}

#[test]
fn a_moving_average_is_the_exact_sum_of_its_months_divided_once() {
    let source = source_of(&[100, 200, 300, 400, 500], &[]);
    let march_to_may = period(date(2026, 3, 1), date(2026, 5, 31));
    let trend = Trend {
        length: 3,
        ..Trend::default()
    };
    let result = moving_average(
        &source,
        march_to_may,
        &Filter::default(),
        TrendSeries::Income,
        trend,
    )
    .expect("average");
    assert_eq!(result.len(), 1);
    // March (100 + 200 + 300) / 3, April 300, May 400; January and February
    // are read for the first window and not shown.
    assert_eq!(
        minors(&result[0].rows),
        [(300, 200), (400, 300), (500, 400)]
    );
}

#[test]
fn an_average_is_rounded_once_in_the_books_rounding() {
    let source = source_of(&[100, 100, 101], &[]);
    let whole = period(date(2026, 1, 1), date(2026, 3, 31));
    let trend = Trend {
        length: 3,
        ..Trend::default()
    };
    let result = moving_average(
        &source,
        whole,
        &Filter::default(),
        TrendSeries::Income,
        trend,
    )
    .expect("average");
    // 301 / 3 = 100.33…, once, to 100; January and February divide by 3
    // too, since the months before the first transaction count as zero.
    assert_eq!(minors(&result[0].rows), [(100, 33), (100, 67), (101, 100)]);
}

#[test]
fn the_series_chosen_is_followed() {
    let source = source_of(&[1000, 1000, 1000], &[400, 600, 800]);
    let whole = period(date(2026, 1, 1), date(2026, 3, 31));
    let trend = Trend {
        length: 2,
        ..Trend::default()
    };
    let rows = |series| {
        let result =
            moving_average(&source, whole, &Filter::default(), series, trend).expect("average");
        minors(&result[0].rows)
    };
    assert_eq!(
        rows(TrendSeries::Income),
        [(1000, 500), (1000, 1000), (1000, 1000)]
    );
    assert_eq!(
        rows(TrendSeries::Expenses),
        [(400, 200), (600, 500), (800, 700)]
    );
    assert_eq!(rows(TrendSeries::Net), [(600, 300), (400, 500), (200, 300)]);
}

#[test]
fn macd_is_the_short_average_less_the_long_one_with_a_signal_and_a_gap() {
    // Net 100 a month more than the month before.
    let source = source_of(&[100, 200, 300, 400, 500, 600], &[]);
    let whole = period(date(2026, 4, 1), date(2026, 6, 30));
    let trend = Trend {
        length: 2,
        slow: 4,
        signal: 2,
    };
    let result = macd(&source, whole, &Filter::default(), TrendSeries::Net, trend).expect("macd");
    let rows: Vec<(i64, i64, i64)> = result[0]
        .rows
        .iter()
        .map(|row| {
            (
                row.macd.minor_units(),
                row.signal.minor_units(),
                row.gap.minor_units(),
            )
        })
        .collect();
    // April: (300+400)/2 - (100+200+300+400)/4 = 350 - 250 = 100; March was
    // (200+300)/2 - (0+100+200+300)/4 = 250 - 150 = 100. Steady growth keeps
    // the gap between the averages at 100 once the window is full.
    assert_eq!(rows, [(100, 100, 0), (100, 100, 0), (100, 100, 0)]);
}

#[test]
fn the_months_read_before_the_period_are_the_ones_the_averages_need() {
    let whole = period(date(2026, 4, 15), date(2026, 6, 30));
    let trend = Trend {
        length: 3,
        slow: 6,
        signal: 3,
    };
    assert_eq!(
        trend
            .read_span(MOVING_AVERAGE, whole)
            .expect("span")
            .first(),
        date(2026, 2, 1)
    );
    assert_eq!(
        trend.read_span(MACD, whole).expect("span").first(),
        date(2025, 9, 1)
    );
    assert_eq!(
        trend.read_span(MACD, whole).expect("span").last(),
        whole.last()
    );
}

#[test]
fn the_words_of_the_lengths_are_read_and_the_long_average_stays_longer() {
    assert_eq!(Trend::read("", "", ""), Trend::default());
    assert_eq!(
        Trend::read("6-months", "4-months", "2-months"),
        Trend {
            length: 6,
            slow: 7,
            signal: 2
        }
    );
    assert_eq!(Trend::read("x", "100-months", "7"), Trend::default());
    for length in TREND_LENGTH_WORDS {
        for slow in TREND_SLOW_WORDS {
            let trend = Trend::read(length, slow, "3-months");
            assert!(trend.slow > trend.length, "{trend:?}");
        }
    }
}

proptest! {
    #[test]
    fn an_average_stays_between_the_months_it_spans_and_the_gap_adds_up(
        months in proptest::collection::vec(0i64..1_000_000, 6..12),
        length in 2u32..4,
    ) {
        let source = source_of(&months, &[]);
        let whole = period(date(2026, 1, 1), date(2026, 12, 31));
        let trend = Trend { length, slow: length + 2, signal: 2 };
        let result = moving_average(
            &source, whole, &Filter::default(), TrendSeries::Income, trend,
        ).expect("average");
        let rows = &result[0].rows;
        prop_assert_eq!(rows.len(), 12);
        for (at, row) in rows.iter().enumerate() {
            let from = (at + 1).saturating_sub(length as usize);
            let window: Vec<i64> = (from..=at)
                .map(|month| months.get(month).copied().unwrap_or(0))
                .collect();
            let least = *window.iter().min().expect("a window");
            let most = *window.iter().max().expect("a window");
            let average = row.average.minor_units();
            // Zeros before the first month count, so the window may be
            // shorter than the length: pad it as the code does.
            let padded_least = if window.len() < length as usize { 0.min(least) } else { least };
            prop_assert!(average >= padded_least && average <= most, "{} {} {}", average, padded_least, most);
        }
        let result = macd(&source, whole, &Filter::default(), TrendSeries::Income, trend)
            .expect("macd");
        for row in &result[0].rows {
            prop_assert_eq!(
                row.gap.minor_units(),
                row.macd.minor_units() - row.signal.minor_units()
            );
        }
    }
}
