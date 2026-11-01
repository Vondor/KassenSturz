// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the notices *Update all accounts* gives (M6d.7, D-449): on a
//! real book made with the store's own functions, and properties on exact
//! figures.

use kassensturz_core::{AccountKind, Split, Transaction};
use proptest::prelude::*;

use super::*;
use crate::tests::{EUR, USD, account, date, money};

struct Book2 {
    book: Book,
    checking: AccountId,
    food: AccountId,
    groceries: AccountId,
    salary: AccountId,
}

fn book() -> Book2 {
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    book.use_currency(USD).expect("currency used");
    let checking = account(&mut book, "Checking account", AccountKind::Asset, EUR, None);
    let food = account(&mut book, "Food", AccountKind::Expense, EUR, None);
    let groceries = account(
        &mut book,
        "Groceries",
        AccountKind::Expense,
        EUR,
        Some(food),
    );
    let salary = account(&mut book, "Salary", AccountKind::Income, EUR, None);
    Book2 {
        book,
        checking,
        food,
        groceries,
        salary,
    }
}

/// Money out of the checking account into `category` (`None`: not sorted).
fn spend(
    book: &mut Book2,
    day: Date,
    category: Option<AccountId>,
    minor_units: i64,
) -> TransactionId {
    let to = match category {
        Some(category) => Split::new(category, money(minor_units)),
        None => Split::uncategorized(money(minor_units)),
    };
    let splits = vec![Split::new(book.checking, money(-minor_units)), to];
    let transaction =
        Transaction::new(TransactionId::generate(), day, None, splits).expect("balanced");
    book.book.add_transaction(&transaction).expect("added");
    transaction.id()
}

/// Five earlier groceries of 45.00 in the month before 2026-10-20.
fn usual_groceries(book: &mut Book2) {
    let groceries = book.groceries;
    for day in [1, 5, 9, 13, 17] {
        spend(book, date(2026, 9, day), Some(groceries), 4500);
    }
}

#[test]
fn an_amount_three_times_the_usual_gets_a_notice_and_a_usual_one_none() {
    let mut book = book();
    usual_groceries(&mut book);
    let groceries = book.groceries;
    let big = spend(&mut book, date(2026, 10, 20), Some(groceries), 41200);
    let normal = spend(&mut book, date(2026, 10, 20), Some(groceries), 5000);
    let found = notices(&book.book, &[big, normal]).expect("notices");
    assert_eq!(
        found,
        vec![Notice {
            transaction: big,
            reason: Reason::Unusual {
                category: groceries,
                amount: money(41200),
                usual: money(4500),
            },
        }]
    );
}

#[test]
fn the_limits_are_three_times_and_a_third_exactly() {
    let mut book = book();
    usual_groceries(&mut book);
    let groceries = book.groceries;
    let ids: Vec<_> = [13500, 13499, 1500, 1501]
        .into_iter()
        .map(|amount| spend(&mut book, date(2026, 10, 20), Some(groceries), amount))
        .collect();
    let found: Vec<_> = notices(&book.book, &ids)
        .expect("notices")
        .into_iter()
        .map(|notice| notice.transaction)
        .collect();
    assert_eq!(found, vec![ids[0], ids[2]]);
}

#[test]
fn with_fewer_than_five_earlier_ones_nothing_is_unusual() {
    let mut book = book();
    let groceries = book.groceries;
    for day in [1, 5, 9, 13] {
        spend(&mut book, date(2026, 9, day), Some(groceries), 4500);
    }
    let big = spend(&mut book, date(2026, 10, 20), Some(groceries), 99900);
    assert_eq!(notices(&book.book, &[big]).expect("notices"), vec![]);
}

#[test]
fn a_transaction_no_rule_sorted_gets_a_notice() {
    let mut book = book();
    let unsorted = spend(&mut book, date(2026, 10, 20), None, 1234);
    assert_eq!(
        notices(&book.book, &[unsorted]).expect("notices"),
        vec![Notice {
            transaction: unsorted,
            reason: Reason::Unsorted,
        }]
    );
}

#[test]
fn subcategories_count_as_their_category_but_not_the_other_way() {
    let mut book = book();
    usual_groceries(&mut book);
    let food = book.food;
    // Food's earlier transactions are the five groceries.
    let in_food = spend(&mut book, date(2026, 10, 20), Some(food), 41200);
    assert_eq!(notices(&book.book, &[in_food]).expect("notices").len(), 1);
    // Groceries doesn't count what is booked to Food itself.
    let mut book = self::book();
    let (food, groceries) = (book.food, book.groceries);
    for day in [1, 5, 9, 13, 17] {
        spend(&mut book, date(2026, 9, day), Some(food), 4500);
    }
    let in_groceries = spend(&mut book, date(2026, 10, 20), Some(groceries), 41200);
    assert_eq!(
        notices(&book.book, &[in_groceries]).expect("notices"),
        vec![]
    );
}

