// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the user has, owes and is worth (D-348, D-349): the datasets
//! `account_balances` and `net_worth_over_time`, and the balances of the
//! user's own accounts that they and `cash_flow_by_month` are made from.
//!
//! A balance on a day is [`Book::balance_on`]'s: every part booked to the
//! account up to the end of that day, by the day the bank booked it where it
//! says so, otherwise by the day the account shows it with
//! ([`AccountPart::bank_date`](kassensturz_store::AccountPart::bank_date)),
//! in the account's currency (D-043). [`Source::read_with_balances`] reads
//! each own account's balance before its period and the parts booked in it
//! in the same read transaction as everything else, so no figure needs a
//! second read (D-365).

use std::collections::{HashMap, HashSet};

use kassensturz_core::{
    Account, AccountId, AccountKind, Currency, CurrencyCode, Date, DatedRate, Money, TransactionId,
};
use kassensturz_store::{Book, StoreError};

use crate::{Filter, Period, ReportError, Source, TextFormat, TextTable};

/// The ID of what each account held on the period's last day (D-348).
pub const ACCOUNT_BALANCES: &str = "account_balances";

/// The ID of what you have, owe and are worth at each month's end (D-348).
pub const NET_WORTH_OVER_TIME: &str = "net_worth_over_time";

fn overflow<E>(_: E) -> ReportError {
    ReportError::Store(StoreError::AmountOverflow)
}

/// One part booked to one of the user's own accounts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Movement {
    /// The account it is booked to.
    pub(crate) account: AccountId,
    /// The transaction it is a part of.
    pub(crate) transaction: TransactionId,
    /// The day it counts on: the day the bank booked it, else the day the
    /// account shows it with.
    pub(crate) day: Date,
    /// What it does to the account, in the account's currency.
    pub(crate) amount: Money,
    /// Whether its transaction sets a starting balance: it has a part booked
    /// to the internal account for starting balances.
    pub(crate) starting: bool,
}

/// The balances of the user's own accounts (kind asset or liability) before
/// a period, what was booked to them on its days, and the book's exchange
/// rates (D-365).
#[derive(Clone, Debug)]
pub(crate) struct Holdings {
    /// Each own account's balance at the end of the day before the period.
    opening: HashMap<AccountId, Money>,
    /// The parts booked to own accounts on the days of the period, oldest
    /// first.
    movements: Vec<Movement>,
    /// Every rate of the book's price table (D-028).
    rates: Vec<DatedRate>,
}

/// A moment a balance is taken at: the start or the end of a day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Moment {
    /// Before anything booked on the day.
    Start(Date),
    /// After everything booked on the day.
    End(Date),
}

impl Moment {
    fn day(self) -> Date {
        match self {
            Self::Start(day) | Self::End(day) => day,
        }
    }

    /// Whether a part that counts on `day` is in the balance.
    fn takes(self, day: Date) -> bool {
        match self {
            Self::Start(at) => day < at,
            Self::End(at) => day <= at,
        }
    }

    /// The order of moments: by day, a day's start before its end.
    fn key(self) -> (Date, bool) {
        (self.day(), matches!(self, Self::End(_)))
    }
}

impl Holdings {
    /// Reads every own account's parts with [`Book::account_parts`], the
    /// function [`Book::balance_on`] adds up, so both use the same day rule,
    /// and the book's exchange rates. `accounts` are all the book's accounts.
    pub(crate) fn read(
        book: &Book,
        accounts: &[Account],
        period: Period,
    ) -> Result<Self, ReportError> {
        // A transaction with a part on the internal account sets a starting
        // balance (D-089).
        let mut starting: HashSet<TransactionId> = HashSet::new();
        for account in accounts {
            if account.kind == AccountKind::Equity {
                starting.extend(
                    book.account_parts(account.id)?
                        .iter()
                        .map(|part| part.transaction),
                );
            }
        }
        let mut opening = HashMap::new();
        let mut movements = Vec::new();
        for account in accounts.iter().filter(|account| account.kind.is_account()) {
            let mut before = Money::zero(account.currency);
            for part in book.account_parts(account.id)? {
                let day = part.bank_date();
                if day < period.first() {
                    before = before.checked_add(part.amount).map_err(overflow)?;
                } else if day <= period.last() {
                    movements.push(Movement {
                        account: account.id,
                        transaction: part.transaction,
                        day,
                        amount: part.amount,
                        starting: starting.contains(&part.transaction),
                    });
                }
            }
            opening.insert(account.id, before);
        }
        Ok(Self::new(opening, movements, book.all_rates()?))
    }

