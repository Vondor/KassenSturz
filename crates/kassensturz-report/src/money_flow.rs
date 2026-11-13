// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `money_flow`: where money came from and where it went (US7, #821,
//! D-758, `docs/MONEY_FLOW.md`). Nodes and the bands between them, in exact
//! minor units; the layout is the charts crate's.
//!
//! Every column of the result adds up to the same total, and for every node
//! the bands in equal the bands out.

use std::collections::HashMap;

use kassensturz_core::{AccountId, CurrencyCode, Money};
use kassensturz_store::StoreError;

use crate::source::Kind;
use crate::{Filter, Period, ReportError, Source};

/// The dataset's ID, as dashboards and the AI name it (D-277).
pub const MONEY_FLOW: &str = "money_flow";

/// The most nodes in one column on one side (D-758): the largest ones and
/// *Other (n)*.
pub const MAX_PER_COLUMN: usize = 8;

/// What the user chose for the chart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlowOptions {
    /// Sub-categories get a column of their own behind their category.
    pub sub_categories: bool,
    /// Payees get a column at the outer edge.
    pub payees: bool,
}

/// What a node stands for. The words the screen shows for the kinds without
/// a name are the screen's (D-034).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlowNodeKind {
    /// A payee; the name is the payee's.
    Payee,
    /// Money with no payee.
    NoPayee,
    /// A category; the name is the category's.
    Category,
    /// What was booked to a category itself, when its sub-categories are
    /// shown; the name is the category's.
    CategoryItself,
    /// Money not sorted into a category yet.
    Unsorted,
    /// The middle node, *Your money*.
    Middle,
    /// What is left when income is larger than spending.
    LeftOver,
    /// What makes up the gap when spending is larger than income.
    Reserves,
    /// *Other (n)*: what the column's limit folded together, listed in
    /// `holds`.
    Other,
}

/// One node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlowNode {
    /// What it stands for.
    pub kind: FlowNodeKind,
    /// The name from the book (category or payee); empty for the kinds the
    /// screen names.
    pub name: String,
    /// The place from the middle: negative on the income side (left),
    /// positive on the spending side (right), 0 for the middle node. 1 is
    /// the top-level categories, 2 their sub-categories (when shown), then
    /// the payees (2, or 3 when sub-categories are shown).
    pub column: i8,
    /// What flows through it; never negative or zero.
    pub amount: Money,
    /// For a category: its ID, so a click can open its transactions.
    pub category: Option<AccountId>,
    /// For *Other*: what it holds, largest first, with each name (empty for
    /// money not sorted yet or without a payee) and amount.
    pub holds: Vec<(String, Money)>,
}

/// A band from one node to another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flow {
    /// The index in [`MoneyFlow::nodes`] it leaves.
    pub from: usize,
    /// The index it enters.
    pub to: usize,
    /// Its amount, more than zero.
    pub amount: Money,
}

/// The money flow of one currency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoneyFlow {
    /// The currency of every amount.
    pub currency: CurrencyCode,
    /// The nodes, the middle one first.
    pub nodes: Vec<FlowNode>,
    /// The bands, each pair of nodes at most once.
    pub flows: Vec<Flow>,
    /// What passes through the middle node: the larger of income and
    /// spending.
    pub total: Money,
    /// Everything that moved in this currency, income and spending
    /// together, so the screen can name what it does not show.
    pub volume: Money,
}

/// One part of a transaction, with its amount positive when it brings money
/// in and negative when it takes money out.
#[derive(Clone, Debug)]
struct Atom {
    top: Option<AccountId>,
    category: Option<AccountId>,
    payee: Option<String>,
    signed: i64,
}

