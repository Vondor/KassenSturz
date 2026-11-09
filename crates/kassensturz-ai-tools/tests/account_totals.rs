// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `category_totals` and `payee_totals` limited to some of the user's own
//! accounts (M6-B206, D-667): a transaction counts, as a whole, when it has
//! a part in one of them, as the report filters and budgets count it. The
//! book is synthetic and filled through the store's own operations.

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

struct Fixture {
    book: Book,
    checking: AccountId,
    savings: AccountId,
    groceries: AccountId,
}

/// Groceries from two accounts, a salary into one, and a move between them.
fn fixture() -> Fixture {
    let mut book = Book::create_in_memory().expect("a book in memory");
    book.use_currency(CurrencyCode::new("EUR").expect("EUR"))
        .expect("euros");
    let checking = account("Everyday Checking", AccountKind::Asset);
    let savings = account("Rainy Day Savings", AccountKind::Asset);
    let groceries = account("Groceries", AccountKind::Expense);
    let salary = account("Salary", AccountKind::Income);
    for a in [&checking, &savings, &groceries, &salary] {
        book.add_account(a).expect("an account");
    }
    let mut add = |day: i8, payee: Option<&str>, from: AccountId, to: AccountId, minor: i64| {
        let transaction = Transaction::new(
            TransactionId::generate(),
            Date::constant(2026, 10, day),
            payee.map(str::to_owned),
            vec![Split::new(from, eur(-minor)), Split::new(to, eur(minor))],
        )
        .expect("a transaction");
        book.add_transaction(&transaction).expect("added");
    };
    add(2, Some("Corner Shop"), checking.id, groceries.id, 3_000);
    add(3, Some("Corner Shop"), savings.id, groceries.id, 1_200);
    add(
        4,
        Some("Synthetic Employer"),
        salary.id,
        checking.id,
        200_000,
    );
    add(5, None, checking.id, savings.id, 10_000);
    Fixture {
        book,
        checking: checking.id,
        savings: savings.id,
        groceries: groceries.id,
    }
}

fn totals(f: &Fixture, function: &str, accounts: Option<Value>) -> Result<Value, ToolError> {
    let mut arguments = json!({ "from": "2026-10-01", "to": "2026-10-31" });
    if let Some(accounts) = accounts {
        arguments["account_ids"] = accounts;
    }
    Ok(call(&f.book, function, &arguments, TODAY)?.value)
}

fn ids(list: &[AccountId]) -> Value {
    Value::Array(list.iter().map(|id| json!(id.to_string())).collect())
}

#[test]
fn category_totals_count_only_the_chosen_accounts() -> Result<(), ToolError> {
    let f = fixture();

    let checking = totals(&f, "category_totals", Some(ids(&[f.checking])))?;
    assert_eq!(checking["accounts"], json!(["Everyday Checking"]));
    let eur = &checking["currencies"][0];
    assert_eq!(eur["money_out"], json!("30.00"));
    assert_eq!(eur["money_in"], json!("2000.00"));
    assert_eq!(eur["left"], json!("1970.00"));
    // The move to savings touches checking: still neither income nor spending.
    assert_eq!(checking["transfers"], json!(1));

    let savings = totals(&f, "category_totals", Some(ids(&[f.savings])))?;
    let eur = &savings["currencies"][0];
    assert_eq!(eur["money_out"], json!("12.00"));
    assert_eq!(eur["money_in"], json!("0.00"));
    assert_eq!(savings["transfers"], json!(1));

    // Both accounts, or none chosen, count everything; only a filter is named.
    let both = totals(&f, "category_totals", Some(ids(&[f.checking, f.savings])))?;
    let all = totals(&f, "category_totals", None)?;
    assert_eq!(both["currencies"], all["currencies"]);
    assert_eq!(all["currencies"][0]["money_out"], json!("42.00"));
    assert!(all.get("accounts").is_none());
    let empty = totals(&f, "category_totals", Some(json!([])))?;
    assert_eq!(empty, all);
    Ok(())
}

#[test]
fn payee_totals_count_only_the_chosen_accounts() -> Result<(), ToolError> {
    let f = fixture();

    let checking = totals(&f, "payee_totals", Some(ids(&[f.checking])))?;
    assert_eq!(checking["accounts"], json!(["Everyday Checking"]));
    let eur = &checking["currencies"][0];
    assert_eq!(
        eur["paid"],
        json!([{ "payee": "Corner Shop", "amount": "30.00", "transactions": 1 }])
    );
    assert_eq!(
        eur["received"],
        json!([{ "payee": "Synthetic Employer", "amount": "2000.00", "transactions": 1 }])
    );

    let savings = totals(&f, "payee_totals", Some(ids(&[f.savings])))?;
    let eur = &savings["currencies"][0];
    assert_eq!(
        eur["paid"],
        json!([{ "payee": "Corner Shop", "amount": "12.00", "transactions": 1 }])
    );
    assert_eq!(eur["received"], json!([]));

    let all = totals(&f, "payee_totals", None)?;
    assert_eq!(all["currencies"][0]["paid"][0]["amount"], json!("42.00"));
    assert!(all.get("accounts").is_none());
    Ok(())
}

#[test]
fn account_ids_must_name_own_accounts() {
    let f = fixture();
    for function in ["category_totals", "payee_totals"] {
        // A category isn't one of the user's accounts.
        assert!(matches!(
            totals(&f, function, Some(ids(&[f.groceries]))),
            Err(ToolError::UnknownAccount)
        ));
        assert!(matches!(
            totals(&f, function, Some(ids(&[AccountId::generate()]))),
            Err(ToolError::UnknownAccount)
        ));
        for wrong in [
            json!(f.checking.to_string()),
            json!(["not an ID"]),
            json!([1]),
            ids(&vec![f.checking; 21]),
        ] {
            assert!(
                matches!(
                    totals(&f, function, Some(wrong.clone())),
                    Err(ToolError::BadArgument {
                        argument: "account_ids"
                    })
                ),
                "{function}: {wrong}"
            );
        }
        // The same account twice counts once.
        let twice = totals(&f, function, Some(ids(&[f.checking, f.checking])))
            .expect("the same account twice");
        let once = totals(&f, function, Some(ids(&[f.checking]))).expect("one account");
        assert_eq!(twice, once);
    }
}