    /// Holdings of these balances before the period, parts and rates.
    pub(crate) fn new(
        opening: HashMap<AccountId, Money>,
        mut movements: Vec<Movement>,
        rates: Vec<DatedRate>,
    ) -> Self {
        movements.sort_by_key(|movement| movement.day);
        Self {
            opening,
            movements,
            rates,
        }
    }

    /// The parts booked to own accounts on the days of the period, oldest
    /// first.
    pub(crate) fn movements(&self) -> &[Movement] {
        &self.movements
    }

    /// The rate that converts `from` into `to` on `day`, as
    /// [`Book::convert`] chooses it: the newest one on or before `day` in
    /// either direction; on the same day the one from `from` into `to`.
    /// `true` if it is multiplied by, `false` if divided by (D-079).
    fn rate(&self, from: CurrencyCode, to: CurrencyCode, day: Date) -> Option<(DatedRate, bool)> {
        let newest = |from: CurrencyCode, to: CurrencyCode| {
            self.rates
                .iter()
                .filter(|rate| rate.from == from && rate.to == to && rate.date <= day)
                .max_by_key(|rate| rate.date)
                .copied()
        };
        match (newest(from, to), newest(to, from)) {
            (Some(forward), Some(backward)) if backward.date > forward.date => {
                Some((backward, false))
            }
            (Some(forward), _) => Some((forward, true)),
            (None, Some(backward)) => Some((backward, false)),
            (None, None) => None,
        }
    }
}

impl Source {
    /// The own accounts (kind asset or liability) `filter` chooses, all of
    /// them if it chooses none; closed ones too. Have before owe, then by
    /// name as the account lists sort them, then by ID.
    pub(crate) fn own_accounts(&self, filter: &Filter) -> Vec<&Account> {
        let chosen: Option<HashSet<AccountId>> = filter
            .accounts
            .as_ref()
            .map(|ids| ids.iter().copied().collect());
        let mut accounts: Vec<&Account> = self
            .accounts()
            .values()
            .filter(|account| account.kind.is_account())
            .filter(|account| chosen.as_ref().is_none_or(|ids| ids.contains(&account.id)))
            .collect();
        accounts.sort_by_cached_key(|account| {
            (
                account.kind == AccountKind::Liability,
                account.name.to_lowercase(),
                account.id,
            )
        });
        accounts
    }

    /// Fails with [`ReportError::PeriodNotRead`] if the `Source` was read
    /// for fewer days than `period`, as [`Source::parts`] does, so a dataset
    /// never answers for days it did not read.
    pub(crate) fn require(&self, period: Period) -> Result<(), ReportError> {
        let covers = self.covers();
        if period.first() < covers.first() || period.last() > covers.last() {
            Err(ReportError::PeriodNotRead)
        } else {
            Ok(())
        }
    }

    /// Every own account's balance at each of `moments`, in their order.
    ///
    /// Fails with [`ReportError::PeriodNotRead`] for a moment outside the
    /// days the `Source` was read for.
    pub(crate) fn balances_at(
        &self,
        moments: &[Moment],
    ) -> Result<Vec<HashMap<AccountId, Money>>, ReportError> {
        let covers = self.covers();
        if moments.iter().any(|moment| !covers.contains(moment.day())) {
            return Err(ReportError::PeriodNotRead);
        }
        let holdings = self.holdings()?;
        let mut order: Vec<usize> = (0..moments.len()).collect();
        order.sort_by_key(|&index| moments[index].key());
        let mut results = vec![HashMap::new(); moments.len()];
        let mut running = holdings.opening.clone();
        let mut next = 0;
        for index in order {
            let moment = moments[index];
            while let Some(movement) = holdings.movements.get(next) {
                if !moment.takes(movement.day) {
                    break;
                }
                let balance = running
                    .entry(movement.account)
                    .or_insert_with(|| Money::zero(movement.amount.currency()));
                *balance = balance.checked_add(movement.amount).map_err(overflow)?;
                next += 1;
            }
            results[index].clone_from(&running);
        }
        Ok(results)
    }

