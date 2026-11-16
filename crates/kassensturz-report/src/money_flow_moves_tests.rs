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
use crate::tests::{EUR, USD, account, date, money, period};

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

/// The part of the oracle that holds even where opposite moves cancel: what
/// is left over less what was drawn from reserves is the register's net change
/// of all chosen accounts.
fn check_net_against_register(
    flow: &MoneyFlow,
    bookings: &[Transaction],
    own: &[(&str, AccountId)],
) {
    let net: i64 = own
        .iter()
        .map(|&(_, id)| {
            let (into, out) = register(bookings, id);
            into - out
        })
        .sum();
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

#[test]
fn a_band_says_how_much_of_it_went_both_ways_and_one_way_bands_say_nothing() {
    // 70,000 to savings and 30,000 back: one band of 40,000 that says 30,000
    // also went the other way. A move to a third account that nothing
    // comes back from says nothing.
    let mut book = Book::create_in_memory().expect("book");
    book.use_currency(EUR).expect("currency");
    let a = account(&mut book, "Current account", AccountKind::Asset, EUR, None);
    let b = account(&mut book, "Savings", AccountKind::Asset, EUR, None);
    let c = account(&mut book, "Household", AccountKind::Asset, EUR, None);
    let salary = account(&mut book, "Salary", AccountKind::Income, EUR, None);
    let bookings = vec![
        parts(&[(a, 200_000), (salary, -200_000)], 1),
        open_side(a, -70_000, 2),
        open_side(b, 70_000, 2),
        open_side(b, -30_000, 9),
        open_side(a, 30_000, 9),
        open_side(a, -10_000, 12),
        open_side(c, 10_000, 12),
    ];
    for transaction in &bookings {
        book.add_transaction(transaction).expect("added");
    }
    let whole = period(date(2026, 10, 1), date(2026, 10, 31));
    let source = Source::read_with_moves(&book, whole).expect("read");
    let flows = money_flow(&source, whole, &Filter::default(), FLOW).expect("flow");
    let flow = &flows[0];
    assert_eq!(
        band(flow, "Current account", "Savings"),
        Some((40_000, 40_000))
    );
    assert_eq!(returned(flow, "Current account", "Savings"), Some(30_000));
    assert_eq!(returned(flow, "Current account", "Household"), Some(0));
    // The chart's net change still equals the register's: 200,000 in, nothing
    // spent, so all of it is left over.
    let left_over: i64 = flow
        .nodes
        .iter()
        .filter(|n| n.kind == FlowNodeKind::LeftOver)
        .map(|n| n.amount.minor_units())
        .sum();
    assert_eq!(left_over, 200_000);
}

fn foreign_open_side(account: AccountId, amount: i64, day: i8) -> Transaction {
    let usd = |minor| kassensturz_core::Money::from_minor_units(minor, USD);
    Transaction::new(
        TransactionId::generate(),
        date(2026, 10, day),
        None,
        vec![
            Split::new(account, usd(amount)),
            Split::uncategorized(usd(-amount)),
        ],
    )
    .expect("balanced")
}

#[test]
fn a_money_loop_between_two_accounts_matches_the_register() {
    // A to B and part of it back to A: one assumed band of the difference.
    let mut book = Book::create_in_memory().expect("book");
    book.use_currency(EUR).expect("currency");
    let a = account(&mut book, "Current account", AccountKind::Asset, EUR, None);
    let b = account(&mut book, "Savings", AccountKind::Asset, EUR, None);
    let salary = account(&mut book, "Salary", AccountKind::Income, EUR, None);
    let bookings = vec![
        parts(&[(a, 200_000), (salary, -200_000)], 1),
        open_side(a, -70_000, 2),
        open_side(b, 70_000, 2),
        open_side(b, -30_000, 9),
        open_side(a, 30_000, 9),
    ];
    for transaction in &bookings {
        book.add_transaction(transaction).expect("added");
    }
    let whole = period(date(2026, 10, 1), date(2026, 10, 31));
    let source = Source::read_with_moves(&book, whole).expect("read");
    let flows = money_flow(&source, whole, &Filter::default(), FLOW).expect("flow");
    let flow = &flows[0];
    // Opposite moves between the same two accounts are one band of the
    // difference, as for every pair of nodes (the band is drawn once).
    assert_eq!(
        band(flow, "Current account", "Savings"),
        Some((40_000, 40_000))
    );
    assert_eq!(band(flow, "Savings", "Current account"), None);
    // The round trip of 30,000 cancels inside the one band, so the node is
    // as wide as the register's inflow less the part that went round; the
    // net change still equals the register's.
    let width = |name: &str| {
        flow.nodes
            .iter()
            .find(|n| n.kind == FlowNodeKind::Account && n.name == name)
            .map(|n| n.amount.minor_units())
    };
    assert_eq!(width("Current account"), Some(200_000));
    assert_eq!(width("Savings"), Some(40_000));
    check_net_against_register(flow, &bookings, &[("Current account", a), ("Savings", b)]);
}

#[test]
fn a_booking_split_over_three_accounts_matches_the_register() {
    // One booking takes money out of A and puts it into B and C at once (not
    // a pair, so not a move), next to a pair that is one. The figures must
    // still equal the register.
    let mut book = Book::create_in_memory().expect("book");
    book.use_currency(EUR).expect("currency");
    let a = account(&mut book, "Current account", AccountKind::Asset, EUR, None);
    let b = account(&mut book, "Household", AccountKind::Asset, EUR, None);
    let c = account(&mut book, "Savings", AccountKind::Asset, EUR, None);
    let salary = account(&mut book, "Salary", AccountKind::Income, EUR, None);
    let bookings = vec![
        parts(&[(a, 300_000), (salary, -300_000)], 1),
        parts(&[(a, -100_000), (b, 60_000), (c, 40_000)], 3),
        open_side(a, -25_000, 4),
        open_side(c, 25_000, 5),
    ];
    for transaction in &bookings {
        book.add_transaction(transaction).expect("added");
    }
    let whole = period(date(2026, 10, 1), date(2026, 10, 31));
    let source = Source::read_with_moves(&book, whole).expect("read");
    let flows = money_flow(&source, whole, &Filter::default(), FLOW).expect("flow");
    let flow = &flows[0];
    assert_eq!(
        band(flow, "Current account", "Savings").map(|b| b.1),
        Some(25_000)
    );
    check_against_register(
        flow,
        &bookings,
        &[("Current account", a), ("Household", b), ("Savings", c)],
    );
}

#[test]
fn sides_in_two_currencies_are_never_a_move_and_each_currency_matches_its_register() {
    // Same figure out of a euro account and into a dollar account: not a
    // move (one currency only, D-043); a real dollar move beside it is.
    let mut book = Book::create_in_memory().expect("book");
    book.use_currency(EUR).expect("currency");
    book.use_currency(USD).expect("currency");
    let euro = account(&mut book, "Current account", AccountKind::Asset, EUR, None);
    let dollar = account(&mut book, "Dollar account", AccountKind::Asset, USD, None);
    let dollar_two = account(&mut book, "Travel account", AccountKind::Asset, USD, None);
    let bookings = vec![
        open_side(euro, -10_000, 2),
        foreign_open_side(dollar, 10_000, 2),
        foreign_open_side(dollar, -4_000, 6),
        foreign_open_side(dollar_two, 4_000, 6),
    ];
    for transaction in &bookings {
        book.add_transaction(transaction).expect("added");
    }
    let whole = period(date(2026, 10, 1), date(2026, 10, 31));
    let source = Source::read_with_moves(&book, whole).expect("read");
    let flows = money_flow(&source, whole, &Filter::default(), FLOW).expect("flow");
    assert_eq!(flows.len(), 2);
    let euros = flows.iter().find(|f| f.currency == EUR).expect("EUR");
    let dollars = flows.iter().find(|f| f.currency == USD).expect("USD");
    assert!(euros.flows.iter().all(|f| f.assumed.is_zero()));
    assert_eq!(
        band(dollars, "Dollar account", "Travel account"),
        Some((4_000, 4_000))
    );
    let euro_bookings: Vec<_> = bookings[..1].to_vec();
    let dollar_bookings: Vec<_> = bookings[1..].to_vec();
    check_against_register(euros, &euro_bookings, &[("Current account", euro)]);
    check_against_register(
        dollars,
        &dollar_bookings,
        &[("Dollar account", dollar), ("Travel account", dollar_two)],
    );
}

#[test]
fn sides_that_cancel_in_not_sorted_yet_draw_no_node_without_the_accounts_layer() {
    // The same amount out of one account and into another, neither sorted:
    // without the accounts they net to zero in *Not sorted yet*, so no node
    // is drawn (the dataset description says so, B-312); with the accounts
    // the pair is an assumed move.
    let mut book = Book::create_in_memory().expect("book");
    book.use_currency(EUR).expect("currency");
    let a = account(&mut book, "Current account", AccountKind::Asset, EUR, None);
    let b = account(&mut book, "Savings", AccountKind::Asset, EUR, None);
    let salary = account(&mut book, "Salary", AccountKind::Income, EUR, None);
    for transaction in [
        parts(&[(a, 100_000), (salary, -100_000)], 1),
        open_side(a, -3_980, 4),
        open_side(b, 3_980, 5),
    ] {
        book.add_transaction(&transaction).expect("added");
    }
    let whole = period(date(2026, 10, 1), date(2026, 10, 31));
    let plain = Source::read(&book, whole).expect("read");
    let flows = money_flow(
        &plain,
        whole,
        &Filter::default(),
        FlowOptions {
            accounts: false,
            ..FLOW
        },
    )
    .expect("flow");
    assert!(
        flows[0]
            .nodes
            .iter()
            .all(|n| n.kind != FlowNodeKind::Unsorted)
    );
    let with_moves = Source::read_with_moves(&book, whole).expect("read");
    let flows = money_flow(&with_moves, whole, &Filter::default(), FLOW).expect("flow");
    assert_eq!(
        band(&flows[0], "Current account", "Savings"),
        Some((3_980, 3_980))
    );
}

#[test]
fn folding_a_crowded_column_keeps_the_accounts_nodes_in_place() {
    // B-350: more spending categories than a column holds. The small ones
    // fold into *Other*, which renumbers the nodes; an account first used by
    // a folded category must still be found by its number.
    let mut book = Book::create_in_memory().expect("book");
    book.use_currency(EUR).expect("currency");
    let current = account(&mut book, "Current account", AccountKind::Asset, EUR, None);
    let cash = account(&mut book, "Cash box", AccountKind::Asset, EUR, None);
    let salary = account(&mut book, "Salary", AccountKind::Income, EUR, None);
    let mut bookings = vec![parts(&[(current, 900_000), (salary, -900_000)], 1)];
    for number in 1..=12_i64 {
        let category = account(
            &mut book,
            &format!("Category {number:02}"),
            AccountKind::Expense,
            EUR,
            None,
        );
        // The first twelve are paid from the current account, the smallest
        // from the cash box, which no larger category uses.
        let (from, amount) = if number == 12 {
            (cash, 100)
        } else {
            (current, 100_000 - number * 1_000)
        };
        bookings.push(parts(&[(from, -amount), (category, amount)], 2));
    }
    for transaction in &bookings {
        book.add_transaction(transaction).expect("added");
    }
    let whole = period(date(2026, 10, 1), date(2026, 10, 31));
    let source = Source::read(&book, whole).expect("read");
    let flows = money_flow(&source, whole, &Filter::default(), FLOW).expect("flow");
    let flow = &flows[0];
    let accounts: Vec<(&str, i8)> = flow
        .nodes
        .iter()
        .filter(|n| n.kind == FlowNodeKind::Account)
        .map(|n| (n.name.as_str(), n.column))
        .collect();
    assert_eq!(accounts, [("Current account", 0), ("Cash box", 0)]);
    assert!(flow.nodes.iter().any(|n| n.kind == FlowNodeKind::Other));
    for node in flow
        .nodes
        .iter()
        .filter(|n| n.kind == FlowNodeKind::Account)
    {
        assert!(node.account.is_some(), "{}", node.name);
    }
}
