// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! B-338: a bill link is applied only to the transaction as the user
//! reviewed it, like every other change of a transaction. Synthetic book
//! and a synthetic bill file only.

#![cfg(feature = "ai")]

use std::fs;

use kassensturz_ai_tools::{ChangeError, Confirmed, PROPOSE_CHANGES, prepare_named_with_bills};
use kassensturz_core::ai_switch::{Field, Permissions};
use kassensturz_core::{
    Account, AccountId, AccountKind, AccountType, CurrencyCode, Date, Money, Split, Transaction,
    TransactionId,
};
use kassensturz_store::Book;
use serde_json::json;

fn eur() -> CurrencyCode {
    CurrencyCode::new("EUR").unwrap()
}

fn open() -> Permissions {
    Field::ALL
        .iter()
        .fold(Permissions::all(), |p, field| p.with_lock(*field, false))
}

fn account(name: &str, kind: AccountKind, account_type: Option<AccountType>) -> Account {
    Account {
        id: AccountId::generate(),
        name: name.into(),
        kind,
        currency: eur(),
        account_type,
        parent: None,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    }
}

const TODAY: Date = Date::constant(2026, 11, 15);

#[test]
fn a_bill_is_not_attached_to_a_transaction_changed_since_the_review() {
    let mut book = Book::create_in_memory().unwrap();
    book.use_currency(eur()).unwrap();
    let wallet = account("Wallet", AccountKind::Asset, Some(AccountType::Cash));
    let food = account("Food", AccountKind::Expense, None);
    let fun = account("Fun", AccountKind::Expense, None);
    for a in [&wallet, &food, &fun] {
        book.add_account(a).unwrap();
    }
    let m = |cents| Money::from_minor_units(cents, eur());
    let id = TransactionId::generate();
    let reviewed = Transaction::new(
        id,
        Date::constant(2026, 10, 20),
        Some("Corner Bakery".into()),
        vec![Split::new(wallet.id, m(-450)), Split::new(food.id, m(450))],
    )
    .unwrap();
    book.add_transaction(&reviewed).unwrap();

    let folder = std::env::temp_dir().join(format!("kassensturz-audit-ai-2-{id}"));
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("bakery.pdf"), b"synthetic bill").unwrap();

    let both = prepare_named_with_bills(
        &book,
        PROPOSE_CHANGES.name,
        &json!({"attach_bills": [{"transaction_id": id.to_string(), "file": "bakery.pdf"}]}),
        open(),
        TODAY,
        Some(&folder),
    )
    .expect("prepared")
    .changes;
    // For comparison, a change of the same transaction's category.
    let categorize = prepare_named_with_bills(
        &book,
        PROPOSE_CHANGES.name,
        &json!({"categorize": [{"transaction_id": id.to_string(), "category_id": fun.id.to_string()}]}),
        open(),
        TODAY,
        Some(&folder),
    )
    .expect("prepared")
    .changes;

    // The transaction changes after the review: another day, payee, amount.
    let changed = Transaction::new(
        id,
        Date::constant(2026, 11, 2),
        Some("Garden Centre".into()),
        vec![
            Split::new(wallet.id, m(-8_990)),
            Split::new(food.id, m(8_990)),
        ],
    )
    .unwrap();
    book.update_transaction(&changed).unwrap();

    let sorted = categorize.apply(&mut book, Confirmed::by_the_user());
    assert!(matches!(sorted, Err(ChangeError::OutOfDate)));
    let attached = both.apply(&mut book, Confirmed::by_the_user());
    let _ = fs::remove_dir_all(&folder);
    assert!(
        matches!(attached, Err(ChangeError::OutOfDate)),
        "the bill reviewed for 'Corner Bakery, 4.50, 20 Oct' was linked to \
         'Garden Centre, 89.90, 2 Nov': {:?}",
        attached.map_err(|e| e.to_string())
    );
}
