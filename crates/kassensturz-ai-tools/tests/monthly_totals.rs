// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `monthly_totals` (US7, #852, D-772, T-65): one call for "each month".
//! Where steps meet: every month is held to what `category_totals` says for
//! that month alone, on a book filled through the store's own operations.

#![cfg(feature = "ai")]

use kassensturz_ai_tools::{ToolError, call, call_allowed};
use kassensturz_core::ai_switch::{Permission, Permissions};
use kassensturz_core::{
    Account, AccountId, AccountKind, CurrencyCode, Date, Money, Report as _, Split, Transaction,
    TransactionId,
};
use kassensturz_store::Book;
use serde_json::{Value, json};

const TODAY: Date = Date::constant(2026, 12, 20);

fn money(minor_units: i64, code: &str) -> Money {
    Money::from_minor_units(minor_units, CurrencyCode::new(code).expect("a currency"))
}

fn account(name: &str, kind: AccountKind, code: &str) -> Account {
    Account {
        id: AccountId::generate(),
        name: name.to_owned(),
        kind,
        currency: CurrencyCode::new(code).expect("a currency"),
        account_type: None,
        parent: None,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    }
}

struct Fixture {
    book: Book,
    checking: AccountId,
    groceries: AccountId,
}

/// Groceries and a salary in January and March 2026 and groceries in
/// November, a move between the user's own accounts in March, and a
/// spending in dollars in January.
fn fixture() -> Fixture {
    let mut book = Book::create_in_memory().expect("a book in memory");
    for code in ["EUR", "USD"] {
        book.use_currency(CurrencyCode::new(code).expect("a currency"))
            .expect("a currency");
    }
    let checking = account("Everyday Checking", AccountKind::Asset, "EUR");
    let savings = account("Rainy Day Savings", AccountKind::Asset, "EUR");
    let travel = account("Travel Wallet", AccountKind::Asset, "USD");
    let groceries = account("Groceries", AccountKind::Expense, "EUR");
    let gifts = account("Gifts", AccountKind::Expense, "USD");
    let salary = account("Salary", AccountKind::Income, "EUR");
    for a in [&checking, &savings, &travel, &groceries, &gifts, &salary] {
        book.add_account(a).expect("an account");
    }
    let mut add = |month: i8, day: i8, from: AccountId, to: AccountId, minor: i64, code: &str| {
        let transaction = Transaction::new(
            TransactionId::generate(),
            Date::constant(2026, month, day),
            Some("Synthetic".to_owned()),
            vec![
                Split::new(from, money(-minor, code)),
                Split::new(to, money(minor, code)),
            ],
        )
        .expect("a transaction");
        book.add_transaction(&transaction).expect("added");
    };
    add(1, 3, checking.id, groceries.id, 3_000, "EUR");
    add(1, 28, salary.id, checking.id, 200_000, "EUR");
    add(1, 30, travel.id, gifts.id, 1_500, "USD");
    add(3, 1, checking.id, groceries.id, 4_250, "EUR");
    add(3, 31, salary.id, checking.id, 200_000, "EUR");
    add(3, 15, checking.id, savings.id, 10_000, "EUR");
    add(11, 30, checking.id, groceries.id, 999, "EUR");
    Fixture {
        book,
        checking: checking.id,
        groceries: groceries.id,
    }
}

fn months(f: &Fixture, arguments: Value) -> Result<Value, ToolError> {
    call(&f.book, "monthly_totals", &arguments, TODAY).map(|output| output.value)
}

fn category_totals(f: &Fixture, from: &str, to: &str) -> Value {
    call(
        &f.book,
        "category_totals",
        &json!({ "from": from, "to": to }),
        TODAY,
    )
    .expect("category totals")
    .value
}

