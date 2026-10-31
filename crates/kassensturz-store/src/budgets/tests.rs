// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of budgets (D-344, D-345) on books in memory. All names and
//! amounts are made up (AGENTS.md §3.1 rule 2).

use kassensturz_core::{
    AccountId, AccountKind, Budget, BudgetAmount, BudgetEvery, BudgetId, Money,
};

use super::{MAX_AMOUNT, MAX_AMOUNTS, MAX_BUDGETS};
use crate::tests::{EUR, Sample, USD, account, category_in, date, eur, sample};
use crate::{Action, Book, HistoryRetention, StoreError};

use BudgetEvery::{Month, Quarter, Range, Year};

/// Compares the error `result` holds with `expected` (`StoreError` has no
/// `PartialEq`, as it can hold an SQLite error).
#[track_caller]
fn assert_refused<T: std::fmt::Debug>(result: Result<T, StoreError>, expected: StoreError) {
    assert_eq!(
        format!("{:?}", result.err()),
        format!("{:?}", Some(expected))
    );
}

#[track_caller]
fn assert_period<T: std::fmt::Debug>(result: Result<T, StoreError>, reason: &'static str) {
    assert_refused(result, StoreError::BudgetPeriod { reason });
}

fn row(from_day: kassensturz_core::Date, minor_units: i64, carry_over: bool) -> BudgetAmount {
    BudgetAmount {
        from_day,
        amount: eur(minor_units),
        carry_over,
    }
}

fn last_action(book: &Book) -> Action {
    book.history().expect("history")[0].action
}

/// A monthly Groceries budget of 400.00 from September 2026 on.
fn monthly(book: &mut Book, groceries: AccountId) -> BudgetId {
    book.add_budget(groceries, Month, date(2026, 9, 1), None, eur(40_000), true)
        .expect("added")
}

#[test]
fn a_budget_is_kept_with_its_period_and_amount() {
    let Sample {
        mut book,
        groceries,
        ..
    } = sample();
    assert_eq!(book.budgets().expect("read"), Vec::new());
    let id = monthly(&mut book, groceries);
    assert_eq!(last_action(&book), Action::AddBudget);
    let trip = account(&mut book, "Summer trip", AccountKind::Expense, EUR);
    let once = book
        .add_budget(
            trip,
            Range,
            date(2026, 7, 10),
            Some(date(2026, 7, 24)),
            eur(150_000),
            false,
        )
        .expect("added");
    let expected = Budget {
        id,
        category: groceries,
        every: Month,
        first_day: date(2026, 9, 1),
        last_day: None,
        amounts: vec![row(date(2026, 9, 1), 40_000, true)],
    };
    assert_eq!(book.budget(id).expect("read"), expected);
    // In the order they were made.
    let budgets = book.budgets().expect("read");
    assert_eq!(budgets.len(), 2);
    assert_eq!(budgets[0], expected);
    assert_eq!(budgets[1].id, once);
    assert_eq!(budgets[1].last_day, Some(date(2026, 7, 24)));
    // Nothing else in the book changed: the category is as it was.
    assert_eq!(book.account(groceries).expect("read").name, "Groceries");

    let unknown = BudgetId::generate();
    assert_refused(book.budget(unknown), StoreError::UnknownBudget(unknown));
    assert_refused(
        book.remove_budget(unknown),
        StoreError::UnknownBudget(unknown),
    );
    assert_refused(
        book.set_budget_last_day(unknown, None),
        StoreError::UnknownBudget(unknown),
    );
}

#[test]
fn only_a_spending_category_has_a_budget() {
    let Sample {
        mut book,
        checking,
        salary,
        ..
    } = sample();
    for not_spending in [checking, salary] {
        assert_refused(
            book.add_budget(not_spending, Month, date(2026, 9, 1), None, eur(100), false),
            StoreError::BudgetNotSpending(not_spending),
        );
    }
    let unknown = AccountId::generate();
    assert_refused(
        book.add_budget(unknown, Month, date(2026, 9, 1), None, eur(100), false),
        StoreError::UnknownAccount(unknown),
    );
    assert_eq!(book.budgets().expect("read"), Vec::new());
}

