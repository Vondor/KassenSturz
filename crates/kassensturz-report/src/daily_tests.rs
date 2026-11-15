// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the daily balance with its averages and MACD (US7, #1036,
//! D-794): each day's balance is the book's own `balance_on` for that day,
//! the last day shown is today, the chart starts at the first entry, and
//! the averages are exact with one rounding a day. Synthetic names only.

use jiff::ToSpan;
use kassensturz_core::{AccountId, AccountKind, Date, Money, RoundingMode};
use kassensturz_store::Book;
use proptest::prelude::*;

use super::daily::{ema, step};
use super::*;
use crate::tests::{EUR, account, date, money, period, transaction};

/// A synthetic book: a checking account with a starting balance of 1,000.00
/// on 1 December 2025, a salary of 2,500.00 on the first of each month from
/// January 2026, a card payment of 12.34 every day from 2 January, and one
/// payment planned for after `TODAY`.
fn salary_and_card() -> (Book, AccountId) {
    let mut book = Book::create_in_memory().unwrap();
    book.use_currency(EUR).unwrap();
    let checking = account(&mut book, "Checking", AccountKind::Asset, EUR, None);
    let opening = account(
        &mut book,
        "Starting balances",
        AccountKind::Equity,
        EUR,
        None,
    );
    let salary = account(&mut book, "Salary", AccountKind::Income, EUR, None);
    let shop = account(&mut book, "Groceries", AccountKind::Expense, EUR, None);
    let mut add = |day: Date, other: AccountId, amount: i64| {
        book.add_transaction(&transaction(
            day,
            &[
                (Some(checking), money(amount)),
                (Some(other), money(-amount)),
            ],
        ))
        .unwrap();
    };
    add(date(2025, 12, 1), opening, 100_000);
    for month in 1..=11 {
        add(date(2026, month, 1), salary, 250_000);
    }
    let mut day = date(2026, 1, 2);
    while day <= TODAY {
        add(day, shop, -1_234);
        day = day.tomorrow().unwrap();
    }
    add(date(2026, 11, 20), shop, -50_000);
    (book, checking)
}

const TODAY: Date = Date::constant(2026, 11, 14);

fn only(account: AccountId) -> Filter {
    Filter {
        accounts: Some(vec![account]),
        categories: None,
        tags: None,
    }
}

/// The daily balance of `filter` for `shown` on `TODAY`, read as the app
/// reads it.
fn daily(book: &Book, shown: Period, filter: &Filter, lengths: Ema) -> Vec<DailyBalance> {
    let days = daily_days(shown, TODAY).unwrap().unwrap();
    let source = Source::read_with_balances(book, lengths.read_span(days).unwrap()).unwrap();
    daily_balance(&source, shown, TODAY, filter, lengths).unwrap()
}

fn last_six_months() -> Period {
    period(date(2026, 6, 1), date(2026, 11, 30))
}

#[test]
fn each_day_is_the_books_balance_on_that_day_and_the_last_is_today() {
    let (book, checking) = salary_and_card();
    let results = daily(&book, last_six_months(), &only(checking), Ema::default());
    assert_eq!(results.len(), 1);
    let result = &results[0];
    assert_eq!(result.currency, EUR);
    assert!(!result.starts_late);
    assert_eq!(result.rows.first().unwrap().day, date(2026, 6, 1));
    // The period runs to 30 November, but the days shown stop today: the
    // payment planned for the 20th is not in the last balance.
    let last = result.rows.last().unwrap();
    assert_eq!(last.day, TODAY);
    assert_eq!(last.balance, book.balance_on(checking, TODAY).unwrap());
    assert_eq!(result.rows.len(), 167);
    for row in &result.rows {
        assert_eq!(
            row.balance,
            book.balance_on(checking, row.day).unwrap(),
            "{}",
            row.day
        );
        assert_eq!(row.macd, row.fast.checked_sub(row.slow).unwrap());
        assert_eq!(row.gap, row.macd.checked_sub(row.signal).unwrap());
    }
    // The salary day lifts the balance by what came in less that day's
    // card payment.
    let on = |day: Date| result.rows.iter().find(|row| row.day == day).unwrap();
    assert_eq!(
        on(date(2026, 7, 1))
            .balance
            .checked_sub(on(date(2026, 6, 30)).balance)
            .unwrap(),
        money(250_000 - 1_234)
    );
}

#[test]
fn without_a_choice_the_checking_savings_and_cash_accounts_count() {
    let (book, checking) = salary_and_card();
    let all = daily(&book, last_six_months(), &Filter::default(), Ema::default());
    let one = daily(&book, last_six_months(), &only(checking), Ema::default());
    assert_eq!(all, one);
}

#[test]
fn the_averages_have_settled_by_the_first_day_shown() {
    // The averages start well before the period, so the first day shown
    // is the same whether the book is read from far back or not.
    let (book, checking) = salary_and_card();
    let lengths = Ema::default();
    let results = daily(&book, last_six_months(), &only(checking), lengths);
    let first = results[0].rows[0];
    // A balance that rises by about 2,130 a month and falls a little each
    // day: the short average stays nearer to it than the long one.
    let near = |average: Money| (average.minor_units() - first.balance.minor_units()).abs();
    assert!(near(first.fast) < near(first.slow));
    assert!(lengths.warm_up_days() >= 4 * lengths.slow);
}

