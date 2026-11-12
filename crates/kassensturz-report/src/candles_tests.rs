// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the balance candles (US4.10, D-718, D-719): on the financial
//! tests' real book against `cash_flow_by_month`'s own start and end, the
//! candle length of a period, a day's bookings counted only at its end, and
//! properties against balances added up by hand.

use jiff::civil::Weekday;
use kassensturz_core::{Account, Date, Money};
use proptest::prelude::*;

use super::*;
use crate::financial_tests::{financial, holdings_source, movements, only, own_accounts};
use crate::tests::{EUR, USD, Words, date, period};

#[test]
fn the_length_follows_the_period() {
    let first = date(2026, 1, 1);
    let days = |n: i64| {
        period(
            first,
            first.checked_add(jiff::Span::new().days(n - 1)).unwrap(),
        )
    };
    assert_eq!(CandleLength::of(days(1)), CandleLength::Day);
    assert_eq!(CandleLength::of(days(31)), CandleLength::Day);
    assert_eq!(CandleLength::of(days(32)), CandleLength::Week);
    assert_eq!(CandleLength::of(days(184)), CandleLength::Week);
    assert_eq!(CandleLength::of(days(185)), CandleLength::Month);
    assert_eq!(CandleLength::of(days(3650)), CandleLength::Month);
}

#[test]
fn month_candles_start_and_end_where_the_cash_flow_does() {
    let f = financial();
    let shown = period(date(2026, 5, 1), date(2026, 11, 30));
    let source = Source::read_with_balances(&f.book, shown).unwrap();
    let candles = balance_candles(&source, shown, &Filter::default(), None).unwrap();
    let flows = cash_flow_by_month(&source, shown, &Filter::default()).unwrap();
    assert_eq!(
        candles.iter().map(|r| r.currency).collect::<Vec<_>>(),
        flows.iter().map(|r| r.currency).collect::<Vec<_>>()
    );
    assert_eq!(candles[0].currency, EUR);
    assert_eq!(candles[1].currency, USD);
    for (result, flow) in candles.iter().zip(&flows) {
        assert_eq!(result.length, CandleLength::Month);
        assert_eq!(result.candles.len(), flow.rows.len());
        for (candle, row) in result.candles.iter().zip(&flow.rows) {
            assert_eq!(candle.first, row.month);
            assert_eq!(candle.last, row.month.last_of_month());
            assert_eq!((candle.open, candle.close), (row.start, row.end));
            assert!(
                candle.low.minor_units()
                    <= candle.open.minor_units().min(candle.close.minor_units())
            );
            assert!(
                candle.high.minor_units()
                    >= candle.open.minor_units().max(candle.close.minor_units())
            );
        }
    }
    // October in euros: the salary comes in, then the Transfer to savings,
    // the card payment and the changed money go out, so the month's highest
    // balance is above both its start and its end.
    let october = &candles[0].candles[5];
    assert!(
        october.high.minor_units() > october.open.minor_units().max(october.close.minor_units())
    );
    let table = candles[0].to_text(&Words);
    assert_eq!(
        table.columns,
        ["period", "start", "highest", "lowest", "end"]
    );
    assert_eq!(table.rows.len(), 7);
}

#[test]
fn a_chosen_account_has_candles_of_its_own() {
    let f = financial();
    let october = period(date(2026, 10, 1), date(2026, 10, 31));
    let source = Source::read_with_balances(&f.book, october).unwrap();
    let card = balance_candles(&source, october, &only(&[f.card]), None).unwrap();
    assert_eq!(card.len(), 1);
    let days = &card[0];
    assert_eq!(days.length, CandleLength::Day);
    assert_eq!(days.candles.len(), 31);
    // The card owed 200.00 on 1 October, was paid off on the 5th and owed
    // 30.00 again from the 18th.
    assert_eq!(days.candles[0].open.minor_units(), -20_000);
    assert_eq!(days.candles[4].close.minor_units(), 0);
    assert_eq!(days.candles[17].close.minor_units(), -3_000);
    assert_eq!(days.candles[30].close.minor_units(), -3_000);
    let flows = cash_flow_by_month(&source, october, &only(&[f.card])).unwrap();
    assert_eq!(days.candles[30].close, flows[0].total.end);
    // No choice of accounts leaves the card out, as the cash flow does.
    let all = balance_candles(&source, october, &Filter::default(), None).unwrap();
    assert_ne!(all[0].candles[0].open, days.candles[0].open);
    let _ = (f.checking, f.savings);
}

#[test]
fn weeks_run_from_monday_and_cut_at_the_period() {
    let accounts = own_accounts();
    let (source, _) = holdings_source(&accounts, &[0, 0, 0, 0], &[]);
    // A Thursday to a Tuesday nine weeks later.
    let shown = period(date(2026, 1, 1), date(2026, 3, 3));
    let result = balance_candles(&source, shown, &Filter::default(), None).unwrap();
    let weeks = &result[0].candles;
    assert_eq!(result[0].length, CandleLength::Week);
    assert_eq!(weeks[0].first, date(2026, 1, 1));
    assert_eq!(weeks[0].last, date(2026, 1, 4));
    for week in &weeks[1..] {
        assert_eq!(week.first.weekday(), Weekday::Monday);
    }
    for pair in weeks.windows(2) {
        assert_eq!(pair[0].last.tomorrow().unwrap(), pair[1].first);
    }
    assert_eq!(weeks.last().unwrap().last, date(2026, 3, 3));
}

