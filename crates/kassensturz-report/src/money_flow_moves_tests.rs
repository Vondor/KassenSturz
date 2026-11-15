// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the moves between own accounts that are not joined as Transfers
//! (US7, #1035): pairs that look like moves are drawn as assumed moves
//! between the accounts, and the chart's figures must equal the register's.
//! The oracle works from the booked parts only, never from the chart's own
//! counting. Synthetic names only.

use std::collections::HashMap;

use kassensturz_core::{
    AccountId, AccountKind, Split, Transaction, TransactionId, TransferSide, find_transfers,
};
use kassensturz_store::Book;

use super::*;
use crate::tests::{EUR, account, date, money, period};

/// What the register says about one own account: the money that came in and
/// the money that went out, from the booked parts alone.
fn register(bookings: &[Transaction], own: AccountId) -> (i64, i64) {
    let (mut into, mut out) = (0_i64, 0_i64);
    for transaction in bookings {
        for split in transaction.splits() {
            if split.account == Some(own) {
                let amount = split.amount.minor_units();
                if amount > 0 {
                    into += amount;
                } else {
                    out -= amount;
                }
            }
        }
    }
    (into, out)
}

/// The oracle: an account's node is as wide as the larger of what the
/// register says came in and went out, and what is left over less what was
/// drawn from reserves is the register's net change of all chosen accounts.
fn check_against_register(flow: &MoneyFlow, bookings: &[Transaction], own: &[(&str, AccountId)]) {
    let mut net = 0_i64;
    for &(name, id) in own {
        let (into, out) = register(bookings, id);
        net += into - out;
        let node = flow
            .nodes
            .iter()
            .find(|n| n.kind == FlowNodeKind::Account && n.name == name)
            .unwrap_or_else(|| panic!("{name} has a node"));
        assert_eq!(
            node.amount.minor_units(),
            into.max(out),
            "{name}: the node is as wide as the register says"
        );
    }
    let sum = |kind: FlowNodeKind| -> i64 {
        flow.nodes
            .iter()
            .filter(|n| n.kind == kind)
            .map(|n| n.amount.minor_units())
            .sum()
    };
    assert_eq!(
        sum(FlowNodeKind::LeftOver) - sum(FlowNodeKind::Reserves),
        net,
        "what is left over less the reserves is the register's net change"
    );
}

/// What the band says went both ways (#1050 follow-up).
fn returned(flow: &MoneyFlow, from: &str, to: &str) -> Option<i64> {
    let name = |index: usize| flow.nodes[index].name.as_str();
    flow.flows
        .iter()
        .find(|f| name(f.from) == from && name(f.to) == to)
        .map(|f| f.returned.minor_units())
}

fn band(flow: &MoneyFlow, from: &str, to: &str) -> Option<(i64, i64)> {
    let name = |index: usize| flow.nodes[index].name.as_str();
    flow.flows
        .iter()
        .find(|f| name(f.from) == from && name(f.to) == to)
        .map(|f| (f.amount.minor_units(), f.assumed.minor_units()))
}

fn parts(parts: &[(AccountId, i64)], day: i8) -> Transaction {
    Transaction::new(
        TransactionId::generate(),
        date(2026, 10, day),
        None,
        parts
            .iter()
            .map(|&(account, amount)| Split::new(account, money(amount)))
            .collect(),
    )
    .expect("balanced")
}

fn open_side(account: AccountId, amount: i64, day: i8) -> Transaction {
    Transaction::new(
        TransactionId::generate(),
        date(2026, 10, day),
        None,
        vec![
            Split::new(account, money(amount)),
            Split::uncategorized(money(-amount)),
        ],
    )
    .expect("balanced")
}

const FLOW: FlowOptions = FlowOptions {
    sub_categories: false,
    payees: false,
    accounts: true,
};

