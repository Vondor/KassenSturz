// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Moving averages and the MACD indicator of money in, money out and what
//! is left, month by month (US7, #875, D-777).
//!
//! Everything is counted from the exact monthly amounts of
//! [`income_expense_by_month`](crate::income_expense_by_month). A moving
//! average of `n` months is the exact sum of the month and the `n - 1`
//! before it, divided once by `n` in the book's rounding mode (AGENTS.md
//! §5, D-027). The months an average needs from before the period are read
//! too ([`Trend::read_span`]) and counted in full, so the first month shown
//! already has its whole window; they are not shown. Months before the
//! first transaction count as zero, as in D-668.
//!
//! MACD is the plain-language "short-term against long-term trend": the
//! average of the last `length` months less the average of the last `slow`
//! months, and a *signal* line, the average of that difference over the
//! last `signal` months. What is left of the difference after the signal
//! is the *gap*. Each figure is rounded once, as above, and the gap is the
//! difference of the two rounded figures shown next to it, so the table
//! always adds up.

use jiff::ToSpan;
use kassensturz_core::{CurrencyCode, Date, Money, RoundingMode};
use kassensturz_store::StoreError;

use crate::{
    Filter, IncomeExpenseByMonth, Period, ReportError, Source, TextFormat, TextTable,
    income_expense_by_month,
};

/// The dataset's ID for a series and its moving average.
pub const MOVING_AVERAGE: &str = "moving_average";
/// The dataset's ID for the MACD indicator.
pub const MACD: &str = "macd";

/// The words of the series a trend follows, the default first: what is
/// left, money in, or money out.
pub const TREND_SERIES: [&str; 3] = ["net", "income", "expenses"];

/// The words of `params.trend_length`, the default first: how many months
/// a moving average (and MACD's short-term average) spans.
pub const TREND_LENGTH_WORDS: [&str; 8] = [
    "3-months",
    "2-months",
    "4-months",
    "5-months",
    "6-months",
    "9-months",
    "12-months",
    "24-months",
];
/// The words of `params.trend_slow`, the default first: how many months
/// MACD's long-term average spans.
pub const TREND_SLOW_WORDS: [&str; 7] = [
    "6-months",
    "4-months",
    "9-months",
    "12-months",
    "18-months",
    "24-months",
    "36-months",
];
/// The words of `params.trend_signal`, the default first: how many months
/// MACD's signal line spans.
pub const TREND_SIGNAL_WORDS: [&str; 6] = [
    "3-months",
    "2-months",
    "4-months",
    "6-months",
    "9-months",
    "12-months",
];

/// The lengths a trend is worked out with, in whole months.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Trend {
    /// The months of a moving average, and of MACD's short-term average.
    pub length: u32,
    /// The months of MACD's long-term average.
    pub slow: u32,
    /// The months of MACD's signal line.
    pub signal: u32,
}

impl Default for Trend {
    fn default() -> Self {
        Self {
            length: 3,
            slow: 6,
            signal: 3,
        }
    }
}

impl Trend {
    /// The word of `months` for `params.trend_length` and its two
    /// companions: `6-months`.
    pub fn word(months: u32) -> String {
        format!("{months}-months")
    }

    /// The lengths of the words of `params.trend_length`, `trend_slow` and
    /// `trend_signal`; a word that is not in its list takes the default.
    /// MACD needs the long-term average to span more months than the
    /// short-term one, so a `slow` that does not is the first longer one in
    /// [`TREND_SLOW_WORDS`] order of length (never fewer than `length + 1`).
    pub fn read(length: &str, slow: &str, signal: &str) -> Self {
        let word = |text: &str, words: &[&str], default: u32| {
            words
                .iter()
                .find(|word| **word == text)
                .and_then(|word| word.strip_suffix("-months")?.parse().ok())
                .unwrap_or(default)
        };
        let defaults = Self::default();
        let length = word(length, &TREND_LENGTH_WORDS, defaults.length);
        let slow = word(slow, &TREND_SLOW_WORDS, defaults.slow).max(length + 1);
        let signal = word(signal, &TREND_SIGNAL_WORDS, defaults.signal);
        Self {
            length,
            slow,
            signal,
        }
    }

