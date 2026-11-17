// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! An import the user cancels (B-362). A test program of its own, since
//! the cancel holds for the whole program. All data is made up
//! (AGENTS.md §3.1 rule 2).

use kassensturz_core::{Account, AccountId, AccountKind, Currency, CurrencyCode, Date, Money};
use kassensturz_import::{Entry, ImportError, Statement, StatementAccount};
use kassensturz_pipeline::{Choices, PipelineError, import_statements, stop};
use kassensturz_store::Book;

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};

/// The flags hold for the whole program: one test at a time.
static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn one_at_a_time() -> std::sync::MutexGuard<'static, ()> {
    ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn sample() -> (Book, AccountId, Statement) {
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
    (book, checking, statement)
}

#[test]
fn a_cancelled_import_writes_nothing_and_the_next_one_runs() {
    let _one = one_at_a_time();
    let (mut book, checking, statement) = sample();
    stop::start();
    stop::cancel();
    let cancelled = import_statements(
        &mut book,
        checking,
        std::slice::from_ref(&statement),
        &Choices::default(),
        None,
    );
    assert!(matches!(
        cancelled,
        Err(PipelineError::Import(ImportError::Cancelled))
    ));
    assert!(book.register(checking).expect("register").is_empty());

    // The next import starts afresh, and its progress counts every entry.
    stop::start();
    assert_eq!(stop::progress(), (0, 0));
    import_statements(&mut book, checking, &[statement], &Choices::default(), None)
        .expect("imported");
    assert_eq!(book.register(checking).expect("register").len(), 20);
    assert_eq!(stop::progress(), (20, 20));
}

/// Once an import saves, a cancel comes too late and says so (B-367): the
/// app hides Cancel then, and the import is saved whole.
#[test]
fn a_cancel_once_the_import_saves_comes_too_late() {
    let _one = one_at_a_time();
    let (mut book, checking, statement) = sample();
    stop::start();
    assert!(!stop::saving());
    import_statements(&mut book, checking, &[statement], &Choices::default(), None)
        .expect("imported");
    assert!(stop::saving());
    assert!(!stop::cancel());
    assert_eq!(book.register(checking).expect("register").len(), 20);

    // The next import can be cancelled again.
    stop::start();
    assert!(!stop::saving());
    assert!(stop::cancel());
    assert!(stop::cancel());
}
