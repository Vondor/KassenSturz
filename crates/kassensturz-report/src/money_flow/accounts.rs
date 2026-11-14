// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The money flow with *Show accounts* on (US7, #821, D-758, §6 of
//! `docs/MONEY_FLOW.md`): the chosen accounts replace *Your money*. Income
//! flows into the account it arrived in, spending out of the account it was
//! paid from, a Transfer between two chosen accounts is a band between them
//! and a Transfer to or from an account that is not chosen is a band to or
//! from *To / From other accounts*. Every account node has its own *Left
//! over* or *From your reserves* band, so per account the bands in equal the
//! bands out exactly.

use std::collections::{BTreeMap, HashMap, HashSet};

use kassensturz_core::{AccountId, AccountKind, Transaction};

use super::{Atom, Builder, FlowNodeKind, add, drop_unused, overflow};
use crate::{ReportError, Source};

/// A Transfer: from an account (`None`: one that is not chosen), to an
/// account, an amount above zero.
pub(super) type Transfer = (Option<AccountId>, Option<AccountId>, i64);

/// One category part of a transaction, or a share of it, and the chosen
/// account its money passed through.
pub(super) struct Piece {
    /// Which split of the transaction it belongs to.
    pub(super) split: usize,
    /// The account.
    pub(super) account: AccountId,
    /// How much, above zero.
    pub(super) amount: i64,
    /// Whether the money came in from the category (else it went out to it).
    pub(super) money_in: bool,
}

/// What a transaction moved through the chosen accounts.
pub(super) struct Moved {
    pub(super) pieces: Vec<Piece>,
    pub(super) transfers: Vec<Transfer>,
}

fn is_own(kind: AccountKind) -> bool {
    matches!(kind, AccountKind::Asset | AccountKind::Liability)
}

/// The own accounts a book has: what *Show accounts* shows when none is
/// chosen.
pub(super) fn own_accounts(source: &Source) -> HashSet<AccountId> {
    source
        .accounts()
        .values()
        .filter(|account| is_own(account.kind))
        .map(|account| account.id)
        .collect()
}

/// Takes `need` from `pool`, the chosen accounts first, and says how much
/// each gave.
fn draw(pool: &mut [(Option<AccountId>, i64)], mut need: i64) -> Vec<(Option<AccountId>, i64)> {
    let mut taken = Vec::new();
    for chosen in [true, false] {
        for entry in pool.iter_mut().filter(|entry| entry.0.is_some() == chosen) {
            let part = need.min(entry.1);
            if part > 0 {
                entry.1 -= part;
                need -= part;
                taken.push((entry.0, part));
            }
        }
    }
    taken
}

/// Works out through which chosen account each category part of `tx` went
/// and which Transfers it holds. Money into an account (a positive part of
/// it) pays what flows in from categories; money out of one pays what flows
/// out to them; what is left of the accounts' parts is a Transfer from the
/// ones that gave to the ones that received. `None` when the transaction
/// touches no chosen account or holds a starting balance.
pub(super) fn allocate(
    source: &Source,
    tx: &Transaction,
    chosen: &HashSet<AccountId>,
) -> Result<Option<Moved>, ReportError> {
    let mut into: Vec<(Option<AccountId>, i64)> = Vec::new();
    let mut out: Vec<(Option<AccountId>, i64)> = Vec::new();
    let mut first_chosen = None;
    let mut categories: Vec<(usize, i64)> = Vec::new();
    for (index, split) in tx.splits().iter().enumerate() {
        let amount = split.amount.minor_units();
        if amount == 0 {
            continue;
        }
        let kind = split
            .account
            .and_then(|id| source.accounts().get(&id))
            .map(|account| account.kind);
        match kind {
            Some(AccountKind::Equity) => return Ok(None),
            Some(kind) if is_own(kind) => {
                let id = split.account.filter(|id| chosen.contains(id));
                if id.is_some() && first_chosen.is_none() {
                    first_chosen = id;
                }
                if amount > 0 {
                    into.push((id, amount));
                } else {
                    out.push((id, amount.checked_neg().ok_or_else(overflow)?));
                }
            }
            // A category, or a part not sorted yet: what it brings in is
            // positive.
            _ => categories.push((index, amount.checked_neg().ok_or_else(overflow)?)),
        }
    }
    let Some(first) = first_chosen else {
        return Ok(None);
    };
    let mut moved = Moved {
        pieces: Vec::new(),
        transfers: Vec::new(),
    };
    for (split, brings) in categories {
        let money_in = brings > 0;
        let need = brings.checked_abs().ok_or_else(overflow)?;
        let pool = if money_in { &mut into } else { &mut out };
        for (account, amount) in draw(pool, need) {
            match account {
                Some(account) => moved.pieces.push(Piece {
                    split,
                    account,
                    amount,
                    money_in,
                }),
                None => {
                    // Paid from or into an account that is not chosen: its
                    // money passes through the first chosen account.
                    moved.pieces.push(Piece {
                        split,
                        account: first,
                        amount,
                        money_in,
                    });
                    moved.transfers.push(if money_in {
                        (Some(first), None, amount)
                    } else {
                        (None, Some(first), amount)
                    });
                }
            }
        }
    }
    // What is left of the accounts' parts moves between them.
    for payer in out.iter_mut() {
        for receiver in into.iter_mut() {
            let part = payer.1.min(receiver.1);
            if part == 0 || (payer.0.is_none() && receiver.0.is_none()) || payer.0 == receiver.0 {
                continue;
            }
            payer.1 -= part;
            receiver.1 -= part;
            moved.transfers.push((payer.0, receiver.0, part));
        }
    }
    Ok(Some(moved))
}

