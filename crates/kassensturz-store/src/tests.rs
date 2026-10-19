// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests on books in memory. All data is made up (AGENTS.md §3.1 rule 2).

use super::*;
use kassensturz_core::{Money, Split, Transaction, TransactionError, TransactionId};

/// The book's database, for a test to change it as another program could
/// (D-046): past KassenSturz's checks and outside the change history, which
/// otherwise refuses every change outside a user action. The history records
/// again once it is dropped, at the end of the statement that uses it.
pub(crate) struct AnotherProgram<'a>(&'a Connection);

impl Book {
    pub(crate) fn as_another_program(&self) -> AnotherProgram<'_> {
        self.conn
            .execute("INSERT INTO temp.history_state (recording) VALUES (0)", [])
            .expect("recording paused");
        AnotherProgram(&self.conn)
    }
}

impl std::ops::Deref for AnotherProgram<'_> {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        self.0
    }
}

impl Drop for AnotherProgram<'_> {
    fn drop(&mut self) {
        self.0
            .execute("DELETE FROM temp.history_state", [])
            .expect("recording resumed");
    }
}

pub(crate) const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};
pub(crate) const USD: CurrencyCode = match CurrencyCode::new("USD") {
    Ok(code) => code,
    Err(_) => panic!("USD is a valid code"),
};

pub(crate) fn eur(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, EUR)
}

pub(crate) fn date(year: i16, month: i8, day: i8) -> Date {
    Date::new(year, month, day).expect("valid date")
}

pub(crate) fn account(
    book: &mut Book,
    name: &str,
    kind: AccountKind,
    currency: CurrencyCode,
) -> AccountId {
    let id = AccountId::generate();
    let account = Account {
        id,
        name: name.into(),
        kind,
        currency,
        account_type: None,
        closed: false,
        parent: None,
    };
    book.add_account(&account).expect("account added");
    id
}

pub(crate) fn transaction(
    booking_date: Date,
    payee: &str,
    parts: &[(AccountId, Money)],
) -> Transaction {
    let splits = parts
        .iter()
        .map(|&(account, amount)| Split::new(account, amount))
        .collect();
    Transaction::new(
        TransactionId::generate(),
        booking_date,
        Some(payee.into()),
        splits,
    )
    .expect("balanced")
}

pub(crate) struct Sample {
    pub(crate) book: Book,
    pub(crate) checking: AccountId,
    pub(crate) groceries: AccountId,
    pub(crate) salary: AccountId,
}