#[test]
fn the_days_must_fit_the_kind_of_period() {
    let Sample {
        mut book,
        groceries,
        ..
    } = sample();
    let mut add = |every, first, last, carry_over| {
        book.add_budget(groceries, every, first, last, eur(100), carry_over)
    };
    assert_period(
        add(Month, date(2026, 9, 2), None, false),
        "not-period-start",
    );
    assert_period(
        add(Quarter, date(2026, 11, 1), None, false),
        "not-period-start",
    );
    assert_period(add(Year, date(2026, 2, 1), None, false), "not-period-start");
    assert_period(
        add(Month, date(2026, 9, 1), Some(date(2026, 9, 29)), false),
        "not-period-end",
    );
    assert_period(
        add(Quarter, date(2026, 10, 1), Some(date(2026, 11, 30)), false),
        "not-period-end",
    );
    assert_period(
        add(Year, date(2026, 1, 1), Some(date(2025, 12, 31)), false),
        "ends-before-starts",
    );
    assert_period(
        add(Range, date(2026, 7, 10), Some(date(2026, 7, 9)), false),
        "ends-before-starts",
    );
    assert_period(add(Range, date(2026, 7, 10), None, false), "no-last-day");
    assert_period(
        add(Range, date(2026, 7, 10), Some(date(2026, 7, 24)), true),
        "carry-over-on-range",
    );
    // A range touches at most 1,200 calendar months: 100 years.
    assert_period(
        add(Range, date(2026, 1, 15), Some(date(2126, 1, 1)), false),
        "too-long",
    );
    add(Range, date(2026, 1, 15), Some(date(2125, 12, 31)), false).expect("100 years");
    assert_eq!(book.budgets().expect("read").len(), 1);
}

#[test]
fn the_amount_is_above_zero_in_the_categorys_currency() {
    let Sample {
        mut book,
        groceries,
        ..
    } = sample();
    let mut add =
        |amount: Money| book.add_budget(groceries, Month, date(2026, 9, 1), None, amount, false);
    let refused = |reason| StoreError::BudgetAmount { reason };
    assert_refused(add(eur(0)), refused("zero"));
    assert_refused(add(eur(-100)), refused("zero"));
    assert_refused(add(eur(MAX_AMOUNT + 1)), refused("too-large"));
    assert_refused(add(Money::from_minor_units(100, USD)), refused("currency"));
    // 1,200 periods of the largest amount still add up.
    assert!(MAX_AMOUNT.checked_mul(MAX_AMOUNTS as i64).is_some());
    add(eur(MAX_AMOUNT)).expect("the largest amount");
}

#[test]
fn a_category_has_one_budget_at_a_time() {
    let Sample {
        mut book,
        groceries,
        ..
    } = sample();
    let ongoing = monthly(&mut book, groceries);
    let overlap = StoreError::BudgetOverlap {
        category: groceries,
    };
    // Any day of the ongoing one, even years later.
    assert_refused(
        book.add_budget(groceries, Year, date(2030, 1, 1), None, eur(100), false),
        overlap,
    );
    // Days before it are free.
    let before = book
        .add_budget(
            groceries,
            Quarter,
            date(2026, 4, 1),
            Some(date(2026, 6, 30)),
            eur(100_000),
            false,
        )
        .expect("added");
    // A budget on a category inside it is a separate figure (D-344).
    let fruit = category_in(&mut book, "Fruit", AccountKind::Expense, Some(groceries));
    book.add_budget(fruit, Month, date(2026, 9, 1), None, eur(5_000), false)
        .expect("added");
    // Making the earlier one keep going would overlap the ongoing one.
    assert_refused(
        book.set_budget_last_day(before, None),
        StoreError::BudgetOverlap {
            category: groceries,
        },
    );
    // Once the ongoing one stops, a new one may follow it.
    book.set_budget_last_day(ongoing, Some(date(2026, 12, 31)))
        .expect("stopped");
    book.add_budget(groceries, Year, date(2027, 1, 1), None, eur(500_000), false)
        .expect("added");
    assert_eq!(book.budgets().expect("read").len(), 4);
}

