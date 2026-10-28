// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The five-year Space Colony book (D-270), as the KMyMoney importer, the
//! pipeline and the store really fill it, read by the AI's own functions
//! (AGENTS.md §9.4: tests where steps meet).
//!
//! The book is made by `tools/space-colony/generate.py`. The balances here
//! were added up by that script from its own lists, apart from the store, so
//! they hold the import and the store to the generator's figures. The
//! answers of the local-model questionnaire (Q1 to Q9) are pinned, so a
//! change to the generator that moves one of them fails here and must be
//! noted in the questionnaire.

#![cfg(feature = "ai")]

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use kassensturz_ai_tools::{ToolError, call, propose as propose_with};
use kassensturz_core::ai_switch::Permissions;
use kassensturz_core::{Date, Report};
use kassensturz_import::{Imported, KmySqlite, Limits, import};
use kassensturz_pipeline::{BookImportReport, fill_book_from};
use kassensturz_store::Book;
use serde_json::{Value, json};

const TODAY: Date = Date::constant(2026, 10, 28);

fn load(relative: &str) -> (Book, BookImportReport) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../kassensturz-import/tests")
        .join(relative);
    let bytes = fs::read(&path).expect("the Space Colony book is in the repository");
    let Imported::Book(imported) =
        import(&KmySqlite, &bytes, &Limits::default()).expect("the file reads")
    else {
        panic!("a book");
    };
    let mut book = Book::create_in_memory().expect("a book in memory");
    let report = fill_book_from(&mut book, &imported, &HashMap::new()).expect("filled");
    (book, report)
}

fn five_years() -> (Book, BookImportReport) {
    load("books/space-colony-5y-v15.sqlite")
}

fn run(book: &Book, name: &str, arguments: Value) -> Value {
    call(book, name, &arguments, TODAY)
        .unwrap_or_else(|error| panic!("{name}: {error}"))
        .value
}

fn amounts(rows: &Value, key: &str) -> Vec<(String, String)> {
    rows.as_array()
        .expect("a list")
        .iter()
        .map(|row| {
            (
                row.get(key)
                    .and_then(Value::as_str)
                    .unwrap_or("(none)")
                    .to_owned(),
                row["amount"].as_str().expect("an amount").to_owned(),
            )
        })
        .collect()
}

fn category_id(book: &Book, name: &str) -> String {
    let found = run(book, "list_categories", json!({}));
    found["categories"]
        .as_array()
        .expect("categories")
        .iter()
        .find(|category| category["name"] == name)
        .unwrap_or_else(|| panic!("no category {name}"))["id"]
        .as_str()
        .expect("an ID")
        .to_owned()
}

