// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Another program's book into a new book (M2.9), built in code, so it
//! waits for no file format. All data is made up (AGENTS.md §3.1 rule 2).

use kassensturz_core::{
    AccountKind, AccountType, Currency, CurrencyCode, Date, ExchangeRate, ImportedDetails, Money,
};
use kassensturz_import::{
    BookAccount, BookPart, BookRate, BookTransaction, ImportError, ImportedBook, Skipped,
    SkippedData,
};
use kassensturz_pipeline::{PipelineError, fill_book_from};
use kassensturz_store::{Book, StoreError};

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};
const USD: CurrencyCode = match CurrencyCode::new("USD") {
    Ok(code) => code,
    Err(_) => panic!("USD is a valid code"),
};

fn currency(code: CurrencyCode) -> Currency {
    Currency::new(code, 2).expect("two decimal places")
}

fn eur(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, EUR)
}

fn usd(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, USD)
}

fn day(day: i8) -> Date {
    Date::new(2026, 9, day).expect("a valid date")
}

fn account(key: &str, name: &str, kind: AccountKind) -> BookAccount {
    BookAccount {
        key: key.into(),
        name: name.into(),
        kind,
        account_type: None,
        currency: currency(EUR),
        parent: None,
        closed: false,
    }
}

fn part(account: Option<&str>, amount: Money) -> BookPart {
    BookPart {
        account: account.map(str::to_owned),
        amount,
        account_amount: None,
        details: None,
    }
}

fn transaction(date: Date, payee: Option<&str>, parts: Vec<BookPart>) -> BookTransaction {
    BookTransaction {
        date,
        payee: payee.map(str::to_owned),
        currency: currency(EUR),
        parts,
    }
}

/// A small household as another program kept it: two accounts (one
/// closed and empty, one closed with money still in it), a category tree,
/// a starting balance, a purchase, a transfer, an entry imported from a
/// bank without a category yet, a payment in dollars and a rate.
fn household() -> ImportedBook {
    let mut checking = account("A1", "Checking", AccountKind::Asset);
    checking.account_type = Some(AccountType::Checking);
    let mut old = account("A2", "Old savings", AccountKind::Asset);
    old.closed = true;
    let mut card = account("A3", "Card", AccountKind::Liability);
    card.account_type = Some(AccountType::CreditCard);
    card.closed = true;
    let mut groceries = account("E2", "Groceries", AccountKind::Expense);
    groceries.parent = Some("E1".into());
    let mut travel = account("E3", "Travel", AccountKind::Expense);
    travel.currency = currency(USD);
    travel.parent = Some("E1".into());
    let accounts = vec![
        checking,
        old,
        card,
        account("Q1", "Opening balances", AccountKind::Equity),
        account("E1", "Food", AccountKind::Expense),
        groceries,
        travel,
        account("I1", "Salary", AccountKind::Income),
        // Taken already by an account, ignoring capitals.
        account("A4", "checking ", AccountKind::Asset),
    ];
    let bank = ImportedDetails {
        bank_id: Some("REF-0001".into()),
        remittance_information: Some("Card payment\nCorner Grocery".into()),
        ..ImportedDetails::default()
    };
    let transactions = vec![
        transaction(
            day(1),
            None,
            vec![
                part(Some("A1"), eur(150_000)),
                part(Some("Q1"), eur(-150_000)),
            ],
        ),
        transaction(
            day(2),
            Some("Corner Grocery"),
            vec![
                BookPart {
                    details: Some(bank),
                    ..part(Some("A1"), eur(-5_320))
                },
                part(Some("E2"), eur(5_320)),
            ],
        ),
        // A transfer that empties the old savings account.
        transaction(
            day(3),
            None,
            vec![
                part(Some("A2"), eur(20_000)),
                part(Some("A2"), eur(-20_000)),
            ],
        ),
        // Money onto the card, which stays owed.
        transaction(
            day(4),
            None,
            vec![part(Some("A3"), eur(-1_000)), part(Some("A1"), eur(1_000))],
        ),
        // Only the account's part: the other program had no category yet.
        transaction(
            day(5),
            Some("Payroll"),
            vec![part(Some("A1"), eur(250_000))],
        ),
        // A hotel bill in dollars, paid in euros.
        transaction(
            day(6),
            Some("Hotel"),
            vec![
                part(Some("A1"), eur(-9_250)),
                BookPart {
                    account_amount: Some(usd(10_000)),
                    ..part(Some("E3"), eur(9_250))
                },
            ],
        ),
    ];
    ImportedBook {
        accounts,
        transactions,
        rates: vec![BookRate {
            date: day(6),
            from: EUR,
            to: USD,
            rate: "1.0811".parse::<ExchangeRate>().expect("a rate"),
        }],
        skipped: vec![Skipped {
            what: SkippedData::Schedules,
            count: 2,
        }],
    }
}