#[test]
fn a_year_in_one_call_gives_each_month_the_figures_of_asking_for_it_alone() {
    let f = fixture();
    let result = months(&f, json!({ "from": "2026-01-01", "to": "2026-12-31" })).expect("a year");
    let list = result["months"].as_array().expect("months");
    assert_eq!(list.len(), 12);
    let ends = [
        "31", "28", "31", "30", "31", "30", "31", "31", "30", "31", "30", "31",
    ];
    for (index, month) in list.iter().enumerate() {
        let number = index + 1;
        assert_eq!(month["month"], format!("2026-{number:02}"));
        assert!(month.get("partial").is_none(), "{month}");
        let alone = category_totals(
            &f,
            &format!("2026-{number:02}-01"),
            &format!("2026-{number:02}-{}", ends[index]),
        );
        let alone = alone["currencies"].as_array().expect("currencies");
        let got = month["currencies"].as_array().expect("currencies");
        assert_eq!(got.len(), alone.len(), "month {number}");
        for (got, alone) in got.iter().zip(alone) {
            for key in ["currency", "money_in", "money_out", "left"] {
                assert_eq!(got[key], alone[key], "month {number} {key}");
            }
            // Only the totals unless asked.
            assert!(got.get("spending").is_none() && got.get("income").is_none());
        }
        assert_eq!(month["transfers"], alone_transfers(&f, number, ends[index]));
    }
    // A month with nothing in it is listed with no currencies.
    assert_eq!(list[1]["currencies"], json!([]));
    // Real figures, read from the output: January in euros and dollars.
    let january = list[0]["currencies"].as_array().unwrap();
    assert_eq!(january.len(), 2);
    let eur = january.iter().find(|c| c["currency"] == "EUR").unwrap();
    assert_eq!(eur["money_in"], "2000.00");
    assert_eq!(eur["money_out"], "30.00");
    assert_eq!(eur["left"], "1970.00");
    // March's move between own accounts is not spending or income.
    let march = list[2]["currencies"].as_array().unwrap();
    assert_eq!(march[0]["money_out"], "42.50");
}

fn alone_transfers(f: &Fixture, number: usize, last: &str) -> Value {
    category_totals(
        f,
        &format!("2026-{number:02}-01"),
        &format!("2026-{number:02}-{last}"),
    )["transfers"]
        .clone()
}

