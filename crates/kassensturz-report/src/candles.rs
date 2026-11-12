// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The balance of the chosen accounts as candles (US4.10, #699, D-718,
//! D-719): for each day, week or month of a period, what the accounts held
//! together at its start, at its highest and lowest, and at its end.
//!
//! The accounts are the ones `cash_flow_by_month` counts (without a choice,
//! every checking, savings and cash account), so a candle's start and end
//! are exactly that dataset's start and end for the same days. Every figure
//! is an exact balance at the end of a day (D-718): several bookings on one
//! day count as that day's end balance, so the order of bookings inside a
//! day never matters. The highest and the lowest include the start, the
//! balance at the end of the day before.

use std::collections::HashMap;

use jiff::ToSpan;
use kassensturz_core::{AccountId, CurrencyCode, Date, Money};
use kassensturz_store::StoreError;

use crate::balances::Moment;
use crate::cash_flow::chosen;
use crate::{Filter, Period, ReportError, Source, TextFormat, TextTable};

fn overflow<E>(_: E) -> ReportError {
    ReportError::Store(StoreError::AmountOverflow)
}

/// How many days one candle covers, which follows the length of the period
/// (D-719), so a chart never has a single candle nor hundreds of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandleLength {
    /// One candle per day: for a period of up to [`CandleLength::DAYS`] days.
    Day,
    /// One candle per week, Monday to Sunday (ISO 8601): for a period of up
    /// to [`CandleLength::WEEKS`] days.
    Week,
    /// One candle per calendar month: for every longer period.
    Month,
}

impl CandleLength {
    /// The longest period, in days, drawn with a candle per day: a month.
    pub const DAYS: i32 = 31;
    /// The longest period, in days, drawn with a candle per week: half a
    /// year (at most 28 candles).
    pub const WEEKS: i32 = 184;

    /// The length of the candles of `period`.
    pub fn of(period: Period) -> Self {
        // The days from the first to the last, both counted.
        let days = period
            .first()
            .until(period.last())
            .map_or(i32::MAX, |span| span.get_days().saturating_add(1));
        if days <= Self::DAYS {
            Self::Day
        } else if days <= Self::WEEKS {
            Self::Week
        } else {
            Self::Month
        }
    }

    /// The fixed name, as QML spells it: `day`, `week` or `month`.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
        }
    }

    /// The first day of each candle of `period`, oldest first. The first
    /// and the last candle cover only the days inside the period.
    fn starts(self, period: Period) -> Vec<Date> {
        let mut starts = vec![period.first()];
        let mut day = period.first();
        loop {
            let next = match self {
                Self::Day => day.tomorrow().ok(),
                Self::Week => {
                    // The next Monday.
                    let ahead = 7 - i64::from(day.weekday().to_monday_zero_offset());
                    day.checked_add(ahead.days()).ok()
                }
                Self::Month => day.first_of_month().checked_add(1.month()).ok(),
            };
            match next {
                Some(next) if next <= period.last() => {
                    starts.push(next);
                    day = next;
                }
                _ => return starts,
            }
        }
    }
}

/// One candle: the balance of the chosen accounts together over its days.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Candle {
    /// Its first day.
    pub first: Date,
    /// Its last day.
    pub last: Date,
    /// What the accounts held at the start of its first day: the end of the
    /// day before.
    pub open: Money,
    /// The highest of `open` and of the balances at the end of its days.
    pub high: Money,
    /// The lowest of `open` and of the balances at the end of its days.
    pub low: Money,
    /// What the accounts held at the end of its last day.
    pub close: Money,
}

/// The candles of one currency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BalanceCandles {
    /// The currency of every amount.
    pub currency: CurrencyCode,
    /// How many days each candle covers.
    pub length: CandleLength,
    /// Every candle of the period, oldest first, also those in which nothing
    /// was booked.
    pub candles: Vec<Candle>,
}

/// The candles of the chosen accounts' balance over `period`, one result per
/// currency in the order of their codes; amounts in different currencies are
/// never added together. A currency is listed if one of its chosen accounts
/// is open or a figure is not zero, as in `cash_flow_by_month`.
/// `filter.categories` does not apply.
pub fn balance_candles(
    source: &Source,
    period: Period,
    filter: &Filter,
) -> Result<Vec<BalanceCandles>, ReportError> {
    source.require(period)?;
    let accounts = chosen(source, filter);
    let currency_of: HashMap<AccountId, CurrencyCode> = accounts
        .iter()
        .map(|account| (account.id, account.currency))
        .collect();
    let opening = source
        .balances_at(&[Moment::Start(period.first())])?
        .pop()
        .unwrap_or_default();
    let length = CandleLength::of(period);
    let starts = length.starts(period);

    let mut currencies: Vec<CurrencyCode> = currency_of.values().copied().collect();
    currencies.sort_unstable();
    currencies.dedup();
    let movements = source.holdings()?.movements();
    let mut results = Vec::with_capacity(currencies.len());
    for currency in currencies {
        let mut running = Money::zero(currency);
        for (account, balance) in &opening {
            if currency_of.get(account) == Some(&currency) {
                running = running.checked_add(*balance).map_err(overflow)?;
            }
        }
        // This currency's parts on the period's days, oldest first.
        let mut parts = movements
            .iter()
            .filter(|movement| {
                period.contains(movement.day)
                    && currency_of.get(&movement.account) == Some(&currency)
            })
            .peekable();
        let mut candles = Vec::with_capacity(starts.len());
        let mut moved = !running.is_zero();
        for (at, &first) in starts.iter().enumerate() {
            let last = match starts.get(at + 1) {
                Some(next) => next.yesterday().map_err(|_| ReportError::PeriodNotRead)?,
                None => period.last(),
            };
            let open = running;
            let (mut high, mut low) = (open, open);
            while let Some(day) = parts.peek().map(|movement| movement.day) {
                if day > last {
                    break;
                }
                // Every part of the day first: only the day's end counts.
                while let Some(movement) = parts.next_if(|movement| movement.day == day) {
                    running = running.checked_add(movement.amount).map_err(overflow)?;
                    moved = true;
                }
                if running.minor_units() > high.minor_units() {
                    high = running;
                }
                if running.minor_units() < low.minor_units() {
                    low = running;
                }
            }
            candles.push(Candle {
                first,
                last,
                open,
                high,
                low,
                close: running,
            });
        }
        let open = accounts
            .iter()
            .any(|account| account.currency == currency && !account.closed);
        if open || moved {
            results.push(BalanceCandles {
                currency,
                length,
                candles,
            });
        }
    }
    Ok(results)
}

impl BalanceCandles {
    /// The result as a table of text with the columns `period`, `start`,
    /// `highest`, `lowest` and `end`. A candle is named by its month, or by
    /// its first day for a day or a week.
    pub fn to_text(&self, format: &impl TextFormat) -> TextTable {
        TextTable {
            currency: self.currency.to_string(),
            columns: vec!["period", "start", "highest", "lowest", "end"],
            rows: self
                .candles
                .iter()
                .map(|candle| {
                    vec![
                        self.name(candle, format),
                        format.amount(candle.open),
                        format.amount(candle.high),
                        format.amount(candle.low),
                        format.amount(candle.close),
                    ]
                })
                .collect(),
        }
    }

    /// The name of `candle` on the chart and in the table.
    pub fn name(&self, candle: &Candle, format: &impl TextFormat) -> String {
        match self.length {
            CandleLength::Month => format.month(candle.first.first_of_month()),
            CandleLength::Day | CandleLength::Week => format.day(candle.first),
        }
    }
}