/// A group of atoms under one name.
struct Group {
    key: Key,
    net: i64,
    atoms: Vec<Atom>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Key {
    Category(Option<AccountId>),
    Payee(Option<String>),
}

fn overflow() -> ReportError {
    ReportError::Store(StoreError::AmountOverflow)
}

fn add(a: i64, b: i64) -> Result<i64, ReportError> {
    a.checked_add(b).ok_or_else(overflow)
}

/// Where money came from and where it went in `period`, one result per
/// currency in the order of their codes. Transfers between the user's own
/// accounts and starting balances are not counted (they are neither income
/// nor spending, so money that moves through several accounts counts once);
/// a refund is netted inside its category.
pub fn money_flow(
    source: &Source,
    period: Period,
    filter: &Filter,
    options: FlowOptions,
) -> Result<Vec<MoneyFlow>, ReportError> {
    let mut atoms: HashMap<CurrencyCode, Vec<Atom>> = HashMap::new();
    for part in source.parts(period, filter)? {
        let transaction = &source.transactions()[part.transaction];
        let split = &transaction.splits()[part.split];
        let amount = part.amount.minor_units();
        let signed = match part.kind {
            Kind::Income => amount,
            Kind::Spending => amount.checked_neg().ok_or_else(overflow)?,
        };
        atoms.entry(part.currency).or_default().push(Atom {
            top: part.category.map(|id| source.top_level(id)),
            category: part.category,
            payee: transaction
                .payee_of(split)
                .map(str::trim)
                .filter(|payee| !payee.is_empty())
                .map(str::to_owned),
            signed,
        });
    }
    let mut currencies: Vec<CurrencyCode> = atoms.keys().copied().collect();
    currencies.sort_unstable();
    let mut results = Vec::new();
    for currency in currencies {
        if let Some(flow) = build(source, currency, &atoms[&currency], options)? {
            results.push(flow);
        }
    }
    Ok(results)
}

struct Builder<'a> {
    source: &'a Source,
    currency: CurrencyCode,
    nodes: Vec<FlowNode>,
    flows: Vec<(usize, usize, i64)>,
    /// The payee nodes already made, by side and payee.
    payees: HashMap<(i8, Option<String>), usize>,
    /// The *Other* node of the payee column of a side.
    payee_other: HashMap<i8, usize>,
}

impl Builder<'_> {
    fn money(&self, minor: i64) -> Money {
        Money::from_minor_units(minor, self.currency)
    }

    fn node(&mut self, kind: FlowNodeKind, name: String, column: i8, amount: i64) -> usize {
        let amount = self.money(amount);
        self.nodes.push(FlowNode {
            kind,
            name,
            column,
            amount,
            category: None,
            holds: Vec::new(),
        });
        self.nodes.len() - 1
    }

    fn name(&self, id: AccountId) -> String {
        self.source
            .accounts()
            .get(&id)
            .map(|account| account.name.clone())
            .unwrap_or_default()
    }

    /// A band between a node of this side and the one next to it towards
    /// the middle: income bands go towards the middle, spending bands away.
    fn band(&mut self, side: i8, outer: usize, inner: usize, amount: i64) {
        if side < 0 {
            self.flows.push((outer, inner, amount));
        } else {
            self.flows.push((inner, outer, amount));
        }
    }
}

/// Splits `atoms` by `key` into groups with their nets.
fn group_by(atoms: &[Atom], key: impl Fn(&Atom) -> Key) -> Result<Vec<Group>, ReportError> {
    let mut groups: Vec<Group> = Vec::new();
    let mut at: HashMap<Key, usize> = HashMap::new();
    for atom in atoms {
        let key = key(atom);
        let place = *at.entry(key.clone()).or_insert_with(|| {
            groups.push(Group {
                key,
                net: 0,
                atoms: Vec::new(),
            });
            groups.len() - 1
        });
        groups[place].net = add(groups[place].net, atom.signed)?;
        groups[place].atoms.push(atom.clone());
    }
    groups.retain(|group| group.net != 0);
    Ok(groups)
}

/// What `distribute` made of a node's children: the ones that stay, and the
/// rest (with the groups it holds), which is never negative.
struct Split {
    kept: Vec<Group>,
    rest: Option<Group>,
    folded: Vec<Group>,
}

