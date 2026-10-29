// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of My groups and the hide list in the datasets (M6b.4, D-291):
//! groups and hidden categories made through the book's own store
//! functions and read back with `Source::read`, so the datasets use the
//! store's real output; and properties that no grouping or hiding changes
//! what the money adds up to.

use kassensturz_core::{
    AccountId, AccountKind, CategoryGroup, GroupId, GroupKind, GroupMember, Money,
};
use proptest::prelude::*;

use super::tests::{EUR, USD, Words, account, autumn, build, chart, date, entries, period, sample};
use super::*;

/// The rows as (name, minor units), the unsorted row as "-".
fn named(rows: &[CategoryRow]) -> Vec<(String, i64)> {
    rows.iter()
        .map(|row| {
            let name = if row.is_unsorted() {
                "-".into()
            } else {
                row.path.join(" › ")
            };
            (name, row.total.minor_units())
        })
        .collect()
}

fn shown(groups: &[GroupId]) -> Shown {
    Shown {
        groups: groups.to_vec(),
        ..Shown::default()
    }
}

fn spending(source: &Source, level: Level, shown: &Shown) -> Vec<SpendingByCategory> {
    spending_by_category_shown(source, autumn(), &Filter::default(), level, shown)
        .expect("spending")
}

fn group(book: &mut kassensturz_store::Book, name: &str, members: &[GroupMember]) -> GroupId {
    book.add_category_group(name, GroupKind::Expense, members)
        .expect("group added")
}

#[test]
fn a_group_counts_its_categories_with_everything_inside_once() {
    let mut sample = sample();
    // Food and Groceries inside it: Groceries is reached twice.
    let household = group(
        &mut sample.book,
        "Household",
        &[
            GroupMember::Category(sample.food),
            GroupMember::Category(sample.groceries),
        ],
    );
    let source = Source::read(&sample.book, autumn()).expect("read");
    for level in [Level::Each, Level::TopLevel] {
        let results = spending(&source, level, &shown(&[household]));
        let [eur, usd] = results.as_slice() else {
            panic!("{results:?}");
        };
        assert_eq!(
            named(&eur.rows),
            [
                ("Rent".into(), 240_000),
                ("Household".into(), 8_820),
                ("-".into(), 1_250),
            ]
        );
        assert_eq!(eur.rows[1].group, Some(household));
        assert_eq!(eur.rows[1].category, None);
        assert_eq!(eur.total.minor_units(), 250_070);
        assert!(eur.hidden.is_empty() && eur.hidden_total.is_zero());
        // The group holds no dollars: Travel stays on its own.
        assert_eq!(named(&usd.rows), [("Travel".into(), 9_900)]);
    }
    // Without groups the same money, category by category.
    let plain = spending(&source, Level::TopLevel, &Shown::default());
    assert_eq!(
        named(&plain[0].rows),
        [
            ("Rent".into(), 240_000),
            ("Food".into(), 8_820),
            ("-".into(), 1_250),
        ]
    );
    assert_eq!(plain[0].total.minor_units(), 250_070);
    assert_eq!(
        source.counted(GroupMember::Group(household)),
        [sample.food, sample.groceries]
    );
}

#[test]
fn a_group_inside_a_group_brings_its_members_along() {
    let mut sample = sample();
    let fixed = group(
        &mut sample.book,
        "Fixed costs",
        &[GroupMember::Category(sample.rent)],
    );
    let all = group(
        &mut sample.book,
        "Everything",
        &[
            GroupMember::Group(fixed),
            GroupMember::Category(sample.food),
        ],
    );
    let source = Source::read(&sample.book, autumn()).expect("read");
    let results = spending(&source, Level::Each, &shown(&[all]));
    assert_eq!(
        named(&results[0].rows),
        [("Everything".into(), 248_820), ("-".into(), 1_250)]
    );
    let mut counted = source.counted(GroupMember::Group(all));
    counted.sort_unstable();
    let mut expected = vec![sample.food, sample.groceries, sample.rent];
    expected.sort_unstable();
    assert_eq!(counted, expected);
    // The same group chosen twice is one row.
    let twice = spending(&source, Level::Each, &shown(&[all, all]));
    assert_eq!(twice, results);
}

