// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests on books in memory. All data is made up (AGENTS.md §3.1 rule 2).

use super::*;
use kassensturz_core::TransactionError;

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};
const USD: CurrencyCode = match CurrencyCode::new("USD") {
    Ok(code) => code,
    Err(_) => panic!("USD is a valid code"),
};

fn eur(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, EUR)
}

fn date(year: i16, month: i8, day: i8) -> Date {
    Date::new(year, month, day).expect("valid date")
}

fn account(book: &mut Book, name: &str, kind: AccountKind, currency: CurrencyCode) -> AccountId {
    let id = AccountId::generate();
    let account = Account {
        id,
        name: name.into(),
        kind,
        currency,
    };
    book.add_account(&account).expect("account added");
    id
}

fn transaction(booking_date: Date, payee: &str, parts: &[(AccountId, Money)]) -> Transaction {
    let splits = parts
        .iter()
        .map(|&(account, amount)| Split {
            id: SplitId::generate(),
            account,
            amount,
        })
        .collect();
    Transaction::new(
        TransactionId::generate(),
        booking_date,
        Some(payee.into()),
        splits,
    )
    .expect("balanced")
}

struct Sample {
    book: Book,
    checking: AccountId,
    groceries: AccountId,
    salary: AccountId,
}

fn sample() -> Sample {
    let mut book = Book::create_in_memory().expect("book created");
    book.add_currency(EUR, 2).expect("currency added");
    let checking = account(&mut book, "Checking account", AccountKind::Asset, EUR);
    let groceries = account(&mut book, "Groceries", AccountKind::Expense, EUR);
    let salary = account(&mut book, "Salary", AccountKind::Income, EUR);
    Sample {
        book,
        checking,
        groceries,
        salary,
    }
}

#[test]
fn register_lists_an_accounts_transactions_oldest_first() {
    let Sample {
        mut book,
        checking,
        groceries,
        salary,
    } = sample();
    let shopping = transaction(
        date(2026, 10, 5),
        "Corner Grocery",
        &[(checking, eur(-5320)), (groceries, eur(5320))],
    );
    let pay = transaction(
        date(2026, 10, 1),
        "Example Employer",
        &[(checking, eur(240_000)), (salary, eur(-240_000))],
    );
    book.add_transaction(&shopping).expect("added");
    book.add_transaction(&pay).expect("added");

    let register = book.register(checking).expect("register");
    assert_eq!(
        register,
        vec![
            RegisterEntry {
                transaction: pay.id(),
                booking_date: date(2026, 10, 1),
                payee: Some("Example Employer".into()),
                amount: eur(240_000),
            },
            RegisterEntry {
                transaction: shopping.id(),
                booking_date: date(2026, 10, 5),
                payee: Some("Corner Grocery".into()),
                amount: eur(-5320),
            },
        ]
    );
    assert_eq!(book.balance(checking).expect("balance"), eur(234_680));
    assert_eq!(book.balance(groceries).expect("balance"), eur(5320));
}

#[test]
fn transactions_come_back_unchanged() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let household = account(&mut book, "Household", AccountKind::Expense, EUR);
    let shopping = transaction(
        date(2026, 10, 5),
        "Corner Grocery",
        &[
            (checking, eur(-5320)),
            (groceries, eur(4520)),
            (household, eur(800)),
        ],
    );
    book.add_transaction(&shopping).expect("added");
    assert_eq!(
        book.transaction(shopping.id()).expect("read back"),
        shopping
    );
}

#[test]
fn an_account_in_several_parts_appears_once_in_its_register() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let shopping = transaction(
        date(2026, 10, 5),
        "Corner Grocery",
        &[
            (checking, eur(-5000)),
            (checking, eur(-320)),
            (groceries, eur(5320)),
        ],
    );
    book.add_transaction(&shopping).expect("added");
    let register = book.register(checking).expect("register");
    assert_eq!(register.len(), 1);
    assert_eq!(register[0].amount, eur(-5320));
}

#[test]
fn a_currencys_decimal_places_stay_fixed() {
    let mut book = Book::create_in_memory().expect("book created");
    book.add_currency(EUR, 2).expect("first use");
    book.add_currency(EUR, 2)
        .expect("same decimal places again");
    assert!(matches!(
        book.add_currency(EUR, 3),
        Err(StoreError::CurrencyDigitsConflict {
            stored: 2,
            requested: 3,
            ..
        })
    ));
    assert_eq!(book.currency_minor_digits(EUR).expect("stored"), 2);
}