#[test]
fn a_new_amount_applies_from_its_period_on_and_keeps_the_past() {
    let Sample {
        mut book,
        groceries,
        ..
    } = sample();
    let id = monthly(&mut book, groceries);
    book.edit_budget_amount(id, date(2026, 11, 1), eur(45_000), true)
        .expect("edited");
    assert_eq!(last_action(&book), Action::EditBudget);
    book.edit_budget_amount(id, date(2027, 3, 1), eur(50_000), false)
        .expect("edited");
    let planned = vec![
        row(date(2026, 9, 1), 40_000, true),
        row(date(2026, 11, 1), 45_000, true),
        row(date(2027, 3, 1), 50_000, false),
    ];
    assert_eq!(book.budget(id).expect("read").amounts, planned);
    let before = book.budgets().expect("read");

    // From January on: what was planned for March goes, the past stays.
    book.edit_budget_amount(id, date(2027, 1, 1), eur(42_000), true)
        .expect("edited");
    assert_eq!(
        book.budget(id).expect("read").amounts,
        [
            row(date(2026, 9, 1), 40_000, true),
            row(date(2026, 11, 1), 45_000, true),
            row(date(2027, 1, 1), 42_000, true),
        ]
    );
    let after = book.budgets().expect("read");
    book.undo().expect("undone");
    assert_eq!(book.budgets().expect("read"), before);
    book.redo().expect("redone");
    assert_eq!(book.budgets().expect("read"), after);

    // From the first period on corrects it from the start.
    book.edit_budget_amount(id, date(2026, 9, 1), eur(38_000), false)
        .expect("edited");
    assert_eq!(
        book.budget(id).expect("read").amounts,
        [row(date(2026, 9, 1), 38_000, false)]
    );

    // Only the start of one of its periods, never before it begins.
    assert_period(
        book.edit_budget_amount(id, date(2026, 11, 15), eur(1), false),
        "not-period-start",
    );
    assert_period(
        book.edit_budget_amount(id, date(2026, 8, 1), eur(1), false),
        "amount-before-start",
    );
    assert_refused(
        book.edit_budget_amount(id, date(2026, 10, 1), eur(0), false),
        StoreError::BudgetAmount { reason: "zero" },
    );
}

#[test]
fn an_edit_that_changes_nothing_records_nothing() {
    let Sample {
        mut book,
        groceries,
        ..
    } = sample();
    let id = monthly(&mut book, groceries);
    let history = book.history().expect("history").len();
    book.edit_budget_amount(id, date(2026, 9, 1), eur(40_000), true)
        .expect("edited");
    book.set_budget_last_day(id, None).expect("set");
    assert_eq!(book.history().expect("history").len(), history);
}

#[test]
fn a_range_has_one_amount_and_never_carries_over() {
    let Sample {
        mut book,
        groceries,
        ..
    } = sample();
    let trip = book
        .add_budget(
            groceries,
            Range,
            date(2026, 7, 10),
            Some(date(2026, 7, 24)),
            eur(20_000),
            false,
        )
        .expect("added");
    assert_period(
        book.edit_budget_amount(trip, date(2026, 7, 11), eur(1), false),
        "not-period-start",
    );
    assert_period(
        book.edit_budget_amount(trip, date(2026, 7, 10), eur(1), true),
        "carry-over-on-range",
    );
    assert_period(book.set_budget_last_day(trip, None), "no-last-day");
    book.edit_budget_amount(trip, date(2026, 7, 10), eur(25_000), false)
        .expect("edited");
    book.set_budget_last_day(trip, Some(date(2026, 7, 31)))
        .expect("longer");
    let budget = book.budget(trip).expect("read");
    assert_eq!(budget.amounts, [row(date(2026, 7, 10), 25_000, false)]);
    assert_eq!(budget.last_day, Some(date(2026, 7, 31)));
}

