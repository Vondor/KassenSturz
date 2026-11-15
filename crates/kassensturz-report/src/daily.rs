// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `daily_balance`: what the chosen accounts held at the end of each day of
//! a period, with two exponential moving averages of that balance and the
//! MACD indicator made from them (US7, #1036, D-794).
//!
//! The balance of a day is the one [`Book::balance_on`] gives for it: every
//! entry counted on the day the bank booked it where it says so, otherwise
//! on its date, starting balances included. The shown days end on the
//! period's last day or today, whichever comes first, so the last balance
//! is today's `account_balance`.
//!
//! An exponential moving average (EMA) of `n` days follows the balance with
//! the weight `2 / (n + 1)` for the newest day: each day's EMA is the day
//! before's plus `2 × (balance − EMA the day before) / (n + 1)`, worked out
//! in exact minor units and rounded once a day in the book's rounding mode
//! (D-027). It starts at the first day's balance. MACD is the short EMA
//! less the long one, its *signal* line the EMA of MACD over `signal` days
//! (starting at the first day's MACD), and the *gap* MACD less the signal.
//!
//! So the first days shown already carry a settled average, the averages
//! start [`Ema::warm_up_days`] days before the period (and are not shown
//! for those days). They never start before the chosen accounts' first
//! entry: a day before it has no balance yet, and the chart starts on that
//! first entry when it lies inside the period ([`DailyBalance::starts_late`]).
//!
//! [`Book::balance_on`]: kassensturz_store::Book::balance_on

use jiff::ToSpan;
use kassensturz_core::{CurrencyCode, Date, Money, RoundingMode};
use kassensturz_store::StoreError;

use crate::balances::Moment;
use crate::cash_flow::chosen;
use crate::{Filter, Period, ReportError, Source, TextFormat, TextTable};

/// The dataset's ID, as dashboards and the AI name it (D-794).
pub const DAILY_BALANCE: &str = "daily_balance";

/// The most days a daily balance shows: about three years. A longer period
/// is refused with `report.period-too-long`; the monthly datasets show
/// longer ones.
pub const MAX_DAILY_DAYS: usize = 1100;

/// The words of the grouping of `daily_balance`, the default first: what
/// the chart shows. The balance with both averages and the MACD panel
/// under it; the balance with both averages; the balance alone; or the
/// MACD panel alone.
pub const DAILY_PARTS: [&str; 4] = [
    "balance-averages-macd",
    "balance-averages",
    "balance",
    "macd",
];

/// The words of `params.ema_fast`, the default first: how many days the
/// short average spans.
pub const EMA_FAST_WORDS: [&str; 8] = [
    "12-days", "5-days", "7-days", "9-days", "10-days", "14-days", "20-days", "50-days",
];
/// The words of `params.ema_slow`, the default first: how many days the
/// long average spans.
pub const EMA_SLOW_WORDS: [&str; 8] = [
    "26-days", "14-days", "20-days", "30-days", "50-days", "90-days", "100-days", "200-days",
];
/// The words of `params.ema_signal`, the default first: how many days the
/// signal line spans.
pub const EMA_SIGNAL_WORDS: [&str; 6] =
    ["9-days", "3-days", "5-days", "7-days", "12-days", "14-days"];

/// What a daily balance chart shows: the grouping of `daily_balance`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DailyParts {
    /// The balance with both averages, and the MACD panel under it.
    All,
    /// The balance with both averages.
    Averages,
    /// The balance alone.
    Balance,
    /// The MACD panel alone.
    Macd,
}

impl DailyParts {
    /// The word of [`DAILY_PARTS`]; any other word is `All`.
    pub fn read(word: &str) -> Self {
        match word {
            "balance-averages" => Self::Averages,
            "balance" => Self::Balance,
            "macd" => Self::Macd,
            _ => Self::All,
        }
    }

    /// The word [`DailyParts::read`] reads.
    pub const fn word(self) -> &'static str {
        match self {
            Self::All => "balance-averages-macd",
            Self::Averages => "balance-averages",
            Self::Balance => "balance",
            Self::Macd => "macd",
        }
    }

    /// Whether the balance panel is shown.
    pub const fn balance_panel(self) -> bool {
        !matches!(self, Self::Macd)
    }

    /// Whether the two averages are drawn on the balance panel.
    pub const fn averages(self) -> bool {
        matches!(self, Self::All | Self::Averages)
    }

    /// Whether the MACD panel is shown.
    pub const fn macd_panel(self) -> bool {
        matches!(self, Self::All | Self::Macd)
    }
}

/// The lengths of the averages, in days.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ema {
    /// The days of the short average.
    pub fast: u32,
    /// The days of the long average, always more than `fast`.
    pub slow: u32,
    /// The days of MACD's signal line.
    pub signal: u32,
}

