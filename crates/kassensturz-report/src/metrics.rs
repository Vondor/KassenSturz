// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Calculated metrics and the comparison of two periods (D-285).
//!
//! The one place a division happens: a ratio in percent. It is calculated
//! exactly in 128 bits and rounded once, half away from zero, to one decimal
//! place of a percent, whatever the book's rounding setting says (D-285). A
//! ratio that has no meaning, such as a savings rate without income, is
//! "no value" (`None`), never 0 % or infinity.

use kassensturz_core::{CurrencyCode, Money, RoundingMode};
use kassensturz_store::StoreError;

use crate::{
    Filter, IncomeExpenseByMonth, Period, ReportError, Source, TextFormat, income_expense_by_month,
};

/// A ratio in percent with one decimal place, kept as tenths of a percent so
/// no floating point is involved: 12.3 % is 123.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Percent(i64);

impl Percent {
    /// 12.3 % from 123.
    pub const fn from_tenths(tenths: i64) -> Self {
        Self(tenths)
    }

    /// The ratio in tenths of a percent.
    pub const fn tenths(self) -> i64 {
        self.0
    }

    /// The ratio as the user's number format writes it.
    pub fn to_text(self, format: &impl TextFormat) -> String {
        format.percent(self)
    }
}

fn overflow<E>(_: E) -> ReportError {
    ReportError::Store(StoreError::AmountOverflow)
}

/// `part` as a percentage of `whole`'s size, rounded to one decimal place,
/// half away from zero; `None` if `whole` is zero.
fn percent_of(part: Money, whole: Money) -> Result<Option<Percent>, ReportError> {
    if whole.is_zero() {
        return Ok(None);
    }
    let size = whole
        .minor_units()
        .checked_abs()
        .ok_or_else(|| overflow(()))?;
    // Money carries the result; its minor units are tenths of a percent.
    let tenths = part
        .checked_mul_ratio(1000, size, RoundingMode::HalfAwayFromZero)
        .map_err(overflow)?;
    Ok(Some(Percent(tenths.minor_units())))
}

/// What is left of income, in percent: `net` ÷ `income`. `None` if there was
/// no income (zero or less), since a rate of nothing means nothing.
pub fn savings_rate(income: Money, net: Money) -> Result<Option<Percent>, ReportError> {
    if income.minor_units() <= 0 {
        return Ok(None);
    }
    percent_of(net, income)
}

/// How a figure changed from one period to the next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Change {
    /// The figure in the earlier period.
    pub previous: Money,
    /// The figure in the period asked for.
    pub current: Money,
    /// `current` less `previous`.
    pub amount: Money,
    /// `amount` as a percentage of the size of `previous`, so a rise is
    /// positive and a fall negative also when `previous` was negative.
    /// `None` if `previous` was zero.
    pub percent: Option<Percent>,
}

impl Change {
    /// The change from `previous` to `current`, in the same currency.
    pub fn new(previous: Money, current: Money) -> Result<Self, ReportError> {
        let amount = current.checked_sub(previous).map_err(overflow)?;
        Ok(Self {
            previous,
            current,
            amount,
            percent: percent_of(amount, previous)?,
        })
    }
}

/// Which earlier period to compare with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compare {
    /// The period right before (`Period::before`).
    PreviousPeriod,
    /// The same days a year earlier (`Period::year_before`).
    SameLastYear,
}

impl Compare {
    /// The period to compare `period` with.
    pub fn earlier(self, period: Period) -> Result<Period, ReportError> {
        match self {
            Self::PreviousPeriod => period.before(),
            Self::SameLastYear => period.year_before(),
        }
    }

    /// The fixed name that dashboards and the AI use.
    pub const fn id(self) -> &'static str {
        match self {
            Self::PreviousPeriod => "previous_period",
            Self::SameLastYear => "same_last_year",
        }
    }

    /// The comparison with this name.
    pub fn from_id(id: &str) -> Option<Self> {
        [Self::PreviousPeriod, Self::SameLastYear]
            .into_iter()
            .find(|compare| compare.id() == id)
    }
}

/// Income, expenses and what is left in one currency, now and in the
/// earlier period.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IncomeExpenseChange {
    /// The currency of every amount.
    pub currency: CurrencyCode,
    /// What came in.
    pub income: Change,
    /// What went out.
    pub expenses: Change,
    /// Income less expenses.
    pub net: Change,
    /// The savings rate of the earlier period, if it has one.
    pub previous_savings_rate: Option<Percent>,
    /// The savings rate of the period asked for, if it has one.
    pub savings_rate: Option<Percent>,
}

impl IncomeExpenseByMonth {
    /// What is left of this result's income, in percent (see
    /// [`savings_rate`]).
    pub fn savings_rate(&self) -> Result<Option<Percent>, ReportError> {
        savings_rate(self.income, self.net)
    }
}

impl crate::MonthRow {
    /// What is left of this month's income, in percent (see
    /// [`savings_rate`]).
    pub fn savings_rate(&self) -> Result<Option<Percent>, ReportError> {
        savings_rate(self.income, self.net)
    }
}

/// Income, expenses and net of `period` against the earlier period `compare`
/// names, one result per currency in either period, in the order of their
/// codes. Returns the earlier period too. `source` must have been read for
/// both periods (from the earlier one's first day to `period`'s last), or
/// the call fails with [`ReportError::PeriodNotRead`] rather than leave
/// transactions out.
pub fn compare_income_expense(
    source: &Source,
    period: Period,
    filter: &Filter,
    compare: Compare,
) -> Result<(Period, Vec<IncomeExpenseChange>), ReportError> {
    let earlier = compare.earlier(period)?;
    let now = income_expense_by_month(source, period, filter)?;
    let then = income_expense_by_month(source, earlier, filter)?;
    let mut currencies: Vec<CurrencyCode> = now
        .iter()
        .chain(&then)
        .map(|result| result.currency)
        .collect();
    currencies.sort_unstable();
    currencies.dedup();
    let totals = |results: &[IncomeExpenseByMonth], currency| {
        results
            .iter()
            .find(|result| result.currency == currency)
            .map_or_else(
                || {
                    let zero = Money::zero(currency);
                    (zero, zero, zero)
                },
                |result| (result.income, result.expenses, result.net),
            )
    };
    let mut changes = Vec::with_capacity(currencies.len());
    for currency in currencies {
        let (then_income, then_expenses, then_net) = totals(&then, currency);
        let (now_income, now_expenses, now_net) = totals(&now, currency);
        changes.push(IncomeExpenseChange {
            currency,
            income: Change::new(then_income, now_income)?,
            expenses: Change::new(then_expenses, now_expenses)?,
            net: Change::new(then_net, now_net)?,
            previous_savings_rate: savings_rate(then_income, then_net)?,
            savings_rate: savings_rate(now_income, now_net)?,
        });
    }
    Ok((earlier, changes))
}

/// The period to read a [`Source`] for when comparing: from the earlier
/// period's first day to the period's last.
pub fn comparison_span(period: Period, compare: Compare) -> Result<Period, ReportError> {
    let earlier = compare.earlier(period)?;
    Period::new(earlier.first().min(period.first()), period.last())
}
