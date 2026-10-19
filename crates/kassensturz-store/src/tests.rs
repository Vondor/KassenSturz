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
    book.use_currency(EUR).expect("currency used");
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
fn a_currency_gets_its_decimal_places_from_the_iso_4217_table() {
    let mut book = Book::create_in_memory().expect("book created");
    let yen = CurrencyCode::new("JPY").expect("valid code");
    let dinar = CurrencyCode::new("BHD").expect("valid code");
    assert_eq!(book.use_currency(EUR).expect("first use"), 2);
    assert_eq!(book.use_currency(yen).expect("first use"), 0);
    assert_eq!(book.use_currency(dinar).expect("first use"), 3);
    assert_eq!(book.use_currency(EUR).expect("used again"), 2);
    assert_eq!(book.currency_minor_digits(yen).expect("stored"), 0);
}

#[test]
fn a_currencys_decimal_places_stay_fixed() {
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
    // As if the book had started using the euro with a table that said 3.
    book.conn
        .execute(
            "UPDATE currencies SET minor_digits = 3 WHERE code = 'EUR'",
            [],
        )
        .expect("decimal places changed");
    // The stored decimal places count, not today's table, and the stored
    // amounts are never rescaled (D-027).
    assert_eq!(book.use_currency(EUR).expect("used again"), 3);
    assert_eq!(book.currency_minor_digits(EUR).expect("stored"), 3);
    assert_eq!(
        book.transaction(shopping.id()).expect("read back"),
        shopping
    );
}

#[test]
fn currencies_the_table_does_not_list_are_refused() {
    let mut book = Book::create_in_memory().expect("book created");
    // Gold has no decimal places in ISO 4217; the mark ended in 2002.
    for text in ["XAU", "DEM"] {
        let currency = CurrencyCode::new(text).expect("valid code");
        assert!(matches!(
            book.use_currency(currency),
            Err(StoreError::UnlistedCurrency(found)) if found == currency
        ));
        assert!(matches!(
            book.currency_minor_digits(currency),
            Err(StoreError::UnknownCurrency(_))
        ));
    }
}

#[test]
fn new_books_round_half_away_from_zero() {
    let book = Book::create_in_memory().expect("book created");
    assert_eq!(
        book.rounding_mode().expect("rounding mode"),
        RoundingMode::HalfAwayFromZero
    );
}

#[test]
fn changing_the_rounding_mode_changes_no_stored_amount() {
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
    let register = book.register(checking).expect("register");
    for rounding in RoundingMode::ALL {
        book.set_rounding_mode(rounding).expect("rounding mode set");
        assert_eq!(book.rounding_mode().expect("rounding mode"), rounding);
        assert_eq!(book.register(checking).expect("register"), register);
        assert_eq!(book.balance(checking).expect("balance"), eur(-5320));
    }
}

