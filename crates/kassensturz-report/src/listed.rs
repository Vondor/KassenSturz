// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The transactions tile's list (US4.7, D-716): single transactions of a
//! period, for some accounts and categories, newest first. Nothing is added
//! up here: each row keeps the amount its account shows, in that account's
//! currency (AGENTS.md §5).

use std::collections::{HashMap, HashSet};

use kassensturz_core::{AccountId, AccountKind, CurrencyCode, Date, Money, Transaction};
use kassensturz_store::StoreError;

use crate::{Filter, Period, ReportError, Source};

/// The most rows the tile lists (D-281, D-716).
pub const MAX_LISTED: usize = 200;

/// The row counts the tile offers, the default first (`params.rows`).
pub const LISTED_ROWS: [&str; 5] = ["20", "10", "50", "100", "200"];

/// What a listed transaction's other side is, seen from the account it is
/// listed from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OtherSide {
    /// One category.
    Category(AccountId),
    /// Another of the user's accounts: a Transfer.
    Transfer(AccountId),
    /// Several Parts, or the account booked more than once.
    Parts,
    /// One part with no category yet.
    NotSorted,
}

/// One row of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    /// The transaction.
    pub transaction: kassensturz_core::TransactionId,
    /// The user's account it is listed from.
    pub account: AccountId,
    /// The date that account shows it with (D-108).
    pub date: Date,
    /// The name that account shows as From or To; may be empty.
    pub other_party: String,
    /// What it was booked against.
    pub other: OtherSide,
    /// What it did to the account, in the account's currency: positive for
    /// money in.
    pub amount: Money,
}

/// The rows and how many transactions fit in all.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ListedTransactions {
    /// The newest rows, at most the number asked for.
    pub rows: Vec<Listed>,
    /// How many transactions fit, the rows not shown included.
    pub total: usize,
}

/// The transactions booked in `period` that pass `filter`, newest first,
/// at most `limit` (and never more than [`MAX_LISTED`]) of them.
///
/// - A transaction counts by its booking date, as every figure does
///   (D-108); its row shows the date of the side it is listed from.
/// - With an accounts filter only the chosen accounts' sides are listed;
///   without one, every open account's. A transaction with no such side is
///   left out, and so is an account's starting balance.
/// - With a categories filter a transaction is listed when one of its parts
///   is booked to a chosen category or one inside it; Transfers and money
///   not sorted yet have none, so they are left out.
/// - A Transfer between two listed accounts is one row, from the side with
///   the later date, or, on the same date, from the account the money
///   reached (D-108).
///
/// Fails with [`ReportError::PeriodNotRead`] if `source` was read for fewer
/// days than `period`.
pub fn listed_transactions(
    source: &Source,
    period: Period,
    filter: &Filter,
    limit: usize,
) -> Result<ListedTransactions, ReportError> {
    let covers = source.covers();
    if period.first() < covers.first() || period.last() > covers.last() {
        return Err(ReportError::PeriodNotRead);
    }
    let accounts = source.accounts();
    let chosen_accounts: Option<HashSet<AccountId>> = filter
        .accounts
        .as_ref()
        .map(|ids| ids.iter().copied().collect());
    let chosen_categories: Option<HashSet<AccountId>> = filter.categories.as_ref().map(|ids| {
        source
            .inside(&ids.iter().copied().collect())
            .into_keys()
            .collect()
    });
    let kind_of = |id: AccountId| -> Result<&kassensturz_core::Account, ReportError> {
        accounts
            .get(&id)
            .ok_or(ReportError::Store(StoreError::UnknownAccount(id)))
    };
    let mut found = Vec::new();
    for transaction in source.transactions() {
        if !period.contains(transaction.booking_date()) {
            continue;
        }
        let mut own: Vec<AccountId> = Vec::new();
        let mut starting = false;
        for split in transaction.splits() {
            let Some(id) = split.account else { continue };
            let account = kind_of(id)?;
            if account.kind == AccountKind::Equity {
                starting = true;
            }
            if account.kind.is_account() && !own.contains(&id) {
                own.push(id);
            }
        }
        if starting || own.is_empty() {
            continue;
        }
        if let Some(chosen) = &chosen_categories
            && !transaction
                .splits()
                .iter()
                .any(|split| split.account.is_some_and(|id| chosen.contains(&id)))
        {
            continue;
        }
        let mut sides = Vec::new();
        for id in own {
            let listed = match &chosen_accounts {
                Some(chosen) => chosen.contains(&id),
                None => !kind_of(id)?.closed,
            };
            if listed {
                sides.push(side(transaction, id, accounts)?);
            }
        }
        // The later date first; on one date the side money reached.
        let Some(row) = sides
            .into_iter()
            .max_by_key(|row| (row.date, row.amount.minor_units() > 0))
        else {
            continue;
        };
        found.push(row);
    }
    // Newest first; on one day the one added last first (its ID begins with
    // the time it was added, D-067).
    found.sort_by_key(|row| core::cmp::Reverse((row.date, row.transaction)));
    let total = found.len();
    found.truncate(limit.min(MAX_LISTED));
    Ok(ListedTransactions { rows: found, total })
}

/// The row of `transaction` as `account`, one of its own accounts, shows it.
fn side(
    transaction: &Transaction,
    account: AccountId,
    accounts: &HashMap<AccountId, kassensturz_core::Account>,
) -> Result<Listed, ReportError> {
    let overflow = |_| ReportError::Store(StoreError::AmountOverflow);
    let currency: CurrencyCode = accounts
        .get(&account)
        .ok_or(ReportError::Store(StoreError::UnknownAccount(account)))?
        .currency;
    let mut amount = Money::zero(currency);
    let mut first = None;
    let mut others = Vec::new();
    for split in transaction.splits() {
        if split.account == Some(account) {
            amount = amount
                .checked_add(split.account_amount())
                .map_err(overflow)?;
            first.get_or_insert(split);
        } else {
            others.push(split.account);
        }
    }
    let first = first.ok_or(ReportError::Store(StoreError::UnknownAccount(account)))?;
    let booked_twice = transaction
        .splits()
        .iter()
        .filter(|split| split.account == Some(account))
        .count()
        > 1;
    let other = match others.as_slice() {
        [None] if !booked_twice => OtherSide::NotSorted,
        [Some(id)] if !booked_twice => {
            let other = accounts
                .get(id)
                .ok_or(ReportError::Store(StoreError::UnknownAccount(*id)))?;
            if other.kind.is_account() {
                OtherSide::Transfer(*id)
            } else {
                OtherSide::Category(*id)
            }
        }
        _ => OtherSide::Parts,
    };
    Ok(Listed {
        transaction: transaction.id(),
        account,
        date: transaction.date_of(first),
        other_party: transaction.payee_of(first).unwrap_or_default().to_owned(),
        other,
        amount,
    })
}

#[cfg(test)]
#[path = "listed_tests.rs"]
mod tests;
