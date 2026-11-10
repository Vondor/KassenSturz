// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Moves between the user's own accounts that are not joined as a Transfer
//! (US2.14 (a), #594, D-672): the AI tools mark them as possible Transfers,
//! because their two entries count as income and spending. Checked on a
//! synthetic book filled through the store, and on the synthetic sample
//! book as the KMyMoney importer and the pipeline really fill it, against
//! the app's own Transfer suggestions (AGENTS.md §9.4).

#![cfg(feature = "ai")]

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use kassensturz_ai_tools::call;
use kassensturz_core::{
    Account, AccountId, AccountKind, BankLink, CurrencyCode, Date, ImportedDetails, Money, Split,
    Transaction, TransactionId,
};
use kassensturz_import::{Imported, KmySqlite, Limits, import};
use kassensturz_pipeline::fill_book_from;
use kassensturz_store::Book;
use serde_json::{Value, json};

const TODAY: Date = Date::constant(2026, 10, 25);

/// Example IBANs from SWIFT's IBAN Registry (D-055).
const SAVINGS_IBAN: &str = "DE89 3704 0044 0532 0130 00";
const SHOP_IBAN: &str = "GB29NWBK60161331926819";

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

fn add(
    book: &mut Book,
    day: i8,
    payee: &str,
    own: (AccountId, i64),
    other: Option<AccountId>,
    other_iban: Option<&str>,
) {
    let mut part = Split::new(own.0, eur(own.1));
    part.imported = Some(ImportedDetails {
        other_party_iban: other_iban.map(str::to_owned),
        ..ImportedDetails::default()
    });
    let rest = match other {
        Some(category) => Split::new(category, eur(-own.1)),
        None => Split::uncategorized(eur(-own.1)),
    };
    let transaction = Transaction::new(
        TransactionId::generate(),
        Date::constant(2026, 10, day),
        Some(payee.to_owned()),
        vec![part, rest],
    )
    .expect("a transaction");
    book.add_transaction(&transaction).expect("added");
}

/// A book with a standing order to savings whose sides a Rule or the user
/// booked to categories, a move whose other side isn't in the book but
/// whose bank names the savings account, a joined Transfer, and plain
/// spending of the same size.
fn book() -> Book {
    let mut book = Book::create_in_memory().expect("a book in memory");
    book.use_currency(CurrencyCode::new("EUR").expect("EUR"))
        .expect("euros");
    let checking = account("Everyday Checking", AccountKind::Asset);
    let savings = account("Rainy Day Savings", AccountKind::Asset);
    let other_income = account("Other income", AccountKind::Income);
    let household = account("Household", AccountKind::Expense);
    for a in [&checking, &savings, &other_income, &household] {
        book.add_account(a).expect("an account");
    }
    book.link_bank_account(&BankLink {
        account: savings.id,
        iban: Some(SAVINGS_IBAN.replace(' ', "")),
        bank_code: None,
        number: None,
        last_fetched_on: None,
    })
    .expect("a bank link");
    // Friday out of checking, Monday into savings: one business day.
    add(
        &mut book,
        2,
        "Standing order",
        (checking.id, -20_000),
        Some(household.id),
        None,
    );
    add(
        &mut book,
        5,
        "Standing order",
        (savings.id, 20_000),
        Some(other_income.id),
        None,
    );
    // The other side is missing, but the bank names the savings account,
    // written with spaces and in lower case.
    add(
        &mut book,
        10,
        "To savings",
        (checking.id, -5_000),
        Some(household.id),
        Some(&SAVINGS_IBAN.to_lowercase()),
    );
    // A shop of the same size as the standing order, far from it in time.
    add(
        &mut book,
        20,
        "Hardware store",
        (checking.id, -20_000),
        Some(household.id),
        Some(SHOP_IBAN),
    );
    let joined = Transaction::new(
        TransactionId::generate(),
        Date::constant(2026, 10, 21),
        None,
        vec![
            Split::new(checking.id, eur(-7_000)),
            Split::new(savings.id, eur(7_000)),
        ],
    )
    .expect("a Transfer");
    book.add_transaction(&joined).expect("added");
    book
}