/// Connects the top-level category node `node` of `side` to the accounts
/// its money passed through. An account whose part has the other direction
/// (a refund paid back into another account) is cancelled against the
/// largest ones, so every band is positive and the bands add up to the node.
pub(super) fn connect(
    b: &mut Builder<'_>,
    side: i8,
    node: usize,
    atoms: &[Atom],
) -> Result<(), ReportError> {
    let mut by_account: BTreeMap<AccountId, i64> = BTreeMap::new();
    for atom in atoms {
        let Some(account) = atom.account else {
            continue;
        };
        let value = if side < 0 {
            atom.signed
        } else {
            atom.signed.checked_neg().ok_or_else(overflow)?
        };
        let entry = by_account.entry(account).or_insert(0);
        *entry = add(*entry, value)?;
    }
    let mut owed = 0_i64;
    let mut kept: Vec<(AccountId, i64)> = Vec::new();
    for (account, value) in by_account {
        if value < 0 {
            owed = add(owed, value.checked_neg().ok_or_else(overflow)?)?;
        } else if value > 0 {
            kept.push((account, value));
        }
    }
    kept.sort_by_key(|&(account, value)| (std::cmp::Reverse(value), account));
    for (account, value) in kept {
        let cancelled = owed.min(value);
        owed -= cancelled;
        let value = value - cancelled;
        if value > 0 {
            let target = account_node(b, account);
            b.band(side, node, target, value);
        }
    }
    Ok(())
}

/// The node of `account`, made when first needed.
fn account_node(b: &mut Builder<'_>, account: AccountId) -> usize {
    if let Some(&node) = b.accounts.get(&account) {
        return node;
    }
    let name = b.name(account);
    let node = b.node(FlowNodeKind::Account, name, 0, 0);
    b.nodes[node].account = Some(account);
    b.accounts.insert(account, node);
    node
}

