// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the sum by tag (US5.2, D-714): which parts a tag filter counts,
//! on a book whose tags the store itself wrote (US4.8's real output, layout
//! 30), and the component's own `params.tags` from a real dashboard
//! document through the filter. Synthetic data only.

use kassensturz_core::{
    AccountId, AccountKind, CurrencyCode, Date, Money, Split, Tag, TagId, Transaction,
    TransactionId,
};
use kassensturz_store::Book;
use serde_json::json;

use super::*;
use crate::tests::{EUR, USD, account, date, money};

struct Fixture {
    book: Book,
    checking: AccountId,
    savings: AccountId,
    dollars: AccountId,
    opening: AccountId,
    food: AccountId,
    gifts: AccountId,
    salary: AccountId,
    holiday: TagId,
    tax: TagId,
}

fn tag(book: &mut Book, name: &str) -> TagId {
    let id = TagId::generate();
    book.add_tag(&Tag {
        id,
        name: name.into(),
    })
    .expect("tag added");
    id
}

fn fixture() -> Fixture {
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    book.use_currency(USD).expect("currency used");
    let checking = account(&mut book, "Checking", AccountKind::Asset, EUR, None);
    let savings = account(&mut book, "Savings", AccountKind::Asset, EUR, None);
    let dollars = account(&mut book, "Dollar account", AccountKind::Asset, USD, None);
    let opening = account(&mut book, "Opening", AccountKind::Equity, EUR, None);
    let food = account(&mut book, "Food", AccountKind::Expense, EUR, None);
    let gifts = account(&mut book, "Gifts", AccountKind::Expense, EUR, None);
    let salary = account(&mut book, "Salary", AccountKind::Income, EUR, None);
    let holiday = tag(&mut book, "Holiday");
    let tax = tag(&mut book, "Tax");
    Fixture {
        book,
        checking,
        savings,
        dollars,
        opening,
        food,
        gifts,
        salary,
        holiday,
        tax,
    }
}

fn add(book: &mut Book, transaction: Transaction) {
    book.add_transaction(&transaction).expect("added");
}

fn entry(day: Date, splits: Vec<Split>) -> Transaction {
    Transaction::new(TransactionId::generate(), day, None, splits).expect("balanced")
}

fn tagged(split: Split, tags: &[TagId]) -> Split {
    Split {
        tags: tags.to_vec(),
        ..split
    }
}

fn november() -> Period {
    Period::new(date(2026, 11, 1), date(2026, 11, 30)).expect("period")
}

fn only(tags: &[TagId]) -> Filter {
    Filter {
        tags: Some(tags.to_vec()),
        ..Filter::default()
    }
}

/// The spending rows by category name, and the total, in minor units.
/// One currency's rows and total.
type Spent = (CurrencyCode, Vec<(String, i64)>, i64);

fn spending(book: &Book, filter: &Filter) -> Vec<Spent> {
    let source = Source::read(book, november()).expect("read");
    spending_by_category(&source, november(), filter, Level::Each)
        .expect("dataset")
        .into_iter()
        .map(|result| {
            (
                result.currency,
                result
                    .rows
                    .iter()
                    .map(|row| (row.path.join(" › "), row.total.minor_units()))
                    .collect(),
                result.total.minor_units(),
            )
        })
        .collect()
}

#[test]
fn a_tag_on_the_whole_entry_counts_each_category_part_once_and_never_the_account() {
    let mut f = fixture();
    // 90.00 from checking: 60.00 food and 30.00 gifts, the whole entry
    // tagged Holiday.
    add(
        &mut f.book,
        entry(
            date(2026, 11, 3),
            vec![
                Split::new(f.checking, money(-9_000)),
                Split::new(f.food, money(6_000)),
                Split::new(f.gifts, money(3_000)),
            ],
        )
        .with_tags(vec![f.holiday]),
    );
    // Something untagged that must not count.
    add(
        &mut f.book,
        entry(
            date(2026, 11, 4),
            vec![
                Split::new(f.checking, money(-1_500)),
                Split::new(f.food, money(1_500)),
            ],
        ),
    );
    assert_eq!(
        spending(&f.book, &only(&[f.holiday])),
        [(
            EUR,
            vec![("Food".to_owned(), 6_000), ("Gifts".to_owned(), 3_000)],
            9_000
        )]
    );
    // Without a tag filter both entries count, as before.
    assert_eq!(spending(&f.book, &Filter::default())[0].2, 10_500);
}