/// Divides what flows through a node on `side` among its children. A child
/// whose net has the opposite sign is netted into the rest, and so are as
/// many of the smallest others as it takes for the rest to be positive, so
/// every band is positive and the sums stay exact.
fn distribute(side: i8, groups: Vec<Group>) -> Result<Split, ReportError> {
    // Make the side's own direction positive.
    let mut matching: Vec<Group> = Vec::new();
    let mut folded: Vec<Group> = Vec::new();
    for mut group in groups {
        group.net = if side < 0 {
            group.net
        } else {
            group.net.checked_neg().ok_or_else(overflow)?
        };
        if group.net > 0 {
            matching.push(group);
        } else {
            folded.push(group);
        }
    }
    let mut rest = 0_i64;
    for group in &folded {
        rest = add(rest, group.net)?;
    }
    if folded.is_empty() {
        return Ok(Split {
            kept: matching,
            rest: None,
            folded,
        });
    }
    // The smallest first, so the largest stay.
    matching.sort_by_key(|group| std::cmp::Reverse(group.net));
    while rest <= 0 {
        let Some(group) = matching.pop() else { break };
        rest = add(rest, group.net)?;
        folded.push(group);
    }
    let atoms = folded
        .iter()
        .flat_map(|group| group.atoms.clone())
        .collect();
    let rest = (rest > 0).then_some(Group {
        key: Key::Payee(None),
        net: rest,
        atoms,
    });
    Ok(Split {
        kept: matching,
        rest,
        folded,
    })
}