pub(crate) fn sample() -> Sample {
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
    book.as_another_program()
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
    book.as_another_program()
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
    book.as_another_program()
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
        account_type: None,
        closed: false,
        parent: None,
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
    book.as_another_program()
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
    let result = book.as_another_program().execute(
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
            .as_another_program()
            .execute(&format!("UPDATE prices SET {column} = {value}"), []);
        assert!(result.is_err(), "{column} = {value} was accepted");
    }
    // Without them, e.g. in a file changed by another program (D-046), the
    // rate is reported, never returned.
    book.as_another_program()
        .execute_batch(
            "PRAGMA ignore_check_constraints = ON;
             UPDATE prices SET rate_decimal_places = 40;",
        )
        .expect("changed behind KassenSturz's back");
    assert!(matches!(
        book.rate(rate.id),
        Err(StoreError::Corrupt(Corruption::BadRate(id))) if id == rate.id
    ));
    book.as_another_program()
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

fn typed_account(
    book: &mut Book,
    name: &str,
    account_type: AccountType,
) -> Result<AccountId, StoreError> {
    let id = AccountId::generate();
    book.add_account(&Account {
        id,
        name: name.into(),
        kind: account_type.kind(),
        currency: EUR,
        account_type: Some(account_type),
        closed: false,
        parent: None,
    })?;
    Ok(id)
}

#[test]
fn every_account_type_belongs_to_one_kind() {
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    for account_type in AccountType::ALL {
        let id = typed_account(&mut book, &format!("{account_type:?}"), account_type)
            .expect("account added");
        let stored = book.account(id).expect("account found");
        assert_eq!(stored.account_type, Some(account_type));
        assert_eq!(stored.kind, account_type.kind());
    }
    // A credit card is money the user owes; the others are money they have.
    assert_eq!(AccountType::CreditCard.kind(), AccountKind::Liability);
    assert_eq!(AccountType::Cash.kind(), AccountKind::Asset);
}

#[test]
fn an_accounts_type_must_fit_its_kind() {
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    for (kind, account_type) in [
        (AccountKind::Asset, AccountType::CreditCard),
        (AccountKind::Liability, AccountType::Checking),
        (AccountKind::Expense, AccountType::Cash),
        (AccountKind::Income, AccountType::Savings),
    ] {
        let result = book.add_account(&Account {
            id: AccountId::generate(),
            name: "Mismatch".into(),
            kind,
            currency: EUR,
            account_type: Some(account_type),
            closed: false,
            parent: None,
        });
        assert!(
            matches!(result, Err(StoreError::AccountTypeMismatch { .. })),
            "{kind:?} {account_type:?}"
        );
    }
    assert!(book.accounts(true).expect("accounts").is_empty());
}

#[test]
fn accounts_are_listed_by_name_without_categories_or_closed_ones() {
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    let visa = typed_account(&mut book, "visa", AccountType::CreditCard).expect("added");
    let cash = typed_account(&mut book, "Cash", AccountType::Cash).expect("added");
    let old = typed_account(&mut book, "Old savings", AccountType::Savings).expect("added");
    account(&mut book, "Groceries", AccountKind::Expense, EUR);
    account(&mut book, "Opening balances", AccountKind::Equity, EUR);
    book.close_account(old).expect("closed");

    let names = |include_closed| -> Vec<String> {
        book.accounts(include_closed)
            .expect("accounts")
            .into_iter()
            .map(|account| account.name)
            .collect()
    };
    assert_eq!(names(false), ["Cash", "visa"]);
    assert_eq!(names(true), ["Cash", "Old savings", "visa"]);
    let ids: Vec<_> = book
        .accounts(false)
        .expect("accounts")
        .into_iter()
        .map(|account| account.id)
        .collect();
    assert_eq!(ids, [cash, visa]);
}

#[test]
fn account_names_are_not_empty_and_not_shared() {
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    let checking = typed_account(&mut book, "Checking", AccountType::Checking).expect("added");
    for name in ["", "   ", "\t\n"] {
        assert!(matches!(
            typed_account(&mut book, name, AccountType::Cash),
            Err(StoreError::EmptyAccountName)
        ));
    }
    // Capitals and spaces around the name don't make another name, closed
    // accounts count, and the type doesn't matter.
    for name in ["checking", "CHECKING ", " Checking"] {
        assert!(
            matches!(
                typed_account(&mut book, name, AccountType::CreditCard),
                Err(StoreError::DuplicateAccountName { existing }) if existing == checking
            ),
            "{name:?}"
        );
    }
    book.close_account(checking).expect("closed");
    assert!(matches!(
        typed_account(&mut book, "Checking", AccountType::Checking),
        Err(StoreError::DuplicateAccountName { .. })
    ));
    // Categories can share names with accounts: they get their own rules
    // with the category tree (M1.5).
    account(&mut book, "Checking", AccountKind::Expense, EUR);
}

#[test]
fn renaming_follows_the_rules_for_names() {
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    let checking = typed_account(&mut book, "Checking", AccountType::Checking).expect("added");
    let savings = typed_account(&mut book, "Savings", AccountType::Savings).expect("added");

    book.rename_account(checking, "Household checking")
        .expect("renamed");
    assert_eq!(
        book.account(checking).expect("account").name,
        "Household checking"
    );
    // Only the capitals change: it isn't a clash with itself.
    book.rename_account(checking, "HOUSEHOLD CHECKING")
        .expect("renamed");
    assert!(matches!(
        book.rename_account(savings, "household checking"),
        Err(StoreError::DuplicateAccountName { existing }) if existing == checking
    ));
    assert!(matches!(
        book.rename_account(savings, " "),
        Err(StoreError::EmptyAccountName)
    ));
    assert!(matches!(
        book.rename_account(AccountId::generate(), "Nobody"),
        Err(StoreError::UnknownAccount(_))
    ));
    assert_eq!(book.account(savings).expect("account").name, "Savings");
}

#[test]
fn an_account_closes_only_when_empty_and_takes_no_transactions_then() {
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

    // Money is still in it, or owed from it; categories aren't accounts.
    assert!(matches!(
        book.close_account(checking),
        Err(StoreError::AccountNotEmpty { account, balance })
            if account == checking && balance == eur(-5320)
    ));
    assert!(matches!(
        book.close_account(groceries),
        Err(StoreError::NotAnAccount(id)) if id == groceries
    ));
    assert!(!book.account(checking).expect("account").closed);

    let paid_back = transaction(
        date(2026, 10, 6),
        "Refund",
        &[(checking, eur(5320)), (groceries, eur(-5320))],
    );
    book.add_transaction(&paid_back).expect("added");
    book.close_account(checking).expect("closed");
    book.close_account(checking).expect("closing twice is fine");
    assert!(book.account(checking).expect("account").closed);

    // The history stays; new transactions are refused until it's opened again.
    assert_eq!(book.register(checking).expect("register").len(), 2);
    let another = transaction(
        date(2026, 10, 7),
        "Corner Grocery",
        &[(checking, eur(-100)), (groceries, eur(100))],
    );
    assert!(matches!(
        book.add_transaction(&another),
        Err(StoreError::AccountClosed(id)) if id == checking
    ));
    assert!(matches!(
        book.transaction(another.id()),
        Err(StoreError::UnknownTransaction(_))
    ));
    book.reopen_account(checking).expect("opened again");
    book.reopen_account(checking)
        .expect("opening twice is fine");
    book.add_transaction(&another).expect("added");
}

#[test]
fn bad_account_types_in_the_file_are_reported() {
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    let checking = typed_account(&mut book, "Checking", AccountType::Checking).expect("added");
    // The table's checks refuse a type that doesn't exist or a closed flag
    // that isn't 0 or 1.
    for update in [
        "UPDATE accounts SET account_type = 'wallet'",
        "UPDATE accounts SET closed = 2",
        "UPDATE accounts SET closed = NULL",
    ] {
        assert!(
            book.as_another_program().execute(update, []).is_err(),
            "{update}"
        );
    }
    // Without them, e.g. in a file changed by another program (D-046), the
    // account is reported, never returned.
    book.as_another_program()
        .execute_batch(
            "PRAGMA ignore_check_constraints = ON;
             UPDATE accounts SET account_type = 'wallet';",
        )
        .expect("changed behind KassenSturz's back");
    assert!(matches!(
        book.account(checking),
        Err(StoreError::Corrupt(Corruption::BadAccountType(text))) if text == "wallet"
    ));
    book.as_another_program()
        .execute_batch("UPDATE accounts SET account_type = 'credit_card';")
        .expect("changed behind KassenSturz's back");
    assert!(matches!(
        book.accounts(true),
        Err(StoreError::Corrupt(Corruption::AccountTypeMismatch(id))) if id == checking
    ));
}

fn category(
    book: &mut Book,
    name: &str,
    kind: AccountKind,
    parent: Option<AccountId>,
) -> Result<AccountId, StoreError> {
    let id = AccountId::generate();
    book.add_account(&Account {
        id,
        name: name.into(),
        kind,
        currency: EUR,
        account_type: None,
        closed: false,
        parent,
    })?;
    Ok(id)
}

fn tree(book: &Book) -> Vec<String> {
    book.categories()
        .expect("categories")
        .into_iter()
        .map(|entry| entry.path.join(" > "))
        .collect()
}

fn book_in_euros() -> Book {
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    book
}

#[test]
fn categories_form_a_tree_of_any_depth() {
    let mut book = book_in_euros();
    let food = category(&mut book, "Food", AccountKind::Expense, None).expect("added");
    let groceries =
        category(&mut book, "groceries", AccountKind::Expense, Some(food)).expect("added");
    category(&mut book, "Restaurants", AccountKind::Expense, Some(food)).expect("added");
    let organic =
        category(&mut book, "Organic", AccountKind::Expense, Some(groceries)).expect("added");
    category(&mut book, "Baby food", AccountKind::Expense, Some(organic)).expect("added");
    category(&mut book, "Home", AccountKind::Expense, None).expect("added");
    let salary = category(&mut book, "Salary", AccountKind::Income, None).expect("added");
    category(&mut book, "Bonus", AccountKind::Income, Some(salary)).expect("added");
    // Accounts and internal accounts aren't categories.
    account(&mut book, "Checking account", AccountKind::Asset, EUR);
    account(&mut book, "Opening balances", AccountKind::Equity, EUR);

    // Income first, each category followed by what is inside it, siblings by
    // name.
    assert_eq!(
        tree(&book),
        [
            "Salary",
            "Salary > Bonus",
            "Food",
            "Food > groceries",
            "Food > groceries > Organic",
            "Food > groceries > Organic > Baby food",
            "Food > Restaurants",
            "Home",
        ]
    );
    assert_eq!(
        book.account(organic).expect("account").parent,
        Some(groceries)
    );
    assert_eq!(book.account(food).expect("account").parent, None);
}

#[test]
fn a_category_sits_inside_a_category_of_its_own_kind_and_currency() {
    let mut book = book_in_euros();
    book.use_currency(USD).expect("currency used");
    let food = category(&mut book, "Food", AccountKind::Expense, None).expect("added");
    let checking = account(&mut book, "Checking account", AccountKind::Asset, EUR);
    let opening = account(&mut book, "Opening balances", AccountKind::Equity, EUR);

    assert!(matches!(
        category(&mut book, "Salary", AccountKind::Income, Some(food)),
        Err(StoreError::ParentKindMismatch { parent, .. }) if parent == food
    ));
    for parent in [checking, opening] {
        assert!(matches!(
            category(&mut book, "Fees", AccountKind::Expense, Some(parent)),
            Err(StoreError::ParentNotACategory(id)) if id == parent
        ));
    }
    assert!(matches!(
        category(
            &mut book,
            "Fees",
            AccountKind::Expense,
            Some(AccountId::generate())
        ),
        Err(StoreError::UnknownAccount(_))
    ));
    let dollars = Account {
        id: AccountId::generate(),
        name: "Souvenirs".into(),
        kind: AccountKind::Expense,
        currency: USD,
        account_type: None,
        closed: false,
        parent: Some(food),
    };
    assert!(matches!(
        book.add_account(&dollars),
        Err(StoreError::ParentCurrencyMismatch { parent, parent_currency, currency })
            if parent == food && parent_currency == EUR && currency == USD
    ));
    // Only categories sit inside categories.
    for kind in [
        AccountKind::Asset,
        AccountKind::Liability,
        AccountKind::Equity,
    ] {
        let result = book.add_account(&Account {
            kind,
            name: "Inside".into(),
            currency: EUR,
            parent: Some(food),
            ..dollars.clone()
        });
        assert!(
            matches!(result, Err(StoreError::NotACategory(_))),
            "{kind:?}"
        );
    }
    assert_eq!(tree(&book), ["Food"]);
}

#[test]
fn siblings_do_not_share_a_name() {
    let mut book = book_in_euros();
    let food = category(&mut book, "Food", AccountKind::Expense, None).expect("added");
    let home = category(&mut book, "Home", AccountKind::Expense, None).expect("added");
    let other = category(&mut book, "Other", AccountKind::Expense, Some(food)).expect("added");

    // Capitals and spaces don't make another name.
    for name in ["other", " OTHER "] {
        assert!(matches!(
            category(&mut book, name, AccountKind::Expense, Some(food)),
            Err(StoreError::DuplicateAccountName { existing }) if existing == other
        ));
    }
    assert!(matches!(
        category(&mut book, "food", AccountKind::Expense, None),
        Err(StoreError::DuplicateAccountName { existing }) if existing == food
    ));
    // The same name elsewhere, or as income, is another category.
    category(&mut book, "Other", AccountKind::Expense, Some(home)).expect("added");
    category(&mut book, "Other", AccountKind::Expense, None).expect("added");
    category(&mut book, "Food", AccountKind::Income, None).expect("added");
    // Renaming follows the same rule.
    let treats = category(&mut book, "Treats", AccountKind::Expense, Some(food)).expect("added");
    assert!(matches!(
        book.rename_account(treats, "Other"),
        Err(StoreError::DuplicateAccountName { existing }) if existing == other
    ));
    book.rename_account(treats, "Sweets").expect("renamed");
}

#[test]
fn a_category_moves_with_what_is_inside_it() {
    let mut book = book_in_euros();
    let food = category(&mut book, "Food", AccountKind::Expense, None).expect("added");
    let home = category(&mut book, "Home", AccountKind::Expense, None).expect("added");
    let groceries =
        category(&mut book, "Groceries", AccountKind::Expense, Some(food)).expect("added");
    category(&mut book, "Organic", AccountKind::Expense, Some(groceries)).expect("added");

    book.move_category(groceries, Some(home)).expect("moved");
    assert_eq!(
        tree(&book),
        [
            "Food",
            "Home",
            "Home > Groceries",
            "Home > Groceries > Organic"
        ]
    );
    book.move_category(groceries, None)
        .expect("moved to the top");
    assert_eq!(
        tree(&book),
        ["Food", "Groceries", "Groceries > Organic", "Home"]
    );
    // Moving where it is already does nothing.
    book.move_category(groceries, None).expect("moved");
}

#[test]
fn a_category_cannot_move_inside_itself_or_where_it_does_not_fit() {
    let mut book = book_in_euros();
    let food = category(&mut book, "Food", AccountKind::Expense, None).expect("added");
    let groceries =
        category(&mut book, "Groceries", AccountKind::Expense, Some(food)).expect("added");
    let organic =
        category(&mut book, "Organic", AccountKind::Expense, Some(groceries)).expect("added");
    let salary = category(&mut book, "Salary", AccountKind::Income, None).expect("added");
    let checking = account(&mut book, "Checking account", AccountKind::Asset, EUR);
    category(&mut book, "Organic", AccountKind::Expense, None).expect("added");
    let before = tree(&book);

    for inside in [food, groceries, organic] {
        assert!(matches!(
            book.move_category(food, Some(inside)),
            Err(StoreError::CategoryInsideItself(id)) if id == food
        ));
    }
    assert!(matches!(
        book.move_category(organic, Some(salary)),
        Err(StoreError::ParentKindMismatch { .. })
    ));
    assert!(matches!(
        book.move_category(organic, Some(checking)),
        Err(StoreError::ParentNotACategory(_))
    ));
    // A top-level "Organic" exists already.
    assert!(matches!(
        book.move_category(organic, None),
        Err(StoreError::DuplicateAccountName { .. })
    ));
    assert!(matches!(
        book.move_category(checking, Some(food)),
        Err(StoreError::NotACategory(id)) if id == checking
    ));
    assert!(matches!(
        book.move_category(AccountId::generate(), None),
        Err(StoreError::UnknownAccount(_))
    ));
    assert_eq!(tree(&book), before);
}

#[test]
fn categories_keep_their_transactions_when_they_move() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let food = category(&mut book, "Food", AccountKind::Expense, None).expect("added");
    let shopping = transaction(
        date(2026, 10, 5),
        "Corner Grocery",
        &[(checking, eur(-5320)), (groceries, eur(5320))],
    );
    book.add_transaction(&shopping).expect("added");
    book.move_category(groceries, Some(food)).expect("moved");
    assert_eq!(book.balance(groceries).expect("balance"), eur(5320));
    assert_eq!(book.register(groceries).expect("register").len(), 1);
}