#[test]
fn a_chart_of_parts_of_a_whole_refuses_two_groups_that_share_a_category() {
    let mut sample = sample();
    let household = group(
        &mut sample.book,
        "Household",
        &[GroupMember::Category(sample.food)],
    );
    let shop = group(
        &mut sample.book,
        "Shop",
        &[GroupMember::Category(sample.groceries)],
    );
    let source = Source::read(&sample.book, autumn()).expect("read");
    let refused = spending_by_category_shown(
        &source,
        autumn(),
        &Filter::default(),
        Level::Each,
        &shown(&[household, shop]),
    );
    let Err(ReportError::GroupsOverlap {
        category,
        first,
        second,
    }) = refused
    else {
        panic!("refused, not {refused:?}");
    };
    assert_eq!(
        (category.as_str(), first.as_str(), second.as_str()),
        ("Food › Groceries", "Household", "Shop")
    );
    // Separate bars may show both; the total still counts Groceries once.
    let bars = Shown {
        groups: vec![household, shop],
        overlap: Overlap::Allowed,
        ..Shown::default()
    };
    let results = spending(&source, Level::Each, &bars);
    assert_eq!(
        named(&results[0].rows),
        [
            ("Rent".into(), 240_000),
            ("Household".into(), 8_820),
            ("Shop".into(), 7_820),
            ("-".into(), 1_250),
        ]
    );
    assert_eq!(results[0].total.minor_units(), 250_070);
}

#[test]
fn hidden_categories_leave_the_chart_and_are_named_under_it() {
    let mut sample = sample();
    sample
        .book
        .set_hidden_categories(&[sample.rent])
        .expect("hidden");
    let source = Source::read(&sample.book, autumn()).expect("read");
    let results = spending(&source, Level::Each, &Shown::default());
    let eur = &results[0];
    assert_eq!(
        named(&eur.rows),
        [
            ("Food › Groceries".into(), 7_820),
            ("-".into(), 1_250),
            ("Food".into(), 1_000),
        ]
    );
    assert_eq!(named(&eur.hidden), [("Rent".into(), 240_000)]);
    assert_eq!(eur.hidden_total.minor_units(), 240_000);
    // The shown total is what is shown.
    assert_eq!(eur.total.minor_units(), 10_070);
    // The one component that shows them anyway, and the charts of M6a.
    let anyway = Shown {
        show_hidden: true,
        ..Shown::default()
    };
    for all in [
        spending(&source, Level::Each, &anyway),
        spending_by_category(&source, autumn(), &Filter::default(), Level::Each).expect("spending"),
    ] {
        assert_eq!(all[0].rows[0].path, ["Rent"]);
        assert_eq!(all[0].total.minor_units(), 250_070);
        assert!(all[0].hidden.is_empty());
    }
}

#[test]
fn a_hidden_category_takes_everything_inside_it_along() {
    let mut sample = sample();
    sample
        .book
        .set_hidden_categories(&[sample.food])
        .expect("hidden");
    let source = Source::read(&sample.book, autumn()).expect("read");
    for level in [Level::Each, Level::TopLevel] {
        let results = spending(&source, level, &Shown::default());
        assert_eq!(
            named(&results[0].rows),
            [("Rent".into(), 240_000), ("-".into(), 1_250)]
        );
        assert_eq!(named(&results[0].hidden), [("Food".into(), 8_820)]);
    }
    // A hidden subcategory leaves the row of the category it sits in.
    sample
        .book
        .set_hidden_categories(&[sample.groceries])
        .expect("hidden");
    let source = Source::read(&sample.book, autumn()).expect("read");
    let results = spending(&source, Level::TopLevel, &Shown::default());
    assert_eq!(
        named(&results[0].rows),
        [
            ("Rent".into(), 240_000),
            ("-".into(), 1_250),
            ("Food".into(), 1_000),
        ]
    );
    assert_eq!(
        named(&results[0].hidden),
        [("Food › Groceries".into(), 7_820)]
    );
}

#[test]
fn a_group_counts_its_hidden_members_and_a_currency_with_only_hidden_money_says_so() {
    let mut sample = sample();
    sample
        .book
        .set_hidden_categories(&[sample.groceries, sample.travel])
        .expect("hidden");
    let household = group(
        &mut sample.book,
        "Household",
        &[GroupMember::Category(sample.food)],
    );
    let source = Source::read(&sample.book, autumn()).expect("read");
    let results = spending(&source, Level::Each, &shown(&[household]));
    let [eur, usd] = results.as_slice() else {
        panic!("{results:?}");
    };
    // Q-225 = A: a group's figure is the same everywhere.
    assert_eq!(
        named(&eur.rows),
        [
            ("Rent".into(), 240_000),
            ("Household".into(), 8_820),
            ("-".into(), 1_250),
        ]
    );
    assert!(eur.hidden.is_empty());
    // Nothing shown in dollars, but the note says what is left out.
    assert!(usd.rows.is_empty() && usd.total.is_zero());
    assert_eq!(named(&usd.hidden), [("Travel".into(), 9_900)]);
    assert_eq!(usd.hidden_total, Money::from_minor_units(9_900, USD));
}

