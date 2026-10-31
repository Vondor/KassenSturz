// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the Statistic Builder's columns (D-339): made over the book's
//! own sample and read back through `Source::read`, so the columns use the
//! store's real output. A column never changes a figure of the book; it
//! only puts figures together.

use kassensturz_core::{GroupId, GroupKind, GroupMember};

use super::tests::{EUR, USD, autumn, sample};
use super::*;

fn column(name: &str, members: &[GroupMember]) -> Column {
    Column {
        name: name.to_owned(),
        members: members.to_vec(),
        formula: None,
    }
}

fn totals(data: &CategoryOverTime) -> Vec<i64> {
    data.totals
        .iter()
        .map(|total| total.minor_units())
        .collect()
}

#[test]
fn a_column_adds_up_its_categories_with_everything_inside() {
    let sample = sample();
    let source = Source::read(&sample.book, autumn()).expect("read");
    let columns = [
        column("Home", &[GroupMember::Category(sample.rent)]),
        column("Eating", &[GroupMember::Category(sample.food)]),
    ];
    let results =
        columns_over_time(&source, autumn(), &Filter::default(), &columns, EUR).expect("columns");
    let [euros] = results.as_slice() else {
        panic!("only euros have money in these columns: {results:?}");
    };
    assert_eq!(euros.currency, EUR);
    assert_eq!(totals(euros), [240_000, 8_820]);
    let names: Vec<_> = euros.series.iter().map(|s| s.path.join("/")).collect();
    assert_eq!(names, ["Home", "Eating"]);
    assert!(euros.series.iter().all(|series| series.of.is_none()));
    // One row for every month of the period; the months add up to the total.
    assert_eq!(euros.rows.len(), 3);
    let home: i64 = euros
        .rows
        .iter()
        .map(|row| row.totals[0].minor_units())
        .sum();
    assert_eq!(home, 240_000);
}

#[test]
fn a_column_can_hold_a_group_and_a_category_each_counted_once() {
    let mut sample = sample();
    let household = sample
        .book
        .add_category_group(
            "Household",
            GroupKind::Expense,
            &[GroupMember::Category(sample.food)],
        )
        .expect("group");
    let source = Source::read(&sample.book, autumn()).expect("read");
    // Food is reached through the group and as Groceries, and counted once.
    let columns = [column(
        "Both",
        &[
            GroupMember::Group(household),
            GroupMember::Category(sample.groceries),
            GroupMember::Category(sample.travel),
        ],
    )];
    let results =
        columns_over_time(&source, autumn(), &Filter::default(), &columns, EUR).expect("columns");
    let [euros, dollars] = results.as_slice() else {
        panic!("{results:?}");
    };
    assert_eq!((euros.currency, dollars.currency), (EUR, USD));
    assert_eq!(totals(euros), [8_820]);
    assert_eq!(totals(dollars), [9_900]);
}

#[test]
fn columns_that_share_a_category_count_it_in_each_and_say_so() {
    let sample = sample();
    let source = Source::read(&sample.book, autumn()).expect("read");
    let columns = [
        column("Work food", &[GroupMember::Category(sample.groceries)]),
        column("All food", &[GroupMember::Category(sample.food)]),
        column("Home", &[GroupMember::Category(sample.rent)]),
    ];
    // Groceries (7,820) sits in both food columns; no error, no refusal.
    let results =
        columns_over_time(&source, autumn(), &Filter::default(), &columns, EUR).expect("columns");
    assert_eq!(totals(&results[0]), [7_820, 8_820, 240_000]);
    assert_eq!(
        shared_categories(&source, &columns),
        [SharedCategory {
            path: vec!["Food".to_owned(), "Groceries".to_owned()],
            columns: vec!["Work food".to_owned(), "All food".to_owned()],
        }]
    );
    // The rows of a pie add up to more than was spent, the total does not.
    let spending =
        columns_spending(&source, autumn(), &Filter::default(), &columns, EUR).expect("spending");
    let euros = &spending[0];
    let rows: Vec<(String, i64)> = euros
        .rows
        .iter()
        .map(|row| (row.path.join(" › "), row.total.minor_units()))
        .collect();
    assert_eq!(
        rows,
        [
            ("Home".to_owned(), 240_000),
            ("All food".to_owned(), 8_820),
            ("Work food".to_owned(), 7_820),
        ]
    );
    assert_eq!(euros.total.minor_units(), 248_820);
    assert!(euros.rows.iter().all(|row| !row.is_unsorted()));
    // Without sharing nothing is reported.
    assert!(shared_categories(&source, &columns[1..]).is_empty());
}

