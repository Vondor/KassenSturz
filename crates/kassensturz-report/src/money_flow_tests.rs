// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the money flow (US7, #821, D-758): the nodes, the netting, the
//! limit per column, and the property that for every node the bands in equal
//! the bands out. Synthetic names only.

use std::collections::HashMap;

use kassensturz_core::{Account, AccountId, AccountKind, Split, Transaction, TransactionId};
use kassensturz_store::CategoryEntry;
use proptest::prelude::*;

use super::*;
use crate::tests::{EUR, USD, date, money, period, sample};

struct World {
    accounts: Vec<Account>,
    tree: Vec<CategoryEntry>,
    names: HashMap<&'static str, AccountId>,
    own: AccountId,
}

/// An own account and the categories `(name, kind, parent)`.
fn world(categories: &[(&'static str, AccountKind, Option<&'static str>)]) -> World {
    let make = |name: &str, kind, parent| Account {
        id: AccountId::generate(),
        name: name.into(),
        kind,
        currency: EUR,
        account_type: None,
        parent,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    };
    let own = make("Current account", AccountKind::Asset, None);
    let mut names = HashMap::new();
    let mut accounts = vec![own.clone()];
    let mut tree = Vec::new();
    for &(name, kind, parent) in categories {
        let account = make(name, kind, parent.map(|parent| names[parent]));
        names.insert(name, account.id);
        let mut path: Vec<String> = parent.iter().map(|p| (*p).to_owned()).collect();
        path.push(name.into());
        tree.push(CategoryEntry {
            path,
            category: account.clone(),
        });
        accounts.push(account);
    }
    World {
        own: own.id,
        accounts,
        tree,
        names,
    }
}

impl World {
    /// A booking to `category` (`None`: not sorted yet) of `amount` minor
    /// units: positive is money in.
    fn book(&self, payee: Option<&str>, category: Option<&str>, amount: i64) -> Transaction {
        let category = category.map(|name| self.names[name]);
        let side = match category {
            Some(id) => Split::new(id, money(-amount)),
            None => Split::uncategorized(money(-amount)),
        };
        Transaction::new(
            TransactionId::generate(),
            date(2026, 10, 5),
            payee.map(str::to_owned),
            vec![Split::new(self.own, money(amount)), side],
        )
        .expect("balanced")
    }

    fn flow(&self, bookings: Vec<Transaction>, options: FlowOptions) -> Vec<MoneyFlow> {
        let whole = period(date(2026, 1, 1), date(2026, 12, 31));
        let source = Source::from_parts(whole, self.accounts.clone(), self.tree.clone(), bookings);
        money_flow(&source, whole, &Filter::default(), options).expect("flow")
    }
}

/// The rule of D-758: for every node the bands in equal the bands out, every
/// column adds up to the total, and what an *Other* holds adds up to it.
fn check(flow: &MoneyFlow) {
    let total = flow.total.minor_units();
    let mut column_sums: HashMap<i8, i64> = HashMap::new();
    for (index, node) in flow.nodes.iter().enumerate() {
        let into: i64 = flow
            .flows
            .iter()
            .filter(|f| f.to == index)
            .map(|f| f.amount.minor_units())
            .sum();
        let out: i64 = flow
            .flows
            .iter()
            .filter(|f| f.from == index)
            .map(|f| f.amount.minor_units())
            .sum();
        let amount = node.amount.minor_units();
        assert!(amount > 0, "{node:?}");
        match node.column {
            0 => assert_eq!((into, out), (total, total), "middle"),
            c if c < 0 => {
                assert_eq!(out, amount, "{node:?}");
                assert!(into == 0 || into == amount, "{node:?}");
            }
            _ => {
                assert_eq!(into, amount, "{node:?}");
                assert!(out == 0 || out == amount, "{node:?}");
            }
        }
        if node.kind == FlowNodeKind::Other {
            let held: i64 = node.holds.iter().map(|(_, m)| m.minor_units()).sum();
            assert_eq!(held, amount, "what Other holds: {node:?}");
        }
        *column_sums.entry(node.column).or_default() += amount;
    }
    for flow in &flow.flows {
        assert!(flow.amount.minor_units() > 0);
    }
    // Each side's nearest column and the middle all carry the total.
    assert_eq!(column_sums[&0], total);
    let side = |sign: i8| -> i64 {
        flow.nodes
            .iter()
            .filter(|n| n.column == sign)
            .map(|n| n.amount.minor_units())
            .sum()
    };
    assert_eq!(side(-1), total);
    assert_eq!(side(1), total);
    // No pair of nodes has two bands.
    for (i, a) in flow.flows.iter().enumerate() {
        for b in &flow.flows[i + 1..] {
            assert!((a.from, a.to) != (b.from, b.to));
        }
    }
}