#[test]
fn the_key_figures_never_change_with_groups_or_the_hide_list() {
    let mut sample = sample();
    let before = income_expense_by_month(
        &Source::read(&sample.book, autumn()).expect("read"),
        autumn(),
        &Filter::default(),
    )
    .expect("months");
    group(
        &mut sample.book,
        "Household",
        &[GroupMember::Category(sample.food)],
    );
    sample
        .book
        .set_hidden_categories(&[sample.rent, sample.travel])
        .expect("hidden");
    let source = Source::read(&sample.book, autumn()).expect("read");
    assert_eq!(
        income_expense_by_month(&source, autumn(), &Filter::default()).expect("months"),
        before
    );
}

#[test]
fn category_over_time_counts_each_series_by_month() {
    let mut sample = sample();
    let household = group(
        &mut sample.book,
        "Household",
        &[GroupMember::Category(sample.food)],
    );
    // A chosen series counts even when it is hidden: it was named.
    sample
        .book
        .set_hidden_categories(&[sample.rent])
        .expect("hidden");
    let source = Source::read(&sample.book, autumn()).expect("read");
    let series = [
        GroupMember::Group(household),
        GroupMember::Category(sample.rent),
        GroupMember::Category(sample.salary),
    ];
    let results = category_over_time(
        &source,
        autumn(),
        &Filter::default(),
        &series,
        Overlap::Refused,
    )
    .expect("over time");
    // Travel is not chosen, so there is no dollar result.
    let [eur] = results.as_slice() else {
        panic!("{results:?}");
    };
    assert_eq!(eur.currency, EUR);
    let paths: Vec<String> = eur.series.iter().map(|s| s.path.join(" › ")).collect();
    assert_eq!(paths, ["Household", "Rent", "Salary"]);
    let months: Vec<(String, Vec<i64>)> = eur
        .rows
        .iter()
        .map(|row| {
            (
                Words.month(row.month),
                row.totals.iter().map(|total| total.minor_units()).collect(),
            )
        })
        .collect();
    assert_eq!(
        months,
        [
            ("2026-08".into(), vec![5_320, 80_000, 250_000]),
            ("2026-09".into(), vec![3_500, 80_000, 250_000]),
            ("2026-10".into(), vec![0, 80_000, 250_000]),
        ]
    );
    let totals: Vec<i64> = eur.totals.iter().map(|total| total.minor_units()).collect();
    assert_eq!(totals, [8_820, 240_000, 750_000]);
    // The two datasets agree on the group's figure.
    let pie = spending(&source, Level::Each, &shown(&[household]));
    let row = pie[0]
        .rows
        .iter()
        .find(|row| row.group == Some(household))
        .expect("group row");
    assert_eq!(row.total, eur.totals[0]);

    let text = eur.to_text(&Words);
    assert_eq!(text.columns, ["month", "category", "total"]);
    assert_eq!(text.rows.len(), 9);
    assert_eq!(text.rows[3], ["2026-09", "Household", "3500 EUR"]);
}

#[test]
fn stacked_series_refuse_a_category_inside_another_and_unknown_ones_are_named() {
    let sample = sample();
    let source = Source::read(&sample.book, autumn()).expect("read");
    let nested = [
        GroupMember::Category(sample.food),
        GroupMember::Category(sample.groceries),
    ];
    let refused = category_over_time(
        &source,
        autumn(),
        &Filter::default(),
        &nested,
        Overlap::Refused,
    );
    assert!(
        matches!(&refused, Err(ReportError::GroupsOverlap { category, .. }) if category == "Food › Groceries"),
        "{refused:?}"
    );
    let lines = category_over_time(
        &source,
        autumn(),
        &Filter::default(),
        &nested,
        Overlap::Allowed,
    )
    .expect("lines");
    let totals: Vec<i64> = lines[0].totals.iter().map(|t| t.minor_units()).collect();
    assert_eq!(totals, [8_820, 7_820]);

    // A group the book doesn't know and an account that is no category are
    // skipped, and named so the chart can say so.
    let gone = GroupMember::Group(GroupId::generate());
    let not_a_category = GroupMember::Category(sample.checking);
    let chosen = [gone, GroupMember::Category(sample.rent), not_a_category];
    assert_eq!(source.unknown(&chosen), [gone, not_a_category]);
    let results = category_over_time(
        &source,
        autumn(),
        &Filter::default(),
        &chosen,
        Overlap::Refused,
    )
    .expect("over time");
    assert_eq!(results[0].series.len(), 1);
    assert!(source.counted(gone).is_empty());
    let ignored = spending(&source, Level::Each, &shown(&[GroupId::generate()]));
    assert_eq!(ignored[0].rows[0].path, ["Rent"]);
}