#[test]
fn a_column_never_changes_the_book_and_unknown_members_are_counted() {
    let sample = sample();
    let source = Source::read(&sample.book, autumn()).expect("read");
    let gone = GroupMember::Group(GroupId::generate());
    let columns = [
        column("Odd", &[gone, GroupMember::Category(sample.rent)]),
        column("Empty", &[]),
    ];
    assert_eq!(unknown_column_members(&source, &columns), 1);
    let results =
        columns_over_time(&source, autumn(), &Filter::default(), &columns, EUR).expect("columns");
    assert_eq!(totals(&results[0]), [240_000, 0]);
    // The spending dataset of the same book is as before.
    let plain =
        spending_by_category(&source, autumn(), &Filter::default(), Level::Each).expect("spending");
    assert_eq!(plain[0].total.minor_units(), 250_070);
    // A column with no money is no row.
    let spending =
        columns_spending(&source, autumn(), &Filter::default(), &columns, EUR).expect("spending");
    assert_eq!(spending[0].rows.len(), 1);
}

fn formula_column(name: &str, formula: &str) -> Column {
    Column {
        name: name.to_owned(),
        members: Vec::new(),
        formula: Some(formula.to_owned()),
    }
}

#[test]
fn a_column_can_be_a_formula_that_names_the_other_columns() {
    let sample = sample();
    let source = Source::read(&sample.book, autumn()).expect("read");
    let columns = [
        column("Home", &[GroupMember::Category(sample.rent)]),
        column("Eating", &[GroupMember::Category(sample.food)]),
        formula_column("Both", "Home + Eating"),
        formula_column("Twice the rent", "2 * Home"),
    ];
    let results =
        columns_over_time(&source, autumn(), &Filter::default(), &columns, EUR).expect("columns");
    let [euros] = results.as_slice() else {
        panic!("only euros: {results:?}");
    };
    assert_eq!(totals(euros), [240_000, 8_820, 248_820, 480_000]);
    // Month by month the formula is worked out for that month alone, so an
    // additive one adds up to its whole figure.
    assert_eq!(euros.rows.len(), 3);
    for index in 0..4 {
        let months: i64 = euros
            .rows
            .iter()
            .map(|row| row.totals[index].minor_units())
            .sum();
        assert_eq!(months, euros.totals[index].minor_units(), "series {index}");
    }
    // As the rows of a pie the formula column is a row like the others, and
    // the total counts each part once, the formulas adding nothing.
    let spending =
        columns_spending(&source, autumn(), &Filter::default(), &columns, EUR).expect("spending");
    assert_eq!(spending[0].total.minor_units(), 248_820);
    assert_eq!(spending[0].rows.len(), 4);
    assert_eq!(spending[0].rows[0].path, ["Twice the rent"]);
}

#[test]
fn a_formula_column_must_give_an_amount_and_may_only_name_columns_with_members() {
    let sample = sample();
    let source = Source::read(&sample.book, autumn()).expect("read");
    let home = column("Home", &[GroupMember::Category(sample.rent)]);
    let ratio = [
        home.clone(),
        formula_column("Share", "PERCENT_OF(Home; Home)"),
    ];
    let answer = columns_over_time(&source, autumn(), &Filter::default(), &ratio, EUR);
    assert!(matches!(
        answer,
        Err(ReportError::ColumnFormulaNotAmount { column }) if column == "Share"
    ));
    // Another formula column has no figure to name, and a column cannot name
    // itself.
    let loops = [
        home,
        formula_column("A", "B + 1"),
        formula_column("B", "A + 1"),
    ];
    assert!(matches!(
        columns_over_time(&source, autumn(), &Filter::default(), &loops, EUR),
        Err(ReportError::FormulaUnknownName { .. })
    ));
}
