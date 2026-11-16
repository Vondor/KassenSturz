// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A stop asked for during an import (B-362). A test program of its own,
//! since the request holds for the whole program. All data is made up
//! (AGENTS.md §3.1 rule 2).

use kassensturz_core::{Account, AccountId, AccountKind, Currency, CurrencyCode, Date, Money};
use kassensturz_import::{Entry, ImportError, Statement, StatementAccount};
use kassensturz_pipeline::{Choices, PipelineError, import_statements, stop};
use kassensturz_store::Book;

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};

#[test]
fn an_import_asked_to_stop_writes_nothing() {
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    let checking = AccountId::generate();
    book.add_account(&Account {
        id: checking,
        name: "Checking account".into(),
        kind: AccountKind::Asset,
        currency: EUR,
        account_type: None,
        parent: None,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    })
    .expect("account added");
    let entries = (1..=20)
        .map(|day| {
            let mut entry = Entry::booked(
                Date::new(2026, 10, day).expect("a date"),
                Money::from_minor_units(-100, EUR),
            );
            entry.bank_id = Some(format!("B{day}"));
            entry
        })
        .collect();
    let statement = Statement {
        account: StatementAccount::default(),
        currency: Currency::new(EUR, 2).expect("places"),
        opening_balance: None,
        closing_balance: None,
        entries,
    };
    let history = book.history().expect("history").len();

    stop::request();
    assert!(stop::requested());
    let stopped = import_statements(&mut book, checking, &[statement], &Choices::default(), None);
    assert!(matches!(
        stopped,
        Err(PipelineError::Import(ImportError::Stopped))
    ));
    // Nothing of it was written, and no step to undo was added.
    assert!(book.register(checking).expect("register").is_empty());
    assert_eq!(book.history().expect("history").len(), history);
}
