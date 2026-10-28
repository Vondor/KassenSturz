// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Writing down and removing payments in cash (D-258) on the five-year Space
//! Colony book, with what the read functions give a model: the balance of the
//! cash account moves by exactly the amount, undo takes all of it back, and
//! what a bank delivered or a transfer made is not removed.

#![cfg(feature = "ai")]

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use kassensturz_ai_tools::{Confirmed, ProposedChange, call, propose as propose_with};
use kassensturz_core::ai_switch::{Field, Permissions};
use kassensturz_core::{Account, AccountId, AccountType, CurrencyCode, Date, Report};
use kassensturz_import::{Imported, KmySqlite, Limits, import};
use kassensturz_pipeline::fill_book_from;
use kassensturz_store::{Action, Book};
use serde_json::{Value, json};

/// Everything allowed, nothing locked.
fn open() -> Permissions {
    Field::ALL
        .iter()
        .fold(Permissions::all(), |p, field| p.with_lock(*field, false))
}

const TODAY: Date = Date::constant(2026, 10, 28);

fn space_colony() -> Book {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../kassensturz-import/tests/books/space-colony-5y-v15.sqlite");
    let bytes = fs::read(&path).expect("the Space Colony book is in the repository");
    let Imported::Book(imported) =
        import(&KmySqlite, &bytes, &Limits::default()).expect("the file reads")
    else {
        panic!("a book");
    };
    let mut book = Book::create_in_memory().expect("a book in memory");
    fill_book_from(&mut book, &imported, &HashMap::new()).expect("filled");
    // The colony's wallet: the import types no account as cash.
    let eur = CurrencyCode::new("EUR").expect("EUR");
    book.use_currency(eur).expect("currency");
    book.add_account(&Account {
        id: AccountId::generate(),
        name: "Pocket Cash".into(),
        kind: AccountType::Cash.kind(),
        currency: eur,
        account_type: Some(AccountType::Cash),
        closed: false,
        parent: None,
        number: None,
        notes: None,
        institution: None,
    })
    .expect("a cash account");
    book
}

fn run(book: &Book, name: &str, arguments: Value) -> Value {
    call(book, name, &arguments, TODAY)
        .unwrap_or_else(|error| panic!("{name}: {error}"))
        .value
}

/// The accounts as a model reads them from `list_accounts`.
fn accounts(book: &Book) -> Vec<Value> {
    run(book, "list_accounts", json!({}))["accounts"]
        .as_array()
        .expect("accounts")
        .clone()
}

fn cash_account(book: &Book) -> Value {
    accounts(book)
        .into_iter()
        .find(|account| account["type"] == "cash")
        .expect("a cash account in the book")
}

#[test]
fn a_payment_in_cash_moves_the_balance_by_exactly_its_amount_and_undo_takes_it_back() {
    let mut book = space_colony();
    let wallet = cash_account(&book);
    let before = accounts(&book);
    let history = book.history().expect("history").len();

    let proposal = propose_with(
        &book,
        &json!({"new_transactions": [{
            "account_id": wallet["id"],
            "date": "2026-10-27",
            "amount": "-12.34",
            "payee": "Dome Kiosk",
            "note": "receipt kept",
        }]}),
        open(),
    )
    .expect("prepared");
    assert_eq!(accounts(&book), before, "proposing changes nothing");
    let ProposedChange::NewTransaction(new) = &proposal.changes.changes()[0] else {
        panic!("a new transaction");
    };
    assert_eq!(new.account_name, wallet["name"].as_str().unwrap());

    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    let after = accounts(&book);
    let now = after.iter().find(|a| a["id"] == wallet["id"]).unwrap();
    let was: f64_free::Cents = f64_free::cents(wallet["balance"].as_str().unwrap());
    assert_eq!(
        f64_free::cents(now["balance"].as_str().unwrap()),
        was - 1234
    );
    for (a, b) in before.iter().zip(&after) {
        if a["id"] != wallet["id"] {
            assert_eq!(a, b, "no other balance changes");
        }
    }
    let found = run(
        &book,
        "search_transactions",
        json!({"account_id": wallet["id"], "limit": 5}),
    );
    assert!(
        found["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["payee"] == "Dome Kiosk" && row["amount"] == "-12.34"),
        "{found}"
    );
    assert_eq!(book.history().expect("history").len(), history + 1);
    assert_eq!(book.history().unwrap()[0].action, Action::ApplyAiChanges);

    book.undo().expect("undone").expect("a change");
    assert_eq!(accounts(&book), before, "undo takes back all of it");
}

/// Balances as whole minor units, without floating point.
mod f64_free {
    pub type Cents = i64;
    pub fn cents(text: &str) -> Cents {
        let negative = text.starts_with('-');
        let digits: String = text.chars().filter(char::is_ascii_digit).collect();
        let value: i64 = digits.parse().expect("digits");
        if negative { -value } else { value }
    }
}

#[test]
fn only_a_hand_entered_cash_transaction_is_removed_never_a_transfer_or_a_bank_entry() {
    let mut book = space_colony();
    let wallet = cash_account(&book);
    // An entry the bank delivered stays.
    let entry = run(&book, "search_transactions", json!({"limit": 5}));
    let entry = entry["rows"][0].clone();
    let error = propose_with(
        &book,
        &json!({"delete_transactions": [{"transaction_id": entry["transaction_id"]}]}),
        open(),
    )
    .expect_err("a bank entry stays");
    assert_eq!(error.report().code, "ai-tools.not-deletable");

    // What the AI wrote itself can go again, and comes back with undo.
    let added = propose_with(
        &book,
        &json!({"new_transactions": [{
            "account_id": wallet["id"], "date": "2026-10-27", "amount": "-3.00", "payee": "Bus"
        }]}),
        open(),
    )
    .expect("prepared");
    let ProposedChange::NewTransaction(new) = &added.changes.changes()[0] else {
        panic!("a new transaction");
    };
    let id = new.transaction;
    added
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    let before = accounts(&book);
    let removal = propose_with(
        &book,
        &json!({"delete_transactions": [{"transaction_id": id.to_string()}]}),
        open(),
    )
    .expect("a hand-entered cash payment");
    removal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("removed");
    assert_ne!(accounts(&book), before);
    book.undo().expect("undone").expect("a change");
    assert_eq!(accounts(&book), before);
}
