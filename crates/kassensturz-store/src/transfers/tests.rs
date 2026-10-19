// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of Transfer suggestions and joining, on books in memory. All data
//! is made up (AGENTS.md §3.1 rule 2); the IBANs are the examples in SWIFT's
//! IBAN Registry (D-055).

use kassensturz_core::{
    AccountKind, ImportedDetails, Money, Split, Transaction, TransferError, TransferSide,
};

use super::*;
use crate::tests::{EUR, Sample, USD, account, date, eur, sample, transaction};

/// One bank's side of a move between accounts, as an import (M2) would add
/// it: the account's part with what the bank delivered, and a part without
/// a category yet (D-083). `day` is in October 2026; the 5th is a Monday.
fn side(book: &mut Book, account: AccountId, minor_units: i64, day: i8) -> Transaction {
    let delivered = ImportedDetails {
        bank_id: Some(format!("2026-10-{day:02}-{minor_units}")),
        booking_date: Some(date(2026, 10, day)),
        value_date: Some(date(2026, 10, day)),
        other_party_name: Some("Own transfer".into()),
        other_party_iban: Some("DE89370400440532013000".into()),
        remittance_information: Some("Savings".into()),
    };
    let booked = Split {
        imported: Some(delivered),
        ..Split::new(account, Money::from_minor_units(minor_units, EUR))
    };
    let other = Split::uncategorized(Money::from_minor_units(-minor_units, EUR));
    let transaction = Transaction::new(
        TransactionId::generate(),
        date(2026, 10, day),
        None,
        vec![booked, other],
    )
    .expect("balanced");
    book.add_transaction(&transaction).expect("added");
    transaction
}

struct Accounts {
    book: Book,
    checking: AccountId,
    savings: AccountId,
    groceries: AccountId,
}

fn accounts() -> Accounts {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let savings = account(&mut book, "Savings account", AccountKind::Asset, EUR);
    Accounts {
        book,
        checking,
        savings,
        groceries,
    }
}

fn pairs(book: &Book) -> Vec<(TransactionId, TransactionId)> {
    book.transfer_suggestions()
        .expect("suggestions")
        .into_iter()
        .map(|found| (found.outgoing.transaction, found.incoming.transaction))
        .collect()
}

#[test]
fn both_sides_of_a_move_between_accounts_are_suggested() {
    let Accounts {
        mut book,
        checking,
        savings,
        ..
    } = accounts();
    // Out on Friday, in on Monday: one business day.
    let out = side(&mut book, checking, -20_000, 9);
    let into = side(&mut book, savings, 20_000, 12);
    let suggestions = book.transfer_suggestions().expect("suggestions");
    assert_eq!(suggestions.len(), 1);
    let found = suggestions[0];
    assert_eq!(found.outgoing.transaction, out.id());
    assert_eq!(found.outgoing.account, checking);
    assert_eq!(found.outgoing.amount, eur(-20_000));
    assert_eq!(found.incoming.transaction, into.id());
    assert_eq!(found.incoming.account, savings);
    assert_eq!(found.business_days, 1);
    // Suggesting changes nothing.
    assert_eq!(book.transaction(out.id()).expect("read"), out);
    assert_eq!(book.transaction(into.id()).expect("read"), into);
}

#[test]
fn only_sides_whose_other_part_has_no_category_are_suggested() {
    let Accounts {
        mut book,
        checking,
        savings,
        groceries,
    } = accounts();
    let out = side(&mut book, checking, -20_000, 5);
    let into = side(&mut book, savings, 20_000, 6);
    // The user said what the other part of the deposit was for, so it is
    // no Transfer anymore: manual choices win (D-031).
    let parts = vec![
        into.splits()[0].clone(),
        Split {
            account: Some(groceries),
            ..into.splits()[1].clone()
        },
    ];
    let categorized = Transaction::new(into.id(), date(2026, 10, 6), None, parts).expect("ok");
    book.update_transaction(&categorized).expect("changed");
    assert_eq!(pairs(&book), Vec::new());

    // Neither are categories, nor transactions that are Transfers already.
    let salary = transaction(
        date(2026, 10, 6),
        "Salary",
        &[(checking, eur(20_000)), (groceries, eur(-20_000))],
    );
    book.add_transaction(&salary).expect("added");
    let moved = transaction(
        date(2026, 10, 6),
        "To savings",
        &[(checking, eur(-20_000)), (savings, eur(20_000))],
    );
    book.add_transaction(&moved).expect("added");
    assert_eq!(pairs(&book), Vec::new());

    // A new deposit fits again.
    let again = side(&mut book, savings, 20_000, 7);
    assert_eq!(pairs(&book), vec![(out.id(), again.id())]);
}

#[test]
fn sides_in_other_currencies_or_too_far_apart_are_not_suggested() {
    let Accounts {
        mut book,
        checking,
        savings,
        ..
    } = accounts();
    // Monday and the Friday after: four business days.
    side(&mut book, checking, -20_000, 5);
    side(&mut book, savings, 20_000, 9);
    assert_eq!(pairs(&book), Vec::new());

    book.use_currency(USD).expect("currency used");
    let dollars = account(&mut book, "Dollar account", AccountKind::Asset, USD);
    let deposit = Transaction::new(
        TransactionId::generate(),
        date(2026, 10, 5),
        None,
        vec![
            Split::new(dollars, Money::from_minor_units(20_000, USD)),
            Split::uncategorized(Money::from_minor_units(-20_000, USD)),
        ],
    )
    .expect("balanced");
    book.add_transaction(&deposit).expect("added");
    assert_eq!(pairs(&book), Vec::new());
}

