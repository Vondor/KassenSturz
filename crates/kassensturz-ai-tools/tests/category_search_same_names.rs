// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `search_transactions` by category when an income and an expense tree
//! share their names (#1110): a book may have both (D-082), and a search for
//! one must never bring up the other's payments. The book is synthetic and
//! filled through the store's own operations.

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

fn account(name: &str, kind: AccountKind, parent: Option<AccountId>) -> Account {
    Account {
        id: AccountId::generate(),
        name: name.to_owned(),
        kind,
        currency: CurrencyCode::new("EUR").expect("EUR"),
        account_type: None,
        parent,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    }
}

struct Ids {
    other_expense: AccountId,
    misc_expense: AccountId,
    other_income: AccountId,
    misc_income: AccountId,
}

/// Two trees "Other › Misc", one for spending and one for income, with one
/// payment on each of the four categories.
fn book_with_twin_trees() -> (Book, Ids) {
    let mut book = Book::create_in_memory().expect("a book in memory");
    book.use_currency(CurrencyCode::new("EUR").expect("EUR"))
        .expect("euros");
    let checking = account("Everyday Checking", AccountKind::Asset, None);
    let other_expense = account("Other", AccountKind::Expense, None);
    let misc_expense = account("Misc", AccountKind::Expense, Some(other_expense.id));
    let other_income = account("Other", AccountKind::Income, None);
    let misc_income = account("Misc", AccountKind::Income, Some(other_income.id));
    for a in [
        &checking,
        &other_expense,
        &misc_expense,
        &other_income,
        &misc_income,
    ] {
        book.add_account(a).expect("an account");
    }
    let mut day = 0;
    let mut book_to = |category: &Account, minor_units: i64| {
        day += 1;
        let transaction = Transaction::new(
            TransactionId::generate(),
            Date::constant(2026, 10, day),
            None,
            vec![
                Split::new(checking.id, eur(minor_units)),
                Split::new(category.id, eur(-minor_units)),
            ],
        )
        .expect("a transaction");
        book.add_transaction(&transaction).expect("added");
    };
    book_to(&other_expense, -1_000);
    book_to(&misc_expense, -300);
    book_to(&other_income, 5_000);
    book_to(&misc_income, 700);
    let ids = Ids {
        other_expense: other_expense.id,
        misc_expense: misc_expense.id,
        other_income: other_income.id,
        misc_income: misc_income.id,
    };
    (book, ids)
}

/// The amounts `search_transactions` finds for `category`, sorted.
fn found(book: &Book, category: AccountId) -> Result<(Value, Vec<String>), ToolError> {
    let value = call(
        book,
        "search_transactions",
        &json!({ "category_id": category.to_string(), "limit": 50 }),
        TODAY,
    )?
    .value;
    let mut amounts: Vec<String> = value["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| row["amount"].as_str().expect("an amount").to_owned())
        .collect();
    amounts.sort();
    Ok((value["matches"].clone(), amounts))
}

#[test]
fn a_category_search_keeps_to_its_own_tree() -> Result<(), ToolError> {
    let (book, ids) = book_with_twin_trees();

    let (matches, amounts) = found(&book, ids.other_expense)?;
    assert_eq!(matches, json!(2), "{amounts:?}");
    assert_eq!(amounts, ["-10.00", "-3.00"]);

    let (matches, amounts) = found(&book, ids.other_income)?;
    assert_eq!(matches, json!(2), "{amounts:?}");
    assert_eq!(amounts, ["50.00", "7.00"]);
    Ok(())
}

#[test]
fn a_subcategory_search_keeps_to_its_own_tree() -> Result<(), ToolError> {
    let (book, ids) = book_with_twin_trees();

    let (matches, amounts) = found(&book, ids.misc_expense)?;
    assert_eq!(matches, json!(1), "{amounts:?}");
    assert_eq!(amounts, ["-3.00"]);

    let (matches, amounts) = found(&book, ids.misc_income)?;
    assert_eq!(matches, json!(1), "{amounts:?}");
    assert_eq!(amounts, ["7.00"]);
    Ok(())
}