#[test]
fn stopping_a_budget_keeps_its_past_and_undo_brings_its_plans_back() {
    let Sample {
        mut book,
        groceries,
        ..
    } = sample();
    let id = monthly(&mut book, groceries);
    book.edit_budget_amount(id, date(2027, 1, 1), eur(45_000), true)
        .expect("edited");
    let before = book.budgets().expect("read");
    // Stop after November: what was planned from January goes with it.
    book.set_budget_last_day(id, Some(date(2026, 11, 30)))
        .expect("stopped");
    assert_eq!(last_action(&book), Action::EditBudget);
    let stopped = book.budget(id).expect("read");
    assert_eq!(stopped.last_day, Some(date(2026, 11, 30)));
    assert_eq!(stopped.amounts, [row(date(2026, 9, 1), 40_000, true)]);
    assert_period(
        book.edit_budget_amount(id, date(2026, 12, 1), eur(1), false),
        "amount-after-end",
    );
    assert_period(
        book.set_budget_last_day(id, Some(date(2026, 11, 29))),
        "not-period-end",
    );
    assert_period(
        book.set_budget_last_day(id, Some(date(2026, 8, 31))),
        "ends-before-starts",
    );
    book.undo().expect("undone");
    assert_eq!(book.budgets().expect("read"), before);
}

#[test]
fn deleting_a_budget_takes_its_amounts_and_undo_brings_them_back() {
    let Sample {
        mut book,
        groceries,
        ..
    } = sample();
    let id = monthly(&mut book, groceries);
    book.edit_budget_amount(id, date(2026, 12, 1), eur(45_000), false)
        .expect("edited");
    let before = book.budgets().expect("read");
    book.remove_budget(id).expect("removed");
    assert_eq!(last_action(&book), Action::RemoveBudget);
    assert_eq!(book.budgets().expect("read"), Vec::new());
    let left: i64 = book
        .conn
        .query_row("SELECT count(*) FROM budget_amounts", [], |row| row.get(0))
        .expect("counted");
    assert_eq!(left, 0);
    book.undo().expect("undone");
    assert_eq!(book.budgets().expect("read"), before);
    book.redo().expect("redone");
    assert_eq!(book.budgets().expect("read"), Vec::new());
}

#[test]
fn a_category_with_a_budget_is_not_removed() {
    let Sample {
        mut book,
        groceries,
        ..
    } = sample();
    let id = monthly(&mut book, groceries);
    assert_eq!(book.category_use(groceries).expect("read"), Some("budgets"));
    assert_refused(
        book.remove_category(groceries),
        StoreError::CategoryInUse {
            category: groceries,
            used_by: "budgets",
        },
    );
    // Once the budget is deleted, nothing uses it.
    book.remove_budget(id).expect("removed");
    assert_eq!(book.category_use(groceries).expect("read"), None);
    book.remove_category(groceries).expect("removed");
}

#[test]
fn moving_or_renaming_the_category_keeps_the_budget() {
    let Sample {
        mut book,
        groceries,
        ..
    } = sample();
    let id = monthly(&mut book, groceries);
    let before = book.budget(id).expect("read");
    let food = account(&mut book, "Food", AccountKind::Expense, EUR);
    book.move_category(groceries, Some(food)).expect("moved");
    book.rename_account(groceries, "Groceries and drinks")
        .expect("renamed");
    assert_eq!(book.budget(id).expect("read"), before);
}