    /// `amount` in `to` on `day` by [`Book::convert`]'s rule, rounded once in
    /// the book's rounding mode, with the rate used; `None` if the book has
    /// no rate between the two on or before `day` (D-349).
    fn convert(
        &self,
        amount: Money,
        to: CurrencyCode,
        day: Date,
    ) -> Result<Option<(Money, DatedRate)>, ReportError> {
        let from = amount.currency();
        let Some((rate, multiply)) = self.holdings()?.rate(from, to, day) else {
            return Ok(None);
        };
        let currency = |code: CurrencyCode| Currency::new(code, self.minor_digits(code));
        let (Some(from), Some(to)) = (currency(from), currency(to)) else {
            return Ok(None);
        };
        let converted = if multiply {
            amount.checked_mul_rate(from, rate.rate, to, self.rounding())
        } else {
            amount.checked_div_rate(from, rate.rate, to, self.rounding())
        };
        Ok(Some((converted.map_err(overflow)?, rate)))
    }
}

/// What you have, what you owe and what you are worth, in one currency.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Worth {
    /// The currency of the three amounts.
    pub currency: CurrencyCode,
    /// What you have: the balances of the accounts of kind asset (checking,
    /// savings, cash) added up. An overdrawn account lowers it.
    pub assets: Money,
    /// What you owe: the balances of the accounts of kind liability (credit
    /// cards) added up, as a positive amount for a debt. A card in credit
    /// lowers it.
    pub liabilities: Money,
    /// `assets` less `liabilities`.
    pub net_worth: Money,
}

impl Worth {
    fn zero(currency: CurrencyCode) -> Self {
        Self {
            currency,
            assets: Money::zero(currency),
            liabilities: Money::zero(currency),
            net_worth: Money::zero(currency),
        }
    }

    /// Adds `balance`, the balance of an account of `kind`.
    fn add(&mut self, kind: AccountKind, balance: Money) -> Result<(), ReportError> {
        match kind {
            AccountKind::Asset => {
                self.assets = self.assets.checked_add(balance).map_err(overflow)?;
            }
            // A debt is a negative balance; what is owed is its opposite.
            AccountKind::Liability => {
                self.liabilities = self.liabilities.checked_sub(balance).map_err(overflow)?;
            }
            AccountKind::Income | AccountKind::Expense | AccountKind::Equity => return Ok(()),
        }
        self.net_worth = self.net_worth.checked_add(balance).map_err(overflow)?;
        Ok(())
    }

    /// Adds what you have and what you owe of `other`, in the same currency.
    fn add_worth(&mut self, assets: Money, liabilities: Money) -> Result<(), ReportError> {
        self.assets = self.assets.checked_add(assets).map_err(overflow)?;
        self.liabilities = self
            .liabilities
            .checked_add(liabilities)
            .map_err(overflow)?;
        self.net_worth = self
            .assets
            .checked_sub(self.liabilities)
            .map_err(overflow)?;
        Ok(())
    }

    fn is_zero(&self) -> bool {
        self.assets.is_zero() && self.liabilities.is_zero()
    }
}

/// The figures of every currency together in one currency, on one day
/// (D-349, D-367).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Converted {
    /// The day the rates apply to.
    pub day: Date,
    /// The total in the currency asked for: its own figures plus every other
    /// currency's that has a rate. What you have and what you owe are each
    /// converted once and rounded once; what you are worth is the first less
    /// the second, so the three always add up.
    pub worth: Worth,
    /// The rate used for each converted currency, in the order of their
    /// codes.
    pub rates: Vec<DatedRate>,
    /// The currencies without a rate on or before `day`, with their own
    /// figures: they are not in `worth`, and the component says so.
    pub left_out: Vec<Worth>,
}

