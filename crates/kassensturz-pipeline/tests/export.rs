// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Gathering transactions for export from books in memory. All data is made
//! up (AGENTS.md §3.1 rule 2).

use kassensturz_core::{
    Account, AccountId, AccountKind, CurrencyCode, Date, ImportedDetails, Money, Split,
    Transaction, TransactionId,
};
use kassensturz_export::{AccountTransactions, Counterpart, ExportData};
use kassensturz_pipeline::export_transactions;
use kassensturz_store::{Book, StoreError};

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};

fn eur(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, EUR)
}

fn day(day: i8) -> Date {
    Date::new(2026, 10, day).expect("a valid date")
}

fn add(book: &mut Book, name: &str, kind: AccountKind, parent: Option<AccountId>) -> AccountId {
    let id = AccountId::generate();
    book.add_account(&Account {
        id,
        name: name.into(),
        kind,
        currency: EUR,
        account_type: None,
        parent,
        closed: false,
    })
    .expect("account added");
    id
}

fn add_transaction(book: &mut Book, date: Date, payee: Option<&str>, splits: Vec<Split>) {
    let transaction = Transaction::new(
        TransactionId::generate(),
        date,
        payee.map(str::to_owned),
        splits,
    )
    .expect("balanced");
    book.add_transaction(&transaction).expect("added");
}

fn only(data: ExportData) -> Vec<AccountTransactions> {
    match data {
        ExportData::Transactions(accounts) => accounts,
        _ => panic!("transactions were exported"),
    }
}

#[test]
fn rows_carry_category_transfer_parts_and_memo() {
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    let checking = add(&mut book, "Checking account", AccountKind::Asset, None);
    let savings = add(&mut book, "Savings account", AccountKind::Asset, None);
    let food = add(&mut book, "Food", AccountKind::Expense, None);
    let groceries = add(&mut book, "Groceries", AccountKind::Expense, Some(food));
    let fun = add(&mut book, "Fun", AccountKind::Expense, None);

    // A category two levels down, with what a bank delivered on the account's
    // own part.
    let mut own = Split::new(checking, eur(-1250));
    own.imported = Some(ImportedDetails {
        remittance_information: Some("Card payment 4711".into()),
        ..ImportedDetails::default()
    });
    add_transaction(
        &mut book,
        day(5),
        Some("Corner Shop"),
        vec![own, Split::new(groceries, eur(1250))],
    );
    // A Transfer.
    add_transaction(
        &mut book,
        day(6),
        None,
        vec![
            Split::new(checking, eur(-5000)),
            Split::new(savings, eur(5000)),
        ],
    );
    // Several other parts.
    add_transaction(
        &mut book,
        day(7),
        Some("Market"),
        vec![
            Split::new(checking, eur(-3000)),
            Split::new(groceries, eur(2000)),
            Split::new(fun, eur(1000)),
        ],
    );
    // No category yet.
    add_transaction(
        &mut book,
        day(8),
        Some("Unknown"),
        vec![
            Split::new(checking, eur(-100)),
            Split::uncategorized(eur(100)),
        ],
    );

    let accounts = only(export_transactions(&book, &[checking, savings]).expect("gathered"));
    assert_eq!(accounts.len(), 2);

    let checking_rows = &accounts[0];
    assert_eq!(checking_rows.account, "Checking account");
    assert_eq!(checking_rows.currency, EUR);
    assert_eq!(checking_rows.minor_digits, 2);
    let summary: Vec<_> = checking_rows
        .rows
        .iter()
        .map(|row| {
            (
                row.date,
                row.payee.as_deref(),
                row.counterpart.clone(),
                row.memo.as_deref(),
                row.amount,
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            (
                day(5),
                Some("Corner Shop"),
                Counterpart::Category("Food/Groceries".into()),
                Some("Card payment 4711"),
                eur(-1250)
            ),
            (
                day(6),
                None,
                Counterpart::Transfer("Savings account".into()),
                None,
                eur(-5000)
            ),
            (
                day(7),
                Some("Market"),
                Counterpart::Parts(2),
                None,
                eur(-3000)
            ),
            (
                day(8),
                Some("Unknown"),
                Counterpart::Uncategorized,
                None,
                eur(-100)
            ),
        ]
    );
    // The rows add up to the balance.
    let total: i64 = checking_rows
        .rows
        .iter()
        .map(|row| row.amount.minor_units())
        .sum();
    assert_eq!(eur(total), book.balance(checking).expect("balance"));

    // The other side of the Transfer.
    assert_eq!(accounts[1].rows.len(), 1);
    assert_eq!(accounts[1].rows[0].amount, eur(5000));
    assert_eq!(
        accounts[1].rows[0].counterpart,
        Counterpart::Transfer("Checking account".into())
    );
}

#[test]
fn a_category_can_be_exported_and_the_order_given_is_kept() {
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    let checking = add(&mut book, "Checking account", AccountKind::Asset, None);
    let rent = add(&mut book, "Rent", AccountKind::Expense, None);
    add_transaction(
        &mut book,
        day(1),
        Some("Landlord"),
        vec![
            Split::new(checking, eur(-70000)),
            Split::new(rent, eur(70000)),
        ],
    );
    let accounts = only(export_transactions(&book, &[rent, checking]).expect("gathered"));
    let names: Vec<_> = accounts.iter().map(|a| a.account.as_str()).collect();
    assert_eq!(names, ["Rent", "Checking account"]);
    assert_eq!(accounts[0].rows[0].amount, eur(70000));
    assert_eq!(
        accounts[0].rows[0].counterpart,
        Counterpart::Transfer("Checking account".into())
    );
}

#[test]
fn an_unknown_account_is_refused_and_an_empty_list_gives_no_accounts() {
    let book = Book::create_in_memory().expect("book created");
    let unknown = AccountId::generate();
    assert!(matches!(
        export_transactions(&book, &[unknown]),
        Err(StoreError::UnknownAccount(id)) if id == unknown
    ));
    assert!(only(export_transactions(&book, &[]).expect("gathered")).is_empty());
}