#[test]
fn a_currency_has_at_most_18_decimal_places() {
    let mut book = Book::create_in_memory().expect("book created");
    assert!(matches!(
        book.add_currency(EUR, 19),
        Err(StoreError::InvalidMinorDigits(19))
    ));
}

#[test]
fn accounts_need_a_currency_in_use() {
    let mut book = Book::create_in_memory().expect("book created");
    let account = Account {
        id: AccountId::generate(),
        name: "Checking account".into(),
        kind: AccountKind::Asset,
        currency: EUR,
    };
    assert!(matches!(
        book.add_account(&account),
        Err(StoreError::UnknownCurrency(EUR))
    ));
}

#[test]
fn parts_must_be_booked_to_existing_accounts() {
    let Sample {
        mut book, checking, ..
    } = sample();
    let unknown = AccountId::generate();
    let shopping = transaction(
        date(2026, 10, 5),
        "Corner Grocery",
        &[(checking, eur(-5320)), (unknown, eur(5320))],
    );
    assert!(matches!(
        book.add_transaction(&shopping),
        Err(StoreError::UnknownAccount(id)) if id == unknown
    ));
    assert!(book.register(checking).expect("register").is_empty());
}

#[test]
fn parts_must_be_in_their_accounts_currency() {
    let Sample {
        mut book, checking, ..
    } = sample();
    book.add_currency(USD, 2).expect("currency added");
    let travel = account(&mut book, "Travel", AccountKind::Expense, USD);
    let hotel = transaction(
        date(2026, 10, 7),
        "Example Hotel",
        &[(checking, eur(-9250)), (travel, eur(9250))],
    );
    assert!(matches!(
        book.add_transaction(&hotel),
        Err(StoreError::SplitCurrencyMismatch { account, .. }) if account == travel
    ));
}

#[test]
fn dates_before_the_year_1_are_refused() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let ancient = transaction(
        date(0, 12, 31),
        "Corner Grocery",
        &[(checking, eur(-100)), (groceries, eur(100))],
    );
    assert!(matches!(
        book.add_transaction(&ancient),
        Err(StoreError::DateOutOfRange(_))
    ));
}

#[test]
fn a_balance_too_large_for_64_bits_is_reported() {
    let Sample {
        mut book,
        checking,
        salary,
        ..
    } = sample();
    for _ in 0..2 {
        let huge = transaction(
            date(2026, 10, 1),
            "Example Employer",
            &[(checking, eur(i64::MAX)), (salary, eur(-i64::MAX))],
        );
        book.add_transaction(&huge).expect("each one balances");
    }
    assert!(matches!(
        book.balance(checking),
        Err(StoreError::AmountOverflow)
    ));
}

#[test]
fn an_unbalanced_transaction_in_the_file_is_reported() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let shopping = transaction(
        date(2026, 10, 5),
        "Corner Grocery",
        &[(checking, eur(-5320)), (groceries, eur(5320))],
    );
    book.add_transaction(&shopping).expect("added");
    // As if another program had changed the book (D-046).
    book.conn
        .execute("UPDATE splits SET amount = 5300 WHERE amount = 5320", [])
        .expect("changed behind KassenSturz's back");
    assert!(matches!(
        book.transaction(shopping.id()),
        Err(StoreError::Corrupt(Corruption::InvalidTransaction {
            error: TransactionError::Unbalanced { .. },
            ..
        }))
    ));
}

#[test]
fn the_book_refuses_floating_point_amounts() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let shopping = transaction(
        date(2026, 10, 5),
        "Corner Grocery",
        &[(checking, eur(-5320)), (groceries, eur(5320))],
    );
    book.add_transaction(&shopping).expect("added");
    // STRICT tables reject a REAL amount instead of storing it (AGENTS.md
    // §3.1 rule 1). Everything else about this part is valid.
    let result = book.conn.execute(
        "INSERT INTO splits (id, transaction_id, position, account_id, amount)
         VALUES (randomblob(16), ?1, 2, ?2, 53.2)",
        params![shopping.id().to_bytes(), checking.to_bytes()],
    );
    let Err(rusqlite::Error::SqliteFailure(error, _)) = result else {
        panic!("a REAL amount was accepted: {result:?}");
    };
    // SQLITE_CONSTRAINT_DATATYPE (sqlite.org/rescode.html), spelled out
    // because rusqlite's bindings for a system SQLite may predate it.
    const SQLITE_CONSTRAINT_DATATYPE: i32 = 3091;
    assert_eq!(error.extended_code, SQLITE_CONSTRAINT_DATATYPE);
}
