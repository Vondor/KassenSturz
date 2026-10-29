// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the chart models, made from the real output of the datasets of
//! `kassensturz-report` on a book (tests where steps meet).

use kassensturz_core::{
    Account, AccountId, AccountKind, AccountType, CurrencyCode, Date, Money, Split, Transaction,
    TransactionId,
};
use kassensturz_report::{
    Filter, Level, Period, Source, TextFormat, income_expense_by_month as monthly_dataset,
    spending_by_category as spending_dataset,
};
use kassensturz_store::Book;
use proptest::prelude::*;

use super::*;

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};

fn date(year: i16, month: i8, day: i8) -> Date {
    Date::new(year, month, day).expect("valid date")
}

fn money(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, EUR)
}

/// Words for the tests, so the output shows what the chart was given.
struct Words;

impl TextFormat for Words {
    fn amount(&self, money: Money) -> String {
        format!(
            "{},{:02} EUR",
            money.minor_units() / 100,
            (money.minor_units() % 100).abs()
        )
    }

    fn month(&self, first_day: Date) -> String {
        format!("{}-{:02}", first_day.year(), first_day.month())
    }

    fn unsorted(&self) -> String {
        "(unsorted)".into()
    }
}

fn account(book: &mut Book, name: &str, kind: AccountKind, parent: Option<AccountId>) -> AccountId {
    let id = AccountId::generate();
    book.add_account(&Account {
        id,
        name: name.into(),
        kind,
        currency: EUR,
        account_type: (kind == AccountKind::Asset).then_some(AccountType::Checking),
        parent,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    })
    .expect("account added");
    id
}

fn add(book: &mut Book, day: Date, parts: &[(Option<AccountId>, i64)]) {
    let splits = parts
        .iter()
        .map(|&(account, amount)| match account {
            Some(account) => Split::new(account, money(amount)),
            None => Split::uncategorized(money(amount)),
        })
        .collect();
    let transaction =
        Transaction::new(TransactionId::generate(), day, None, splits).expect("balanced");
    book.add_transaction(&transaction).expect("added");
}

/// Two months of salary, rent, groceries inside Food, and money not sorted.
fn book() -> Book {
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    let checking = account(&mut book, "Checking account", AccountKind::Asset, None);
    let food = account(&mut book, "Food", AccountKind::Expense, None);
    let groceries = account(&mut book, "Groceries", AccountKind::Expense, Some(food));
    let rent = account(&mut book, "Rent", AccountKind::Expense, None);
    let salary = account(&mut book, "Salary", AccountKind::Income, None);
    for month in [9, 10] {
        add(
            &mut book,
            date(2026, month, 1),
            &[(Some(checking), 250_000), (Some(salary), -250_000)],
        );
        add(
            &mut book,
            date(2026, month, 3),
            &[(Some(checking), -80_000), (Some(rent), 80_000)],
        );
    }
    add(
        &mut book,
        date(2026, 10, 5),
        &[(Some(checking), -5_320), (Some(groceries), 5_320)],
    );
    add(
        &mut book,
        date(2026, 10, 6),
        &[(Some(checking), -1_234), (None, 1_234)],
    );
    book
}

fn autumn() -> Period {
    Period::new(date(2026, 9, 1), date(2026, 10, 31)).expect("a period")
}

fn spending(book: &Book) -> kassensturz_report::SpendingByCategory {
    let source = Source::read(book, autumn()).expect("source");
    spending_dataset(&source, autumn(), &Filter::default(), Level::Each)
        .expect("dataset")
        .remove(0)
}

fn monthly(book: &Book) -> kassensturz_report::IncomeExpenseByMonth {
    let source = Source::read(book, autumn()).expect("source");
    monthly_dataset(&source, autumn(), &Filter::default())
        .expect("dataset")
        .remove(0)
}

