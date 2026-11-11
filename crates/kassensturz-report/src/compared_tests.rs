// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Spending next to a comparison (US3.1a, D-687), on synthetic books only.

use kassensturz_core::{AccountKind, Date, Money, RoundingMode};
use kassensturz_store::Book;
use serde_json::json;

use crate::tests::{EUR, USD, account, date, money, period, sample, transaction};
use crate::{
    Asked, COMPARE_WORDS, Compare, CompareWith, ComparedEarly, Component, ComponentType, Filter,
    Filters, Level, Per, Period, ReportError, SPENDING_BY_CATEGORY, Setting, Shown, Source,
    SpendingCompared, compared_early, spending_by_category_shown, spending_compared,
};

fn october() -> Period {
    period(date(2026, 10, 1), date(2026, 10, 31))
}

fn compared(book: &Book, shown: Period, compare: CompareWith) -> Vec<SpendingCompared> {
    let source = Source::read(book, compare.read_span(shown).unwrap()).unwrap();
    spending_compared(
        &source,
        shown,
        &Filter::default(),
        Level::Each,
        &Shown::default(),
        compare,
    )
    .unwrap()
}

/// Each row's name, its amount now and its comparison's sum.
fn rows(result: &SpendingCompared) -> Vec<(String, i64, i64)> {
    result
        .rows
        .iter()
        .map(|compared| {
            let name = if compared.row.is_unsorted() {
                "-".to_owned()
            } else {
                compared.row.path.join(" › ")
            };
            (
                name,
                compared.row.total.minor_units(),
                compared.earlier.minor_units(),
            )
        })
        .collect()
}

#[test]
fn the_words_read_back_and_none_compares_with_nothing() {
    assert_eq!(COMPARE_WORDS[0], "none");
    assert_eq!(CompareWith::read("none"), None);
    assert_eq!(CompareWith::read("average-5"), None);
    for word in &COMPARE_WORDS[1..] {
        let compare = CompareWith::read(word).expect(word);
        assert_eq!(compare.word(), *word);
    }
    assert_eq!(
        CompareWith::read("previous_period"),
        Some(CompareWith::Earlier(Compare::PreviousPeriod))
    );
    assert_eq!(
        CompareWith::read("average-6"),
        Some(CompareWith::Average(6))
    );
}

#[test]
fn an_average_compares_with_the_periods_right_before_back_to_back() {
    let average = CompareWith::Average(3);
    assert_eq!(
        average.periods(october()).unwrap(),
        [
            period(date(2026, 7, 1), date(2026, 7, 31)),
            period(date(2026, 8, 1), date(2026, 8, 31)),
            period(date(2026, 9, 1), date(2026, 9, 30)),
        ]
    );
    assert_eq!(
        average.span(october()).unwrap(),
        period(date(2026, 7, 1), date(2026, 9, 30))
    );
    assert_eq!(
        average.read_span(october()).unwrap(),
        period(date(2026, 7, 1), date(2026, 10, 31))
    );
    // A year chart compares with the average year (§4 choice 1).
    let year = period(date(2026, 1, 1), date(2026, 12, 31));
    assert_eq!(
        CompareWith::Average(3).span(year).unwrap(),
        period(date(2023, 1, 1), date(2025, 12, 31))
    );
    // Days that are not whole months compare with as many days before.
    let ten_days = period(date(2026, 10, 11), date(2026, 10, 20));
    assert_eq!(
        CompareWith::Average(3).span(ten_days).unwrap(),
        period(date(2026, 9, 11), date(2026, 10, 10))
    );
    // The same period last year reads the year in between too.
    assert_eq!(
        CompareWith::Earlier(Compare::SameLastYear)
            .read_span(october())
            .unwrap(),
        period(date(2025, 10, 1), date(2026, 10, 31))
    );
}

#[test]
fn a_comparison_longer_than_the_most_months_is_refused() {
    // A hundred years are the most a chart reads; their average of 12 more
    // is far beyond.
    let century = period(date(1927, 1, 1), date(2026, 12, 31));
    assert!(matches!(
        CompareWith::Average(12).read_span(century),
        Err(ReportError::PeriodTooLong)
    ));
    // A year with the average of the 12 years before is 13 years.
    let year = period(date(2026, 1, 1), date(2026, 12, 31));
    assert_eq!(
        CompareWith::Average(12).read_span(year).unwrap(),
        period(date(2014, 1, 1), date(2026, 12, 31))
    );
}