impl Default for Ema {
    fn default() -> Self {
        Self {
            fast: 12,
            slow: 26,
            signal: 9,
        }
    }
}

impl Ema {
    /// The word of `days` for `params.ema_fast` and its two companions:
    /// `12-days`.
    pub fn word(days: u32) -> String {
        format!("{days}-days")
    }

    /// The lengths of the words of `params.ema_fast`, `ema_slow` and
    /// `ema_signal`; a word that is not in its list takes the default. The
    /// long average always spans more days than the short one: a `slow`
    /// that does not is taken as `fast + 1`.
    pub fn read(fast: &str, slow: &str, signal: &str) -> Self {
        let word = |text: &str, words: &[&str], default: u32| {
            words
                .iter()
                .find(|word| **word == text)
                .and_then(|word| word.strip_suffix("-days")?.parse().ok())
                .unwrap_or(default)
        };
        let defaults = Self::default();
        let fast = word(fast, &EMA_FAST_WORDS, defaults.fast);
        let slow = word(slow, &EMA_SLOW_WORDS, defaults.slow).max(fast + 1);
        let signal = word(signal, &EMA_SIGNAL_WORDS, defaults.signal);
        Self { fast, slow, signal }
    }

    /// How many days before the period the averages start: four times the
    /// long average and the signal line together, after which the first
    /// day's weight is below 0.05 % of the average for every length offered.
    pub const fn warm_up_days(self) -> u32 {
        4 * (self.slow + self.signal)
    }

    /// The days to read the book for: the warm-up days before `period` and
    /// the period itself.
    pub fn read_span(self, period: Period) -> Result<Period, ReportError> {
        let first = period
            .first()
            .checked_sub((self.warm_up_days() as i32).days())
            .map_err(|_| ReportError::BadPeriod)?;
        Period::new(first, period.last())
    }
}

/// The days shown for `period` on `today`: up to the period's last day or
/// today, whichever comes first; `None` if the period lies wholly after
/// today. Refused with [`ReportError::PeriodTooLong`] for more than
/// [`MAX_DAILY_DAYS`] days.
pub fn daily_days(period: Period, today: Date) -> Result<Option<Period>, ReportError> {
    let last = period.last().min(today);
    if last < period.first() {
        return Ok(None);
    }
    let shown = Period::new(period.first(), last)?;
    let days = shown
        .first()
        .until(shown.last())
        .ok()
        .map_or(i32::MAX, |span| span.get_days());
    if usize::try_from(days).map_or(true, |days| days >= MAX_DAILY_DAYS) {
        return Err(ReportError::PeriodTooLong);
    }
    Ok(Some(shown))
}

/// One day of a daily balance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DailyRow {
    /// The day.
    pub day: Date,
    /// What the accounts held at the end of the day.
    pub balance: Money,
    /// The short average of the balance.
    pub fast: Money,
    /// The long average of the balance.
    pub slow: Money,
    /// `fast` less `slow`.
    pub macd: Money,
    /// The average of `macd` over the signal's days.
    pub signal: Money,
    /// `macd` less `signal`.
    pub gap: Money,
}

/// The daily balance of the chosen accounts of one currency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DailyBalance {
    /// The currency of every amount.
    pub currency: CurrencyCode,
    /// The lengths worked with.
    pub ema: Ema,
    /// Whether the first day shown is the accounts' first entry, which lies
    /// after the period's first day: the days before it had no balance yet.
    pub starts_late: bool,
    /// The days shown, oldest first. Empty when the accounts have no entry
    /// up to the last day shown.
    pub rows: Vec<DailyRow>,
}

fn overflow<E>(_: E) -> ReportError {
    ReportError::Store(StoreError::AmountOverflow)
}

/// The next day's average of `days` days: `previous` plus
/// `2 × (value − previous) / (days + 1)`, rounded once.
pub(crate) fn step(
    previous: i64,
    value: i64,
    days: u32,
    rounding: RoundingMode,
) -> Result<i64, ReportError> {
    let change = rounding
        .divide(
            2 * (i128::from(value) - i128::from(previous)),
            i128::from(days) + 1,
        )
        .ok_or_else(|| overflow(()))?;
    i64::try_from(i128::from(previous) + change).map_err(overflow)
}

/// The exponential moving average of `values` over `days` days, starting
/// at the first value.
pub(crate) fn ema(
    values: &[i64],
    days: u32,
    rounding: RoundingMode,
) -> Result<Vec<i64>, ReportError> {
    let mut averages = Vec::with_capacity(values.len());
    let mut previous = None;
    for &value in values {
        let average = match previous {
            None => value,
            Some(previous) => step(previous, value, days, rounding)?,
        };
        averages.push(average);
        previous = Some(average);
    }
    Ok(averages)
}