fn named<'a>(flow: &'a MoneyFlow, name: &str) -> &'a FlowNode {
    flow.nodes
        .iter()
        .find(|node| node.name == name)
        .unwrap_or_else(|| panic!("no node {name}"))
}

fn between(flow: &MoneyFlow, from: &str, to: &str) -> Option<i64> {
    let at = |name: &str| {
        flow.nodes
            .iter()
            .position(|n| n.name == name || (name == "*" && n.kind == FlowNodeKind::Middle))
    };
    let (from, to) = (at(from)?, at(to)?);
    flow.flows
        .iter()
        .find(|f| f.from == from && f.to == to)
        .map(|f| f.amount.minor_units())
}

#[test]
fn income_and_spending_meet_in_one_middle_node_with_a_left_over() {
    let w = world(&[
        ("Salary", AccountKind::Income, None),
        ("Rent", AccountKind::Expense, None),
    ]);
    let flows = w.flow(
        vec![
            w.book(Some("Acme"), Some("Salary"), 300_000),
            w.book(Some("Landlord"), Some("Rent"), -100_000),
        ],
        FlowOptions::default(),
    );
    let [flow] = flows.as_slice() else { panic!() };
    check(flow);
    assert_eq!(flow.total.minor_units(), 300_000);
    assert_eq!(between(flow, "Salary", "*"), Some(300_000));
    assert_eq!(between(flow, "*", "Rent"), Some(100_000));
    let left = flow
        .nodes
        .iter()
        .find(|n| n.kind == FlowNodeKind::LeftOver)
        .unwrap();
    assert_eq!(left.amount.minor_units(), 200_000);
    assert!(flow.nodes.iter().all(|n| n.kind != FlowNodeKind::Reserves));
}

#[test]
fn spending_above_income_is_made_up_from_reserves() {
    let w = world(&[
        ("Salary", AccountKind::Income, None),
        ("Rent", AccountKind::Expense, None),
    ]);
    let flows = w.flow(
        vec![
            w.book(None, Some("Salary"), 10_000),
            w.book(None, Some("Rent"), -25_000),
        ],
        FlowOptions::default(),
    );
    let flow = &flows[0];
    check(flow);
    let reserves = flow
        .nodes
        .iter()
        .find(|n| n.kind == FlowNodeKind::Reserves)
        .unwrap();
    assert_eq!(
        (reserves.amount.minor_units(), reserves.column),
        (15_000, -1)
    );
    assert_eq!(flow.total.minor_units(), 25_000);
}

#[test]
fn a_refund_is_netted_and_a_category_that_came_back_is_on_the_income_side() {
    let w = world(&[
        ("Salary", AccountKind::Income, None),
        ("Shoes", AccountKind::Expense, None),
        ("Books", AccountKind::Expense, None),
    ]);
    let flows = w.flow(
        vec![
            w.book(None, Some("Salary"), 50_000),
            w.book(None, Some("Shoes"), -8_000),
            w.book(None, Some("Shoes"), 3_000),
            // More came back than went out.
            w.book(None, Some("Books"), -1_000),
            w.book(None, Some("Books"), 4_000),
        ],
        FlowOptions::default(),
    );
    let flow = &flows[0];
    check(flow);
    assert_eq!(between(flow, "*", "Shoes"), Some(5_000));
    assert_eq!(named(flow, "Books").column, -1);
    assert_eq!(between(flow, "Books", "*"), Some(3_000));
}

#[test]
fn money_not_sorted_yet_gets_a_node_on_its_own_side() {
    let w = world(&[("Salary", AccountKind::Income, None)]);
    let flows = w.flow(
        vec![
            w.book(None, Some("Salary"), 1_000),
            w.book(None, None, -400),
            w.book(None, None, 100),
        ],
        FlowOptions::default(),
    );
    let flow = &flows[0];
    check(flow);
    let unsorted = flow
        .nodes
        .iter()
        .find(|n| n.kind == FlowNodeKind::Unsorted)
        .unwrap();
    assert_eq!((unsorted.amount.minor_units(), unsorted.column), (300, 1));
}