#[test]
fn october_next_to_september_lists_what_stopped_too() {
    let sample = sample();
    let results = compared(
        &sample.book,
        october(),
        CompareWith::Earlier(Compare::PreviousPeriod),
    );
    let [euros, dollars] = results.as_slice() else {
        panic!("one result per currency, not {results:?}");
    };
    // October's rows in October's order, then what was spent only in
    // September, largest first, with nothing now.
    assert_eq!(
        rows(euros),
        [
            ("Rent".to_owned(), 80_000, 80_000),
            ("-".to_owned(), 1_250, 0),
            ("Food › Groceries".to_owned(), 0, 2_500),
            ("Food".to_owned(), 0, 1_000),
        ]
    );
    assert_eq!(euros.now.total, money(81_250));
    assert_eq!(euros.earlier_total, money(83_500));
    // The other currency compares within itself.
    assert_eq!(rows(dollars), [("Travel".to_owned(), 9_900, 0)]);
    assert_eq!(dollars.earlier_total, Money::from_minor_units(0, USD));
}

#[test]
fn an_average_adds_the_exact_amounts_and_divides_once() {
    let sample = sample();
    let average = CompareWith::Average(3);
    let results = compared(&sample.book, october(), average);
    // July to September: rent twice, groceries less the refund, food once.
    assert_eq!(
        rows(&results[0]),
        [
            ("Rent".to_owned(), 80_000, 160_000),
            ("-".to_owned(), 1_250, 0),
            ("Food › Groceries".to_owned(), 0, 7_820),
            ("Food".to_owned(), 0, 1_000),
        ]
    );
    let today = date(2026, 11, 10);
    let rounding = RoundingMode::HalfAwayFromZero;
    let divisor = average.divisor(october(), None, today, rounding).unwrap();
    assert_eq!(divisor.count, 3);
    // 78.20 € ÷ 3 = 26.0666… is 26.07 €, from the exact sum.
    assert_eq!(divisor.of(money(7_820)), money(2_607));
    // A chart per month divides by the months the comparison counts, so a
    // three-month chart against the period before is per month too.
    let autumn = period(date(2026, 8, 1), date(2026, 10, 31));
    let before = CompareWith::Earlier(Compare::PreviousPeriod);
    assert_eq!(
        before
            .divisor(autumn, Some(Per::Month), today, rounding)
            .unwrap()
            .count,
        3
    );
    assert_eq!(
        before.divisor(autumn, None, today, rounding).unwrap().count,
        1
    );
    assert_eq!(
        average
            .divisor(october(), Some(Per::Quarter), today, rounding)
            .unwrap()
            .count,
        1
    );
}

/// Tests where steps meet (AGENTS.md §9.4): each side is what the dataset
/// itself gives for its own days, for every comparison and both levels.
#[test]
fn both_sides_are_what_spending_by_category_gives_for_their_days() {
    let sample = sample();
    let filters = [
        Filter::default(),
        Filter {
            accounts: Some(vec![sample.checking]),
            categories: None,
        },
        Filter {
            accounts: None,
            categories: Some(vec![sample.food]),
        },
    ];
    for shown in [october(), period(date(2026, 9, 1), date(2026, 10, 31))] {
        for word in &COMPARE_WORDS[1..] {
            let compare = CompareWith::read(word).unwrap();
            let source = Source::read(&sample.book, compare.read_span(shown).unwrap()).unwrap();
            for level in [Level::Each, Level::TopLevel] {
                for filter in &filters {
                    let results = spending_compared(
                        &source,
                        shown,
                        filter,
                        level,
                        &Shown::default(),
                        compare,
                    )
                    .unwrap();
                    let now = spending_by_category_shown(
                        &source,
                        shown,
                        filter,
                        level,
                        &Shown::default(),
                    )
                    .unwrap();
                    let then = spending_by_category_shown(
                        &source,
                        compare.span(shown).unwrap(),
                        filter,
                        level,
                        &Shown::default(),
                    )
                    .unwrap();
                    for result in &results {
                        let own = now.iter().find(|own| own.currency == result.now.currency);
                        let earlier = then
                            .iter()
                            .find(|then| then.currency == result.now.currency);
                        assert_eq!(own.map(|own| own.total), own.map(|_| result.now.total));
                        assert_eq!(
                            earlier.map_or(0, |then| then.total.minor_units()),
                            result.earlier_total.minor_units(),
                            "{word}"
                        );
                        for compared in &result.rows {
                            let same = |row: &crate::CategoryRow| {
                                row.category == compared.row.category
                                    && row.group == compared.row.group
                            };
                            let now_total = own
                                .and_then(|own| own.rows.iter().find(|row| same(row)))
                                .map_or(0, |row| row.total.minor_units());
                            let then_total = earlier
                                .and_then(|then| then.rows.iter().find(|row| same(row)))
                                .map_or(0, |row| row.total.minor_units());
                            assert_eq!(compared.row.total.minor_units(), now_total, "{word}");
                            assert_eq!(compared.earlier.minor_units(), then_total, "{word}");
                        }
                        // Every row of either side is there once.
                        let sides = own.map_or(0, |own| own.rows.len())
                            + earlier.map_or(0, |then| {
                                then.rows
                                    .iter()
                                    .filter(|row| {
                                        own.is_none_or(|own| {
                                            !own.rows.iter().any(|mine| {
                                                mine.category == row.category
                                                    && mine.group == row.group
                                            })
                                        })
                                    })
                                    .count()
                            });
                        assert_eq!(result.rows.len(), sides, "{word}");
                    }
                }
            }
        }
    }
}

