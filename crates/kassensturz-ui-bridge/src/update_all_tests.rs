// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of *Update all accounts* (M6d.7, D-446, D-449) with the fake
//! bank's real statements, the real import and the real Rules.

use std::sync::atomic::AtomicBool;

use kassensturz_bank::{Bank, FakeBank, LoginId, MemoryPinStore, Pin, PinMode, PinSession};
use kassensturz_core::{
    Account, AccountKind, BankLink, CurrencyCode, Date, Money, Split, Transaction,
};

use super::*;
use crate::bank_connection::Connection;
use crate::register::tests::TestLocale;
use crate::rules::{self, ConditionForm, RuleForm};

const IBAN: &str = "DE89370400440532013000";

fn eur() -> CurrencyCode {
    CurrencyCode::new("EUR").expect("EUR")
}

fn day(month: i8, day: i8) -> Date {
    Date::new(2026, month, day).expect("a date")
}

fn account(book: &mut Book, name: &str, kind: AccountKind) -> AccountId {
    let id = AccountId::generate();
    book.use_currency(eur()).expect("currency");
    book.add_account(&Account {
        id,
        name: name.to_owned(),
        kind,
        currency: eur(),
        account_type: None,
        parent: None,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    })
    .expect("account");
    id
}

fn link(book: &mut Book, account: AccountId) {
    book.link_bank_account(&BankLink {
        account,
        iban: Some(IBAN.to_owned()),
        bank_code: Some("37040044".to_owned()),
        number: None,
        last_fetched_on: None,
    })
    .expect("linked");
}

/// The fake bank with one login whose PIN is remembered, as its setup
/// leaves it.
fn connection() -> Connection {
    let mut bank = FakeBank::new();
    let mut pins = PinSession::new(Box::new(MemoryPinStore::new()));
    pins.provide(
        &LoginId::pending(),
        Pin::new(FakeBank::PIN.to_owned()),
        PinMode::RememberUntilClose,
    )
    .expect("provided");
    bank.add_login(&FakeBank::setup(), &mut pins)
        .expect("login added");
    Connection::fake(bank, pins)
}

/// A book with a linked current account, Groceries with five earlier
/// purchases of 4.50 and a Rule that books the fake bank's grocer there.
struct Sample {
    book: Book,
    current: AccountId,
    groceries: AccountId,
}

fn sample() -> Sample {
    let mut book = Book::create_in_memory().expect("book");
    let current = account(&mut book, "Current account", AccountKind::Asset);
    let groceries = account(&mut book, "Groceries", AccountKind::Expense);
    link(&mut book, current);
    for date in [1, 5, 9, 13, 17] {
        let splits = vec![
            Split::new(current, Money::from_minor_units(-450, eur())),
            Split::new(groceries, Money::from_minor_units(450, eur())),
        ];
        let transaction = Transaction::new(
            kassensturz_core::TransactionId::generate(),
            day(9, date),
            Some("Corner shop".to_owned()),
            splits,
        )
        .expect("balanced");
        book.add_transaction(&transaction).expect("added");
    }
    let form = RuleForm {
        name: "Grocer".into(),
        match_all: true,
        conditions: vec![ConditionForm {
            kind: "text".into(),
            field: "other-party-name".into(),
            operator: "contains".into(),
            value: "Grocer".into(),
            ..ConditionForm::default()
        }],
        book_to_kind: "category".into(),
        book_to: groceries.to_string(),
        ..RuleForm::default()
    };
    rules::add_rule(&mut book, &form, &TestLocale).expect("Rule added");
    Sample {
        book,
        current,
        groceries,
    }
}

fn update(book: &mut Book) -> Summary {
    let requests = requests(book, &TestLocale).expect("requests");
    let fetched = bank_connection::update_all(
        &mut connection(),
        requests,
        day(10, 9),
        &AtomicBool::new(false),
    )
    .expect("fetched");
    book_all(book, fetched, &TestLocale)
}

