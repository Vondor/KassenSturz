// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `cash_flow_by_month`: the money that went into and out of the chosen
//! accounts in each calendar month, with what they held at the start and
//! the end (D-348, D-366).

use std::collections::HashMap;

use kassensturz_core::{AccountId, AccountKind, CurrencyCode, Date, Money, TransactionId};
use kassensturz_store::StoreError;

use crate::balances::Moment;
use crate::{Filter, Period, ReportError, Source, TextFormat, TextTable};

/// The dataset's ID, as dashboards and the AI name it (D-348).
pub const CASH_FLOW_BY_MONTH: &str = "cash_flow_by_month";

fn overflow<E>(_: E) -> ReportError {
    ReportError::Store(StoreError::AmountOverflow)
}

/// One calendar month of the chosen accounts together. Always
/// `start + money_in − money_out + starting = end`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CashFlowRow {
    /// The month's first day. A first or last month the period covers only
    /// in part counts only the days inside the period.
    pub month: Date,
    /// What the accounts held at the start of the month's first day in the
    /// period.
    pub start: Money,
    /// What came in: each transaction's net amount on the chosen accounts
    /// where it is above zero.
    pub money_in: Money,
    /// What went out, as a positive amount: each transaction's net amount
    /// where it is below zero.
    pub money_out: Money,
    /// The starting balances set in the month, as booked: negative for a
    /// debt or an overdraft.
    pub starting: Money,
    /// `money_in` less `money_out`; starting balances are not in it.
    pub net_change: Money,
    /// What the accounts held at the end of the month's last day in the
    /// period.
    pub end: Money,
}

/// The months of one currency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CashFlowByMonth {
    /// The currency of every amount.
    pub currency: CurrencyCode,
    /// Every month the period touches, oldest first, also months in which
    /// nothing happened.
    pub rows: Vec<CashFlowRow>,
    /// The whole period: from the first month's start to the last month's
    /// end, with everything in between added up.
    pub total: CashFlowRow,
}

/// The accounts the dataset counts: those `filter` chooses that are the
/// user's own, or without a choice every account of kind asset (checking,
/// savings, cash), closed ones included.
fn chosen<'a>(source: &'a Source, filter: &Filter) -> Vec<&'a kassensturz_core::Account> {
    source
        .own_accounts(filter)
        .into_iter()
        .filter(|account| filter.accounts.is_some() || account.kind == AccountKind::Asset)
        .collect()
}