#[test]
fn a_credit_card_payment_is_a_transfer_too() {
    let Accounts {
        mut book, checking, ..
    } = accounts();
    let card = account(&mut book, "Credit card", AccountKind::Liability, EUR);
    // The payment lowers what the user owes: money flows into the card.
    let into = side(&mut book, card, 30_000, 5);
    let out = side(&mut book, checking, -30_000, 6);
    assert_eq!(pairs(&book), vec![(out.id(), into.id())]);
    book.join_transfer(into.id(), out.id()).expect("joined");
    assert_eq!(book.balance(card).expect("balance"), eur(30_000));
    assert_eq!(book.balance(checking).expect("balance"), eur(-30_000));
}

#[test]
fn joining_makes_one_transfer_that_keeps_what_both_banks_delivered() {
    let Accounts {
        mut book,
        checking,
        savings,
        ..
    } = accounts();
    let out = side(&mut book, checking, -20_000, 9);
    let into = side(&mut book, savings, 20_000, 12);

    let joined = book.join_transfer(into.id(), out.id()).expect("joined");
    assert_eq!(joined, out.id());
    let transfer = book.transaction(joined).expect("read");
    assert_eq!(transfer.booking_date(), date(2026, 10, 9));
    assert_eq!(
        transfer.splits(),
        [out.splits()[0].clone(), into.splits()[0].clone()]
    );
    assert!(matches!(
        book.transaction(into.id()),
        Err(StoreError::UnknownTransaction(id)) if id == into.id()
    ));
    // Both registers show the one transaction, and the balances stay.
    let register = |account| {
        book.register(account)
            .expect("register")
            .into_iter()
            .map(|entry| (entry.transaction, entry.amount))
            .collect::<Vec<_>>()
    };
    assert_eq!(register(checking), vec![(joined, eur(-20_000))]);
    assert_eq!(register(savings), vec![(joined, eur(20_000))]);
    assert_eq!(pairs(&book), Vec::new());
    assert_eq!(
        book.history().expect("history")[0].action,
        Action::JoinTransfer
    );

    // Undo brings both back as they were, and redo joins them again.
    book.undo().expect("undone");
    assert_eq!(book.transaction(out.id()).expect("back"), out);
    assert_eq!(book.transaction(into.id()).expect("back"), into);
    assert_eq!(pairs(&book), vec![(out.id(), into.id())]);
    book.redo().expect("redone");
    assert_eq!(book.transaction(joined).expect("read"), transfer);
    assert!(book.transaction(into.id()).is_err());
}

#[test]
fn joining_keeps_the_incoming_payee_if_the_outgoing_side_has_none() {
    let Accounts {
        mut book,
        checking,
        savings,
        ..
    } = accounts();
    let out = side(&mut book, checking, -20_000, 9);
    let into = side(&mut book, savings, 20_000, 9);
    let named = Transaction::new(
        into.id(),
        into.booking_date(),
        Some("From checking".into()),
        into.splits().to_vec(),
    )
    .expect("balanced");
    book.update_transaction(&named).expect("changed");
    let joined = book.join_transfer(out.id(), into.id()).expect("joined");
    assert_eq!(
        book.transaction(joined).expect("read").payee(),
        Some("From checking")
    );
}

#[test]
fn what_is_no_transfer_is_not_joined() {
    let Accounts {
        mut book,
        checking,
        savings,
        groceries,
    } = accounts();
    let out = side(&mut book, checking, -20_000, 5);
    let less = side(&mut book, savings, 19_999, 5);
    let same_account = side(&mut book, checking, 20_000, 5);
    let shopping = transaction(
        date(2026, 10, 5),
        "Corner Grocery",
        &[(savings, eur(20_000)), (groceries, eur(-20_000))],
    );
    book.add_transaction(&shopping).expect("added");
    let unknown = TransactionId::generate();

    let refused = |book: &mut Book, other: TransactionId| {
        let before = book.history().expect("history").len();
        let result = book.join_transfer(out.id(), other);
        assert_eq!(book.history().expect("history").len(), before);
        result
    };
    assert!(matches!(
        refused(&mut book, less.id()),
        Err(StoreError::Transfer(TransferError::AmountsDiffer { .. }))
    ));
    assert!(matches!(
        refused(&mut book, same_account.id()),
        Err(StoreError::Transfer(TransferError::SameAccount(id))) if id == checking
    ));
    assert!(matches!(
        refused(&mut book, shopping.id()),
        Err(StoreError::Transfer(TransferError::NotASide(id))) if id == shopping.id()
    ));
    assert!(matches!(
        refused(&mut book, out.id()),
        Err(StoreError::Transfer(TransferError::SameTransaction(_)))
    ));
    assert!(matches!(
        refused(&mut book, unknown),
        Err(StoreError::UnknownTransaction(id)) if id == unknown
    ));
    // Nothing changed.
    for kept in [&out, &less, &same_account, &shopping] {
        assert_eq!(&book.transaction(kept.id()).expect("read"), kept);
    }
}

#[test]
fn a_category_is_no_side_of_a_transfer() {
    // A transaction between a category and no category yet: there is no
    // account the money moved out of.
    let Accounts {
        mut book,
        groceries,
        ..
    } = accounts();
    let odd = Transaction::new(
        TransactionId::generate(),
        date(2026, 10, 5),
        None,
        vec![
            Split::new(groceries, eur(-20_000)),
            Split::uncategorized(eur(20_000)),
        ],
    )
    .expect("balanced");
    book.add_transaction(&odd).expect("added");
    let accounts = book.account_ids().expect("accounts");
    assert_eq!(
        TransferSide::of(&odd, |id| accounts.contains(&id)),
        Err(TransferError::NotASide(odd.id()))
    );
    assert_eq!(pairs(&book), Vec::new());
}