fn build(
    source: &Source,
    currency: CurrencyCode,
    atoms: &[Atom],
    options: FlowOptions,
) -> Result<Option<MoneyFlow>, ReportError> {
    let mut b = Builder {
        source,
        currency,
        nodes: Vec::new(),
        flows: Vec::new(),
        payees: HashMap::new(),
        payee_other: HashMap::new(),
    };
    let middle = b.node(FlowNodeKind::Middle, String::new(), 0, 0);
    let mut volume = 0_i64;
    for atom in atoms {
        volume = add(volume, atom.signed.checked_abs().ok_or_else(overflow)?)?;
    }
    let mut tops = group_by(atoms, |atom| Key::Category(atom.top))?;
    tops.sort_by_key(|group| {
        (
            std::cmp::Reverse(group.net.unsigned_abs()),
            match group.key {
                Key::Category(Some(id)) => b.name(id),
                _ => String::new(),
            },
        )
    });
    let (mut income, mut spending) = (0_i64, 0_i64);
    for top in tops {
        let side: i8 = if top.net > 0 { -1 } else { 1 };
        let amount = top.net.checked_abs().ok_or_else(overflow)?;
        let Key::Category(id) = top.key else { continue };
        let (kind, name) = match id {
            Some(id) => (FlowNodeKind::Category, b.name(id)),
            None => (FlowNodeKind::Unsorted, String::new()),
        };
        let node = b.node(kind, name.clone(), side.signum(), amount);
        b.nodes[node].category = id;
        b.band(side, node, middle, amount);
        if side < 0 {
            income = add(income, amount)?;
        } else {
            spending = add(spending, amount)?;
        }
        // The leaves under the top-level category.
        let real_subs = top.atoms.iter().any(|atom| atom.category != id);
        // With sub-categories on, a category without any still gets its node
        // in the column behind it, so that column adds up to the total like
        // the others and an *Other* of the first column passes on all of its
        // amount (B-264).
        let level_two = options.sub_categories;
        let payee_column = if options.sub_categories { 3 } else { 2 };
        let leaves: Vec<(usize, i64, Vec<Atom>)> = if level_two {
            let by_category = group_by(&top.atoms, |atom| Key::Category(atom.category))?;
            let split = distribute(side, by_category)?;
            let mut leaves = Vec::new();
            let mut others = Vec::new();
            for group in split.kept {
                let Key::Category(category) = group.key else {
                    continue;
                };
                let (kind, name) = match category {
                    Some(c) if Some(c) == id && real_subs => {
                        (FlowNodeKind::CategoryItself, b.name(c))
                    }
                    Some(c) => (FlowNodeKind::Category, b.name(c)),
                    None => (FlowNodeKind::Unsorted, String::new()),
                };
                let leaf = b.node(kind, name, side * 2, group.net);
                b.nodes[leaf].category = category;
                b.band(side, leaf, node, group.net);
                leaves.push((leaf, group.net, group.atoms));
            }
            if let Some(rest) = split.rest {
                let holds = split
                    .folded
                    .iter()
                    .map(|group| {
                        let name = match group.key {
                            Key::Category(Some(c)) => b.name(c),
                            _ => String::new(),
                        };
                        (name, group.net)
                    })
                    .collect::<Vec<_>>();
                let leaf = b.node(FlowNodeKind::Other, String::new(), side * 2, rest.net);
                others.push((leaf, holds));
                b.band(side, leaf, node, rest.net);
                leaves.push((leaf, rest.net, rest.atoms));
            }
            for (leaf, holds) in others {
                b.nodes[leaf].holds = holds
                    .into_iter()
                    .map(|(name, net)| (name, Money::from_minor_units(net, currency)))
                    .collect();
            }
            leaves
        } else {
            vec![(node, amount, top.atoms.clone())]
        };
        if options.payees {
            for (leaf, leaf_amount, leaf_atoms) in leaves {
                let _ = leaf_amount;
                let by_payee = group_by(&leaf_atoms, |atom| Key::Payee(atom.payee.clone()))?;
                let split = distribute(side, by_payee)?;
                for group in split.kept {
                    let Key::Payee(payee) = group.key.clone() else {
                        continue;
                    };
                    let target = payee_node(&mut b, side, payee_column, payee, group.net)?;
                    b.band(side, target, leaf, group.net);
                }
                if let Some(rest) = split.rest {
                    let other = payee_other(&mut b, side, payee_column, rest.net)?;
                    for group in &split.folded {
                        let name = match &group.key {
                            Key::Payee(Some(name)) => name.clone(),
                            _ => String::new(),
                        };
                        b.nodes[other]
                            .holds
                            .push((name, Money::from_minor_units(group.net, currency)));
                    }
                    b.band(side, other, leaf, rest.net);
                }
            }
        }
    }
    if income == 0 && spending == 0 {
        return Ok(None);
    }
    b.nodes[middle].amount = b.money(income.max(spending));
    // The columns' limit.
    for side in [-1_i8, 1] {
        for column in 1..=3 {
            cap(&mut b, side * column)?;
        }
    }
    // The balance node.
    if income > spending {
        let left = income - spending;
        let node = b.node(FlowNodeKind::LeftOver, String::new(), 1, left);
        b.flows.push((middle, node, left));
    } else if spending > income {
        let gap = spending - income;
        let node = b.node(FlowNodeKind::Reserves, String::new(), -1, gap);
        b.flows.push((node, middle, gap));
    }
    let flows = b
        .flows
        .iter()
        .map(|&(from, to, amount)| Flow {
            from,
            to,
            amount: Money::from_minor_units(amount, currency),
        })
        .collect();
    Ok(Some(MoneyFlow {
        currency,
        nodes: b.nodes,
        flows,
        total: Money::from_minor_units(income.max(spending), currency),
        volume: Money::from_minor_units(volume, currency),
    }))
}

fn payee_node(
    b: &mut Builder<'_>,
    side: i8,
    column: i8,
    payee: Option<String>,
    amount: i64,
) -> Result<usize, ReportError> {
    let at = (side, payee.clone());
    if let Some(&node) = b.payees.get(&at) {
        let sum = add(b.nodes[node].amount.minor_units(), amount)?;
        b.nodes[node].amount = b.money(sum);
        return Ok(node);
    }
    let (kind, name) = match payee {
        Some(name) => (FlowNodeKind::Payee, name),
        None => (FlowNodeKind::NoPayee, String::new()),
    };
    let node = b.node(kind, name, side * column, amount);
    b.payees.insert(at, node);
    Ok(node)
}

fn payee_other(
    b: &mut Builder<'_>,
    side: i8,
    column: i8,
    amount: i64,
) -> Result<usize, ReportError> {
    if let Some(&node) = b.payee_other.get(&side) {
        let sum = add(b.nodes[node].amount.minor_units(), amount)?;
        b.nodes[node].amount = b.money(sum);
        return Ok(node);
    }
    let node = b.node(FlowNodeKind::Other, String::new(), side * column, amount);
    b.payee_other.insert(side, node);
    Ok(node)
}