#[test]
fn a_tag_on_one_part_counts_only_that_part() {
    let mut f = fixture();
    add(
        &mut f.book,
        entry(
            date(2026, 11, 3),
            vec![
                Split::new(f.checking, money(-9_000)),
                Split::new(f.food, money(6_000)),
                tagged(Split::new(f.gifts, money(3_000)), &[f.holiday]),
            ],
        ),
    );
    assert_eq!(
        spending(&f.book, &only(&[f.holiday])),
        [(EUR, vec![("Gifts".to_owned(), 3_000)], 3_000)]
    );
    // A tag nothing carries counts nothing.
    assert!(spending(&f.book, &only(&[f.tax])).is_empty());
    // A filter that chooses no tag counts nothing either.
    assert!(spending(&f.book, &only(&[])).is_empty());
}

#[test]
fn a_part_with_two_of_the_tags_counts_once() {
    let mut f = fixture();
    add(
        &mut f.book,
        entry(
            date(2026, 11, 3),
            vec![
                Split::new(f.checking, money(-5_000)),
                tagged(Split::new(f.food, money(5_000)), &[f.tax]),
            ],
        )
        .with_tags(vec![f.holiday]),
    );
    // Each tag on its own counts the part: there is no total over the
    // tags, since it would count the part twice.
    assert_eq!(spending(&f.book, &only(&[f.holiday]))[0].2, 5_000);
    assert_eq!(spending(&f.book, &only(&[f.tax]))[0].2, 5_000);
    // Asked for both, the part counts once.
    assert_eq!(spending(&f.book, &only(&[f.holiday, f.tax]))[0].2, 5_000);
}

#[test]
fn a_tagged_transfer_and_starting_balance_count_nothing() {
    let mut f = fixture();
    // A Transfer between own accounts: two account sides, no category side.
    add(
        &mut f.book,
        entry(
            date(2026, 11, 5),
            vec![
                Split::new(f.checking, money(-20_000)),
                Split::new(f.savings, money(20_000)),
            ],
        )
        .with_tags(vec![f.holiday]),
    );
    // A starting balance (Equity) tagged on its part and on the whole.
    add(
        &mut f.book,
        entry(
            date(2026, 11, 1),
            vec![
                Split::new(f.checking, money(100_000)),
                tagged(Split::new(f.opening, money(-100_000)), &[f.holiday]),
            ],
        )
        .with_tags(vec![f.holiday]),
    );
    let source = Source::read(&f.book, november()).expect("read");
    let filter = only(&[f.holiday]);
    assert!(
        spending_by_category(&source, november(), &filter, Level::Each)
            .unwrap()
            .is_empty()
    );
    let months = income_expense_by_month(&source, november(), &filter).unwrap();
    assert!(
        months
            .iter()
            .all(|result| result.income.is_zero() && result.expenses.is_zero()),
        "{months:?}"
    );
}