#[test]
fn moves_that_are_not_joined_are_drawn_between_the_accounts_marked_assumed() {
    // A salary lands on the current account; part goes to a household
    // account (sorted into categories by hand-made Rules: both sides sit in
    // a category), part to a savings account (both sides not sorted yet).
    let mut book = Book::create_in_memory().expect("book");
    book.use_currency(EUR).expect("currency");
    let current = account(&mut book, "Current account", AccountKind::Asset, EUR, None);
    let household = account(&mut book, "Household", AccountKind::Asset, EUR, None);
    let savings = account(&mut book, "Savings", AccountKind::Asset, EUR, None);
    let salary = account(&mut book, "Salary", AccountKind::Income, EUR, None);
    let rent = account(&mut book, "Rent", AccountKind::Expense, EUR, None);
    let food = account(&mut book, "Food", AccountKind::Expense, EUR, None);
    let bookings = vec![
        parts(&[(current, 300_000), (salary, -300_000)], 1),
        // Current to household: the household's side is not sorted yet.
        open_side(current, -100_000, 2),
        open_side(household, 100_000, 3),
        // Current to savings, over a weekend.
        open_side(current, -50_000, 2),
        open_side(savings, 50_000, 5),
        parts(&[(household, -80_000), (rent, 80_000)], 6),
        parts(&[(current, -20_000), (food, 20_000)], 7),
    ];
    for transaction in &bookings {
        book.add_transaction(transaction).expect("added");
    }
    let whole = period(date(2026, 10, 1), date(2026, 10, 31));
    let source = Source::read_with_moves(&book, whole).expect("read");
    let flows = money_flow(&source, whole, &Filter::default(), FLOW).expect("flow");
    let flow = &flows[0];
    // The moves are bands between the accounts, each fully assumed, and the
    // sides count as neither income nor spending.
    assert_eq!(
        band(flow, "Current account", "Household"),
        Some((100_000, 100_000))
    );
    assert_eq!(
        band(flow, "Current account", "Savings"),
        Some((50_000, 50_000))
    );
    assert_eq!(band(flow, "Household", "Rent"), Some((80_000, 0)));
    assert_eq!(band(flow, "Current account", "Food"), Some((20_000, 0)));
    assert!(flow.nodes.iter().all(|n| n.kind != FlowNodeKind::Unsorted));
    // A receiving account is a node even though nothing is spent from it.
    assert!(flow.nodes.iter().any(|n| n.name == "Savings"));
    check_against_register(
        flow,
        &bookings,
        &[
            ("Current account", current),
            ("Household", household),
            ("Savings", savings),
        ],
    );
    // Read without the moves, the same book has no assumed band (and its
    // not-sorted-yet sides cancel each other, so only the read with the
    // moves can match the register).
    let plain = Source::read(&book, whole).expect("read");
    let flows = money_flow(&plain, whole, &Filter::default(), FLOW).expect("flow");
    assert!(flows[0].flows.iter().all(|f| f.assumed.is_zero()));
}

#[test]
fn a_pair_sorted_by_hand_is_not_assumed_to_be_a_move() {
    let mut book = Book::create_in_memory().expect("book");
    book.use_currency(EUR).expect("currency");
    let current = account(&mut book, "Current account", AccountKind::Asset, EUR, None);
    let savings = account(&mut book, "Savings", AccountKind::Asset, EUR, None);
    let gift = account(&mut book, "Gift", AccountKind::Income, EUR, None);
    let gone = account(&mut book, "Gone", AccountKind::Expense, EUR, None);
    let bookings = vec![
        parts(&[(current, -10_000), (gone, 10_000)], 3),
        parts(&[(savings, 10_000), (gift, -10_000)], 3),
    ];
    for transaction in &bookings {
        book.add_transaction(transaction).expect("added");
    }
    let whole = period(date(2026, 10, 1), date(2026, 10, 31));
    let source = Source::read_with_moves(&book, whole).expect("read");
    let flows = money_flow(&source, whole, &Filter::default(), FLOW).expect("flow");
    assert!(flows[0].flows.iter().all(|f| f.assumed.is_zero()));
    assert_eq!(band(&flows[0], "Gift", "Savings"), Some((10_000, 0)));
    assert_eq!(
        band(&flows[0], "Current account", "Gone"),
        Some((10_000, 0))
    );
}