impl Source {
    /// `worths`, one per currency in the order of their codes, together in
    /// `to` on `day`. A currency whose figures are zero needs no rate.
    fn convert_worths(
        &self,
        worths: &[Worth],
        to: CurrencyCode,
        day: Date,
    ) -> Result<Converted, ReportError> {
        let mut total = Worth::zero(to);
        let mut rates = Vec::new();
        let mut left_out = Vec::new();
        for worth in worths {
            if worth.currency == to {
                total.add_worth(worth.assets, worth.liabilities)?;
                continue;
            }
            if worth.is_zero() {
                continue;
            }
            match (
                self.convert(worth.assets, to, day)?,
                self.convert(worth.liabilities, to, day)?,
            ) {
                (Some((assets, rate)), Some((liabilities, _))) => {
                    total.add_worth(assets, liabilities)?;
                    rates.push(rate);
                }
                _ => left_out.push(*worth),
            }
        }
        Ok(Converted {
            day,
            worth: total,
            rates,
            left_out,
        })
    }
}

/// One account and what it held.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountBalance {
    /// The account.
    pub account: AccountId,
    /// Its name, as the user gave it.
    pub name: String,
    /// Whether it is an account you owe on (kind liability, a credit card)
    /// rather than one you have money in.
    pub owed: bool,
    /// Whether the user closed it.
    pub closed: bool,
    /// Its balance at the end of the day, as booked: a debt is negative.
    pub balance: Money,
}

/// The accounts of one currency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrencyBalances {
    /// The currency of every amount.
    pub currency: CurrencyCode,
    /// The accounts: the ones you have money in first, then the ones you
    /// owe on, each by name.
    pub rows: Vec<AccountBalance>,
    /// What they add up to.
    pub worth: Worth,
}

/// What each account held at the end of a period's last day (D-348).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountBalances {
    /// The period's last day.
    pub day: Date,
    /// One result per currency, in the order of their codes.
    pub results: Vec<CurrencyBalances>,
    /// All of them in the currency asked for, if one was (D-349).
    pub converted: Option<Converted>,
}

/// What each own account `filter` chooses held at the end of `period`'s
/// last day, one result per currency, and with `in_currency` all of them
/// together in that currency. An open account is always listed, a closed
/// one only with a balance that is not zero. Categories are never counted;
/// `filter.categories` does not apply.
pub fn account_balances(
    source: &Source,
    period: Period,
    filter: &Filter,
    in_currency: Option<CurrencyCode>,
) -> Result<AccountBalances, ReportError> {
    source.require(period)?;
    let day = period.last();
    let balances = source
        .balances_at(&[Moment::End(day)])?
        .pop()
        .unwrap_or_default();
    let mut by_currency: HashMap<CurrencyCode, CurrencyBalances> = HashMap::new();
    for account in source.own_accounts(filter) {
        let balance = balances
            .get(&account.id)
            .copied()
            .unwrap_or_else(|| Money::zero(account.currency));
        if account.closed && balance.is_zero() {
            continue;
        }
        let result = by_currency
            .entry(account.currency)
            .or_insert_with(|| CurrencyBalances {
                currency: account.currency,
                rows: Vec::new(),
                worth: Worth::zero(account.currency),
            });
        result.worth.add(account.kind, balance)?;
        result.rows.push(AccountBalance {
            account: account.id,
            name: account.name.clone(),
            owed: account.kind == AccountKind::Liability,
            closed: account.closed,
            balance,
        });
    }
    let mut results: Vec<CurrencyBalances> = by_currency.into_values().collect();
    results.sort_unstable_by_key(|result| result.currency);
    let converted = in_currency
        .map(|to| {
            let worths: Vec<Worth> = results.iter().map(|result| result.worth).collect();
            source.convert_worths(&worths, to, day)
        })
        .transpose()?;
    Ok(AccountBalances {
        day,
        results,
        converted,
    })
}

