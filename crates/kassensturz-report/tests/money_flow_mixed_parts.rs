// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! B-330: the money flow with *Show accounts* on must count the same income
//! and spending as the chart without it (docs/MONEY_FLOW.md: "the chart
//! counts exactly what the overview counts"), also for a transaction whose
//! category parts go both ways: a salary booked gross with a deduction, or
//! a purchase with a refund part.

use kassensturz_core::{
    Account, AccountId, AccountKind, AccountType, CurrencyCode, Date, Money, Split, Transaction,
    TransactionId,
};
use kassensturz_report::{
    Filter, FlowNodeKind, FlowOptions, MoneyFlow, Period, Source, money_flow,
};
use kassensturz_store::Book;

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR"),
};

fn day(y: i16, m: i8, d: i8) -> Date {
    Date::new(y, m, d).unwrap()
}

fn eur(minor: i64) -> Money {
    Money::from_minor_units(minor, EUR)
}

fn account(book: &mut Book, name: &str, kind: AccountKind) -> AccountId {
    let id = AccountId::generate();
    book.add_account(&Account {
        id,
        name: name.into(),
        kind,
        currency: EUR,
        account_type: (kind == AccountKind::Asset).then_some(AccountType::Checking),
        parent: None,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    })
    .unwrap();
    id
}

fn node_total(flow: &MoneyFlow, name: &str) -> i64 {
    flow.nodes
        .iter()
        .filter(|node| node.name == name && node.kind != FlowNodeKind::Middle)
        .map(|node| node.amount.minor_units())
        .sum()
}

/// The flow of one transaction on 1 October, without and with accounts,
/// as the totals of the nodes named `names`.
fn totals(parts: &[(&str, AccountKind, i64)], names: &[&str]) -> (Vec<i64>, Vec<i64>) {
    let mut book = Book::create_in_memory().unwrap();
    book.use_currency(EUR).unwrap();
    let splits = parts
        .iter()
        .map(|&(name, kind, minor)| Split::new(account(&mut book, name, kind), eur(minor)))
        .collect();
    book.add_transaction(
        &Transaction::new(TransactionId::generate(), day(2026, 10, 1), None, splits).unwrap(),
    )
    .unwrap();
    let period = Period::new(day(2026, 10, 1), day(2026, 10, 31)).unwrap();
    let source = Source::read_with_moves(&book, period).unwrap();
    let flow = |accounts| {
        money_flow(
            &source,
            period,
            &Filter::default(),
            FlowOptions {
                sub_categories: false,
                payees: false,
                accounts,
            },
        )
        .unwrap()
        .remove(0)
    };
    let figures = |flow: &MoneyFlow| names.iter().map(|name| node_total(flow, name)).collect();
    (figures(&flow(false)), figures(&flow(true)))
}

#[test]
fn a_gross_salary_with_a_deduction_keeps_both_parts_with_accounts_shown() {
    let (plain, with_accounts) = totals(
        &[
            ("Checking", AccountKind::Asset, 280_000),
            ("Salary", AccountKind::Income, -300_000),
            ("Health insurance", AccountKind::Expense, 20_000),
        ],
        &["Salary", "Health insurance", "Checking"],
    );
    assert_eq!(plain[..2], [300_000, 20_000]);
    assert_eq!(with_accounts[..2], plain[..2]);
    // The whole salary passes through the account, the deduction leaves it.
    assert_eq!(with_accounts[2], 300_000);
}

#[test]
fn a_purchase_with_a_refund_part_keeps_both_parts_with_accounts_shown() {
    let (plain, with_accounts) = totals(
        &[
            ("Checking", AccountKind::Asset, -5_000),
            ("Groceries", AccountKind::Expense, 6_000),
            ("Bottle deposit", AccountKind::Income, -1_000),
        ],
        &["Groceries", "Bottle deposit"],
    );
    assert_eq!(plain, [6_000, 1_000]);
    assert_eq!(with_accounts, plain);
}