#[test]
fn spending_is_a_bar_pie_or_donut_with_the_datasets_own_rows() {
    let book = book();
    let data = spending(&book);
    for kind in [ChartKind::Bar, ChartKind::Pie, ChartKind::Donut] {
        let chart = spending_by_category(&data, kind, &Words).expect("chart");
        // Largest first, as the dataset gives them.
        assert_eq!(
            chart.categories,
            ["Rent", "Food › Groceries", "(unsorted)"],
            "{kind:?}"
        );
        assert_eq!(chart.series.len(), 1);
        let amounts: Vec<i64> = chart.series[0]
            .points
            .iter()
            .map(|p| p.minor_units)
            .collect();
        assert_eq!(amounts, [160_000, 5_320, 1_234]);
        let labels: Vec<&str> = chart.series[0]
            .points
            .iter()
            .map(|p| p.label.as_str())
            .collect();
        assert_eq!(labels, ["1600,00 EUR", "53,20 EUR", "12,34 EUR"]);
        assert_eq!(chart.total, "1665,54 EUR");
        assert_eq!(chart.unsorted, Some(2));
        assert_eq!(chart.currency, "EUR");
        assert_eq!(chart.dataset, "spending_by_category");
        // The table is the dataset's own text table.
        assert_eq!(chart.table, data.to_text(&Words));
        assert_eq!(chart.ticks.is_empty(), kind.is_round());
    }
    assert_eq!(
        spending_by_category(&data, ChartKind::Line, &Words),
        Err(ChartError::KindNotSupported(ChartKind::Line))
    );
}

#[test]
fn income_and_expenses_are_two_series_with_a_point_for_every_month() {
    let book = book();
    let data = monthly(&book);
    for kind in [
        ChartKind::Line,
        ChartKind::Bar,
        ChartKind::StackedBar,
        ChartKind::Area,
    ] {
        let chart = income_expense_by_month(&data, kind, &Words).expect("chart");
        assert_eq!(chart.categories, ["2026-09", "2026-10"]);
        let keys: Vec<&str> = chart.series.iter().map(|s| s.key).collect();
        assert_eq!(keys, ["income", "expenses"]);
        assert_eq!(chart.series[0].color, 0);
        assert_eq!(chart.series[1].color, 1);
        let income: Vec<i64> = chart.series[0]
            .points
            .iter()
            .map(|p| p.minor_units)
            .collect();
        let expenses: Vec<i64> = chart.series[1]
            .points
            .iter()
            .map(|p| p.minor_units)
            .collect();
        assert_eq!(income, [250_000, 250_000]);
        assert_eq!(expenses, [80_000, 86_554]);
        assert_eq!(chart.series[1].points[1].label, "865,54 EUR");
        // What is left for the whole period: 5000,00 less 1665,54.
        assert_eq!(chart.total, "3334,46 EUR");
        assert_eq!(chart.table, data.to_text(&Words));
        assert!(
            chart
                .series
                .iter()
                .all(|s| s.points.len() == chart.categories.len())
        );
    }
    assert_eq!(
        income_expense_by_month(&data, ChartKind::Pie, &Words),
        Err(ChartError::KindNotSupported(ChartKind::Pie))
    );
}

#[test]
fn the_axis_reaches_from_zero_over_the_data_in_whole_steps() {
    let book = book();
    let data = monthly(&book);
    let bars = income_expense_by_month(&data, ChartKind::Bar, &Words).expect("chart");
    let marks: Vec<i64> = bars.ticks.iter().map(|t| t.minor_units).collect();
    // Up to 2500,00: steps of 500,00.
    assert_eq!(marks, [0, 50_000, 100_000, 150_000, 200_000, 250_000]);
    assert_eq!(bars.ticks[1].label, "500,00 EUR");
    // Stacked, the two series pile up to 3365,54, so the axis goes on to
    // the next step of 1000,00.
    let stacked = income_expense_by_month(&data, ChartKind::StackedBar, &Words).expect("chart");
    assert_eq!(stacked.ticks.last().map(|t| t.minor_units), Some(400_000));
}

#[test]
fn the_axis_includes_negative_values_and_nothing_at_all() {
    let tick_marks = |low, high| -> Vec<i64> {
        ticks::ticks(low, high, 100, EUR, &Words)
            .expect("ticks")
            .iter()
            .map(|t| t.minor_units)
            .collect()
    };
    assert_eq!(tick_marks(-30_000, 100_000), [-50_000, 0, 50_000, 100_000]);
    // Nothing to show still gives an axis with a zero mark.
    assert_eq!(tick_marks(0, 0), [0, 100]);
    // Marks are never closer than one major unit, so 3 cents have a mark at 1.
    assert_eq!(tick_marks(0, 3), [0, 100]);
}

#[test]
fn an_empty_dataset_gives_an_empty_chart() {
    let mut book = Book::create_in_memory().expect("book");
    book.use_currency(EUR).expect("currency");
    let data = kassensturz_report::SpendingByCategory {
        currency: EUR,
        rows: Vec::new(),
        total: money(0),
    };
    let chart = spending_by_category(&data, ChartKind::Pie, &Words).expect("chart");
    assert!(chart.categories.is_empty() && chart.series[0].points.is_empty());
    assert_eq!(chart.total, "0,00 EUR");
}