#[test]
fn sub_categories_get_a_column_and_the_category_itself_a_node() {
    let w = world(&[
        ("Salary", AccountKind::Income, None),
        ("Food", AccountKind::Expense, None),
        ("Groceries", AccountKind::Expense, Some("Food")),
        ("Rent", AccountKind::Expense, None),
    ]);
    let bookings = vec![
        w.book(None, Some("Salary"), 90_000),
        w.book(None, Some("Food"), -1_000),
        w.book(None, Some("Groceries"), -7_000),
        w.book(None, Some("Rent"), -50_000),
    ];
    let options = FlowOptions {
        sub_categories: true,
        payees: false,
    };
    let flows = w.flow(bookings.clone(), options);
    let flow = &flows[0];
    check(flow);
    assert_eq!(between(flow, "*", "Food"), Some(8_000));
    assert_eq!(between(flow, "Food", "Groceries"), Some(7_000));
    let itself = flow
        .nodes
        .iter()
        .find(|n| n.kind == FlowNodeKind::CategoryItself)
        .unwrap();
    assert_eq!((itself.name.as_str(), itz(itself)), ("Food", (1_000, 2)));
    // Off: no sub-category node at all.
    let flat = &w.flow(bookings, FlowOptions::default())[0];
    check(flat);
    assert!(flat.nodes.iter().all(|n| n.name != "Groceries"));
}

fn itz(node: &FlowNode) -> (i64, i8) {
    (node.amount.minor_units(), node.column)
}

#[test]
fn payees_are_one_node_across_categories_and_opposite_ones_fold_into_other() {
    let w = world(&[
        ("Salary", AccountKind::Income, None),
        ("Food", AccountKind::Expense, None),
        ("Home", AccountKind::Expense, None),
    ]);
    let flows = w.flow(
        vec![
            w.book(Some("Acme"), Some("Salary"), 90_000),
            w.book(Some("Corner Shop"), Some("Food"), -3_000),
            w.book(Some("Corner Shop"), Some("Home"), -2_000),
            w.book(Some("Big Market"), Some("Food"), -9_000),
            // A payee that gave money back in a spending category.
            w.book(Some("Big Market"), Some("Food"), 500),
            w.book(Some("Odd Payee"), Some("Food"), 400),
            w.book(None, Some("Home"), -700),
        ],
        FlowOptions {
            sub_categories: false,
            payees: true,
        },
    );
    let flow = &flows[0];
    check(flow);
    // In Food the refund of 400 nets into Other, and so does the smallest
    // payee, so the rest is positive: 3000 - 400 = 2600.
    let shop = named(flow, "Corner Shop");
    assert_eq!((shop.amount.minor_units(), shop.column), (2_000, 2));
    assert_eq!(between(flow, "Home", "Corner Shop"), Some(2_000));
    assert_eq!(between(flow, "Food", "Big Market"), Some(8_500));
    assert!(flow.nodes.iter().any(|n| n.kind == FlowNodeKind::NoPayee));
    let other = flow
        .nodes
        .iter()
        .find(|n| n.kind == FlowNodeKind::Other)
        .expect("an Other for the folded payees");
    assert_eq!(other.amount.minor_units(), 2_600);
    let held: Vec<_> = other.holds.iter().map(|(n, _)| n.as_str()).collect();
    assert!(held.contains(&"Odd Payee") && held.contains(&"Corner Shop"));
}

#[test]
fn a_column_has_at_most_eight_nodes_and_the_rest_is_other() {
    let names: [&'static str; 12] = [
        "C01", "C02", "C03", "C04", "C05", "C06", "C07", "C08", "C09", "C10", "C11", "C12",
    ];
    let mut categories = vec![("Salary", AccountKind::Income, None)];
    categories.extend(names.iter().map(|&n| (n, AccountKind::Expense, None)));
    let w = world(&categories);
    let mut bookings = vec![w.book(None, Some("Salary"), 1_000_000)];
    for (i, name) in names.iter().enumerate() {
        bookings.push(w.book(None, Some(name), -(1_000 + 100 * i as i64)));
    }
    bookings.push(w.book(None, None, -50));
    let flow = &w.flow(bookings, FlowOptions::default())[0];
    check(flow);
    let right: Vec<&FlowNode> = flow
        .nodes
        .iter()
        .filter(|n| n.column == 1 && n.kind != FlowNodeKind::LeftOver)
        .collect();
    assert_eq!(right.len(), 8, "{right:#?}");
    let other = right
        .iter()
        .find(|n| n.kind == FlowNodeKind::Other)
        .unwrap();
    // 12 categories, 6 kept next to Other and Not sorted yet: 6 are folded.
    assert_eq!(other.holds.len(), 6);
    // The smallest are in it, the largest stay.
    assert!(named(flow, "C12").amount.minor_units() > 0);
    assert!(other.holds.iter().all(|(name, _)| name != "C12"));
    assert!(right.iter().any(|n| n.kind == FlowNodeKind::Unsorted));
}