fn last_day(year: i32, month: u32) -> u32 {
    match month {
        2 if year % 4 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn row_where(book: &Book, arguments: Value, wanted: impl Fn(&Value) -> bool) -> Value {
    run(book, "search_transactions", arguments)["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|row| wanted(row))
        .expect("a matching entry")
        .clone()
}

fn refusal(book: &Book, transaction: &Value, category: &str) -> String {
    let result = propose_with(
        book,
        &json!({"categorize": [{
            "transaction_id": transaction["transaction_id"],
            "category_id": category,
        }]}),
        Permissions::all(),
    );
    match result {
        Ok(_) => "prepared".to_owned(),
        Err(error) => ToolError::report(&error).code.to_string(),
    }
}

#[test]
fn the_file_imports_whole_and_balanced() {
    let (book, report) = five_years();
    assert_eq!(report.transactions, 3460);
    assert_eq!(
        report.completed, 0,
        "every entry balances as the file has it"
    );
    assert!(report.converted.is_empty() && report.renamed.is_empty());
    assert_eq!(report.categories, 70);
    assert_eq!(report.rates, 60, "one dollar rate a month");
    assert_eq!(report.rules, 33);
    assert_eq!(
        report.uncategorized, 16,
        "the newest lines are bank entries nobody sorted yet"
    );
    assert_eq!(
        book.booking_date_range().expect("dates"),
        Some((Date::constant(2021, 11, 1), Date::constant(2026, 10, 28)))
    );
}

#[test]
fn every_month_of_the_five_years_has_entries() {
    let (book, _) = five_years();
    let mut months = 0;
    for year in 2021..=2026 {
        for month in 1..=12 {
            if (year, month) < (2021, 11) || (year, month) > (2026, 10) {
                continue;
            }
            let found = run(
                &book,
                "search_transactions",
                json!({
                    "from": format!("{year}-{month:02}-01"),
                    "to": format!("{year}-{month:02}-{}", last_day(year, month)),
                    "limit": 1,
                }),
            );
            let matches = found["matches"].as_u64().expect("a count");
            assert!(
                matches >= 20,
                "{year}-{month:02} has only {matches} entries"
            );
            months += 1;
        }
    }
    assert_eq!(months, 60);
}

#[test]
fn the_balances_are_the_generators() {
    let (book, _) = five_years();
    let found = run(&book, "list_accounts", json!({"include_closed": true}));
    let balances: Vec<(String, String, String)> = found["accounts"]
        .as_array()
        .expect("accounts")
        .iter()
        .map(|row| {
            (
                row["name"].as_str().unwrap().to_owned(),
                row["balance"].as_str().unwrap().to_owned(),
                row["currency"].as_str().unwrap().to_owned(),
            )
        })
        .filter(|(_, balance, _)| balance != "0.00")
        .collect();
    let expected = [
        ("Dome Petty Cash", "597.66", "EUR"),
        ("Earth Trip Wallet", "915.90", "USD"),
        ("Everyday Checking", "7432.04", "EUR"),
        ("Mars Rainy-Day Savings", "19493.62", "EUR"),
        ("Shipyard Loan", "-4200.00", "EUR"),
        ("Shuttle Travel Fund", "1210.49", "EUR"),
    ];
    let expected: Vec<_> = expected
        .iter()
        .map(|(a, b, c)| ((*a).to_owned(), (*b).to_owned(), (*c).to_owned()))
        .collect();
    assert_eq!(balances, expected);
    // The dollar account is its own currency, never added to the euros.
    assert_eq!(
        found["together"],
        json!([
            {"currency": "EUR", "together": "24533.81"},
            {"currency": "USD", "together": "915.90"},
        ])
    );
}

#[test]
fn the_questionnaire_answers_of_this_book() {
    let (book, _) = five_years();

    // Q1: all of 2026. The date range up to today and the whole year give
    // the same answer, because the book ends today.
    let year = run(
        &book,
        "category_totals",
        json!({"from": "2026-01-01", "to": "2026-12-31"}),
    );
    let to_today = run(
        &book,
        "category_totals",
        json!({"from": "2026-01-01", "to": "2026-10-28"}),
    );
    assert_eq!(year["currencies"], to_today["currencies"]);
    let euro = &year["currencies"][0];
    assert_eq!(euro["currency"], "EUR");
    assert_eq!(euro["money_out"], "44878.94");
    assert_eq!(euro["money_in"], "49795.98");
    assert_eq!(
        amounts(&euro["spending"], "category")[..5],
        [
            ("Taxes and Fees › Colony Income Tax", "10230.00"),
            ("Habitat › Rent", "7350.00"),
            ("Travel › Earth-Mars Flights", "2779.43"),
            ("Food › Hydroponic Greens", "2667.07"),
            ("Taxes and Fees › Colony Pension Fund", "2325.00"),
        ]
        .map(|(category, amount)| (category.to_owned(), amount.to_owned()))
    );
    // The bank lines nobody sorted yet are one amount without a category.
    assert!(
        amounts(&euro["spending"], "category")
            .contains(&("(none)".to_owned(), "532.90".to_owned()))
    );
    assert_eq!(year["currencies"][1]["currency"], "USD");
    assert_eq!(year["currencies"][1]["money_out"], "2433.53");

    // Q9: Food and what is under it, as the tool lists them.
    let food: Vec<_> = amounts(&euro["spending"], "category")
        .into_iter()
        .filter(|(category, _)| category.starts_with("Food"))
        .collect();
    assert_eq!(
        food,
        [
            ("Food › Hydroponic Greens", "2667.07"),
            ("Food", "1734.03"),
            ("Food › Algae Bars and Nutrient Paste", "672.99"),
        ]
        .map(|(category, amount)| (category.to_owned(), amount.to_owned()))
    );

    // Q2: the ice deliveries come once a month for five years.
    let ice = run(
        &book,
        "search_transactions",
        json!({"text": "Northern Ice"}),
    );
    assert_eq!(
        (ice["matches"].as_u64(), ice["rows_left_out"].as_u64()),
        (Some(60), Some(40))
    );

    // Q6: the ten biggest expenses of 2026, as the tool sorts them, every
    // amount different from the next so the order cannot be argued.
    let biggest = run(
        &book,
        "search_transactions",
        json!({
            "from": "2026-01-01", "to": "2026-12-31", "sort": "largest_out",
            "skip_transfers": true, "limit": 11,
        }),
    );
    let rows = biggest["rows"].as_array().expect("rows");
    let sums: Vec<&str> = rows
        .iter()
        .map(|row| row["amount"].as_str().unwrap())
        .collect();
    assert_eq!(
        sums[..10],
        [
            "-2779.43", "-1240.00", "-1109.20", "-988.75", "-833.85", "-831.08", "-830.02",
            "-827.95", "-825.01", "-822.18",
        ]
    );
    assert_eq!(
        rows[10]["amount"], "-821.04",
        "the eleventh is lower, not equal"
    );
    assert_eq!(biggest["matches"], 528);
}

#[test]
fn the_categories_and_names_the_questionnaire_uses_are_there() {
    let (book, _) = five_years();
    let (old, _) = load("golden/kmy-sqlite/space-colony-v15.sqlite");
    // Q4 asks for the list of categories: the same list as in the older book.
    let names = |book: &Book| -> Vec<(String, String)> {
        run(book, "list_categories", json!({}))["categories"]
            .as_array()
            .unwrap()
            .iter()
            .map(|category| {
                (
                    category["name"].as_str().unwrap().to_owned(),
                    category["kind"].as_str().unwrap().to_owned(),
                )
            })
            .collect()
    };
    assert_eq!(names(&book), names(&old));
    assert!(
        !names(&book).iter().any(|(name, _)| name.ends_with("Pets")),
        "Q5 adds it"
    );
    // Q7 needs a Kepler Sports Club without a Rule, and Q8 a Leisure
    // category to move into; the Nebula Streaming Rule exists.
    let rules = run(&book, "list_rules", json!({}));
    let rule_names: Vec<&str> = rules["rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|rule| rule["name"].as_str().unwrap())
        .collect();
    assert_eq!(rule_names.len(), 33);
    assert!(!rule_names.iter().any(|name| name.contains("Kepler")));
    assert!(rule_names.contains(&"Nebula Streaming"));
    let kepler = run(
        &book,
        "search_transactions",
        json!({"text": "Kepler Sports Club"}),
    );
    assert!(kepler["matches"].as_u64().unwrap() > 50);
    category_id(&book, "Leisure › Zero-G Sports");
    category_id(&book, "Leisure");
}

#[test]
fn what_the_ai_must_leave_alone_is_in_the_book() {
    let (book, _) = five_years();
    let health = category_id(&book, "Health");

    // A pay slip is split over five parts.
    let pay = row_where(
        &book,
        json!({"text": "Pay October 2026", "limit": 5}),
        |row| row["payee"] == "Hydroponics Cooperative",
    );
    assert_eq!(pay["categories"].as_array().map(Vec::len), Some(4));
    assert_eq!(refusal(&book, &pay, &health), "ai-tools.not-categorizable");

    // A move between the user's own accounts.
    let loan = row_where(&book, json!({"text": "Loan repayment", "limit": 5}), |_| {
        true
    });
    assert_eq!(refusal(&book, &loan, &health), "ai-tools.not-categorizable");

    // Spending in dollars: the category is in euros.
    let abroad = row_where(
        &book,
        json!({"from": "2026-07-25", "to": "2026-08-12", "limit": 50}),
        |row| row["currency"] == "USD",
    );
    assert_eq!(refusal(&book, &abroad, &health), "ai-tools.other-currency");

    // A line from the bank nobody sorted yet can be given a category.
    let unsorted = row_where(
        &book,
        json!({"from": "2026-09-01", "to": "2026-10-28", "limit": 50}),
        |row| row["unsorted"] == true,
    );
    assert_eq!(unsorted["account"], "Everyday Checking");
    assert_eq!(refusal(&book, &unsorted, &health), "prepared");
}