/// A provider that draws nothing, to run the default `prepare`.
struct Silent;

impl ChartProvider for Silent {
    fn name(&self) -> &'static str {
        "silent"
    }

    fn supports(&self, _kind: ChartKind) -> bool {
        true
    }
}

#[test]
fn the_provider_gets_drawing_values_that_match_the_exact_amounts() {
    let book = book();
    let chart = spending_by_category(&spending(&book), ChartKind::Bar, &Words).expect("chart");
    let prepared = Silent.prepare(&chart).expect("prepared");
    assert_eq!(prepared.series.len(), 1);
    assert_eq!(prepared.series[0].values, [160_000.0, 5_320.0, 1_234.0]);
    assert_eq!(prepared.ticks.len(), chart.ticks.len());
}

#[test]
fn a_slice_that_came_back_has_no_size_but_keeps_its_place() {
    let mut data = spending(&book());
    // Refunds exceeded the spending of the last category.
    data.rows[2].total = money(-500);
    let chart = spending_by_category(&data, ChartKind::Pie, &Words).expect("chart");
    let prepared = Silent.prepare(&chart).expect("prepared");
    assert_eq!(prepared.series[0].values, [160_000.0, 5_320.0, 0.0]);
    // The label and the table still say what happened.
    assert_eq!(chart.series[0].points[2].label, "-5,00 EUR");
    let bars = spending_by_category(&data, ChartKind::Bar, &Words).expect("chart");
    assert_eq!(
        Silent.prepare(&bars).expect("prepared").series[0].values[2],
        -500.0
    );
}

#[test]
fn an_amount_that_would_lose_digits_is_refused() {
    let limit: i64 = 1 << 53;
    assert_eq!(drawing_value(limit), Ok(9_007_199_254_740_992.0));
    assert_eq!(drawing_value(-limit), Ok(-9_007_199_254_740_992.0));
    assert_eq!(drawing_value(limit + 1), Err(ChartError::ValueTooLarge));
    assert_eq!(drawing_value(i64::MIN), Err(ChartError::ValueTooLarge));
    let mut data = spending(&book());
    data.rows[0].total = money(limit + 1);
    let chart = spending_by_category(&data, ChartKind::Bar, &Words).expect("chart");
    assert_eq!(Silent.prepare(&chart), Err(ChartError::ValueTooLarge));
}

#[test]
fn colors_come_from_the_palette_and_start_again_after_eight() {
    let colors: Vec<u8> = (0..10).map(ChartModel::color_of).collect();
    assert_eq!(colors, [0, 1, 2, 3, 4, 5, 6, 7, 0, 1]);
    assert_eq!(PALETTE_SIZE, 8);
}

#[test]
fn kinds_are_named_the_way_dashboards_spell_them() {
    for kind in [
        ChartKind::Line,
        ChartKind::Bar,
        ChartKind::StackedBar,
        ChartKind::Area,
        ChartKind::Pie,
        ChartKind::Donut,
    ] {
        assert_eq!(ChartKind::from_key(kind.key()), Some(kind));
    }
    assert_eq!(ChartKind::from_key("radar"), None);
    assert_eq!(ChartKind::StackedBar.key(), "stacked-bar");
}

proptest! {
    /// A drawing value is the exact amount, whenever one is made.
    #[test]
    fn drawing_values_are_exact(minor_units in -(1_i64 << 53)..=(1_i64 << 53)) {
        let value = drawing_value(minor_units).expect("in range");
        prop_assert_eq!(value.to_string(), minor_units.to_string());
    }

    /// The marks cover the data, rise in equal steps and stay few.
    #[test]
    fn marks_cover_the_data(low in -1_000_000_000_i64..=0, high in 0_i64..=1_000_000_000) {
        let marks = ticks::ticks(low, high, 100, EUR, &Words).expect("ticks");
        let first = marks.first().expect("a mark").minor_units;
        let last = marks.last().expect("a mark").minor_units;
        prop_assert!(first <= low && last >= high);
        prop_assert!(marks.len() <= 12);
        let step = marks[1].minor_units - marks[0].minor_units;
        prop_assert!(step >= 100);
        prop_assert!(marks.windows(2).all(|w| w[1].minor_units - w[0].minor_units == step));
    }
}