#[test]
fn a_removed_category_leaves_its_groups_and_the_hide_list() {
    let mut sample = sample();
    let phone = account(&mut sample.book, "Phone", AccountKind::Expense, EUR, None);
    let fixed = group(
        &mut sample.book,
        "Fixed costs",
        &[
            GroupMember::Category(sample.rent),
            GroupMember::Category(phone),
        ],
    );
    sample
        .book
        .set_hidden_categories(&[phone, sample.travel])
        .expect("hidden");
    sample.book.remove_category(phone).expect("removed");
    let source = Source::read(&sample.book, autumn()).expect("read");
    assert_eq!(source.counted(GroupMember::Group(fixed)), [sample.rent]);
    assert_eq!(source.hidden(), [sample.travel]);
    let results = spending(&source, Level::Each, &shown(&[fixed]));
    assert_eq!(named(&results[0].rows)[0], ("Fixed costs".into(), 240_000));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// However the categories are grouped and hidden, nothing is lost and
    /// nothing counted twice: the rows add up to the shown total, the shown
    /// and the hidden money add up to all spending, and the group's figure
    /// is the same in the pie and over time.
    #[test]
    fn groups_and_hiding_never_change_what_the_money_adds_up_to(
        entries in entries(),
        members in 0usize..4,
        hidden in 0usize..4,
        level in prop_oneof![Just(Level::Each), Just(Level::TopLevel)],
    ) {
        let chart = chart();
        let whole = period(date(2026, 1, 1), date(2026, 12, 31));
        // The chart's categories: Salary, Food, Food › Groceries.
        let food = chart.tree[1].category.id;
        let groceries = chart.tree[2].category.id;
        let pick = |bits: usize| -> Vec<AccountId> {
            [food, groceries]
                .into_iter()
                .enumerate()
                .filter(|(at, _)| bits & (1 << at) != 0)
                .map(|(_, id)| id)
                .collect()
        };
        let id = GroupId::generate();
        let group = CategoryGroup {
            id,
            name: "Household".into(),
            kind: GroupKind::Expense,
            position: 0,
            members: pick(members).into_iter().map(GroupMember::Category).collect(),
        };
        let source = Source::from_parts(
            whole,
            chart.accounts.clone(),
            chart.tree.clone(),
            build(&chart, &entries),
        )
        .with_groups(vec![group], pick(hidden));

        let all = spending_by_category(&source, whole, &Filter::default(), level).unwrap();
        let spent = all.first().map_or(0, |result| result.total.minor_units());
        let grouped = spending_by_category_shown(
            &source, whole, &Filter::default(), level, &shown(&[id]),
        ).unwrap();
        for result in &grouped {
            let added: i64 = result.rows.iter().map(|row| row.total.minor_units()).sum();
            prop_assert_eq!(added, result.total.minor_units());
            let hidden: i64 = result.hidden.iter().map(|row| row.total.minor_units()).sum();
            prop_assert_eq!(hidden, result.hidden_total.minor_units());
            prop_assert_eq!(result.total.minor_units() + hidden, spent);
        }

        let over_time = category_over_time(
            &source, whole, &Filter::default(), &[GroupMember::Group(id)], Overlap::Refused,
        ).unwrap();
        let in_pie = grouped
            .first()
            .and_then(|result| result.rows.iter().find(|row| row.group == Some(id)))
            .map_or(0, |row| row.total.minor_units());
        let in_lines = over_time.first().map_or(0, |result| result.totals[0].minor_units());
        prop_assert_eq!(in_pie, in_lines);
        for result in &over_time {
            let by_month: i64 = result.rows.iter().map(|row| row.totals[0].minor_units()).sum();
            prop_assert_eq!(by_month, result.totals[0].minor_units());
        }
    }
}