/// The balance of the chosen accounts at the end of each day of `period`
/// up to today, with its averages and MACD (D-794), one result per
/// currency in the order of their codes. Without a choice the accounts
/// are every checking, savings and cash account, as for
/// [`cash_flow_by_month`](crate::cash_flow_by_month). A currency is listed
/// if one of its chosen accounts is open or has an entry.
///
/// `source` must have been read with balances
/// ([`Source::read_with_balances`]) for [`Ema::read_span`] of the shown
/// days ([`daily_days`]).
pub fn daily_balance(
    source: &Source,
    period: Period,
    today: Date,
    filter: &Filter,
    ema_lengths: Ema,
) -> Result<Vec<DailyBalance>, ReportError> {
    let accounts = chosen(source, filter);
    let mut currencies: Vec<CurrencyCode> =
        accounts.iter().map(|account| account.currency).collect();
    currencies.sort_unstable();
    currencies.dedup();
    let shown = daily_days(period, today)?;
    let span = match shown {
        Some(shown) => {
            let span = ema_lengths.read_span(shown)?;
            source.require(span)?;
            Some((shown, span))
        }
        None => None,
    };
    let holdings = source.holdings()?;
    let rounding = source.rounding();
    let mut results = Vec::new();
    for currency in currencies {
        let mine: Vec<_> = accounts
            .iter()
            .filter(|account| account.currency == currency)
            .collect();
        let first_entry = mine
            .iter()
            .filter_map(|account| holdings.first_entry(account.id))
            .min();
        let open = mine.iter().any(|account| !account.closed);
        if !open && first_entry.is_none() {
            continue;
        }
        let mut result = DailyBalance {
            currency,
            ema: ema_lengths,
            starts_late: false,
            rows: Vec::new(),
        };
        let (Some((shown, span)), Some(first_entry)) = (span, first_entry) else {
            results.push(result);
            continue;
        };
        let first_shown = shown.first().max(first_entry);
        if first_shown > shown.last() {
            results.push(result);
            continue;
        }
        result.starts_late = first_entry > shown.first();
        // Every day from the first one counted to the last one shown.
        let mut days = Vec::new();
        let mut day = span.first().max(first_entry);
        while day <= shown.last() {
            days.push(day);
            day = day.tomorrow().map_err(|_| ReportError::BadPeriod)?;
        }
        let moments: Vec<Moment> = days.iter().map(|&day| Moment::End(day)).collect();
        let balances = source.balances_at(&moments)?;
        let values: Vec<i64> = balances
            .iter()
            .map(|held| {
                Money::checked_sum(
                    currency,
                    mine.iter()
                        .filter_map(|account| held.get(&account.id).copied()),
                )
                .map(Money::minor_units)
                .map_err(overflow)
            })
            .collect::<Result<_, _>>()?;
        let fast = ema(&values, ema_lengths.fast, rounding)?;
        let slow = ema(&values, ema_lengths.slow, rounding)?;
        let line: Vec<i64> = fast
            .iter()
            .zip(&slow)
            .map(|(fast, slow)| fast.checked_sub(*slow).ok_or_else(|| overflow(())))
            .collect::<Result<_, _>>()?;
        let signal = ema(&line, ema_lengths.signal, rounding)?;
        let money = |units: i64| Money::from_minor_units(units, currency);
        for (at, &day) in days.iter().enumerate() {
            if day < first_shown {
                continue;
            }
            let (macd, signal) = (money(line[at]), money(signal[at]));
            result.rows.push(DailyRow {
                day,
                balance: money(values[at]),
                fast: money(fast[at]),
                slow: money(slow[at]),
                macd,
                signal,
                gap: macd.checked_sub(signal).map_err(overflow)?,
            });
        }
        results.push(result);
    }
    Ok(results)
}

impl DailyBalance {
    /// The result as a table of text with the columns `day`, `balance`,
    /// `ema_fast`, `ema_slow`, `daily_macd`, `daily_signal` and `daily_gap`.
    pub fn to_text(&self, format: &impl TextFormat) -> TextTable {
        TextTable {
            currency: self.currency.to_string(),
            columns: vec![
                "day",
                "balance",
                "ema_fast",
                "ema_slow",
                "daily_macd",
                "daily_signal",
                "daily_gap",
            ],
            rows: self
                .rows
                .iter()
                .map(|row| {
                    vec![
                        format.day(row.day),
                        format.amount(row.balance),
                        format.amount(row.fast),
                        format.amount(row.slow),
                        format.amount(row.macd),
                        format.amount(row.signal),
                        format.amount(row.gap),
                    ]
                })
                .collect(),
        }
    }
}