fn marked(value: &Value) -> Vec<(String, String)> {
    let mut marked: Vec<(String, String)> = value["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .filter(|row| row["possible_transfer"] == true)
        .map(|row| {
            (
                row["date"].as_str().expect("a day").to_owned(),
                row["amount"].as_str().expect("an amount").to_owned(),
            )
        })
        .collect();
    marked.sort();
    marked
}

#[test]
fn moves_not_joined_as_transfers_are_marked() {
    let book = book();
    let value = call(
        &book,
        "search_transactions",
        &json!({ "limit": 50, "skip_transfers": true }),
        TODAY,
    )
    .expect("a search")
    .value;
    // Both sides of the standing order, and the move the bank names; not
    // the shop, and the joined Transfer is left out by skip_transfers.
    assert_eq!(
        marked(&value),
        [
            ("2026-10-02".to_owned(), "-200.00".to_owned()),
            ("2026-10-05".to_owned(), "200.00".to_owned()),
            ("2026-10-10".to_owned(), "-50.00".to_owned()),
        ]
    );
    assert_eq!(value["matches"], json!(4));
    // No IBAN reaches the model, not even the user's own.
    let text = value.to_string();
    assert!(!text.contains("DE89") && !text.contains("de89") && !text.contains("GB29"));
}

#[test]
fn the_totals_say_what_of_income_and_spending_may_be_moves() {
    let book = book();
    let value = call(
        &book,
        "category_totals",
        &json!({ "from": "2026-10-01", "to": "2026-10-31" }),
        TODAY,
    )
    .expect("totals")
    .value;
    let totals = &value["currencies"][0];
    // The totals are what the app shows; the hint stands beside them.
    assert_eq!(totals["money_out"], json!("450.00"));
    assert_eq!(totals["money_in"], json!("200.00"));
    assert_eq!(value["transfers"], json!(1));
    assert_eq!(
        totals["possible_transfers"],
        json!({
            "income": { "amount": "200.00", "entries": 1 },
            "spending": { "amount": "250.00", "entries": 2 },
        })
    );
    // Only what the account filter counts.
    let savings = call(&book, "list_accounts", &Value::Null, TODAY)
        .expect("accounts")
        .value["accounts"]
        .as_array()
        .expect("accounts")
        .iter()
        .find(|row| row["name"] == "Rainy Day Savings")
        .expect("savings")["id"]
        .clone();
    let only = call(
        &book,
        "category_totals",
        &json!({ "from": "2026-10-01", "to": "2026-10-31", "account_ids": [savings] }),
        TODAY,
    )
    .expect("totals")
    .value;
    assert_eq!(
        only["currencies"][0]["possible_transfers"],
        json!({ "income": { "amount": "200.00", "entries": 1 } })
    );
}

#[test]
fn the_apps_own_transfer_suggestions_are_all_marked_on_the_sample_book() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../kassensturz-import/tests/golden/kmy-sqlite/kassensturz-sample.sqlite");
    let bytes = fs::read(&path).expect("the sample book is in the repository");
    let Imported::Book(imported) =
        import(&KmySqlite, &bytes, &Limits::default()).expect("the file reads")
    else {
        panic!("a book");
    };
    let mut book = Book::create_in_memory().expect("a book in memory");
    fill_book_from(&mut book, &imported, &HashMap::new()).expect("filled");
    let suggested: HashSet<String> = book
        .transfer_suggestions()
        .expect("suggestions")
        .iter()
        .flat_map(|found| [found.outgoing.transaction, found.incoming.transaction])
        .map(|id| id.to_string())
        .collect();
    assert!(
        !suggested.is_empty(),
        "the sample book has open suggestions"
    );
    let mut offset = 0;
    let mut marked = HashSet::new();
    loop {
        let page = call(
            &book,
            "search_transactions",
            &json!({ "limit": 50, "offset": offset }),
            TODAY,
        )
        .expect("a search")
        .value;
        for row in page["rows"].as_array().expect("rows") {
            if row["possible_transfer"] == true {
                let id = row["transaction_id"].as_str().expect("an ID");
                marked.insert(id.to_owned());
            }
        }
        match page.get("next_offset") {
            Some(next) => offset = next.as_u64().expect("a number"),
            None => break,
        }
    }
    assert!(
        suggested.is_subset(&marked),
        "{} suggested sides are not marked",
        suggested.difference(&marked).count()
    );
}