#[test]
fn money_in_counts_as_income_refunds_lower_spending_and_currencies_stay_apart() {
    let mut f = fixture();
    // Salary of 2,000.00, tagged Tax.
    add(
        &mut f.book,
        entry(
            date(2026, 11, 1),
            vec![
                Split::new(f.checking, money(200_000)),
                Split::new(f.salary, money(-200_000)),
            ],
        )
        .with_tags(vec![f.tax]),
    );
    // 40.00 food, then 10.00 of it back, both tagged Tax.
    add(
        &mut f.book,
        entry(
            date(2026, 11, 2),
            vec![
                Split::new(f.checking, money(-4_000)),
                tagged(Split::new(f.food, money(4_000)), &[f.tax]),
            ],
        ),
    );
    add(
        &mut f.book,
        entry(
            date(2026, 11, 9),
            vec![
                Split::new(f.checking, money(1_000)),
                Split::new(f.food, money(-1_000)),
            ],
        )
        .with_tags(vec![f.tax]),
    );
    // 25.00 in dollars, not sorted yet, tagged Tax.
    add(
        &mut f.book,
        Transaction::new(
            TransactionId::generate(),
            date(2026, 11, 10),
            None,
            vec![
                Split::new(f.dollars, Money::from_minor_units(-2_500, USD)),
                tagged(
                    Split::uncategorized(Money::from_minor_units(2_500, USD)),
                    &[f.tax],
                ),
            ],
        )
        .expect("balanced"),
    );
    let source = Source::read(&f.book, november()).expect("read");
    let filter = only(&[f.tax]);
    let months = income_expense_by_month(&source, november(), &filter).unwrap();
    let totals: Vec<_> = months
        .iter()
        .map(|result| {
            (
                result.currency,
                result.income.minor_units(),
                result.expenses.minor_units(),
            )
        })
        .collect();
    assert_eq!(totals, [(EUR, 200_000, 3_000), (USD, 0, 2_500)]);
    let rows = spending(&f.book, &filter);
    assert_eq!(
        rows,
        [
            (EUR, vec![("Food".to_owned(), 3_000)], 3_000),
            (USD, vec![(String::new(), 2_500)], 2_500)
        ]
    );
}

#[test]
fn the_tag_filter_works_with_the_account_and_category_filters() {
    let mut f = fixture();
    for (account, part, amount) in [
        (f.checking, f.food, 1_000),
        (f.checking, f.gifts, 2_000),
        (f.savings, f.food, 4_000),
    ] {
        add(
            &mut f.book,
            entry(
                date(2026, 11, 3),
                vec![
                    Split::new(account, money(-amount)),
                    Split::new(part, money(amount)),
                ],
            )
            .with_tags(vec![f.holiday]),
        );
    }
    let filter = Filter {
        accounts: Some(vec![f.checking]),
        categories: Some(vec![f.food]),
        tags: Some(vec![f.holiday]),
    };
    assert_eq!(
        spending(&f.book, &filter),
        [(EUR, vec![("Food".to_owned(), 1_000)], 1_000)]
    );
}

/// A dashboard with three components of the kinds that take tags, each
/// with its own tags: a pie of spending, a key figure of money out and a key
/// figure of what the accounts hold, which takes no tags.
fn dashboard(tags: &[String]) -> Document {
    let text = json!({
        "format": FORMAT, "version": VERSION, "kind": "dashboard", "name": "Overview",
        "components": [
            {"id": "pie", "type": "chart.pie", "dataset": "spending_by_category",
             "params": {"tags": tags},
             "frame": {"col": 0, "row": 0, "cols": 6, "rows": 4}},
            {"id": "out", "type": "kpi.card", "dataset": "income_expense_by_month",
             "params": {"measure": "expenses", "tags": tags},
             "frame": {"col": 6, "row": 0, "cols": 3, "rows": 2}},
            {"id": "held", "type": "kpi.card", "dataset": "account_balances",
             "params": {"tags": tags},
             "frame": {"col": 9, "row": 0, "cols": 3, "rows": 2}}
        ]
    })
    .to_string();
    Document::read(&text).expect("a valid dashboard")
}