#[test]
fn a_book_keeps_at_most_1000_budgets_of_at_most_1200_amounts() {
    // One budget per month of one category, so none overlaps.
    let category = AccountId::generate();
    let budget = |index: usize| {
        let month = i16::try_from(index).expect("small");
        let first = date(
            2000 + month / 12,
            i8::try_from(month % 12 + 1).expect("month"),
            1,
        );
        Budget {
            id: BudgetId::generate(),
            category,
            every: Month,
            first_day: first,
            last_day: Some(first.last_of_month()),
            amounts: vec![row(first, 100, false)],
        }
    };
    let full: Vec<Budget> = (0..MAX_BUDGETS).map(budget).collect();
    let mut book = Book::create_in_memory().expect("book created");
    book.fill_new(HistoryRetention::Forever, |book| {
        book.use_currency(EUR)?;
        add_with_id(book, category, "Groceries")?;
        book.restore_budgets(&full)
    })
    .expect("filled");
    assert_eq!(book.budgets().expect("read").len(), MAX_BUDGETS);
    assert_refused(
        book.add_budget(category, Month, date(2100, 1, 1), None, eur(100), false),
        StoreError::TooManyBudgets {
            limit: "budgets",
            max: MAX_BUDGETS,
        },
    );

    let many = Budget {
        amounts: (0..=MAX_AMOUNTS)
            .map(|index| {
                let first = budget(index).first_day;
                row(first, 100, false)
            })
            .collect(),
        last_day: None,
        ..budget(0)
    };
    let mut empty = Book::create_in_memory().expect("book created");
    let filled = empty.fill_new(HistoryRetention::Forever, |book| {
        book.use_currency(EUR)?;
        add_with_id(book, category, "Groceries")?;
        book.restore_budgets(std::slice::from_ref(&many))
    });
    assert_refused(
        filled,
        StoreError::TooManyBudgets {
            limit: "amounts",
            max: MAX_AMOUNTS,
        },
    );
    assert!(empty.is_empty().expect("read"));
}

#[test]
fn budgets_come_back_with_their_ids_when_a_book_is_filled() {
    let groceries = AccountId::generate();
    let budgets = vec![Budget {
        id: BudgetId::generate(),
        category: groceries,
        every: Quarter,
        first_day: date(2026, 7, 1),
        last_day: Some(date(2027, 6, 30)),
        amounts: vec![
            row(date(2026, 7, 1), 120_000, true),
            row(date(2027, 1, 1), 130_000, false),
        ],
    }];
    let fill = |budgets: &[Budget]| {
        let mut book = Book::create_in_memory().expect("book created");
        let filled = book.fill_new(HistoryRetention::Forever, |book| {
            book.use_currency(EUR)?;
            add_with_id(book, groceries, "Groceries")?;
            book.restore_budgets(budgets)
        });
        (book, filled)
    };
    let (book, filled) = fill(&budgets);
    filled.expect("filled");
    assert_eq!(book.budgets().expect("read"), budgets);
    assert_eq!(book.history().expect("history"), Vec::new());
    assert!(!book.is_empty().expect("read"));

    // The amounts of a file are oldest first, each period once, and the
    // first starts with the budget; every one is checked. A refused file
    // leaves the book empty.
    let with_amounts = |amounts: Vec<BudgetAmount>| {
        vec![Budget {
            amounts,
            ..budgets[0].clone()
        }]
    };
    for (amounts, reason) in [
        (
            vec![
                row(date(2026, 7, 1), 1, false),
                row(date(2026, 7, 1), 2, false),
            ],
            "amount-twice",
        ),
        (
            vec![
                row(date(2026, 7, 1), 1, false),
                row(date(2027, 4, 1), 2, false),
                row(date(2027, 1, 1), 3, false),
            ],
            "amount-twice",
        ),
        (
            vec![row(date(2026, 10, 1), 1, false)],
            "first-amount-missing",
        ),
        (Vec::new(), "first-amount-missing"),
        (vec![row(date(2026, 4, 1), 1, false)], "amount-before-start"),
        (
            vec![
                row(date(2026, 7, 1), 1, false),
                row(date(2026, 8, 1), 2, false),
            ],
            "not-period-start",
        ),
        (
            vec![
                row(date(2026, 7, 1), 1, false),
                row(date(2027, 7, 1), 2, false),
            ],
            "amount-after-end",
        ),
    ] {
        let (book, filled) = fill(&with_amounts(amounts));
        assert_period(filled, reason);
        assert!(book.is_empty().expect("read"));
    }
}

fn add_with_id(book: &mut Book, id: AccountId, name: &str) -> Result<(), StoreError> {
    book.add_account(&kassensturz_core::Account {
        id,
        name: name.into(),
        kind: AccountKind::Expense,
        currency: EUR,
        account_type: None,
        closed: false,
        parent: None,
        number: None,
        notes: None,
        institution: None,
    })
}