#[test]
fn a_period_that_cuts_months_says_partial_with_its_real_days() {
    let f = fixture();
    let result = months(&f, json!({ "from": "2026-01-15", "to": "2026-03-10" })).expect("cut");
    let list = result["months"].as_array().unwrap();
    assert_eq!(list.len(), 3);
    assert_eq!(list[0]["partial"], true);
    assert_eq!(list[0]["from"], "2026-01-15");
    assert_eq!(list[0]["to"], "2026-01-31");
    assert!(list[1].get("partial").is_none());
    assert_eq!(list[2]["partial"], true);
    assert_eq!(list[2]["to"], "2026-03-10");
    // January from the 15th has the salary and the dollars, not the 3rd.
    let eur = list[0]["currencies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["currency"] == "EUR")
        .unwrap();
    assert_eq!(eur["money_out"], "0.00");
    // A single day inside one month is one partial month.
    let one = months(&f, json!({ "from": "2026-03-01", "to": "2026-03-01" })).unwrap();
    assert_eq!(one["months"].as_array().unwrap().len(), 1);
    assert_eq!(one["months"][0]["partial"], true);
}

#[test]
fn top_names_the_largest_categories_and_category_id_one_row() {
    let f = fixture();
    let top = months(
        &f,
        json!({ "from": "2026-03-01", "to": "2026-03-31", "top": 1 }),
    )
    .unwrap();
    let march = &top["months"][0]["currencies"][0];
    let alone = category_totals(&f, "2026-03-01", "2026-03-31");
    assert_eq!(march["spending"][0], alone["currencies"][0]["spending"][0]);
    assert!(march.get("spending_left_out").is_none());

    let one = months(
        &f,
        json!({ "from": "2026-01-01", "to": "2026-03-31", "category_id": f.groceries.to_string() }),
    )
    .unwrap();
    assert_eq!(one["category"], "Groceries");
    let list = one["months"].as_array().unwrap();
    let totals: Vec<&Value> = list
        .iter()
        .map(|m| &m["currencies"][0]["spending"][0]["total"])
        .collect();
    assert_eq!(totals[0], "30.00");
    assert_eq!(totals[2], "42.50");
    // February has nothing, and a row of another category is never there.
    assert_eq!(list[1]["currencies"], json!([]));
    assert!(
        list[0]["currencies"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["spending"].as_array().is_none_or(|rows| rows.len() == 1))
    );
}

#[test]
fn accounts_limit_each_month_as_they_limit_category_totals() {
    let f = fixture();
    let result = months(
        &f,
        json!({ "from": "2026-01-01", "to": "2026-01-31", "account_ids": [f.checking.to_string()] }),
    )
    .unwrap();
    assert_eq!(result["accounts"].as_array().unwrap().len(), 1);
    let currencies = result["months"][0]["currencies"].as_array().unwrap();
    // The dollar spending was not from this account.
    assert_eq!(currencies.len(), 1);
    assert_eq!(currencies[0]["currency"], "EUR");
}

#[test]
fn what_cannot_be_answered_is_refused_with_a_code() {
    let f = fixture();
    let bad = |arguments: Value| months(&f, arguments).unwrap_err().report().code;
    assert_eq!(
        bad(json!({ "from": "2026-02-01", "to": "2026-01-01" })),
        "ai-tools.bad-period"
    );
    // 25 months.
    assert_eq!(
        bad(json!({ "from": "2024-01-01", "to": "2026-01-31" })),
        "ai-tools.bad-argument"
    );
    // 24 months are fine.
    assert!(months(&f, json!({ "from": "2025-01-01", "to": "2026-12-31" })).is_ok());
    assert_eq!(
        bad(json!({ "from": "2026-01-01", "to": "2026-03-31", "top": 0 })),
        "ai-tools.bad-argument"
    );
    assert_eq!(
        bad(json!({ "from": "2026-01-01", "to": "2026-03-31", "top": 6 })),
        "ai-tools.bad-argument"
    );
    assert_eq!(
        bad(
            json!({ "from": "2026-01-01", "to": "2026-03-31", "top": 2, "category_id": f.groceries.to_string() })
        ),
        "ai-tools.bad-argument"
    );
    // An account is not a category, and an unknown ID is none.
    assert_eq!(
        bad(
            json!({ "from": "2026-01-01", "to": "2026-03-31", "category_id": f.checking.to_string() })
        ),
        "ai-tools.unknown-category"
    );
    assert_eq!(
        bad(
            json!({ "from": "2026-01-01", "to": "2026-03-31", "category_id": AccountId::generate().to_string() })
        ),
        "ai-tools.unknown-category"
    );
    assert_eq!(
        bad(json!({ "from": "2026-01-01" })),
        "ai-tools.bad-argument"
    );
    assert_eq!(
        bad(json!({ "from": "2026-01-01", "to": "2026-03-31", "limit": 3 })),
        "ai-tools.unknown-argument"
    );
}

#[test]
fn it_needs_the_same_readings_as_category_totals() {
    let f = fixture();
    let arguments = json!({ "from": "2026-01-01", "to": "2026-03-31" });
    for denied in [
        Permissions::NONE,
        Permissions::NONE.with(Permission::TransactionsRead, true),
        Permissions::NONE.with(Permission::CategoriesRead, true),
    ] {
        assert_eq!(
            call_allowed(&f.book, "monthly_totals", &arguments, TODAY, denied)
                .unwrap_err()
                .report()
                .code,
            "ai-tools.not-allowed"
        );
        assert_eq!(
            call_allowed(&f.book, "category_totals", &arguments, TODAY, denied)
                .unwrap_err()
                .report()
                .code,
            "ai-tools.not-allowed"
        );
    }
    assert!(
        call_allowed(
            &f.book,
            "monthly_totals",
            &arguments,
            TODAY,
            Permissions::all()
        )
        .is_ok()
    );
}

#[test]
fn the_figures_it_gives_are_known_to_the_number_check() {
    let f = fixture();
    let output = call(
        &f.book,
        "monthly_totals",
        &json!({ "from": "2026-01-01", "to": "2026-03-31" }),
        TODAY,
    )
    .unwrap();
    let unverified =
        kassensturz_ai_tools::check_numbers("In January 1970.00 was left.", &output.figures);
    assert!(unverified.is_empty(), "{unverified:?}");
    let invented =
        kassensturz_ai_tools::check_numbers("In January 1971.00 was left.", &output.figures);
    assert_eq!(invented.len(), 1);
}
