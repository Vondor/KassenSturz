// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The flat tables of the seven datasets (M6e.2), made from the real output
//! of the datasets on the report tests' sample book, so a change to a
//! dataset's result shows up here.

use kassensturz_core::{BudgetEvery, BudgetId, GroupMember, Money};

use super::*;
use crate::tests::{EUR, autumn, date, money, sample};
use crate::{
    BudgetPeriod, BudgetUtilization, Filter, Level, Overlap, Percent, Source, account_balances,
    cash_flow_by_month, category_over_time, income_expense_by_month, net_worth_over_time,
    spending_by_category,
};

fn names(table: &FlatTable) -> Vec<&'static str> {
    table.dataset().fields().iter().map(|f| f.name()).collect()
}

/// The text of the cell `field` of `row`.
fn text(table: &FlatTable, row: usize, field: &str) -> String {
    let at = names(table)
        .iter()
        .position(|name| *name == field)
        .expect("a field");
    match &table.rows()[row][at] {
        Cell::Text(text) => text.clone(),
        Cell::Day(day) => day.to_string(),
        Cell::Amount(amount) => amount.minor_units().to_string(),
        Cell::Percent(percent) => percent.map_or_else(String::new, |p| p.tenths().to_string()),
        Cell::YesNo(value) => value.to_string(),
    }
}

#[test]
fn spending_rows_name_the_category_its_top_level_and_the_unsorted_money() {
    let sample = sample();
    let source = Source::read(&sample.book, autumn()).expect("source");
    let results =
        spending_by_category(&source, autumn(), &Filter::default(), Level::Each).expect("dataset");
    let table = FlatTable::from_spending(&results[0]);
    assert_eq!(table.currency(), EUR);
    let found: Vec<(String, String, String)> = (0..table.rows().len())
        .map(|row| {
            (
                text(&table, row, "category"),
                text(&table, row, "top_level"),
                text(&table, row, "total"),
            )
        })
        .collect();
    // Largest first as the dataset has it; the unsorted money has empty names.
    assert_eq!(
        found,
        [
            ("Rent".into(), "Rent".into(), "240000".into()),
            ("Food › Groceries".into(), "Food".into(), "7820".into()),
            (String::new(), String::new(), "1250".into()),
            ("Food".into(), "Food".into(), "1000".into()),
        ]
    );
    assert_eq!(text(&table, 0, "currency"), "EUR");
}

#[test]
fn monthly_rows_have_a_row_per_month() {
    let sample = sample();
    let source = Source::read(&sample.book, autumn()).expect("source");
    let results = income_expense_by_month(&source, autumn(), &Filter::default()).expect("dataset");
    let table = FlatTable::from_monthly(&results[0]);
    let months: Vec<String> = (0..table.rows().len())
        .map(|row| text(&table, row, "month"))
        .collect();
    assert_eq!(months, ["2026-08-01", "2026-09-01", "2026-10-01"]);
    // net = income - expenses, exactly, in every row.
    for row in 0..table.rows().len() {
        let number = |field: &str| {
            text(&table, row, field)
                .parse::<i64>()
                .expect("minor units")
        };
        assert_eq!(number("net"), number("income") - number("expenses"));
    }
}

#[test]
fn category_over_time_has_a_row_per_month_and_chosen_series() {
    let sample = sample();
    let source = Source::read(&sample.book, autumn()).expect("source");
    let series = [
        GroupMember::Category(sample.rent),
        GroupMember::Category(sample.groceries),
    ];
    let results = category_over_time(
        &source,
        autumn(),
        &Filter::default(),
        &series,
        Overlap::Allowed,
    )
    .expect("dataset");
    let table = FlatTable::from_over_time(&results[0]);
    assert_eq!(table.rows().len(), 6);
    assert_eq!(text(&table, 0, "series"), "Rent");
    assert_eq!(text(&table, 0, "total"), "80000");
    assert_eq!(text(&table, 1, "series"), "Food › Groceries");
    assert_eq!(text(&table, 1, "month"), "2026-08-01");
    assert_eq!(text(&table, 1, "total"), "5320");
}

