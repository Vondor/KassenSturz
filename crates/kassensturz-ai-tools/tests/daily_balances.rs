// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `daily_balances` (US7, #1036, D-794, T-23): one account's balance day by
//! day with its averages and MACD. Where steps meet: every day is held to
//! what `account_balance` says for that day, on a synthetic book filled
//! through the store's own operations.

#![cfg(feature = "ai")]

use kassensturz_ai_tools::{ToolError, call, call_allowed};
use kassensturz_core::ai_switch::{Permission, Permissions};
use kassensturz_core::{
    Account, AccountId, AccountKind, CurrencyCode, Date, Money, Report as _, Split, Transaction,
    TransactionId,
};
use kassensturz_store::Book;
use serde_json::{Value, json};

const TODAY: Date = Date::constant(2026, 11, 14);

fn euros(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, CurrencyCode::new("EUR").expect("a currency"))
}

fn account(book: &mut Book, name: &str, kind: AccountKind) -> AccountId {
    let account = Account {
        id: AccountId::generate(),
        name: name.to_owned(),
        kind,
        currency: CurrencyCode::new("EUR").expect("a currency"),
        account_type: None,
        parent: None,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    };
    book.add_account(&account).expect("an account");
    account.id
}

fn add(book: &mut Book, day: Date, from: AccountId, to: AccountId, minor: i64) {
    let transaction = Transaction::new(
        TransactionId::generate(),
        day,
        Some("Synthetic".to_owned()),
        vec![
            Split::new(from, euros(-minor)),
            Split::new(to, euros(minor)),
        ],
    )
    .expect("a transaction");
    book.add_transaction(&transaction).expect("added");
}

struct Fixture {
    book: Book,
    checking: AccountId,
    /// An account whose first entry is on 3 August 2026.
    late: AccountId,
    groceries: AccountId,
}

/// A starting balance of 1,000.00 on 1 December 2025, a salary of 2,500.00
/// on the 1st of each month, a card payment of 12.34 every day, and one
/// payment dated after today.
fn fixture() -> Fixture {
    let mut book = Book::create_in_memory().expect("a book in memory");
    book.use_currency(CurrencyCode::new("EUR").expect("a currency"))
        .expect("a currency");
    let checking = account(&mut book, "Everyday Checking", AccountKind::Asset);
    let late = account(&mut book, "New Savings", AccountKind::Asset);
    let opening = account(&mut book, "Starting balances", AccountKind::Equity);
    let salary = account(&mut book, "Salary", AccountKind::Income);
    let groceries = account(&mut book, "Groceries", AccountKind::Expense);
    add(
        &mut book,
        Date::constant(2025, 12, 1),
        opening,
        checking,
        100_000,
    );
    let mut day = Date::constant(2025, 12, 1);
    while day <= TODAY {
        if day.day() == 1 {
            add(&mut book, day, salary, checking, 250_000);
        }
        add(&mut book, day, checking, groceries, 1_234);
        day = day.tomorrow().expect("a day");
    }
    add(
        &mut book,
        Date::constant(2026, 11, 20),
        checking,
        groceries,
        50_000,
    );
    add(&mut book, Date::constant(2026, 8, 3), salary, late, 10_000);
    Fixture {
        book,
        checking,
        late,
        groceries,
    }
}

fn daily(f: &Fixture, arguments: Value) -> Result<Value, ToolError> {
    call(&f.book, "daily_balances", &arguments, TODAY).map(|output| output.value)
}

fn balance_on(f: &Fixture, account: AccountId, day: &str) -> Value {
    call(
        &f.book,
        "account_balance",
        &json!({ "account_id": account.to_string(), "date": day }),
        TODAY,
    )
    .expect("a balance")
    .value["balance"]
        .clone()
}

/// An amount's text in minor units: "-12.34" is -1234.
fn minor(text: &Value) -> i64 {
    text.as_str()
        .expect("an amount")
        .replace('.', "")
        .parse()
        .expect("a number")
}