#[test]
fn each_currency_has_its_own_flow_and_transfers_count_nothing() {
    let sample = sample();
    let whole = period(date(2026, 8, 1), date(2026, 10, 31));
    let source = Source::read(&sample.book, whole).expect("source");
    let flows =
        money_flow(&source, whole, &Filter::default(), FlowOptions::default()).expect("flow");
    let codes: Vec<_> = flows.iter().map(|f| f.currency).collect();
    assert_eq!(codes, vec![EUR, USD]);
    let euro = &flows[0];
    check(euro);
    assert_eq!(flows[1].total.currency(), USD);
    check(&flows[1]);
    // Salary 3 x 2500; rent 3 x 800; groceries and food 88.20; unsorted 5.50.
    assert_eq!(euro.total.minor_units(), 750_000);
    assert_eq!(between(euro, "Rent", "*"), None);
    assert_eq!(between(euro, "*", "Rent"), Some(240_000));
    assert_eq!(between(euro, "*", "Food"), Some(8_820));
    let left = euro
        .nodes
        .iter()
        .find(|n| n.kind == FlowNodeKind::LeftOver)
        .unwrap();
    assert_eq!(left.amount.minor_units(), 750_000 - 240_000 - 8_820 - 550);
    // The 300 Transfer to savings shows nowhere.
    assert!(euro.nodes.iter().all(|n| n.name != "Savings"));
    assert_eq!(euro.volume.minor_units(), 1_003_770);
}

#[test]
fn a_book_without_money_in_the_period_gives_nothing() {
    let w = world(&[("Salary", AccountKind::Income, None)]);
    assert!(w.flow(Vec::new(), FlowOptions::default()).is_empty());
    // Income and spending that cancel leave nothing to draw.
    let flows = w.flow(
        vec![
            w.book(None, Some("Salary"), 100),
            w.book(None, Some("Salary"), -100),
        ],
        FlowOptions::default(),
    );
    assert!(flows.is_empty());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    /// Whatever is booked, with every option on or off, the bands in equal
    /// the bands out for every node and every column adds up to the total.
    #[test]
    fn the_bands_in_equal_the_bands_out(
        entries in prop::collection::vec(
            (0usize..14, 0usize..6, -2_000_000i64..2_000_000),
            1..60,
        ),
        subs in any::<bool>(),
        payees in any::<bool>(),
    ) {
        let names: [&'static str; 11] = [
            "Salary", "Gift", "Food", "Groceries", "Rent", "Fun", "Cinema",
            "Travel", "Pets", "Health", "Gear",
        ];
        let w = world(&[
            ("Salary", AccountKind::Income, None),
            ("Gift", AccountKind::Income, None),
            ("Food", AccountKind::Expense, None),
            ("Groceries", AccountKind::Expense, Some("Food")),
            ("Rent", AccountKind::Expense, None),
            ("Fun", AccountKind::Expense, None),
            ("Cinema", AccountKind::Expense, Some("Fun")),
            ("Travel", AccountKind::Expense, None),
            ("Pets", AccountKind::Expense, None),
            ("Health", AccountKind::Expense, None),
            ("Gear", AccountKind::Expense, None),
        ]);
        let payee_names = ["A", "B", "C", "D", "E"];
        let bookings: Vec<Transaction> = entries
            .iter()
            .filter(|&&(_, _, amount)| amount != 0)
            .map(|&(category, payee, amount)| {
                let category = names.get(category).copied();
                w.book(payee_names.get(payee).copied(), category, amount)
            })
            .collect();
        for flow in w.flow(bookings, FlowOptions { sub_categories: subs, payees }) {
            check(&flow);
        }
    }
}
