// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of filling a new book in one go, and of the functions that read a
//! whole book for the complete export, on books in memory. All data is made
//! up (AGENTS.md §3.1 rule 2).

use kassensturz_core::{
    Account, AccountId, AccountKind, Actions, BookTo, Condition, CurrencyCode, DatedRate,
    DatedRateId, ExchangeRate, MatchMode, RateSource, RoundingMode, Rule, Split, TextField,
    TextOperator, Transaction, TransactionId,
};

use super::*;
use crate::RuleMark;
use crate::tests::{EUR, Sample, USD, account, date, eur, sample, transaction};

const JPY: CurrencyCode = match CurrencyCode::new("JPY") {
    Ok(code) => code,
    Err(_) => panic!("JPY is a valid code"),
};

fn category(name: &str, parent: Option<AccountId>) -> Account {
    Account {
        id: AccountId::generate(),
        name: name.into(),
        kind: AccountKind::Expense,
        currency: EUR,
        account_type: None,
        closed: false,
        parent,
        number: None,
        notes: None,
        institution: None,
    }
}

fn rule(name: &str, book_to: AccountId) -> Rule {
    Rule {
        id: RuleId::generate(),
        name: name.into(),
        match_mode: MatchMode::All,
        conditions: vec![Condition::Text {
            field: TextField::OtherPartyName,
            operator: TextOperator::Contains,
            value: "Bakery".into(),
            case_sensitive: false,
        }],
        actions: Actions {
            book_to: Some(BookTo::Category(book_to)),
            payee: None,
        },
    }
}

#[test]
fn a_new_book_is_filled_without_a_history() {
    let mut book = Book::create_in_memory().expect("book created");
    let food = category("Food", None);
    let bakery = category("Bakery", Some(food.id));
    let bread = rule("Bread", bakery.id);
    let mut checking = None;
    let mut bought = None;
    book.fill_new(HistoryRetention::Forever, |book| {
        book.set_rounding_mode(RoundingMode::HalfEven)?;
        // Decimal places other than the table's are kept as given (D-027).
        book.restore_currency(Currency::new(EUR, 2).expect("valid"))?;
        book.restore_currency(Currency::new(JPY, 2).expect("valid"))?;
        let id = account(book, "Checking", AccountKind::Asset, EUR);
        book.add_account(&food)?;
        book.add_account(&bakery)?;
        book.add_rule(&bread, None)?;
        let purchase = transaction(
            date(2026, 10, 5),
            "Bakery",
            &[(id, eur(-450)), (bakery.id, eur(450))],
        );
        book.add_transaction(&purchase)?;
        book.restore_rule_mark(purchase.splits()[1].id, Some(bread.id))?;
        checking = Some(id);
        bought = Some(purchase);
        Ok::<_, StoreError>(())
    })
    .expect("filled");
    let (checking, bought) = (checking.expect("added"), bought.expect("added"));

    assert_eq!(book.history().expect("history"), vec![]);
    assert_eq!(book.change_to_undo().expect("history"), None);
    assert_eq!(
        book.history_retention().expect("retention"),
        HistoryRetention::Forever
    );
    assert_eq!(book.rounding_mode().expect("mode"), RoundingMode::HalfEven);
    assert_eq!(book.currency_minor_digits(JPY).expect("in use"), 2);
    assert_eq!(book.balance(checking).expect("balance"), eur(-450));
    assert_eq!(book.transactions().expect("read"), vec![bought.clone()]);
    assert_eq!(
        book.rule_marks().expect("marks"),
        vec![RuleMark {
            split: bought.splits()[1].id,
            rule: Some(bread.id),
        }]
    );
    // From now on, changes are recorded as usual.
    book.remove_transaction(bought.id()).expect("removed");
    assert_eq!(book.history().expect("history").len(), 1);
}

#[test]
fn only_an_empty_book_can_be_filled() {
    let Sample { mut book, .. } = sample();
    let filled = book.fill_new(HistoryRetention::default(), |_| Ok::<_, StoreError>(()));
    assert!(matches!(filled, Err(StoreError::NotEmpty)));

    // A change in the history counts too, even one to the settings only.
    let mut book = Book::create_in_memory().expect("book created");
    book.set_rounding_mode(RoundingMode::Floor).expect("set");
    let filled = book.fill_new(HistoryRetention::default(), |_| Ok::<_, StoreError>(()));
    assert!(matches!(filled, Err(StoreError::NotEmpty)));

    // So does one kept only in memory.
    let mut book = Book::create_in_memory().expect("book created");
    book.set_history_retention(HistoryRetention::Days(0))
        .expect("set");
    book.set_rounding_mode(RoundingMode::Floor).expect("set");
    let filled = book.fill_new(HistoryRetention::default(), |_| Ok::<_, StoreError>(()));
    assert!(matches!(filled, Err(StoreError::NotEmpty)));
}