#[test]
fn each_day_is_account_balance_for_that_day_and_the_last_is_today() {
    let f = fixture();
    let result = daily(
        &f,
        json!({ "account_id": f.checking.to_string(), "from": "2026-05-15", "to": "2026-11-30" }),
    )
    .expect("half a year");
    assert_eq!(
        result["columns"],
        json!([
            "day",
            "balance",
            "average_short",
            "average_long",
            "macd",
            "signal",
            "histogram"
        ])
    );
    assert_eq!(result["currency"], "EUR");
    assert_eq!(result["account"], "Everyday Checking");
    assert_eq!(
        result["lengths_in_days"],
        json!({ "average_short": 12, "average_long": 26, "signal": 9 })
    );
    assert!(result.get("starts_on_first_entry").is_none());
    let rows = result["rows"].as_array().expect("rows");
    // 15 May to today, 14 November: the payment on 20 November is not in.
    assert_eq!(rows.len(), 184);
    assert_eq!(rows[0][0], "2026-05-15");
    assert_eq!(rows.last().unwrap()[0], TODAY.to_string());
    assert_eq!(result["first_day"], "2026-05-15");
    assert_eq!(result["last_day"], TODAY.to_string());
    for row in rows {
        let day = row[0].as_str().unwrap();
        assert_eq!(row[1], balance_on(&f, f.checking, day), "{day}");
        // MACD is the short average less the long one, the histogram MACD
        // less the signal line.
        assert_eq!(minor(&row[4]), minor(&row[2]) - minor(&row[3]), "{day}");
        assert_eq!(minor(&row[6]), minor(&row[4]) - minor(&row[5]), "{day}");
    }
    // The last balance is everything booked by today.
    let today = rows.last().unwrap();
    assert_eq!(today[1], balance_on(&f, f.checking, &TODAY.to_string()));
}

#[test]
fn chosen_lengths_change_the_averages_not_the_balance() {
    let f = fixture();
    let usual = daily(
        &f,
        json!({ "account_id": f.checking.to_string(), "from": "2026-10-01", "to": "2026-10-31" }),
    )
    .unwrap();
    let faster = daily(
        &f,
        json!({ "account_id": f.checking.to_string(), "from": "2026-10-01", "to": "2026-10-31",
            "ema_fast": 5, "ema_slow": 20, "ema_signal": 3 }),
    )
    .unwrap();
    assert_eq!(
        faster["lengths_in_days"],
        json!({ "average_short": 5, "average_long": 20, "signal": 3 })
    );
    let (usual, faster) = (
        usual["rows"].as_array().unwrap(),
        faster["rows"].as_array().unwrap(),
    );
    assert_eq!(usual.len(), 31);
    assert_eq!(faster.len(), 31);
    for (a, b) in usual.iter().zip(faster) {
        assert_eq!(a[1], b[1]);
    }
    assert_ne!(usual[30][2], faster[30][2]);
}

#[test]
fn an_account_opened_inside_the_period_starts_on_its_first_entry() {
    let f = fixture();
    let result = daily(
        &f,
        json!({ "account_id": f.late.to_string(), "from": "2026-07-01", "to": "2026-09-30" }),
    )
    .unwrap();
    assert_eq!(result["starts_on_first_entry"], true);
    assert_eq!(result["first_day"], "2026-08-03");
    let rows = result["rows"].as_array().unwrap();
    // The averages start at the first balance, not at a zero before it.
    assert_eq!(rows[0][1], "100.00");
    assert_eq!(rows[0][2], "100.00");
    assert_eq!(rows[0][3], "100.00");
    assert_eq!(rows[0][4], "0.00");
}

#[test]
fn a_period_after_today_has_no_days() {
    let f = fixture();
    let result = daily(
        &f,
        json!({ "account_id": f.checking.to_string(), "from": "2026-12-01", "to": "2026-12-31" }),
    )
    .unwrap();
    assert_eq!(result["rows"], json!([]));
    assert_eq!(result["after_today"], true);
}