#[test]
fn missing_or_unknown_settings_are_reported() {
    let mut book = Book::create_in_memory().expect("book created");
    // As if another program had changed the book (D-046).
    book.conn
        .execute("DELETE FROM settings", [])
        .expect("settings deleted");
    assert!(matches!(
        book.rounding_mode(),
        Err(StoreError::Corrupt(Corruption::MissingSettings))
    ));
    assert!(matches!(
        book.set_rounding_mode(RoundingMode::HalfEven),
        Err(StoreError::Corrupt(Corruption::MissingSettings))
    ));
    book.conn
        .execute_batch(
            "PRAGMA ignore_check_constraints = ON;
             INSERT INTO settings (id, rounding_mode) VALUES (1, 'bankers');",
        )
        .expect("unknown rounding mode written");
    assert!(matches!(
        book.rounding_mode(),
        Err(StoreError::Corrupt(Corruption::BadRoundingMode(text))) if text == "bankers"
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
    book.use_currency(USD).expect("currency used");
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

fn dated_rate(date: Date, from: CurrencyCode, to: CurrencyCode, rate: &str) -> DatedRate {
    DatedRate {
        id: DatedRateId::generate(),
        date,
        from,
        to,
        rate: rate.parse().expect("valid rate"),
        source: RateSource::Manual,
    }
}

/// A book using euros and US dollars.
fn two_currencies() -> Book {
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    book.use_currency(USD).expect("currency used");
    book
}

#[test]
fn rates_come_back_unchanged() {
    let mut book = two_currencies();
    // Trailing zeros are kept as typed.
    let rate = dated_rate(date(2026, 10, 5), EUR, USD, "1.0850");
    book.add_rate(&rate).expect("added");
    let stored = book.rate(rate.id).expect("found");
    assert_eq!(stored, rate);
    assert_eq!(stored.rate.to_string(), "1.0850");
    assert_eq!(book.rates(EUR, USD).expect("listed"), vec![rate]);
    // The other direction is a different list.
    assert_eq!(book.rates(USD, EUR).expect("listed"), vec![]);
}

#[test]
fn the_rate_on_a_date_is_the_newest_on_or_before_it() {
    let mut book = two_currencies();
    let early = dated_rate(date(2026, 10, 1), EUR, USD, "1.08");
    let late = dated_rate(date(2026, 10, 15), EUR, USD, "1.09");
    // Added out of order; listed by date.
    book.add_rate(&late).expect("added");
    book.add_rate(&early).expect("added");
    assert_eq!(book.rates(EUR, USD).expect("listed"), vec![early, late]);
    for (day, expected) in [
        (date(2026, 9, 30), None),
        (date(2026, 10, 1), Some(early)),
        (date(2026, 10, 14), Some(early)),
        (date(2026, 10, 15), Some(late)),
        (date(2027, 1, 1), Some(late)),
    ] {
        assert_eq!(
            book.rate_on(EUR, USD, day).expect("looked up"),
            expected,
            "{day}"
        );
    }
    assert_eq!(
        book.rate_on(USD, EUR, date(2027, 1, 1)).expect("looked up"),
        None
    );
}

#[test]
fn a_rate_converts_amounts_in_the_books_currencies() {
    let mut book = two_currencies();
    // D-043's example: a 100 USD hotel bill charged as 92.50 EUR.
    let rate = dated_rate(date(2026, 10, 5), USD, EUR, "0.925");
    book.add_rate(&rate).expect("added");
    let found = book
        .rate_on(USD, EUR, date(2026, 10, 6))
        .expect("looked up")
        .expect("there is one");
    let dollars = book.currency(USD).expect("in use");
    let euros = book.currency(EUR).expect("in use");
    let bill = Money::from_minor_units(10_000, USD);
    let rounding = book.rounding_mode().expect("read");
    assert_eq!(
        bill.checked_mul_rate(dollars, found.rate, euros, rounding),
        Ok(eur(9250))
    );
    assert_eq!(
        eur(9250).checked_div_rate(euros, found.rate, dollars, rounding),
        Ok(bill)
    );
}

#[test]
fn one_rate_per_pair_direction_and_date() {
    let mut book = two_currencies();
    let day = date(2026, 10, 5);
    let first = dated_rate(day, EUR, USD, "1.08");
    book.add_rate(&first).expect("added");
    assert!(matches!(
        book.add_rate(&dated_rate(day, EUR, USD, "1.09")),
        Err(StoreError::DuplicateRate { existing }) if existing == first.id
    ));
    // The other direction and another day are fine.
    book.add_rate(&dated_rate(day, USD, EUR, "0.92"))
        .expect("added");
    book.add_rate(&dated_rate(date(2026, 10, 6), EUR, USD, "1.09"))
        .expect("added");
    // Removing the first one makes room for a corrected rate.
    book.remove_rate(first.id).expect("removed");
    assert!(matches!(
        book.rate(first.id),
        Err(StoreError::UnknownRate(id)) if id == first.id
    ));
    assert!(matches!(
        book.remove_rate(first.id),
        Err(StoreError::UnknownRate(id)) if id == first.id
    ));
    book.add_rate(&dated_rate(day, EUR, USD, "1.09"))
        .expect("added");
}

#[test]
fn rates_need_two_different_currencies_in_use() {
    let mut book = two_currencies();
    let day = date(2026, 10, 5);
    assert!(matches!(
        book.add_rate(&dated_rate(day, EUR, EUR, "1")),
        Err(StoreError::RateWithinOneCurrency(code)) if code == EUR
    ));
    let chf = CurrencyCode::new("CHF").expect("valid code");
    assert!(matches!(
        book.add_rate(&dated_rate(day, EUR, chf, "0.94")),
        Err(StoreError::UnknownCurrency(code)) if code == chf
    ));
    assert!(matches!(
        book.add_rate(&dated_rate(day, chf, EUR, "1.06")),
        Err(StoreError::UnknownCurrency(code)) if code == chf
    ));
    assert!(matches!(
        book.add_rate(&dated_rate(date(0, 1, 1), EUR, USD, "1.08")),
        Err(StoreError::DateOutOfRange(_))
    ));
    assert!(matches!(
        book.currency(chf),
        Err(StoreError::UnknownCurrency(code)) if code == chf
    ));
}

#[test]
fn bad_rates_in_the_file_are_reported() {
    let mut book = two_currencies();
    let rate = dated_rate(date(2026, 10, 5), EUR, USD, "1.08");
    book.add_rate(&rate).expect("added");
    // The table's checks refuse what KassenSturz couldn't read back.
    for (column, value) in [
        ("rate_digits", "0"),
        ("rate_digits", "-108"),
        ("rate_digits", "1000000000000000000"),
        ("rate_decimal_places", "19"),
        ("rate_digits", "1.08"),
        ("source", "'download'"),
        ("to_currency", "'EUR'"),
    ] {
        let result = book
            .conn
            .execute(&format!("UPDATE prices SET {column} = {value}"), []);
        assert!(result.is_err(), "{column} = {value} was accepted");
    }
    // Without them, e.g. in a file changed by another program (D-046), the
    // rate is reported, never returned.
    book.conn
        .execute_batch(
            "PRAGMA ignore_check_constraints = ON;
             UPDATE prices SET rate_decimal_places = 40;",
        )
        .expect("changed behind KassenSturz's back");
    assert!(matches!(
        book.rate(rate.id),
        Err(StoreError::Corrupt(Corruption::BadRate(id))) if id == rate.id
    ));
    book.conn
        .execute(
            "UPDATE prices SET rate_decimal_places = 2, source = 'download'",
            [],
        )
        .expect("changed behind KassenSturz's back");
    assert!(matches!(
        book.rates(EUR, USD),
        Err(StoreError::Corrupt(Corruption::BadRateSource(source))) if source == "download"
    ));
}