#[test]
fn a_components_own_tags_narrow_its_figures_and_say_so() {
    let mut f = fixture();
    add(
        &mut f.book,
        entry(
            date(2026, 11, 3),
            vec![
                Split::new(f.checking, money(-9_000)),
                Split::new(f.food, money(6_000)),
                tagged(Split::new(f.gifts, money(3_000)), &[f.holiday]),
            ],
        ),
    );
    let gone = TagId::generate().to_string();
    let page = dashboard(&[f.holiday.to_string(), gone.clone()]);
    // A written document keeps the tags as they are.
    assert_eq!(Document::read(&page.to_json()).unwrap(), page);
    let source = Source::read_with_balances(&f.book, november()).expect("read");

    let pie = page.asked("pie", &Filters::default()).expect("a pie asks");
    assert_eq!(
        pie.tags.as_ref().map(|chosen| chosen.origin),
        Some(Origin::Own)
    );
    let applied = pie.apply(&source).expect("applied");
    assert_eq!(applied.filter.tags, Some(vec![f.holiday]));
    assert_eq!(applied.own, [Dimension::Tags]);
    assert_eq!(applied.filtered, [Dimension::Tags]);
    assert_eq!(
        applied.notes,
        [FilterNote::Gone {
            dimension: Dimension::Tags,
            id: gone.clone()
        }]
    );
    let [eur] = spending_by_category(&source, november(), &applied.filter, Level::Each)
        .unwrap()
        .try_into()
        .expect("one currency");
    assert_eq!(eur.total, money(3_000));

    let out = page.asked("out", &Filters::default()).unwrap();
    let applied = out.apply(&source).unwrap();
    let [eur] = income_expense_by_month(&source, november(), &applied.filter)
        .unwrap()
        .try_into()
        .expect("one currency");
    assert_eq!(eur.expenses, money(3_000));

    // A balance has no parts to tag: the tags are not applied, and the
    // note says so instead of a figure that only looks filtered.
    let held = page.asked("held", &Filters::default()).unwrap();
    let applied = held.apply(&source).unwrap();
    assert_eq!(applied.filter.tags, None);
    assert_eq!(applied.notes, [FilterNote::NotApplied(Dimension::Tags)]);

    // Every tag gone: the filter chooses nothing, and the pie draws nothing.
    let page = dashboard(std::slice::from_ref(&gone));
    let applied = page
        .asked("pie", &Filters::default())
        .unwrap()
        .apply(&source)
        .unwrap();
    assert!(applied.chooses_nothing());
    assert_eq!(applied.filter.tags, Some(Vec::new()));
}

#[test]
fn a_tag_list_naming_something_else_is_refused() {
    let f = fixture();
    let source = Source::read(&f.book, november()).expect("read");
    for id in [f.food, f.checking] {
        let page = dashboard(&[id.to_string()]);
        let asked = page.asked("pie", &Filters::default()).unwrap();
        assert!(
            matches!(
                asked.apply(&source),
                Err(ReportError::FilterInvalid {
                    reason: "not-a-tag",
                    dimension: "tags"
                })
            ),
            "{id}"
        );
    }
    let twice = [f.tax.to_string(), f.tax.to_string().to_uppercase()];
    let asked = dashboard(&twice).asked("pie", &Filters::default()).unwrap();
    assert!(matches!(
        asked.apply(&source),
        Err(ReportError::FilterInvalid {
            reason: "repeated",
            dimension: "tags"
        })
    ));
}

#[test]
fn the_kinds_that_count_parts_take_tags_and_the_others_do_not() {
    let has_tags = |kind: ComponentType| {
        kind.properties()
            .iter()
            .any(|property| property.key == "tags" && property.kind == Kind::Tags)
    };
    for kind in ComponentType::ALL {
        assert!(has_tags(kind), "{kind:?}");
    }
    for kind in [
        ComponentType::BudgetProgress,
        ComponentType::ChartCandlestick,
        ComponentType::Transactions,
        ComponentType::NetWorth,
        ComponentType::IncomeSources,
        ComponentType::CustomChart,
    ] {
        assert!(!has_tags(kind), "{kind:?}");
    }
    let takes = |id: &str| crate::dataset(id).map(|dataset| dataset.takes.tags);
    for id in [
        SPENDING_BY_CATEGORY,
        INCOME_EXPENSE_BY_MONTH,
        CATEGORY_OVER_TIME,
    ] {
        assert_eq!(takes(id), Some(true), "{id}");
    }
    for id in [
        ACCOUNT_BALANCES,
        NET_WORTH_OVER_TIME,
        CASH_FLOW_BY_MONTH,
        BUDGET_UTILIZATION,
    ] {
        assert_eq!(takes(id), Some(false), "{id}");
    }
    assert_eq!(Dimension::Tags.word(), "tags");
}