    /// How many months before the period a dataset needs.
    fn warm_up(self, dataset: &str) -> u32 {
        if dataset == MACD {
            self.slow - 1 + self.signal - 1
        } else {
            self.length - 1
        }
    }

    /// The days to read the book for: the period and the whole months
    /// before it the dataset's averages need. Refused like any other period
    /// with more than [`MAX_MONTHS`] months (`report.period-too-long`).
    pub fn read_span(self, dataset: &str, period: Period) -> Result<Period, ReportError> {
        let first = period
            .first()
            .first_of_month()
            .checked_sub((self.warm_up(dataset) as i32).months())
            .map_err(|_| ReportError::BadPeriod)?;
        Period::new(first, period.last())
    }
}

/// What a trend follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrendSeries {
    /// Money in less money out.
    Net,
    /// Money in.
    Income,
    /// Money out, less what came back.
    Expenses,
}

impl TrendSeries {
    /// The word of [`TREND_SERIES`]; a word that is not one is `Net`.
    pub fn read(word: &str) -> Self {
        match word {
            "income" => Self::Income,
            "expenses" => Self::Expenses,
            _ => Self::Net,
        }
    }

    /// The word [`TrendSeries::read`] reads.
    pub const fn word(self) -> &'static str {
        match self {
            Self::Net => "net",
            Self::Income => "income",
            Self::Expenses => "expenses",
        }
    }
}

/// One month of a moving average.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AverageRow {
    /// The month's first day.
    pub month: Date,
    /// What the series was in this month.
    pub value: Money,
    /// The average of this month and the ones before it.
    pub average: Money,
}

/// A series and its moving average, in one currency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MovingAverage {
    /// The currency of every amount.
    pub currency: CurrencyCode,
    /// What is followed.
    pub series: TrendSeries,
    /// How many months an average spans.
    pub length: u32,
    /// The months of the period, oldest first.
    pub rows: Vec<AverageRow>,
}

/// One month of MACD.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MacdRow {
    /// The month's first day.
    pub month: Date,
    /// The short-term average less the long-term one.
    pub macd: Money,
    /// The average of `macd` over the signal's months.
    pub signal: Money,
    /// `macd` less `signal`.
    pub gap: Money,
}

/// MACD of a series, in one currency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Macd {
    /// The currency of every amount.
    pub currency: CurrencyCode,
    /// What is followed.
    pub series: TrendSeries,
    /// The lengths worked with.
    pub trend: Trend,
    /// The months of the period, oldest first.
    pub rows: Vec<MacdRow>,
}

fn overflow(_: kassensturz_core::MoneyError) -> ReportError {
    ReportError::Store(StoreError::AmountOverflow)
}

/// The average of the `months` last values up to and including each one,
/// rounded once in `rounding`; the first `months - 1` values average what
/// there is before them as zero.
fn averages(values: &[i64], months: u32, rounding: RoundingMode) -> Result<Vec<i64>, ReportError> {
    let months = months as usize;
    values
        .iter()
        .enumerate()
        .map(|(at, _)| {
            let from = (at + 1).saturating_sub(months);
            let sum: i128 = values[from..=at].iter().map(|v| i128::from(*v)).sum();
            rounding
                .divide(sum, months as i128)
                .and_then(|average| i64::try_from(average).ok())
                .ok_or(ReportError::Store(StoreError::AmountOverflow))
        })
        .collect()
}

/// The months of `data` up to `period`'s months, the series' amounts.
fn series_of(data: &IncomeExpenseByMonth, series: TrendSeries) -> Vec<i64> {
    data.rows
        .iter()
        .map(|row| {
            match series {
                TrendSeries::Net => row.net,
                TrendSeries::Income => row.income,
                TrendSeries::Expenses => row.expenses,
            }
            .minor_units()
        })
        .collect()
}

/// How many of the leading months of `data` are warm-up: the ones before
/// `period`'s first month.
fn warm_months(data: &IncomeExpenseByMonth, period: Period) -> usize {
    let first = period.first().first_of_month();
    data.rows.iter().take_while(|row| row.month < first).count()
}