/// Finishes the chart with accounts: the Transfers between them and to or
/// from other accounts, each account's own balance band, the columns, and
/// the removal of the middle node.
pub(super) fn finish(
    b: &mut Builder<'_>,
    middle: usize,
    transfers: &[Transfer],
) -> Result<(), ReportError> {
    // Net Transfers between two chosen accounts, and add up the others.
    let mut pairs: BTreeMap<(AccountId, AccountId), i64> = BTreeMap::new();
    let mut from_other: BTreeMap<AccountId, i64> = BTreeMap::new();
    let mut to_other: BTreeMap<AccountId, i64> = BTreeMap::new();
    for &(from, to, amount) in transfers {
        match (from, to) {
            (Some(from), Some(to)) if from != to => {
                let (key, signed) = if from < to {
                    ((from, to), amount)
                } else {
                    ((to, from), amount.checked_neg().ok_or_else(overflow)?)
                };
                let entry = pairs.entry(key).or_insert(0);
                *entry = add(*entry, signed)?;
            }
            (None, Some(to)) => {
                let entry = from_other.entry(to).or_insert(0);
                *entry = add(*entry, amount)?;
            }
            (Some(from), None) => {
                let entry = to_other.entry(from).or_insert(0);
                *entry = add(*entry, amount)?;
            }
            _ => {}
        }
    }
    let edges: Vec<(AccountId, AccountId, i64)> = pairs
        .into_iter()
        .filter(|&(_, net)| net != 0)
        .map(|((a, c), net)| if net > 0 { (a, c, net) } else { (c, a, -net) })
        .collect();
    for &(from, to, _) in &edges {
        account_node(b, from);
        account_node(b, to);
    }
    for &account in from_other.keys().chain(to_other.keys()) {
        account_node(b, account);
    }
    // The accounts that give most go first, so most Transfers go from left
    // to right; a Transfer that goes back is a return band.
    let mut balance: HashMap<AccountId, i64> = HashMap::new();
    for &(from, to, amount) in &edges {
        *balance.entry(from).or_insert(0) += amount;
        *balance.entry(to).or_insert(0) -= amount;
    }
    let mut order: Vec<AccountId> = b.accounts.keys().copied().collect();
    order.sort_by_key(|&account| {
        (
            std::cmp::Reverse(balance.get(&account).copied().unwrap_or(0)),
            b.name(account),
            account,
        )
    });
    let place: HashMap<AccountId, usize> = order
        .iter()
        .enumerate()
        .map(|(at, &account)| (account, at))
        .collect();
    let mut depth: HashMap<AccountId, i8> = order.iter().map(|&account| (account, 0)).collect();
    for &account in &order {
        let deepest = edges
            .iter()
            .filter(|&&(from, to, _)| to == account && place[&from] < place[&to])
            .map(|&(from, _, _)| depth[&from] + 1)
            .max()
            .unwrap_or(0);
        depth.insert(account, deepest);
    }
    let width = depth.values().copied().max().unwrap_or(0) + 1;
    // The spending side moves right of the accounts' columns.
    for node in &mut b.nodes {
        if node.column > 0 {
            node.column += width - 1;
        }
    }
    for (&account, &node) in &b.accounts {
        b.nodes[node].column = depth[&account];
    }
    for &(from, to, amount) in &edges {
        let (from, to) = (b.accounts[&from], b.accounts[&to]);
        b.flows.push((from, to, amount));
    }
    for (side, list) in [(-1_i8, &from_other), (1, &to_other)] {
        if list.is_empty() {
            continue;
        }
        let mut sum = 0_i64;
        for amount in list.values() {
            sum = add(sum, *amount)?;
        }
        let column = if side < 0 { -1 } else { width };
        let node = b.node(FlowNodeKind::OtherAccounts, String::new(), column, sum);
        for (&account, &amount) in list {
            let account = b.accounts[&account];
            if side < 0 {
                b.flows.push((node, account, amount));
            } else {
                b.flows.push((account, node, amount));
            }
        }
    }
    // What each account keeps or draws from its reserves.
    let mut left_over: Option<usize> = None;
    let mut reserves: Option<usize> = None;
    let mut accounts: Vec<(AccountId, usize)> = b.accounts.iter().map(|(&a, &n)| (a, n)).collect();
    accounts.sort_by_key(|&(account, _)| place[&account]);
    for (_, node) in accounts {
        let mut into = 0_i64;
        let mut out = 0_i64;
        for &(from, to, amount) in &b.flows {
            if to == node {
                into = add(into, amount)?;
            }
            if from == node {
                out = add(out, amount)?;
            }
        }
        if into > out {
            let extra = into - out;
            let target = match left_over {
                Some(target) => {
                    let sum = add(b.nodes[target].amount.minor_units(), extra)?;
                    b.nodes[target].amount = b.money(sum);
                    target
                }
                None => {
                    let target = b.node(FlowNodeKind::LeftOver, String::new(), width, extra);
                    left_over = Some(target);
                    target
                }
            };
            b.flows.push((node, target, extra));
        } else if out > into {
            let extra = out - into;
            let source = match reserves {
                Some(source) => {
                    let sum = add(b.nodes[source].amount.minor_units(), extra)?;
                    b.nodes[source].amount = b.money(sum);
                    source
                }
                None => {
                    let source = b.node(FlowNodeKind::Reserves, String::new(), -1, extra);
                    reserves = Some(source);
                    source
                }
            };
            b.flows.push((source, node, extra));
        }
        b.nodes[node].amount = b.money(into.max(out));
    }
    drop_unused(b, &HashMap::from([(middle, middle)]));
    Ok(())
}

/// The widest column's sum, which the layout scales to.
pub(super) fn widest_column(b: &Builder<'_>) -> Result<i64, ReportError> {
    let mut sums: HashMap<i8, i64> = HashMap::new();
    for node in &b.nodes {
        let entry = sums.entry(node.column).or_insert(0);
        *entry = add(*entry, node.amount.minor_units())?;
    }
    Ok(sums.values().copied().max().unwrap_or(0))
}