#[test]
fn an_update_books_at_once_gives_the_two_notices_and_undo_takes_it_back() {
    let Sample {
        mut book,
        current,
        groceries,
    } = sample();
    let before = book
        .transactions_between(day(1, 1), day(12, 31))
        .expect("read")
        .len();
    let summary = update(&mut book);
    println!("{summary:#?}");
    assert_eq!((summary.updated, summary.booked), (1, 3));
    assert_eq!(summary.accounts.len(), 1);
    assert_eq!(summary.accounts[0].state, "updated");
    assert_eq!(summary.accounts[0].account, current.to_string());
    let reasons: Vec<(&str, &str)> = summary
        .notices
        .iter()
        .map(|notice| (notice.payee.as_str(), notice.reason))
        .collect();
    assert_eq!(
        reasons,
        vec![
            ("Example Employer", "unsorted"),
            ("Example Grocer", "unusual"),
            ("Own savings", "unsorted"),
        ]
    );
    let grocer = &summary.notices[1];
    assert_eq!(grocer.category, "Groceries");
    assert_eq!(grocer.usual.exact, "4,50 €");
    assert_eq!(grocer.size.exact, "45,23 €");
    assert_eq!(grocer.account_name, "Current account");
    assert_eq!(grocer.date, "2026-10-02");
    // Booked, not waiting: in the book, the grocer sorted by the Rule.
    let after = book
        .transactions_between(day(1, 1), day(12, 31))
        .expect("read");
    assert_eq!(after.len(), before + 3);
    let booked = after
        .iter()
        .find(|transaction| transaction.id().to_string() == grocer.transaction)
        .expect("booked");
    assert!(
        booked
            .splits()
            .iter()
            .any(|split| split.account == Some(groceries))
    );
    // The account notes the fetch day, so the widget says when.
    let state = state(&book, &TestLocale).expect("state");
    assert_eq!(state.linked, 1);
    assert!(!state.never);
    assert_eq!(state.last_updated, "2026-10-09");
    // One history entry for the account: Undo takes the update back.
    book.undo().expect("undone");
    let undone = book
        .transactions_between(day(1, 1), day(12, 31))
        .expect("read");
    assert_eq!(undone.len(), before);
    assert!(state_of(&book).never);
}

fn state_of(book: &Book) -> State {
    state(book, &TestLocale).expect("state")
}

#[test]
fn a_second_update_leaves_what_the_book_has_out() {
    let Sample { mut book, .. } = sample();
    let first = update(&mut book);
    assert_eq!(first.booked, 3);
    let second = update(&mut book);
    assert_eq!((second.updated, second.booked), (1, 0));
    assert!(second.notices.is_empty());
}

#[test]
fn only_linked_open_accounts_are_updated_and_the_widget_knows_when_there_are_none() {
    let mut book = Book::create_in_memory().expect("book");
    assert_eq!(
        state_of(&book),
        State {
            linked: 0,
            last_updated: String::new(),
            never: false,
        }
    );
    let wallet = account(&mut book, "Wallet", AccountKind::Asset);
    let current = account(&mut book, "Current account", AccountKind::Asset);
    link(&mut book, current);
    let found = requests(&book, &TestLocale).expect("requests");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].account, current);
    assert_ne!(found[0].account, wallet);
    assert_eq!(
        state_of(&book),
        State {
            linked: 1,
            last_updated: String::new(),
            never: true,
        }
    );
}

#[test]
fn failed_and_skipped_accounts_get_their_line_and_nothing_is_booked_for_them() {
    let Sample {
        mut book, current, ..
    } = sample();
    let fetched = vec![
        UpdateFetch {
            account: current,
            name: "Current account".into(),
            day: day(10, 9),
            result: Updated::Failed(ErrorText::plain("bank.unreachable")),
        },
        UpdateFetch {
            account: current,
            name: "Current account".into(),
            day: day(10, 9),
            result: Updated::Skipped("cancelled"),
        },
    ];
    let summary = book_all(&mut book, fetched, &TestLocale);
    assert_eq!((summary.updated, summary.booked), (0, 0));
    let lines: Vec<_> = summary
        .accounts
        .iter()
        .map(|line| {
            (
                line.state,
                line.skipped,
                line.error.as_ref().map(|e| e.code),
            )
        })
        .collect();
    assert_eq!(
        lines,
        vec![
            ("failed", "", Some("bank.unreachable")),
            ("skipped", "cancelled", None),
        ]
    );
}
