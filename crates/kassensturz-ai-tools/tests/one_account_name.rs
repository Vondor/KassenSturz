// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The totals take `account_id` like `account_ids` with one ID (#1119):
//! the functions about one account call it so, and models carry the name
//! over. The book is synthetic and filled through the store's own
//! operations.

#![cfg(feature = "ai")]

use kassensturz_ai_tools::{ToolError, call};
use kassensturz_core::{
    Account, AccountId, AccountKind, CurrencyCode, Date, Money, Split, Transaction, TransactionId,
};
use kassensturz_store::Book;
use serde_json::{Value, json};

const TODAY: Date = Date::constant(2026, 10, 25);

fn eur(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, CurrencyCode::new("EUR").expect("EUR"))
}

fn account(name: &str, kind: AccountKind) -> Account {
    Account {
        id: AccountId::generate(),
        name: name.to_owned(),
        kind,
        currency: CurrencyCode::new("EUR").expect("EUR"),
        account_type: None,
        parent: None,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    }
}

/// Two accounts, each paying a shop once.
fn book() -> (Book, AccountId, AccountId) {
    let mut book = Book::create_in_memory().expect("a book in memory");
    book.use_currency(CurrencyCode::new("EUR").expect("EUR"))
        .expect("euros");
    let checking = account("Everyday Checking", AccountKind::Asset);
    let cash = account("Wallet", AccountKind::Asset);
    let food = account("Food", AccountKind::Expense);
    for a in [&checking, &cash, &food] {
        book.add_account(a).expect("an account");
    }
    for (day, from, minor_units) in [(1, &checking, 1_000), (2, &cash, 300)] {
        let transaction = Transaction::new(
            TransactionId::generate(),
            Date::constant(2026, 10, day),
            None,
            vec![
                Split::new(from.id, eur(-minor_units)),
                Split::new(food.id, eur(minor_units)),
            ],
        )
        .expect("a transaction");
        book.add_transaction(&transaction).expect("added");
    }
    (book, checking.id, cash.id)
}

fn spent(book: &Book, filter: Value) -> Result<Value, ToolError> {
    let mut arguments = json!({ "from": "2026-10-01", "to": "2026-10-31" });
    for (key, value) in filter.as_object().expect("an object") {
        arguments[key] = value.clone();
    }
    let value = call(book, "category_totals", &arguments, TODAY)?.value;
    Ok(value["currencies"][0]["money_out"].clone())
}

#[test]
fn account_id_counts_like_account_ids_with_one_id() -> Result<(), ToolError> {
    let (book, checking, _) = book();
    let one = spent(&book, json!({ "account_id": checking.to_string() }))?;
    let list = spent(&book, json!({ "account_ids": [checking.to_string()] }))?;
    assert_eq!(one, json!("10.00"));
    assert_eq!(one, list);
    assert_eq!(spent(&book, json!({}))?, json!("13.00"));
    Ok(())
}

#[test]
fn account_id_and_account_ids_together_are_refused() {
    let (book, checking, cash) = book();
    let both = spent(
        &book,
        json!({ "account_id": checking.to_string(), "account_ids": [cash.to_string()] }),
    );
    assert!(
        matches!(
            both,
            Err(ToolError::BadArgument {
                argument: "account_id"
            })
        ),
        "{both:?}"
    );
    let unknown = spent(
        &book,
        json!({ "account_id": AccountId::generate().to_string() }),
    );
    assert!(
        matches!(unknown, Err(ToolError::UnknownAccount)),
        "{unknown:?}"
    );
}

#[test]
fn every_function_with_account_ids_takes_account_id() -> Result<(), ToolError> {
    let (book, checking, _) = book();
    let one = json!(checking.to_string());
    for (name, mut arguments) in [
        (
            "payee_totals",
            json!({ "from": "2026-10-01", "to": "2026-10-31" }),
        ),
        (
            "monthly_totals",
            json!({ "from": "2026-10-01", "to": "2026-10-31" }),
        ),
        ("forecast", json!({})),
    ] {
        arguments["account_id"] = one.clone();
        call(&book, name, &arguments, TODAY)?;
    }
    Ok(())
}