#[test]
fn a_failed_fill_leaves_the_book_empty() {
    let mut book = Book::create_in_memory().expect("book created");
    let filled = book.fill_new(HistoryRetention::Days(7), |book| {
        book.restore_currency(Currency::new(EUR, 2).expect("valid"))?;
        let checking = account(book, "Checking", AccountKind::Asset, EUR);
        let elsewhere = AccountId::generate();
        let purchase = transaction(
            date(2026, 10, 5),
            "Bakery",
            &[(checking, eur(-450)), (elsewhere, eur(450))],
        );
        book.add_transaction(&purchase)
    });
    assert!(matches!(filled, Err(StoreError::UnknownAccount(_))));
    assert_eq!(book.currencies().expect("currencies"), vec![]);
    assert_eq!(book.all_accounts().expect("accounts"), vec![]);
    assert_eq!(book.history().expect("history"), vec![]);
    assert_eq!(
        book.history_retention().expect("retention"),
        HistoryRetention::default()
    );
    // The book can still be filled, and changed as usual.
    book.fill_new(HistoryRetention::Days(7), |book| {
        book.restore_currency(Currency::new(EUR, 2).expect("valid"))
    })
    .expect("filled");
    book.use_currency(USD).expect("used");
    assert_eq!(book.history().expect("history").len(), 1);
}

#[test]
fn a_mark_names_a_rule_of_the_book_or_none() {
    let mut book = Book::create_in_memory().expect("book created");
    let filled = book.fill_new(HistoryRetention::default(), |book| {
        book.restore_currency(Currency::new(EUR, 2).expect("valid"))?;
        let checking = account(book, "Checking", AccountKind::Asset, EUR);
        let purchase = Transaction::new(
            TransactionId::generate(),
            date(2026, 10, 5),
            None,
            vec![
                Split::new(checking, eur(-450)),
                Split::uncategorized(eur(450)),
            ],
        )
        .expect("balanced");
        book.add_transaction(&purchase)?;
        // A Rule that was removed since leaves its mark without a Rule.
        book.restore_rule_mark(purchase.splits()[1].id, None)?;
        book.restore_rule_mark(purchase.splits()[0].id, Some(RuleId::generate()))
    });
    assert!(matches!(filled, Err(StoreError::UnknownRule(_))));
}

#[test]
#[should_panic(expected = "only a new book being filled")]
fn currencies_are_restored_only_while_filling() {
    let mut book = Book::create_in_memory().expect("book created");
    let _ = book.restore_currency(Currency::new(EUR, 2).expect("valid"));
}

#[test]
#[should_panic(expected = "can't be part of a user action")]
fn filling_is_never_part_of_a_user_action() {
    let mut book = Book::create_in_memory().expect("book created");
    let _ = book.as_one_action(Action::AddAccount, |book| {
        book.fill_new(HistoryRetention::default(), |_| Ok::<_, StoreError>(()))
    });
}

#[test]
fn the_whole_book_is_read_in_a_fixed_order() {
    let Sample {
        mut book,
        checking,
        groceries,
        salary,
    } = sample();
    book.use_currency(USD).expect("used");
    let starting = account(&mut book, "Starting balances", AccountKind::Equity, EUR);
    let late = transaction(
        date(2026, 10, 9),
        "Corner Grocery",
        &[(checking, eur(-100)), (groceries, eur(100))],
    );
    let early = transaction(
        date(2026, 10, 1),
        "Example Employer",
        &[(checking, eur(9000)), (salary, eur(-9000))],
    );
    let first = transaction(
        date(2026, 9, 30),
        "Opening",
        &[(checking, eur(500)), (starting, eur(-500))],
    );
    for added in [&late, &early, &first] {
        book.add_transaction(added).expect("added");
    }
    let rate = |day, from, to| DatedRate {
        id: DatedRateId::generate(),
        date: date(2026, 10, day),
        from,
        to,
        rate: "1.0850".parse::<ExchangeRate>().expect("valid"),
        source: RateSource::Manual,
    };
    let rates = [rate(2, USD, EUR), rate(1, USD, EUR), rate(3, EUR, USD)];
    for added in &rates {
        book.add_rate(added).expect("added");
    }

    assert_eq!(
        book.transactions().expect("read"),
        vec![first, early, late.clone()]
    );
    assert_eq!(
        book.transaction(late.id()).expect("read"),
        book.transactions().expect("read")[2]
    );
    assert_eq!(
        book.all_rates().expect("read"),
        vec![rates[2], rates[1], rates[0]]
    );
    assert_eq!(
        book.currencies().expect("read"),
        vec![
            Currency::new(EUR, 2).expect("valid"),
            Currency::new(USD, 2).expect("valid"),
        ]
    );
    let mut ids = vec![checking, groceries, salary, starting];
    ids.sort();
    let accounts: Vec<AccountId> = book
        .all_accounts()
        .expect("read")
        .into_iter()
        .map(|account| account.id)
        .collect();
    assert_eq!(accounts, ids);
}