/// The moving average of the series for each month of `period`, one result
/// per currency in the order of their codes. `source` must have been read
/// for [`Trend::read_span`] of [`MOVING_AVERAGE`].
pub fn moving_average(
    source: &Source,
    period: Period,
    filter: &Filter,
    series: TrendSeries,
    trend: Trend,
) -> Result<Vec<MovingAverage>, ReportError> {
    let span = trend.read_span(MOVING_AVERAGE, period)?;
    let rounding = source.rounding();
    let mut results = Vec::new();
    for data in income_expense_by_month(source, span, filter)? {
        let values = series_of(&data, series);
        let averaged = averages(&values, trend.length, rounding)?;
        let skip = warm_months(&data, period);
        let rows = data
            .rows
            .iter()
            .zip(values.iter().zip(&averaged))
            .skip(skip)
            .map(|(row, (value, average))| AverageRow {
                month: row.month,
                value: Money::from_minor_units(*value, data.currency),
                average: Money::from_minor_units(*average, data.currency),
            })
            .collect();
        results.push(MovingAverage {
            currency: data.currency,
            series,
            length: trend.length,
            rows,
        });
    }
    Ok(results)
}

/// MACD of the series for each month of `period`, one result per currency
/// in the order of their codes. `source` must have been read for
/// [`Trend::read_span`] of [`MACD`].
pub fn macd(
    source: &Source,
    period: Period,
    filter: &Filter,
    series: TrendSeries,
    trend: Trend,
) -> Result<Vec<Macd>, ReportError> {
    let span = trend.read_span(MACD, period)?;
    let rounding = source.rounding();
    let mut results = Vec::new();
    for data in income_expense_by_month(source, span, filter)? {
        let values = series_of(&data, series);
        let fast = averages(&values, trend.length, rounding)?;
        let slow = averages(&values, trend.slow, rounding)?;
        let line: Vec<i64> = fast
            .iter()
            .zip(&slow)
            .map(|(fast, slow)| {
                Money::from_minor_units(*fast, data.currency)
                    .checked_sub(Money::from_minor_units(*slow, data.currency))
                    .map(Money::minor_units)
                    .map_err(overflow)
            })
            .collect::<Result<_, _>>()?;
        let signal = averages(&line, trend.signal, rounding)?;
        let skip = warm_months(&data, period);
        let mut rows = Vec::new();
        for ((row, macd), signal) in data.rows.iter().zip(&line).zip(&signal).skip(skip) {
            let macd = Money::from_minor_units(*macd, data.currency);
            let signal = Money::from_minor_units(*signal, data.currency);
            rows.push(MacdRow {
                month: row.month,
                macd,
                signal,
                gap: macd.checked_sub(signal).map_err(overflow)?,
            });
        }
        results.push(Macd {
            currency: data.currency,
            series,
            trend,
            rows,
        });
    }
    Ok(results)
}

impl MovingAverage {
    /// The result as a table of text with the columns `month`, `value` and
    /// `average`.
    pub fn to_text(&self, format: &impl TextFormat) -> TextTable {
        TextTable {
            currency: self.currency.to_string(),
            columns: vec!["month", "value", "average"],
            rows: self
                .rows
                .iter()
                .map(|row| {
                    vec![
                        format.month(row.month),
                        format.amount(row.value),
                        format.amount(row.average),
                    ]
                })
                .collect(),
        }
    }
}

impl Macd {
    /// The result as a table of text with the columns `month`, `macd`,
    /// `signal` and `gap`.
    pub fn to_text(&self, format: &impl TextFormat) -> TextTable {
        TextTable {
            currency: self.currency.to_string(),
            columns: vec!["month", "macd", "signal", "gap"],
            rows: self
                .rows
                .iter()
                .map(|row| {
                    vec![
                        format.month(row.month),
                        format.amount(row.macd),
                        format.amount(row.signal),
                        format.amount(row.gap),
                    ]
                })
                .collect(),
        }
    }
}