#[test]
fn the_chart_starts_at_the_first_entry_inside_the_period() {
    let mut book = Book::create_in_memory().unwrap();
    book.use_currency(EUR).unwrap();
    let checking = account(&mut book, "Checking", AccountKind::Asset, EUR, None);
    let salary = account(&mut book, "Salary", AccountKind::Income, EUR, None);
    book.add_transaction(&transaction(
        date(2026, 8, 3),
        &[
            (Some(checking), money(10_000)),
            (Some(salary), money(-10_000)),
        ],
    ))
    .unwrap();
    let results = daily(&book, last_six_months(), &only(checking), Ema::default());
    let result = &results[0];
    assert!(result.starts_late);
    let first = result.rows[0];
    assert_eq!(first.day, date(2026, 8, 3));
    // The averages start at the first balance, not at a zero before it.
    assert_eq!(
        (first.fast, first.slow, first.macd),
        (money(10_000), money(10_000), money(0))
    );
    assert_eq!(result.rows.last().unwrap().day, TODAY);
}

#[test]
fn an_account_without_entries_has_no_days() {
    let mut book = Book::create_in_memory().unwrap();
    book.use_currency(EUR).unwrap();
    let checking = account(&mut book, "Checking", AccountKind::Asset, EUR, None);
    let results = daily(&book, last_six_months(), &only(checking), Ema::default());
    assert_eq!(results.len(), 1);
    assert!(results[0].rows.is_empty());
}

#[test]
fn a_period_after_today_shows_nothing_and_a_too_long_one_is_refused() {
    assert_eq!(
        daily_days(period(date(2026, 12, 1), date(2026, 12, 31)), TODAY).unwrap(),
        None
    );
    assert_eq!(
        daily_days(period(date(2026, 11, 1), date(2026, 11, 30)), TODAY).unwrap(),
        Some(period(date(2026, 11, 1), TODAY))
    );
    let first = TODAY
        .checked_sub((MAX_DAILY_DAYS as i32 - 1).days())
        .unwrap();
    assert!(daily_days(period(first, TODAY), TODAY).is_ok());
    let first = first.yesterday().unwrap();
    assert!(matches!(
        daily_days(period(first, TODAY), TODAY),
        Err(ReportError::PeriodTooLong)
    ));
}

#[test]
fn an_average_moves_two_parts_in_days_plus_one_rounded_once() {
    let half = RoundingMode::HalfAwayFromZero;
    // 3 days: the weight is 2 / 4, so half the way.
    assert_eq!(ema(&[0, 1_000, 1_000], 3, half).unwrap(), vec![0, 500, 750]);
    // 12 days: 2 × 1,000 / 13 = 153.85 cents, rounded once to 154.
    assert_eq!(step(0, 1_000, 12, half).unwrap(), 154);
    assert_eq!(step(0, -1_000, 12, half).unwrap(), -154);
    // Rounded toward zero in a book that says so.
    assert_eq!(step(0, 1_000, 12, RoundingMode::TowardZero).unwrap(), 153);
    // A balance that never changes is its own average.
    assert_eq!(ema(&[4_200; 30], 26, half).unwrap(), vec![4_200; 30]);
}

#[test]
fn lengths_are_read_from_their_words() {
    assert_eq!(Ema::read("", "", ""), Ema::default());
    assert_eq!(
        Ema::read("5-days", "20-days", "3-days"),
        Ema {
            fast: 5,
            slow: 20,
            signal: 3
        }
    );
    // The long average always spans more days than the short one.
    assert_eq!(Ema::read("50-days", "14-days", "9-days").slow, 51);
    // A word from another list is not one of these.
    assert_eq!(
        Ema::read("12-months", "26-months", "9-weeks"),
        Ema::default()
    );
    for word in EMA_FAST_WORDS
        .iter()
        .chain(&EMA_SLOW_WORDS)
        .chain(&EMA_SIGNAL_WORDS)
    {
        let days: u32 = word.strip_suffix("-days").unwrap().parse().unwrap();
        assert_eq!(Ema::word(days), *word);
    }
    for word in DAILY_PARTS {
        assert_eq!(DailyParts::read(word).word(), word);
    }
    assert_eq!(DailyParts::read("other"), DailyParts::All);
}

#[test]
fn the_table_has_every_figure_of_each_day() {
    let (book, checking) = salary_and_card();
    let results = daily(&book, last_six_months(), &only(checking), Ema::default());
    let table = results[0].to_text(&crate::tests::Words);
    assert_eq!(
        table.columns,
        [
            "day",
            "balance",
            "ema_fast",
            "ema_slow",
            "daily_macd",
            "daily_signal",
            "daily_gap"
        ]
    );
    assert_eq!(table.rows.len(), results[0].rows.len());
}

proptest! {
    /// An average never leaves the range of the balances it has seen, and
    /// it moves toward the newest one.
    #[test]
    fn an_average_stays_between_the_balances(
        values in proptest::collection::vec(-10_000_000i64..10_000_000, 1..80),
        days in 2u32..200,
    ) {
        let averages = ema(&values, days, RoundingMode::HalfAwayFromZero).unwrap();
        let (mut low, mut high) = (values[0], values[0]);
        for (at, (&value, &average)) in values.iter().zip(&averages).enumerate() {
            low = low.min(value);
            high = high.max(value);
            prop_assert!(low <= average && average <= high);
            if at > 0 {
                let before = averages[at - 1];
                prop_assert!((average - before).signum() * (value - before).signum() >= 0);
            }
        }
    }
}