#[test]
fn a_category_tree_that_is_wrong_in_the_file_is_reported() {
    let mut book = book_in_euros();
    book.use_currency(USD).expect("currency used");
    let food = category(&mut book, "Food", AccountKind::Expense, None).expect("added");
    let groceries =
        category(&mut book, "Groceries", AccountKind::Expense, Some(food)).expect("added");
    let checking = account(&mut book, "Checking account", AccountKind::Asset, EUR);
    // The table's checks refuse a category inside itself, an account inside
    // a category and a parent that doesn't exist.
    for update in [
        "UPDATE accounts SET parent_id = id WHERE name = 'Food'",
        "UPDATE accounts SET parent_id = (SELECT id FROM accounts WHERE name = 'Food') WHERE name = 'Checking account'",
        "UPDATE accounts SET parent_id = zeroblob(16) WHERE name = 'Food'",
        "UPDATE accounts SET parent_id = x'00' WHERE name = 'Food'",
    ] {
        assert!(
            book.as_another_program().execute(update, []).is_err(),
            "{update}"
        );
    }
    assert_eq!(tree(&book), ["Food", "Food > Groceries"]);

    // Without the checks, e.g. in a file changed by another program (D-046),
    // a loop is reported, never followed...
    book.as_another_program()
        .execute(
            "UPDATE accounts SET parent_id = ?1 WHERE id = ?2",
            params![groceries.to_bytes(), food.to_bytes()],
        )
        .expect("loop made");
    assert!(matches!(
        book.categories(),
        Err(StoreError::Corrupt(Corruption::BadCategoryTree(_)))
    ));
    book.as_another_program()
        .execute(
            "UPDATE accounts SET parent_id = NULL WHERE id = ?1",
            [food.to_bytes()],
        )
        .expect("loop undone");
    assert_eq!(tree(&book), ["Food", "Food > Groceries"]);
    // ...and so is a category inside one of another kind or currency.
    book.as_another_program()
        .execute(
            "UPDATE accounts SET kind = 'income' WHERE id = ?1",
            [groceries.to_bytes()],
        )
        .expect("kind changed");
    assert!(matches!(
        book.categories(),
        Err(StoreError::Corrupt(Corruption::BadCategoryTree(id))) if id == groceries
    ));
    book.as_another_program()
        .execute(
            "UPDATE accounts SET kind = 'expense', currency = 'USD' WHERE id = ?1",
            [groceries.to_bytes()],
        )
        .expect("currency changed");
    assert!(matches!(
        book.categories(),
        Err(StoreError::Corrupt(Corruption::BadCategoryTree(id))) if id == groceries
    ));
    let _ = checking;
}

#[test]
fn a_loop_in_the_file_does_not_hang_new_categories() {
    let mut book = book_in_euros();
    let food = category(&mut book, "Food", AccountKind::Expense, None).expect("added");
    let groceries =
        category(&mut book, "Groceries", AccountKind::Expense, Some(food)).expect("added");
    book.as_another_program()
        .execute(
            "UPDATE accounts SET parent_id = ?1 WHERE id = ?2",
            params![groceries.to_bytes(), food.to_bytes()],
        )
        .expect("loop made behind KassenSturz's back");
    assert!(matches!(
        category(&mut book, "Organic", AccountKind::Expense, Some(food)),
        Err(StoreError::Corrupt(Corruption::BadCategoryTree(_)))
    ));
}