#[test]
fn what_cannot_be_answered_is_refused_with_a_code() {
    let f = fixture();
    let id = f.checking.to_string();
    let bad = |arguments: Value| daily(&f, arguments).unwrap_err().report().code;
    assert_eq!(
        bad(json!({ "account_id": id, "from": "2026-06-02", "to": "2026-06-01" })),
        "ai-tools.bad-period"
    );
    // 201 days up to today.
    assert_eq!(
        bad(json!({ "account_id": id, "from": "2026-04-28", "to": "2026-11-14" })),
        "ai-tools.bad-argument"
    );
    // 200 days are fine, and days after today don't count.
    assert!(
        daily(
            &f,
            json!({ "account_id": id, "from": "2026-04-29", "to": "2027-01-31" })
        )
        .is_ok()
    );
    for lengths in [
        json!({ "ema_fast": 1 }),
        json!({ "ema_fast": 201 }),
        json!({ "ema_fast": 26, "ema_slow": 26 }),
        json!({ "ema_fast": 30 }),
        json!({ "ema_signal": 0 }),
        json!({ "ema_fast": "12-days" }),
    ] {
        let mut arguments = json!({ "account_id": id, "from": "2026-10-01", "to": "2026-10-31" });
        for (key, value) in lengths.as_object().unwrap() {
            arguments[key] = value.clone();
        }
        assert_eq!(bad(arguments), "ai-tools.bad-argument", "{lengths}");
    }
    // A category is not an account, and an unknown ID is none.
    for account in [f.groceries, AccountId::generate()] {
        assert_eq!(
            bad(
                json!({ "account_id": account.to_string(), "from": "2026-10-01", "to": "2026-10-31" })
            ),
            "ai-tools.unknown-account"
        );
    }
    assert_eq!(
        bad(json!({ "account_id": id, "from": "2026-10-01" })),
        "ai-tools.bad-argument"
    );
    assert_eq!(
        bad(json!({ "from": "2026-10-01", "to": "2026-10-31" })),
        "ai-tools.bad-argument"
    );
    assert_eq!(
        bad(json!({ "account_id": id, "from": "2026-10-01", "to": "2026-10-31", "top": 3 })),
        "ai-tools.unknown-argument"
    );
}

#[test]
fn it_needs_the_reading_of_accounts_like_account_balance() {
    let f = fixture();
    let arguments =
        json!({ "account_id": f.checking.to_string(), "from": "2026-10-01", "to": "2026-10-31" });
    let others = Permissions::NONE
        .with(Permission::TransactionsRead, true)
        .with(Permission::CategoriesRead, true);
    for denied in [Permissions::NONE, others] {
        assert_eq!(
            call_allowed(&f.book, "daily_balances", &arguments, TODAY, denied)
                .unwrap_err()
                .report()
                .code,
            "ai-tools.not-allowed"
        );
    }
    let accounts_only = Permissions::NONE.with(Permission::AccountsRead, true);
    assert!(call_allowed(&f.book, "daily_balances", &arguments, TODAY, accounts_only).is_ok());
}

#[test]
fn the_figures_it_gives_are_known_to_the_number_check() {
    let f = fixture();
    let output = call(
        &f.book,
        "daily_balances",
        &json!({ "account_id": f.checking.to_string(), "from": "2026-11-01", "to": "2026-11-14" }),
        TODAY,
    )
    .unwrap();
    let rows = output.value["rows"].as_array().unwrap();
    let last = rows.last().unwrap()[1].as_str().unwrap().to_owned();
    let said = format!("Today the balance is {last}.");
    assert!(kassensturz_ai_tools::check_numbers(&said, &output.figures).is_empty());
    let invented = kassensturz_ai_tools::check_numbers("It was 98765.43 today.", &output.figures);
    assert_eq!(invented.len(), 1);
}