#[test]
fn a_filter_by_category_leaves_assumed_moves_out() {
    let mut book = Book::create_in_memory().expect("book");
    book.use_currency(EUR).expect("currency");
    let current = account(&mut book, "Current account", AccountKind::Asset, EUR, None);
    let savings = account(&mut book, "Savings", AccountKind::Asset, EUR, None);
    let bookings = vec![open_side(current, -5_000, 2), open_side(savings, 5_000, 2)];
    for transaction in &bookings {
        book.add_transaction(transaction).expect("added");
    }
    let whole = period(date(2026, 10, 1), date(2026, 10, 31));
    let source = Source::read_with_moves(&book, whole).expect("read");
    let filter = Filter {
        categories: Some(Vec::new()),
        ..Filter::default()
    };
    let flows = money_flow(&source, whole, &filter, FLOW).expect("flow");
    assert!(
        flows
            .iter()
            .all(|f| f.flows.iter().all(|b| b.assumed.is_zero()))
    );
}

/// The pairs the store would suggest, worked out here with the core's own
/// matching, for a `Source` made by hand.
fn suggestions(
    bookings: &[Transaction],
    own: &HashMap<AccountId, ()>,
) -> Vec<kassensturz_core::TransferMatch> {
    let sides: Vec<TransferSide> = bookings
        .iter()
        .filter_map(|t| TransferSide::of(t, |id| own.contains_key(&id)).ok())
        .collect();
    find_transfers(&sides)
}

#[test]
fn a_move_with_a_part_that_is_not_chosen_is_left_as_it_was() {
    let mut w = Book::create_in_memory().expect("book");
    w.use_currency(EUR).expect("currency");
    let current = account(&mut w, "Current account", AccountKind::Asset, EUR, None);
    let savings = account(&mut w, "Savings", AccountKind::Asset, EUR, None);
    let bookings = vec![open_side(current, -5_000, 2), open_side(savings, 5_000, 2)];
    for transaction in &bookings {
        w.add_transaction(transaction).expect("added");
    }
    let whole = period(date(2026, 10, 1), date(2026, 10, 31));
    let own = HashMap::from([(current, ()), (savings, ())]);
    let source = Source::read(&w, whole)
        .expect("read")
        .with_moves(suggestions(&bookings, &own));
    let filter = Filter {
        accounts: Some(vec![current]),
        ..Filter::default()
    };
    let flows = money_flow(&source, whole, &filter, FLOW).expect("flow");
    assert!(flows[0].flows.iter().all(|f| f.assumed.is_zero()));
}

#[test]
fn sides_a_rule_sorted_into_categories_become_a_move_and_leave_the_categories() {
    // Both sides sit in a category (as a Rule leaves them): the one that
    // took the money out in an expense category, the one that put it in in
    // an income category. As a move they count in neither.
    let mut book = Book::create_in_memory().expect("book");
    book.use_currency(EUR).expect("currency");
    let current = account(&mut book, "Current account", AccountKind::Asset, EUR, None);
    let household = account(&mut book, "Household", AccountKind::Asset, EUR, None);
    let salary = account(&mut book, "Salary", AccountKind::Income, EUR, None);
    let to_house = account(&mut book, "To household", AccountKind::Expense, EUR, None);
    let from_current = account(&mut book, "From current", AccountKind::Income, EUR, None);
    let rent = account(&mut book, "Rent", AccountKind::Expense, EUR, None);
    let bookings = vec![
        parts(&[(current, 200_000), (salary, -200_000)], 1),
        parts(&[(current, -90_000), (to_house, 90_000)], 2),
        parts(&[(household, 90_000), (from_current, -90_000)], 3),
        parts(&[(household, -60_000), (rent, 60_000)], 4),
    ];
    for transaction in &bookings {
        book.add_transaction(transaction).expect("added");
    }
    let whole = period(date(2026, 10, 1), date(2026, 10, 31));
    let own = HashMap::from([(current, ()), (household, ())]);
    let source = Source::read(&book, whole)
        .expect("read")
        .with_moves(suggestions(&bookings, &own));
    let flows = money_flow(&source, whole, &Filter::default(), FLOW).expect("flow");
    let flow = &flows[0];
    assert_eq!(
        band(flow, "Current account", "Household"),
        Some((90_000, 90_000))
    );
    assert!(
        flow.nodes
            .iter()
            .all(|n| n.name != "To household" && n.name != "From current")
    );
    check_against_register(
        flow,
        &bookings,
        &[("Current account", current), ("Household", household)],
    );
}
