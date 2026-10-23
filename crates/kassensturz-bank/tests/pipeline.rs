// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where this crate meets the import pipeline (AGENTS.md §9.4, threat T-11):
//! the fake bank's statements, exactly as [`kassensturz_bank::statement`]
//! builds them, go through `import_statements` into a book in memory. No
//! value here is built by hand; the expectations were read from the
//! pipeline's real output first (M3 retro R3).

use kassensturz_bank::{
    Bank, BankAccountId, Failure, FakeBank, LoginId, MemoryPinStore, Pin, PinMode, PinSession,
    statement,
};
use kassensturz_core::{Account, AccountId, AccountKind, CurrencyCode, Date, Money};
use kassensturz_import::{EntryStatus, Limits, Statement};
use kassensturz_pipeline::{Choices, Duplicate, EntryOutcome, import_statements, preview_import};
use kassensturz_store::Book;

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};

fn eur(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, EUR)
}

fn october(day: i8) -> Date {
    Date::new(2026, 10, day).expect("a valid date")
}

struct Setup {
    bank: FakeBank,
    pins: PinSession,
    current: BankAccountId,
    card: BankAccountId,
    book: Book,
    checking: AccountId,
    credit_card: AccountId,
}

fn add_account(book: &mut Book, name: &str, kind: AccountKind, number: Option<&str>) -> AccountId {
    let id = AccountId::generate();
    book.add_account(&Account {
        id,
        name: name.into(),
        kind,
        currency: EUR,
        account_type: None,
        parent: None,
        closed: false,
        number: number.map(str::to_owned),
        notes: None,
        institution: None,
    })
    .expect("account added");
    id
}

/// A fake bank with one login, and a book with an account for each of its
/// bank accounts.
fn setup() -> Setup {
    let mut bank = FakeBank::new();
    let mut pins = PinSession::new(Box::new(MemoryPinStore::new()));
    pins.provide(
        &LoginId::new("fake-login-1"),
        Pin::new(FakeBank::PIN.to_owned()),
        PinMode::RememberUntilClose,
    )
    .expect("provided");
    let login = bank
        .add_login(&FakeBank::setup(), &mut pins)
        .expect("login added");
    let accounts = bank.accounts(&login).expect("accounts");
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    let checking = add_account(
        &mut book,
        "Checking account",
        AccountKind::Asset,
        accounts[0].iban.as_deref(),
    );
    let credit_card = add_account(&mut book, "Credit card", AccountKind::Liability, None);
    Setup {
        current: accounts[0].id.clone(),
        card: accounts[1].id.clone(),
        bank,
        pins,
        book,
        checking,
        credit_card,
    }
}

impl Setup {
    /// Fetches `account` and converts it, as the fetch screen will (M4.7).
    fn fetch(&mut self, account: &BankAccountId, since: Option<Date>) -> Statement {
        let fetched = self
            .bank
            .fetch(account, since, &mut self.pins)
            .expect("fetched");
        statement(&fetched, &Limits::default()).expect("a statement")
    }
}

#[test]
fn the_fake_banks_statement_is_imported_like_a_file() {
    let mut setup = setup();
    let statement = setup.fetch(&setup.current.clone(), None);
    assert_eq!(statement.entries.len(), 4);
    let report = import_statements(
        &mut setup.book,
        setup.checking,
        std::slice::from_ref(&statement),
        &Choices::default(),
        None,
    )
    .expect("imported");
    let summary = report.summary();
    assert_eq!(
        (
            summary.imported,
            summary.to_decide,
            summary.duplicates,
            summary.pending
        ),
        (3, 0, 0, 1)
    );
    // The pending card payment is the one left out (D-030).
    let pending: Vec<usize> = report
        .entries
        .iter()
        .filter(|result| result.outcome == EntryOutcome::Pending)
        .map(|result| result.entry.entry)
        .collect();
    assert_eq!(pending, vec![3]);
    assert_eq!(statement.entries[3].status, EntryStatus::Pending);
    // The bank's balance is checked against the book's (D-029), and they
    // agree once the starting balance is there: here the account started
    // empty, so the book is short by what was in it before October.
    assert_eq!(report.balances.len(), 1);
    let balance = report.balances[0];
    assert_eq!(balance.date, october(9));
    assert_eq!(balance.bank, eur(325_477));
    assert_eq!(balance.book, eur(225_477));
    assert!(!balance.matches());
    // The bank's texts arrived as the bank sent them.
    let imported = report.imported();
    assert_eq!(imported.len(), 3);
    let salary = setup
        .book
        .transaction(imported[0])
        .expect("the salary transaction");
    assert_eq!(salary.booking_date(), october(1));
    assert_eq!(salary.payee(), Some("Example Employer"));
    assert_eq!(
        setup
            .book
            .balance_on(setup.checking, october(9))
            .expect("a balance"),
        eur(225_477)
    );
}

#[test]
fn fetching_again_imports_nothing_twice() {
    let mut setup = setup();
    let first = setup.fetch(&setup.current.clone(), None);
    import_statements(
        &mut setup.book,
        setup.checking,
        &[first],
        &Choices::default(),
        None,
    )
    .expect("imported");
    // The next fetch from the last fetch day on (M4.4a keeps that day).
    let again = setup.fetch(&setup.current.clone(), Some(october(5)));
    assert_eq!(again.entries.len(), 2);
    let report = preview_import(
        &mut setup.book,
        setup.checking,
        &[again],
        &Choices::default(),
    )
    .expect("previewed");
    let outcomes: Vec<&EntryOutcome> = report
        .entries
        .iter()
        .map(|result| &result.outcome)
        .collect();
    assert!(
        matches!(outcomes[0], EntryOutcome::Duplicate(Duplicate::InBook(_))),
        "{outcomes:?}"
    );
    assert_eq!(outcomes[1], &EntryOutcome::Pending);
    assert_eq!(report.summary().imported, 0);
}

#[test]
fn a_credit_cards_statement_goes_into_a_liability_account() {
    let mut setup = setup();
    let statement = setup.fetch(&setup.card.clone(), None);
    assert_eq!(statement.account.iban, None);
    assert_eq!(
        statement.account.number.as_deref(),
        Some("4000 0000 0000 0001")
    );
    let report = import_statements(
        &mut setup.book,
        setup.credit_card,
        &[statement],
        &Choices::default(),
        None,
    )
    .expect("imported");
    assert_eq!(report.summary().imported, 2);
    // Both charges are debt, so the book owes 123.45 less than the bank
    // says: the card's debt from before October isn't in the book.
    let balance = report.balances[0];
    assert_eq!(balance.bank, eur(-12_345));
    assert_eq!(balance.book, eur(-12_345));
    assert!(balance.matches());
}

#[test]
fn broken_data_from_the_bank_imports_nothing() {
    let mut setup = setup();
    setup.bank.fail_next(Failure::InexactAmount);
    let fetched = setup
        .bank
        .fetch(&setup.current.clone(), None, &mut setup.pins)
        .expect("fetched");
    let error = statement(&fetched, &Limits::default()).expect_err("refused");
    assert_eq!(
        error,
        kassensturz_bank::BankError::InexactAmount {
            amount: "1/3".to_owned()
        }
    );
    assert_eq!(
        setup
            .book
            .balance_on(setup.checking, october(9))
            .expect("a balance"),
        eur(0)
    );
}
