// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `list_budgets` through the running server (M6-B198): the server reads
//! every call inside one read transaction, and the budgets' figures read
//! the book again in a read transaction of their own, which must not fail
//! inside the server's. The book holds the budgets of layouts 25 and 26: one
//! over several categories counted from one account (D-639 to D-642), a
//! salary-month one (D-651), one that carries over and a quarterly one.

#![cfg(all(feature = "ai", target_os = "linux"))]

mod common;

use common::{CHECKING, EUR, Folder, GROCERIES, Session, account};
use kassensturz_core::{AccountId, AccountKind, BudgetEvery, Date, Money};
use kassensturz_store::Book;
use serde_json::json;

const DINING: AccountId = AccountId::from_bytes([7; 16]);
const HOUSEHOLD: AccountId = AccountId::from_bytes([8; 16]);
const FUEL: AccountId = AccountId::from_bytes([9; 16]);
const CLOTHES: AccountId = AccountId::from_bytes([10; 16]);

fn eur(cents: i64) -> Money {
    Money::from_minor_units(cents, EUR)
}

#[test]
fn list_budgets_answers_with_the_current_figures_of_every_budget() {
    let folder = Folder::new("list-budgets");
    folder.switch_ai(true);
    let path = folder.book();
    {
        let mut book = Book::open(&path, &folder.computer()).expect("opened");
        for (id, name) in [
            (DINING, "Dining out"),
            (HOUSEHOLD, "Household"),
            (FUEL, "Fuel"),
            (CLOTHES, "Clothes"),
        ] {
            book.add_account(&account(id, name, AccountKind::Expense))
                .expect("account");
        }
        let first = Date::constant(2026, 1, 1);
        book.add_budget(
            &[GROCERIES],
            &[],
            BudgetEvery::Month,
            first,
            None,
            eur(30_000),
            false,
        )
        .expect("monthly");
        // With a name the user gave it (D-664).
        book.add_named_budget(
            Some("Eating and home"),
            &[DINING, HOUSEHOLD],
            &[CHECKING],
            BudgetEvery::salary_month(25).expect("a salary day"),
            Date::constant(2025, 12, 25),
            None,
            eur(50_000),
            false,
        )
        .expect("salary month, two categories, one account");
        book.add_budget(
            &[FUEL],
            &[],
            BudgetEvery::Month,
            first,
            None,
            eur(10_000),
            true,
        )
        .expect("carries over");
        book.add_budget(
            &[CLOTHES],
            &[CHECKING],
            BudgetEvery::Quarter,
            first,
            None,
            eur(20_000),
            false,
        )
        .expect("quarterly");
    }
    let mut session = Session::start(folder.path(), &path);
    session.initialize("2025-06-18");

    let (answer, is_error) = session.call("list_budgets", json!({}));
    assert!(!is_error, "{answer}");
    let budgets = answer["budgets"].as_array().expect("budgets");
    assert_eq!(budgets.len(), 4, "{answer}");
    for budget in budgets {
        assert!(budget["current_period"]["spent"].is_string(), "{budget}");
    }
    assert_eq!(budgets[1]["salary_day"], json!(25));
    // A budget's name comes only when the user gave it one (D-664).
    assert_eq!(budgets[1]["name"], json!("Eating and home"), "{answer}");
    assert!(budgets[0].get("name").is_none(), "{answer}");
    assert_eq!(
        budgets[1]["account_ids"],
        json!([CHECKING.to_string()]),
        "{answer}"
    );
    assert_eq!(budgets[1]["category_ids"].as_array().map(Vec::len), Some(2));
    session.finish();
}
