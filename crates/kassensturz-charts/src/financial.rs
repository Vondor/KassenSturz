// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Charts and tables made from the datasets of what the user has, owes and
//! is worth and of the money in and out of their accounts (M6c.2, D-348,
//! D-370).
//!
//! Every function takes one currency's result of `kassensturz-report`:
//! amounts in different currencies are never added together, so the caller
//! makes one chart for each.

use kassensturz_core::Money;
use kassensturz_report::{
    ACCOUNT_BALANCES, CASH_FLOW_BY_MONTH, CashFlowByMonth, CurrencyBalances, CurrencyWorth,
    NET_WORTH_OVER_TIME, TextFormat, TextTable,
};

use crate::datasets::{extent, major_unit, point};
use crate::ticks::ticks;
use crate::{ChartError, ChartKind, ChartModel, Series, Slices};

/// What you are worth at the end of each month as a line or a filled line:
/// one series, `net_worth`, with a point for every month. A table (which
/// asks for a bar chart, whose values it shows) gets the same model. The
/// table also lists what you have and what you owe. The total is what you
/// were worth at the end of the last month.
pub fn net_worth_over_time(
    data: &CurrencyWorth,
    kind: ChartKind,
    format: &impl TextFormat,
) -> Result<ChartModel, ChartError> {
    if !matches!(kind, ChartKind::Line | ChartKind::Area | ChartKind::Bar) {
        return Err(ChartError::KindNotSupported(kind));
    }
    let table = data.to_text(format);
    let series = vec![Series {
        key: "net_worth",
        name: None,
        color: 0,
        points: data
            .rows
            .iter()
            .map(|row| point(row.worth.net_worth, format))
            .collect(),
    }];
    let (low, high) = extent(&series, false);
    let ticks = ticks(low, high, major_unit(data.currency), data.currency, format)?;
    let total = data
        .rows
        .last()
        .map_or_else(String::new, |row| format.amount(row.worth.net_worth));
    Ok(ChartModel {
        kind,
        dataset: NET_WORTH_OVER_TIME,
        currency: data.currency.to_string(),
        categories: data
            .rows
            .iter()
            .map(|row| format.month(row.month))
            .collect(),
        unsorted: None,
        series,
        ticks,
        total,
        table,
        slices: Slices::default(),
        hidden: None,
    })
}

/// The money in and out of the chosen accounts in each month as bars: the
/// series `money_in` and `money_out`, one point per month. The table (which
/// asks for the same bar chart) has what the accounts held at the start and
/// at the end and the starting balances set in the month too. The total is
/// the net change of the whole period: money in less money out.
pub fn cash_flow_by_month(
    data: &CashFlowByMonth,
    kind: ChartKind,
    format: &impl TextFormat,
) -> Result<ChartModel, ChartError> {
    if kind != ChartKind::Bar {
        return Err(ChartError::KindNotSupported(kind));
    }
    let table = data.to_text(format);
    let series = vec![
        Series {
            key: "money_in",
            name: None,
            color: 0,
            points: data
                .rows
                .iter()
                .map(|row| point(row.money_in, format))
                .collect(),
        },
        Series {
            key: "money_out",
            name: None,
            color: 1,
            points: data
                .rows
                .iter()
                .map(|row| point(row.money_out, format))
                .collect(),
        },
    ];
    let (low, high) = extent(&series, false);
    let ticks = ticks(low, high, major_unit(data.currency), data.currency, format)?;
    Ok(ChartModel {
        kind,
        dataset: CASH_FLOW_BY_MONTH,
        currency: data.currency.to_string(),
        categories: data
            .rows
            .iter()
            .map(|row| format.month(row.month))
            .collect(),
        unsorted: None,
        series,
        ticks,
        total: format.amount(data.total.net_change),
        table,
        slices: Slices::default(),
        hidden: None,
    })
}

/// One row of a table of balances: the name of its place on the chart, its
/// cells and its amount.
type Entry = (String, Vec<String>, Money);

/// How the accounts are put together in a table (D-370).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BalancesBy {
    /// One row for each account, with whether it is one you have money in
    /// (`have`) or one you owe on (`owe`).
    Account,
    /// One row for what you have (`have`), one for what you owe (`owe`) and
    /// one for what you are worth (`worth`).
    Kind,
}

/// What each account held at the end of the period's last day, as a table
/// (the caller asks for a bar chart, whose values it shows; nothing is
/// drawn). A debt is shown as a positive amount in the `owe` rows, as the
/// figures of the dataset have it. The total is what you are worth.
pub fn account_balances(
    data: &CurrencyBalances,
    by: BalancesBy,
    kind: ChartKind,
    format: &impl TextFormat,
) -> Result<ChartModel, ChartError> {
    if kind != ChartKind::Bar {
        return Err(ChartError::KindNotSupported(kind));
    }
    let (columns, entries): (Vec<&'static str>, Vec<Entry>) = match by {
        BalancesBy::Account => {
            let mut entries = Vec::with_capacity(data.rows.len());
            for row in &data.rows {
                // A debt is what is owed: the opposite of the booked
                // balance, which is negative for a debt.
                let shown = if row.owed {
                    Money::zero(data.currency)
                        .checked_sub(row.balance)
                        .map_err(|_| ChartError::Overflow)?
                } else {
                    row.balance
                };
                let word = if row.owed { "owe" } else { "have" };
                entries.push((
                    row.name.clone(),
                    vec![row.name.clone(), word.to_owned(), format.amount(shown)],
                    shown,
                ));
            }
            (vec!["account", "kind", "balance"], entries)
        }
        BalancesBy::Kind => {
            let worth = data.worth;
            let entries = [
                ("have", worth.assets),
                ("owe", worth.liabilities),
                ("worth", worth.net_worth),
            ]
            .into_iter()
            .map(|(word, amount)| {
                (
                    word.to_owned(),
                    vec![word.to_owned(), format.amount(amount)],
                    amount,
                )
            })
            .collect();
            (vec!["figure", "total"], entries)
        }
    };
    let table = TextTable {
        currency: data.currency.to_string(),
        columns,
        rows: entries.iter().map(|(_, cells, _)| cells.clone()).collect(),
    };
    let series = vec![Series {
        key: "balance",
        name: None,
        color: 0,
        points: entries
            .iter()
            .map(|(_, _, money)| point(*money, format))
            .collect(),
    }];
    Ok(ChartModel {
        kind,
        dataset: ACCOUNT_BALANCES,
        currency: data.currency.to_string(),
        // Names of the book's accounts, or the fixed words the screen
        // translates.
        categories: entries.into_iter().map(|(name, _, _)| name).collect(),
        unsorted: None,
        series,
        ticks: Vec::new(),
        total: format.amount(data.worth.net_worth),
        table,
        slices: Slices::default(),
        hidden: None,
    })
}