#[test]
fn bookings_inside_a_day_count_only_at_its_end() {
    let accounts = own_accounts();
    // On 5 January 1,000.00 comes in and goes out again; on the 6th 50.00
    // goes out and 20.00 comes in.
    let entries = [
        (4, 0, 1, 100_000),
        (4, 0, 2, -100_000),
        (5, 0, 3, -5_000),
        (5, 0, 4, 2_000),
    ];
    let (source, _) = holdings_source(&accounts, &[10_000, 0, 0, 0], &entries);
    let shown = period(date(2026, 1, 1), date(2026, 1, 10));
    let result = balance_candles(&source, shown, &only(&[accounts[0].id]), None).unwrap();
    let days = &result[0].candles;
    let fifth = days[4];
    assert_eq!(
        [fifth.open, fifth.high, fifth.low, fifth.close].map(Money::minor_units),
        [10_000; 4]
    );
    let sixth = days[5];
    assert_eq!(
        [sixth.open, sixth.high, sixth.low, sixth.close].map(Money::minor_units),
        [10_000, 10_000, 7_000, 7_000]
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Every candle chains to the next, its highest and lowest are the
    /// extremes of its start and its days' end balances counted by hand, and
    /// the last one ends where the cash flow does.
    #[test]
    fn candles_are_the_end_of_day_balances(
        entries in movements(),
        opening in prop::collection::vec(-1_000_000i64..1_000_000, 4),
        from in 0i64..60,
        length in 0i64..60,
        everything in any::<bool>(),
    ) {
        let accounts = own_accounts();
        let (source, moved) = holdings_source(&accounts, &opening, &entries);
        let start = date(2026, 1, 1).checked_add(jiff::Span::new().days(from)).unwrap();
        let end = start.checked_add(jiff::Span::new().days(length)).unwrap();
        let shown = period(start, end);
        let filter = if everything {
            only(&accounts.iter().map(|a| a.id).collect::<Vec<_>>())
        } else {
            Filter::default()
        };
        let chosen: Vec<&Account> = accounts
            .iter()
            .filter(|a| everything || a.kind == kassensturz_core::AccountKind::Asset)
            .collect();
        let results = balance_candles(&source, shown, &filter, None).unwrap();
        let flows = cash_flow_by_month(&source, shown, &filter).unwrap();
        for result in &results {
            // The balance at the end of `day` (or before the period), added
            // up from the opening balances and the parts.
            let at = |day: Option<Date>| -> i64 {
                let mut total = 0;
                for (account, units) in accounts.iter().zip(&opening) {
                    if account.currency == result.currency && chosen.iter().any(|c| c.id == account.id) {
                        total += units;
                        total += moved
                            .iter()
                            .filter(|m| m.account == account.id && m.day < start)
                            .map(|m| m.amount.minor_units())
                            .sum::<i64>();
                        if let Some(day) = day {
                            total += moved
                                .iter()
                                .filter(|m| m.account == account.id && m.day >= start && m.day <= day)
                                .map(|m| m.amount.minor_units())
                                .sum::<i64>();
                        }
                    }
                }
                total
            };
            prop_assert_eq!(result.candles.first().unwrap().first, start);
            prop_assert_eq!(result.candles.last().unwrap().last, end);
            for pair in result.candles.windows(2) {
                prop_assert_eq!(pair[0].close, pair[1].open);
            }
            for candle in &result.candles {
                let mut ends = vec![candle.open.minor_units()];
                let mut day = candle.first;
                while day <= candle.last {
                    ends.push(at(Some(day)));
                    day = day.tomorrow().unwrap();
                }
                prop_assert_eq!(candle.close.minor_units(), *ends.last().unwrap());
                prop_assert_eq!(candle.high.minor_units(), *ends.iter().max().unwrap());
                prop_assert_eq!(candle.low.minor_units(), *ends.iter().min().unwrap());
            }
            prop_assert_eq!(result.candles[0].open.minor_units(), at(None));
            let flow = flows.iter().find(|f| f.currency == result.currency).unwrap();
            prop_assert_eq!(result.candles.last().unwrap().close, flow.total.end);
        }
    }
}

#[test]
fn a_chosen_length_replaces_the_automatic_one() {
    // Q-498, D-747: six whole months get a candle per month by the period
    // (they are 184 days, which by length alone would be weeks), and a
    // person who chose a candle per week gets weeks.
    let f = financial();
    let shown = period(date(2026, 5, 1), date(2026, 10, 31));
    assert_eq!(CandleLength::of(shown), CandleLength::Month);
    let source = Source::read_with_balances(&f.book, shown).unwrap();
    let months = balance_candles(&source, shown, &Filter::default(), None).unwrap();
    let flows = cash_flow_by_month(&source, shown, &Filter::default()).unwrap();
    for (result, flow) in months.iter().zip(&flows) {
        assert_eq!(result.length, CandleLength::Month);
        assert_eq!(result.candles.len(), 6);
        for (candle, row) in result.candles.iter().zip(&flow.rows) {
            assert_eq!(
                (candle.first, candle.open, candle.close),
                (row.month, row.start, row.end)
            );
        }
    }
    let weeks =
        balance_candles(&source, shown, &Filter::default(), Some(CandleLength::Week)).unwrap();
    assert_eq!(weeks[0].length, CandleLength::Week);
    assert_eq!(weeks[0].candles.len(), 27);
    // The first opens and the last closes where the months do: only the
    // grouping of the days changes, never a balance.
    assert_eq!(weeks[0].candles[0].open, months[0].candles[0].open);
    assert_eq!(
        weeks[0].candles.last().unwrap().close,
        months[0].candles.last().unwrap().close
    );
    // A day per candle over the same months.
    let days =
        balance_candles(&source, shown, &Filter::default(), Some(CandleLength::Day)).unwrap();
    assert_eq!(days[0].candles.len(), 184);
}

#[test]
fn whole_months_get_a_candle_each() {
    // D-747: "the last 3 months" are three candles, with or without the
    // month today is in, and a year of whole months is twelve.
    let today = date(2026, 11, 12);
    for word in [
        "last-3-complete-months",
        "last-3-months",
        "last-2-complete-months",
    ] {
        let shown = Period::named(word, today, None).unwrap().unwrap();
        assert_eq!(CandleLength::of(shown), CandleLength::Month, "{word}");
    }
    let complete = Period::named("last-3-complete-months", today, None)
        .unwrap()
        .unwrap();
    assert_eq!(CandleLength::Month.starts(complete).len(), 3);
    assert_eq!(
        CandleLength::of(period(date(2026, 1, 1), date(2026, 12, 31))),
        CandleLength::Month
    );
    // One whole month stays a candle per day, never a single candle; a
    // period that doesn't start and end with a month follows its length.
    assert_eq!(
        CandleLength::of(period(date(2026, 2, 1), date(2026, 2, 28))),
        CandleLength::Day
    );
    assert_eq!(
        CandleLength::of(period(date(2026, 8, 13), date(2026, 11, 12))),
        CandleLength::Week
    );
    // The last 7 or 31 days are days.
    for word in ["last-7-days", "last-31-days"] {
        let shown = Period::named(word, today, None).unwrap().unwrap();
        assert_eq!(CandleLength::of(shown), CandleLength::Day, "{word}");
    }
}

#[test]
fn only_lengths_that_give_two_candles_are_offered_and_used() {
    // D-747: seven days can't be drawn per month or per week; one day has
    // no length but the automatic one.
    let f = financial();
    let week = period(date(2026, 10, 26), date(2026, 11, 1));
    assert_eq!(CandleLength::offered(week), ["auto", "day"]);
    assert_eq!(
        CandleLength::offered(period(date(2026, 10, 1), date(2026, 10, 1))),
        ["auto"]
    );
    assert_eq!(
        CandleLength::offered(period(date(2026, 10, 1), date(2026, 10, 8))),
        ["auto", "day", "week"]
    );
    assert_eq!(
        CandleLength::offered(period(date(2026, 10, 1), date(2026, 11, 1))),
        ["auto", "day", "week", "month"]
    );
    // Every length offered gives at least two candles, wherever the period
    // starts.
    for first in 0..40 {
        let start = date(2026, 1, 1)
            .checked_add(jiff::Span::new().days(first))
            .unwrap();
        for length in 1..70 {
            let shown = period(
                start,
                start
                    .checked_add(jiff::Span::new().days(length - 1))
                    .unwrap(),
            );
            for candle in [CandleLength::Day, CandleLength::Week, CandleLength::Month] {
                if candle.fits(shown) {
                    assert!(candle.starts(shown).len() >= 2, "{candle:?} {shown:?}");
                }
            }
        }
    }
    // A month chosen for seven days draws days instead of one candle.
    let source = Source::read_with_balances(&f.book, week).unwrap();
    let drawn =
        balance_candles(&source, week, &Filter::default(), Some(CandleLength::Month)).unwrap();
    assert_eq!(drawn[0].length, CandleLength::Day);
    assert_eq!(drawn[0].candles.len(), 7);
}

#[test]
fn the_words_of_a_length() {
    assert_eq!(CandleLength::read("day"), Some(CandleLength::Day));
    assert_eq!(CandleLength::read("week"), Some(CandleLength::Week));
    assert_eq!(CandleLength::read("month"), Some(CandleLength::Month));
    assert_eq!(CandleLength::read("auto"), None);
    assert_eq!(CandleLength::read("year"), None);
    for length in [CandleLength::Day, CandleLength::Week, CandleLength::Month] {
        assert_eq!(CandleLength::read(length.key()), Some(length));
    }
}