/// The money that went into and out of the chosen accounts in each month of
/// `period`, one result per currency in the order of their codes. A
/// transaction counts with its net amount on the chosen accounts, per
/// currency and month, so a move between two of them counts as nothing. A
/// currency is listed if one of its chosen accounts is open or a figure is
/// not zero. `filter.categories` does not apply.
pub fn cash_flow_by_month(
    source: &Source,
    period: Period,
    filter: &Filter,
) -> Result<Vec<CashFlowByMonth>, ReportError> {
    source.require(period)?;
    let accounts = chosen(source, filter);
    let currency_of: HashMap<AccountId, CurrencyCode> = accounts
        .iter()
        .map(|account| (account.id, account.currency))
        .collect();
    let months = period.months();
    let mut moments = Vec::with_capacity(months.len() * 2);
    for &month in &months {
        moments.push(Moment::Start(month.max(period.first())));
        moments.push(Moment::End(month.last_of_month().min(period.last())));
    }
    let balances = source.balances_at(&moments)?;

    // Each transaction's net amount per currency and month, and whether it
    // sets a starting balance.
    let mut nets: HashMap<(TransactionId, CurrencyCode, usize), (Money, bool)> = HashMap::new();
    for movement in source.holdings()?.movements() {
        let Some(&currency) = currency_of.get(&movement.account) else {
            continue;
        };
        if !period.contains(movement.day) {
            continue;
        }
        let Ok(at) = months.binary_search(&movement.day.first_of_month()) else {
            continue;
        };
        let net = nets
            .entry((movement.transaction, currency, at))
            .or_insert((Money::zero(currency), movement.starting));
        net.0 = net.0.checked_add(movement.amount).map_err(overflow)?;
    }

    let mut currencies: Vec<CurrencyCode> =
        accounts.iter().map(|account| account.currency).collect();
    currencies.sort_unstable();
    currencies.dedup();
    let empty = |month, currency| CashFlowRow {
        month,
        start: Money::zero(currency),
        money_in: Money::zero(currency),
        money_out: Money::zero(currency),
        starting: Money::zero(currency),
        net_change: Money::zero(currency),
        end: Money::zero(currency),
    };
    let mut rows: HashMap<CurrencyCode, Vec<CashFlowRow>> = currencies
        .iter()
        .map(|&currency| {
            let rows = months.iter().map(|&month| empty(month, currency)).collect();
            (currency, rows)
        })
        .collect();
    for ((_, currency, at), (net, starting)) in nets {
        let Some(row) = rows.get_mut(&currency).and_then(|rows| rows.get_mut(at)) else {
            continue;
        };
        if starting {
            row.starting = row.starting.checked_add(net).map_err(overflow)?;
        } else if net.is_negative() {
            row.money_out = row.money_out.checked_sub(net).map_err(overflow)?;
        } else {
            row.money_in = row.money_in.checked_add(net).map_err(overflow)?;
        }
    }
    for account in &accounts {
        let Some(rows) = rows.get_mut(&account.currency) else {
            continue;
        };
        for (at, row) in rows.iter_mut().enumerate() {
            let (start, end) = (&balances[2 * at], &balances[2 * at + 1]);
            if let Some(&balance) = start.get(&account.id) {
                row.start = row.start.checked_add(balance).map_err(overflow)?;
            }
            if let Some(&balance) = end.get(&account.id) {
                row.end = row.end.checked_add(balance).map_err(overflow)?;
            }
        }
    }

    let mut results = Vec::new();
    for currency in currencies {
        let Some(mut rows) = rows.remove(&currency) else {
            continue;
        };
        let mut total = empty(period.first().first_of_month(), currency);
        for row in &mut rows {
            row.net_change = row.money_in.checked_sub(row.money_out).map_err(overflow)?;
            total.money_in = total.money_in.checked_add(row.money_in).map_err(overflow)?;
            total.money_out = total
                .money_out
                .checked_add(row.money_out)
                .map_err(overflow)?;
            total.starting = total.starting.checked_add(row.starting).map_err(overflow)?;
        }
        if let (Some(first), Some(last)) = (rows.first(), rows.last()) {
            total.start = first.start;
            total.end = last.end;
        }
        total.net_change = total
            .money_in
            .checked_sub(total.money_out)
            .map_err(overflow)?;
        let open = accounts
            .iter()
            .any(|account| account.currency == currency && !account.closed);
        let moved = rows.iter().any(|row| {
            [
                row.start,
                row.money_in,
                row.money_out,
                row.starting,
                row.end,
            ]
            .iter()
            .any(|amount| !amount.is_zero())
        });
        if open || moved {
            results.push(CashFlowByMonth {
                currency,
                rows,
                total,
            });
        }
    }
    Ok(results)
}

impl CashFlowByMonth {
    /// The result as a table of text with the columns `month`, `start`,
    /// `money_in`, `money_out`, `starting` and `end`.
    pub fn to_text(&self, format: &impl TextFormat) -> TextTable {
        TextTable {
            currency: self.currency.to_string(),
            columns: vec!["month", "start", "money_in", "money_out", "starting", "end"],
            rows: self
                .rows
                .iter()
                .map(|row| {
                    vec![
                        format.month(row.month),
                        format.amount(row.start),
                        format.amount(row.money_in),
                        format.amount(row.money_out),
                        format.amount(row.starting),
                        format.amount(row.end),
                    ]
                })
                .collect(),
        }
    }
}