/// A synthetic book whose groceries start on 3 August 2026, with one older
/// payment from another account on 10 January 2025.
fn late_start() -> (Book, kassensturz_core::AccountId) {
    let mut book = Book::create_in_memory().unwrap();
    book.use_currency(EUR).unwrap();
    let checking = account(&mut book, "Checking", AccountKind::Asset, EUR, None);
    let old = account(&mut book, "Savings", AccountKind::Asset, EUR, None);
    let food = account(&mut book, "Groceries", AccountKind::Expense, EUR, None);
    let mut add = |day: Date, from| {
        book.add_transaction(&transaction(
            day,
            &[(Some(from), money(-4_000)), (Some(food), money(4_000))],
        ))
        .unwrap();
    };
    add(date(2025, 1, 10), old);
    for month in 8..=10 {
        add(date(2026, month, 3), checking);
    }
    (book, checking)
}

fn early(book: &Book, shown: Period, filter: &Filter, word: &str) -> Option<ComparedEarly> {
    let compare = CompareWith::read(word).unwrap();
    let source = Source::read(book, compare.read_span(shown).unwrap()).unwrap();
    compared_early(book, &source, shown, filter, compare).unwrap()
}

#[test]
fn periods_before_the_first_transaction_are_named() {
    let (book, checking) = late_start();
    let checking_only = Filter {
        accounts: Some(vec![checking]),
        categories: None,
    };
    // April to September, four of them before 3 August.
    assert_eq!(
        early(&book, october(), &checking_only, "average-6"),
        Some(ComparedEarly {
            counted: 6,
            before: 4
        })
    );
    // September has a transaction; October a year ago is before the first.
    assert_eq!(
        early(&book, october(), &checking_only, "previous_period"),
        None
    );
    assert_eq!(
        early(&book, october(), &checking_only, "same_last_year"),
        Some(ComparedEarly {
            counted: 1,
            before: 1
        })
    );
    // The whole book has a payment before: its zeros are real.
    assert_eq!(
        early(&book, october(), &Filter::default(), "average-6"),
        None
    );
    // A month that starts on the first transaction's month is not before.
    assert_eq!(
        early(
            &book,
            period(date(2026, 9, 1), date(2026, 9, 30)),
            &checking_only,
            "previous_period"
        ),
        None
    );
}

#[test]
fn only_spending_bars_and_tables_compare_and_none_is_not_written() {
    let mut bars = Component::new(ComponentType::ChartBar, SPENDING_BY_CATEGORY).unwrap();
    assert_eq!(bars.get("compare"), Some(&Setting::Word("none".into())));
    assert!(bars.to_value()["params"].get("compare").is_none());
    bars.set("compare", Setting::Word("average-6".into()))
        .unwrap();
    assert_eq!(bars.to_value()["params"]["compare"], "average-6");
    let read = Component::from_value(&bars.to_value()).unwrap();
    assert_eq!(read, bars);
    let asked = Asked::new(&read, None, &Filters::default()).unwrap();
    assert_eq!(asked.compare, Some(CompareWith::Average(6)));
    assert!(matches!(
        bars.set("compare", Setting::Word("average-5".into())),
        Err(ReportError::DocumentInvalid { reason: "bad-choice", path }) if path == "params.compare"
    ));

    let mut table = Component::new(ComponentType::TableRows, SPENDING_BY_CATEGORY).unwrap();
    table
        .set("compare", Setting::Word("same_last_year".into()))
        .unwrap();
    // Another dataset offers no comparison but `none`.
    let mut money_in = Component::new(ComponentType::ChartBar, "income_expense_by_month").unwrap();
    assert!(
        money_in
            .set("compare", Setting::Word("previous_period".into()))
            .is_err()
    );
    money_in
        .set("compare", Setting::Word("none".into()))
        .unwrap();
    // A stacked bar and a pie have no place for a second bar.
    for kind in [ComponentType::ChartStackedBar, ComponentType::ChartPie] {
        let document = json!({
            "type": kind.id(),
            "dataset": SPENDING_BY_CATEGORY,
            "params": {"compare": "previous_period"}
        });
        assert!(matches!(
            Component::from_value(&document),
            Err(ReportError::DocumentInvalid { reason: "unknown-property", path })
                if path == "params.compare"
        ));
    }
    // A key figure keeps its two words.
    let card = Component::new(ComponentType::KpiCard, "income_expense_by_month").unwrap();
    let words = card
        .kind()
        .properties()
        .iter()
        .find(|property| property.key == "compare")
        .unwrap()
        .words(card.dataset());
    assert_eq!(words, ["previous_period", "same_last_year"]);
    assert_eq!(
        Asked::new(&card, None, &Filters::default())
            .unwrap()
            .compare,
        None
    );
}