/// One calendar month's end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorthRow {
    /// The month's first day.
    pub month: Date,
    /// The day the figures are for: the month's last day, or the period's
    /// last day in its last month.
    pub day: Date,
    /// What you had, owed and were worth at the end of `day`.
    pub worth: Worth,
}

/// The months of one currency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrencyWorth {
    /// The currency of every amount.
    pub currency: CurrencyCode,
    /// Every month the period touches, oldest first.
    pub rows: Vec<WorthRow>,
}

/// What you had, owed and were worth at each month's end (D-348).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetWorthOverTime {
    /// One result per currency, in the order of their codes.
    pub results: Vec<CurrencyWorth>,
    /// For each month, oldest first, all currencies together in the
    /// currency asked for; empty if none was (D-349).
    pub converted: Vec<Converted>,
}

/// The day each month of `period` ends on inside it.
fn month_ends(period: Period) -> Vec<(Date, Date)> {
    period
        .months()
        .into_iter()
        .map(|month| (month, month.last_of_month().min(period.last())))
        .collect()
}

/// What the own accounts `filter` chooses had, owed and were worth at the
/// end of each calendar month of `period`, one result per currency, and
/// with `in_currency` each month's figures together in that currency. A
/// currency is listed if one of its accounts is open or holds money on one
/// of the days. `filter.categories` does not apply.
pub fn net_worth_over_time(
    source: &Source,
    period: Period,
    filter: &Filter,
    in_currency: Option<CurrencyCode>,
) -> Result<NetWorthOverTime, ReportError> {
    source.require(period)?;
    let ends = month_ends(period);
    let moments: Vec<Moment> = ends.iter().map(|&(_, day)| Moment::End(day)).collect();
    let balances = source.balances_at(&moments)?;
    let accounts = source.own_accounts(filter);
    let mut currencies: Vec<CurrencyCode> = accounts
        .iter()
        .filter(|account| {
            !account.closed
                || balances
                    .iter()
                    .any(|day| day.get(&account.id).is_some_and(|b| !b.is_zero()))
        })
        .map(|account| account.currency)
        .collect();
    currencies.sort_unstable();
    currencies.dedup();
    let mut results: Vec<CurrencyWorth> = currencies
        .iter()
        .map(|&currency| CurrencyWorth {
            currency,
            rows: Vec::with_capacity(ends.len()),
        })
        .collect();
    let mut converted = Vec::new();
    for (&(month, day), balances) in ends.iter().zip(&balances) {
        let mut worths: Vec<Worth> = currencies.iter().map(|&c| Worth::zero(c)).collect();
        for account in &accounts {
            if let (Ok(at), Some(&balance)) = (
                currencies.binary_search(&account.currency),
                balances.get(&account.id),
            ) {
                worths[at].add(account.kind, balance)?;
            }
        }
        for (result, &worth) in results.iter_mut().zip(&worths) {
            result.rows.push(WorthRow { month, day, worth });
        }
        if let Some(to) = in_currency {
            converted.push(source.convert_worths(&worths, to, day)?);
        }
    }
    Ok(NetWorthOverTime { results, converted })
}

impl CurrencyBalances {
    /// The result as a table of text with the columns `account` and
    /// `balance`.
    pub fn to_text(&self, format: &impl TextFormat) -> TextTable {
        TextTable {
            currency: self.currency.to_string(),
            columns: vec!["account", "balance"],
            rows: self
                .rows
                .iter()
                .map(|row| vec![row.name.clone(), format.amount(row.balance)])
                .collect(),
        }
    }
}

impl CurrencyWorth {
    /// The result as a table of text with the columns `month`, `assets`,
    /// `liabilities` and `net_worth`.
    pub fn to_text(&self, format: &impl TextFormat) -> TextTable {
        TextTable {
            currency: self.currency.to_string(),
            columns: vec!["month", "assets", "liabilities", "net_worth"],
            rows: self
                .rows
                .iter()
                .map(|row| {
                    vec![
                        format.month(row.month),
                        format.amount(row.worth.assets),
                        format.amount(row.worth.liabilities),
                        format.amount(row.worth.net_worth),
                    ]
                })
                .collect(),
        }
    }
}