#[test]
fn the_balances_net_worth_and_cash_flow_tables_come_from_the_real_datasets() {
    let sample = sample();
    let source = Source::read_with_balances(&sample.book, autumn()).expect("source");
    let balances = account_balances(&source, autumn(), &Filter::default(), None).expect("balances");
    let euros = balances
        .results
        .iter()
        .find(|result| result.currency == EUR)
        .expect("euro accounts");
    let table = FlatTable::from_balances(euros);
    assert!(!table.rows().is_empty());
    assert_eq!(table.rows().len(), euros.rows.len());
    assert_eq!(text(&table, 0, "owed"), "false");

    let worth = net_worth_over_time(&source, autumn(), &Filter::default(), None).expect("worth");
    let euros = worth
        .results
        .iter()
        .find(|result| result.currency == EUR)
        .expect("euro worth");
    let table = FlatTable::from_worth(euros);
    // A three-month period has a point for every month end and no opening point.
    assert_eq!(table.rows().len(), 3);
    assert_eq!(text(&table, 0, "month"), "2026-08-01");

    let flows = cash_flow_by_month(&source, autumn(), &Filter::default()).expect("flow");
    let euros = flows
        .iter()
        .find(|flow| flow.currency == EUR)
        .expect("euros");
    let table = FlatTable::from_cash_flow(euros);
    assert_eq!(table.rows().len(), 3);
    for row in 0..3 {
        let number = |field: &str| {
            text(&table, row, field)
                .parse::<i64>()
                .expect("minor units")
        };
        assert_eq!(
            number("start") + number("net_change"),
            number("end"),
            "the month starts and ends where its change says"
        );
    }
}

#[test]
fn a_debt_is_a_positive_amount_with_owed_set() {
    let sample = sample();
    let source = Source::read_with_balances(&sample.book, autumn()).expect("source");
    let mut balances =
        account_balances(&source, autumn(), &Filter::default(), None).expect("balances");
    let euros = balances
        .results
        .iter_mut()
        .find(|result| result.currency == EUR)
        .expect("euros");
    euros.rows[0].owed = true;
    euros.rows[0].balance = money(-4_200);
    let table = FlatTable::from_balances(euros);
    assert_eq!(text(&table, 0, "owed"), "true");
    assert_eq!(text(&table, 0, "balance"), "4200");
}

#[test]
fn budget_rows_are_split_by_currency_and_keep_the_percent() {
    let period = |used| BudgetPeriod {
        first_day: date(2026, 10, 1),
        last_day: date(2026, 10, 31),
        counted_to: date(2026, 10, 31),
        planned: money(50_000),
        carried_in: money(0),
        available: money(50_000),
        spent: money(12_500),
        left: money(37_500),
        used,
        carries_over: false,
        counts_changed: false,
        starts_later: false,
        ended: false,
    };
    let budget = |name: &str, currency, used| BudgetUtilization {
        budget: BudgetId::generate(),
        name: None,
        categories: vec![kassensturz_core::AccountId::generate()],
        paths: vec![vec!["Food".into(), name.into()]],
        accounts: Vec::new(),
        every: BudgetEvery::Month,
        currency,
        periods: vec![period(used)],
    };
    let dollars = crate::tests::USD;
    let mut other = budget("Dining", dollars, None);
    other.periods[0].planned = Money::from_minor_units(100, dollars);
    other.periods[0].carried_in = Money::from_minor_units(0, dollars);
    other.periods[0].available = Money::from_minor_units(100, dollars);
    other.periods[0].spent = Money::from_minor_units(0, dollars);
    other.periods[0].left = Money::from_minor_units(100, dollars);
    let budgets = [
        budget("Groceries", EUR, Some(Percent::from_tenths(250))),
        other,
    ];
    let tables = budget_tables(&budgets);
    assert_eq!(tables.len(), 2);
    assert_eq!(tables[0].currency(), EUR);
    assert_eq!(text(&tables[0], 0, "category"), "Food › Groceries");
    assert_eq!(text(&tables[0], 0, "every"), "month");
    assert_eq!(text(&tables[0], 0, "used"), "250");
    assert_eq!(tables[1].currency(), dollars);
    assert_eq!(
        text(&tables[1], 0, "used"),
        "",
        "nothing was available to compare with"
    );
}

#[test]
fn rows_that_do_not_fit_the_dataset_are_refused() {
    let dataset = spec_dataset("income_expense_by_month").expect("dataset");
    let short = vec![vec![Cell::Day(date(2026, 1, 1))]];
    assert_eq!(
        FlatTable::new(dataset, EUR, short),
        Err(RowsError::WrongShape)
    );
    let wrong_kind = vec![vec![
        Cell::Text("x".into()),
        Cell::Amount(money(1)),
        Cell::Amount(money(1)),
        Cell::Amount(money(0)),
        Cell::Text("EUR".into()),
    ]];
    assert_eq!(
        FlatTable::new(dataset, EUR, wrong_kind),
        Err(RowsError::WrongShape)
    );
    let dollars = Money::from_minor_units(1, crate::tests::USD);
    let mixed = vec![vec![
        Cell::Day(date(2026, 1, 1)),
        Cell::Amount(dollars),
        Cell::Amount(money(1)),
        Cell::Amount(money(0)),
        Cell::Text("EUR".into()),
    ]];
    assert_eq!(
        FlatTable::new(dataset, EUR, mixed),
        Err(RowsError::MixedCurrencies)
    );
}