fn named(book: &Book, name: &str) -> kassensturz_core::Account {
    book.all_accounts()
        .expect("accounts")
        .into_iter()
        .find(|account| account.name == name)
        .unwrap_or_else(|| panic!("{name} is in the book"))
}

#[test]
fn a_book_comes_in_with_its_accounts_categories_and_transactions() {
    let mut book = Book::create_in_memory().expect("book created");
    let report = fill_book_from(&mut book, &household()).expect("filled");
    assert_eq!(report.accounts, 4);
    assert_eq!(report.categories, 4);
    assert_eq!(report.transactions, 6);
    assert_eq!(report.rates, 1);
    // Only the payroll entry didn't add up.
    assert_eq!(report.completed, 1);
    assert_eq!(
        report.skipped,
        vec![Skipped {
            what: SkippedData::Schedules,
            count: 2,
        }]
    );

    let checking = named(&book, "Checking");
    assert_eq!(checking.account_type, Some(AccountType::Checking));
    assert_eq!(
        book.balance(checking.id).expect("balance"),
        eur(150_000 - 5_320 + 1_000 + 250_000 - 9_250)
    );
    // The second "checking" got a number, and the report says so.
    let second = named(&book, "checking (2)");
    assert_eq!(report.renamed, vec![(second.id, "checking ".to_owned())]);

    // The tree came along, the dollar category inside the euro one moved
    // to the top.
    let food = named(&book, "Food");
    assert_eq!(named(&book, "Groceries").parent, Some(food.id));
    let travel = named(&book, "Travel");
    assert_eq!(travel.parent, None);
    assert_eq!(report.moved_to_top, vec![travel.id]);
    assert_eq!(book.balance(travel.id).expect("balance"), usd(10_000));

    // The empty account is closed; the card, still owed, is open.
    assert!(named(&book, "Old savings").closed);
    let card = named(&book, "Card");
    assert!(!card.closed);
    assert_eq!(report.left_open, vec![card.id]);

    // What the file delivered stays with the account's part.
    let transactions = book.transactions().expect("transactions");
    let purchase = transactions
        .iter()
        .find(|transaction| transaction.payee() == Some("Corner Grocery"))
        .expect("the purchase");
    let delivered = purchase.splits()[0].imported.as_ref().expect("details");
    assert_eq!(delivered.bank_id.as_deref(), Some("REF-0001"));
    // The payroll entry got a part without a category.
    let payroll = transactions
        .iter()
        .find(|transaction| transaction.payee() == Some("Payroll"))
        .expect("the payroll entry");
    assert_eq!(payroll.splits().len(), 2);
    assert_eq!(payroll.splits()[1].account, None);
    assert_eq!(payroll.splits()[1].amount, eur(-250_000));

    assert_eq!(book.all_rates().expect("rates").len(), 1);
    // Nothing to undo: the book was made this way.
    assert_eq!(book.history().expect("history"), Vec::new());
}

#[test]
fn a_book_that_cant_be_filled_stays_empty() {
    // A part booked to a key no account has.
    let mut broken = household();
    broken.transactions[1].parts[1].account = Some("E9".into());
    let mut book = Book::create_in_memory().expect("book created");
    assert!(matches!(
        fill_book_from(&mut book, &broken),
        Err(PipelineError::Import(ImportError::Malformed { .. }))
    ));
    assert!(book.all_accounts().expect("accounts").is_empty());

    // A name the book refuses.
    let mut unnamed = household();
    unnamed.accounts[0].name = "  ".into();
    assert!(matches!(
        fill_book_from(&mut book, &unnamed),
        Err(PipelineError::Store(StoreError::EmptyAccountName))
    ));
    assert!(book.all_accounts().expect("accounts").is_empty());

    // Only a new, empty book is filled.
    fill_book_from(&mut book, &household()).expect("filled");
    assert!(matches!(
        fill_book_from(&mut book, &household()),
        Err(PipelineError::Store(StoreError::NotEmpty))
    ));
}