/// Folds the smallest nodes of `column` (negative on the income side) into
/// one *Other* when it has more than [`MAX_PER_COLUMN`].
fn cap(b: &mut Builder<'_>, column: i8) -> Result<(), ReportError> {
    let members: Vec<usize> = (0..b.nodes.len())
        .filter(|&i| b.nodes[i].column == column)
        .collect();
    if members.len() <= MAX_PER_COLUMN {
        return Ok(());
    }
    // The nodes that may be folded, largest first.
    let mut free: Vec<usize> = members
        .iter()
        .copied()
        .filter(|&i| {
            !matches!(
                b.nodes[i].kind,
                FlowNodeKind::Unsorted | FlowNodeKind::Other
            )
        })
        .collect();
    free.sort_by(|&x, &y| {
        b.nodes[y]
            .amount
            .minor_units()
            .cmp(&b.nodes[x].amount.minor_units())
            .then_with(|| b.nodes[x].name.cmp(&b.nodes[y].name))
    });
    let others: Vec<usize> = members
        .iter()
        .copied()
        .filter(|&i| b.nodes[i].kind == FlowNodeKind::Other)
        .collect();
    let fixed = members.len() - free.len() - others.len();
    // Room for the named ones: the limit less Other and the fixed ones.
    let keep = MAX_PER_COLUMN.saturating_sub(1 + fixed);
    if free.len() <= keep {
        return Ok(());
    }
    let folding: Vec<usize> = free.split_off(keep);
    let target = match others.first() {
        Some(&other) => other,
        None => b.node(FlowNodeKind::Other, String::new(), column, 0),
    };
    let mut remap: HashMap<usize, usize> = HashMap::new();
    let mut amount = b.nodes[target].amount.minor_units();
    for &node in &folding {
        remap.insert(node, target);
        amount = add(amount, b.nodes[node].amount.minor_units())?;
        let held = (b.nodes[node].name.clone(), b.nodes[node].amount);
        b.nodes[target].holds.push(held);
    }
    for &extra in others.iter().skip(1) {
        remap.insert(extra, target);
        amount = add(amount, b.nodes[extra].amount.minor_units())?;
        let held = b.nodes[extra].holds.clone();
        b.nodes[target].holds.extend(held);
    }
    b.nodes[target].amount = b.money(amount);
    b.nodes[target]
        .holds
        .sort_by_key(|(_, amount)| std::cmp::Reverse(amount.minor_units()));
    // Point the bands at the *Other* node and merge the ones that now share
    // both ends.
    let mut merged: Vec<(usize, usize, i64)> = Vec::new();
    for &(from, to, amount) in &b.flows {
        let from = remap.get(&from).copied().unwrap_or(from);
        let to = remap.get(&to).copied().unwrap_or(to);
        match merged.iter_mut().find(|m| m.0 == from && m.1 == to) {
            Some(m) => m.2 = add(m.2, amount)?,
            None => merged.push((from, to, amount)),
        }
    }
    b.flows = merged;
    // The folded nodes stay in the list with no band; drop them and renumber.
    drop_unused(b, &remap);
    Ok(())
}

/// Removes the nodes `gone` names and renumbers the bands.
fn drop_unused(b: &mut Builder<'_>, gone: &HashMap<usize, usize>) {
    let mut new_index = vec![0_usize; b.nodes.len()];
    let mut kept = Vec::with_capacity(b.nodes.len());
    for (index, node) in std::mem::take(&mut b.nodes).into_iter().enumerate() {
        if gone.contains_key(&index) {
            continue;
        }
        new_index[index] = kept.len();
        kept.push(node);
    }
    b.nodes = kept;
    for flow in &mut b.flows {
        flow.0 = new_index[flow.0];
        flow.1 = new_index[flow.1];
    }
    for node in b.payees.values_mut() {
        *node = new_index[*node];
    }
    for node in b.payee_other.values_mut() {
        *node = new_index[*node];
    }
}