#[test]
fn only_the_year_before_the_same_direction_and_what_the_book_already_had_count() {
    let mut book = book();
    let groceries = book.groceries;
    // Too old: more than 365 days before.
    for day in [1, 5, 9, 13, 17] {
        spend(&mut book, date(2025, 9, day), Some(groceries), 4500);
    }
    // Refunds: money that came back.
    for day in [1, 5, 9, 13, 17] {
        spend(&mut book, date(2026, 9, day), Some(groceries), -4500);
    }
    // Booked in the same update: not earlier.
    let batch: Vec<_> = [1, 2, 3, 4, 5]
        .into_iter()
        .map(|day| spend(&mut book, date(2026, 10, day), Some(groceries), 4500))
        .collect();
    let big = spend(&mut book, date(2026, 10, 20), Some(groceries), 41200);
    let mut new = batch.clone();
    new.push(big);
    assert_eq!(notices(&book.book, &new).expect("notices"), vec![]);
    // Without the batch among the new ones, the five are earlier ones.
    assert_eq!(notices(&book.book, &[big]).expect("notices").len(), 1);
}

#[test]
fn money_in_is_compared_with_money_in() {
    let mut book = book();
    let (checking, salary) = (book.checking, book.salary);
    let pay = |book: &mut Book2, day, minor_units| {
        let splits = vec![
            Split::new(checking, money(minor_units)),
            Split::new(salary, money(-minor_units)),
        ];
        let transaction =
            Transaction::new(TransactionId::generate(), day, None, splits).expect("balanced");
        book.book.add_transaction(&transaction).expect("added");
        transaction.id()
    };
    for month in 4..=8 {
        pay(&mut book, date(2026, month, 28), 300_000);
    }
    let small = pay(&mut book, date(2026, 9, 28), 50_000);
    let found = notices(&book.book, &[small]).expect("notices");
    assert_eq!(
        found,
        vec![Notice {
            transaction: small,
            reason: Reason::Unusual {
                category: salary,
                amount: money(50_000),
                usual: money(300_000),
            },
        }]
    );
}

#[test]
fn the_usual_amount_is_the_lower_middle_one() {
    assert_eq!(usual_amount(&[1, 2, 3, 4]), None);
    assert_eq!(usual_amount(&[5, 1, 4, 2, 3]), Some(3));
    assert_eq!(usual_amount(&[6, 1, 5, 2, 4, 3]), Some(3));
    assert!(is_unusual(i64::MAX, i64::MAX / 3), "no overflow");
    assert!(!is_unusual(i64::MAX, i64::MAX / 2), "no overflow");
    assert!(is_unusual(0, 0), "zero is three times zero");
}

proptest! {
    #[test]
    fn the_usual_amount_does_not_depend_on_the_order(
        mut sizes in proptest::collection::vec(0..1_000_000i64, 0..40),
        seed in any::<u64>(),
    ) {
        let usual = usual_amount(&sizes);
        prop_assert_eq!(usual.is_none(), sizes.len() < MIN_EARLIER);
        // A rotation and a reversal are other orders of the same amounts.
        if !sizes.is_empty() {
            let at = usize::try_from(seed % sizes.len() as u64).expect("small");
            sizes.rotate_left(at);
        }
        prop_assert_eq!(usual_amount(&sizes), usual);
        sizes.reverse();
        prop_assert_eq!(usual_amount(&sizes), usual);
        if let Some(usual) = usual {
            // It is one of the amounts, with at least half of them at or above it.
            prop_assert!(sizes.contains(&usual));
            let at_or_above = sizes.iter().filter(|&&size| size >= usual).count();
            prop_assert!(2 * at_or_above >= sizes.len());
        }
    }

    #[test]
    fn the_limits_hold_for_any_usual_amount(usual in 1..1_000_000_000_000i64) {
        prop_assert!(is_unusual(FACTOR * usual, usual));
        prop_assert!(!is_unusual(FACTOR * usual - 1, usual));
        prop_assert!(!is_unusual(usual, usual));
        prop_assert!(is_unusual(usual / FACTOR, usual));
        prop_assert!(!is_unusual(usual / FACTOR + 1, usual) || usual < FACTOR);
    }
}
