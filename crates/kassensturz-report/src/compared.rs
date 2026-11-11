// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Spending by category next to a comparison (US3.1a, #587, D-687): each
//! category's figure in the chart's period and in an earlier one, or the
//! average of several earlier ones. Both sides are counted exactly as the
//! chart counts its own figure ([`spending_by_category_shown`]), and an
//! average adds the periods' exact amounts and divides once, in the book's
//! rounding mode (D-027).

use std::collections::HashMap;

use kassensturz_core::{AccountId, CurrencyCode, Date, GroupId, Money, RoundingMode};
use kassensturz_store::Book;

use crate::{
    Average, CategoryRow, Compare, Filter, Level, Per, Period, ReportError, Shown, Source,
    SpendingByCategory, spending_by_category_shown,
};

/// The words of a spending chart's `params.compare`, the default first:
/// no comparison, the period before, the same period last year, or the
/// average of the 3, 6 or 12 periods before.
pub const COMPARE_WORDS: [&str; 6] = [
    "none",
    "previous_period",
    "same_last_year",
    "average-3",
    "average-6",
    "average-12",
];

/// What a spending chart compares each category with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompareWith {
    /// One earlier period: the one before, or the same days a year earlier.
    Earlier(Compare),
    /// The average of this many periods right before, each as long as the
    /// chart's own ([`Period::before`] taken this many times).
    Average(u32),
}

impl CompareWith {
    /// The `params.compare` word: `None` for `none` and for a word that is
    /// not one of [`COMPARE_WORDS`].
    pub fn read(word: &str) -> Option<Self> {
        Some(match word {
            "average-3" => Self::Average(3),
            "average-6" => Self::Average(6),
            "average-12" => Self::Average(12),
            _ => Self::Earlier(Compare::from_id(word)?),
        })
    }

    /// The word [`CompareWith::read`] reads.
    pub const fn word(self) -> &'static str {
        match self {
            Self::Earlier(compare) => compare.id(),
            Self::Average(3) => "average-3",
            Self::Average(6) => "average-6",
            Self::Average(_) => "average-12",
        }
    }

    /// The periods compared with, oldest first: one for an earlier period,
    /// `n` back to back for an average of `n`.
    pub fn periods(self, period: Period) -> Result<Vec<Period>, ReportError> {
        match self {
            Self::Earlier(compare) => Ok(vec![compare.earlier(period)?]),
            Self::Average(count) => {
                let mut periods = Vec::new();
                let mut at = period;
                for _ in 0..count {
                    at = at.before()?;
                    periods.push(at);
                }
                periods.reverse();
                Ok(periods)
            }
        }
    }

    /// The days the comparison counts: its periods from the first day of
    /// the oldest to the last day of the newest. They follow each other
    /// without a gap, so the spending of these days is the periods' sum.
    pub fn span(self, period: Period) -> Result<Period, ReportError> {
        let periods = self.periods(period)?;
        match (periods.first(), periods.last()) {
            (Some(first), Some(last)) => Period::new(first.first(), last.last()),
            _ => Err(ReportError::BadPeriod),
        }
    }

    /// The days a [`Source`] must be read for: from the comparison's first
    /// day to the chart's last. Refused like any other period with more
    /// than [`MAX_MONTHS`](crate::MAX_MONTHS) months
    /// (`report.period-too-long`).
    pub fn read_span(self, period: Period) -> Result<Period, ReportError> {
        let span = self.span(period)?;
        Period::new(span.first().min(period.first()), period.last())
    }

    /// What the comparison's sum is divided by to give the figure a bar
    /// shows: the number of periods it averages (one for an earlier
    /// period), or, when the chart shows an average per `per` itself
    /// (D-648), what [`Per::count`] counts in the comparison's days, so both
    /// bars are per month (quarter, year). One division, in `rounding`.
    pub fn divisor(
        self,
        period: Period,
        per: Option<Per>,
        today: Date,
        rounding: RoundingMode,
    ) -> Result<Average, ReportError> {
        let count = match (per, self) {
            (Some(per), _) => per.count(self.span(period)?, today)?,
            (None, Self::Earlier(_)) => 1,
            (None, Self::Average(count)) => count,
        };
        Ok(Average {
            per: per.unwrap_or(Per::Month),
            count,
            rounding,
        })
    }
}

/// One row of spending with its comparison.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComparedRow {
    /// The category, group or money not sorted yet, with its exact amount
    /// in the chart's period (zero when it had spending only in the
    /// comparison).
    pub row: CategoryRow,
    /// Its exact amount in all the days of the comparison, before the
    /// division of [`CompareWith::divisor`].
    pub earlier: Money,
}

/// The spending of one currency with its comparison.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpendingCompared {
    /// The spending of the chart's period, as [`spending_by_category_shown`]
    /// gives it: its total, hidden categories and order.
    pub now: SpendingByCategory,
    /// The rows of `now` in their order, then the rows with spending only in
    /// the comparison, ordered by it, each with its comparison.
    pub rows: Vec<ComparedRow>,
    /// All the comparison counts, from the exact amounts.
    pub earlier_total: Money,
}

type RowKey = (Option<AccountId>, Option<GroupId>);

fn key(row: &CategoryRow) -> RowKey {
    (row.category, row.group)
}

fn nothing(currency: CurrencyCode) -> SpendingByCategory {
    SpendingByCategory {
        currency,
        rows: Vec::new(),
        total: Money::from_minor_units(0, currency),
        hidden: Vec::new(),
        hidden_total: Money::from_minor_units(0, currency),
    }
}

/// Spending in `period` by category next to its comparison `compare`, one
/// result per currency in the order of their codes: each currency compares
/// within itself. Both sides are counted with the same `filter`, `level`
/// and `shown` (groups, "only these", hidden categories). `source` must
/// have been read for [`CompareWith::read_span`].
pub fn spending_compared(
    source: &Source,
    period: Period,
    filter: &Filter,
    level: Level,
    shown: &Shown,
    compare: CompareWith,
) -> Result<Vec<SpendingCompared>, ReportError> {
    let now = spending_by_category_shown(source, period, filter, level, shown)?;
    let earlier = spending_by_category_shown(source, compare.span(period)?, filter, level, shown)?;
    let mut currencies: Vec<CurrencyCode> = now
        .iter()
        .chain(&earlier)
        .map(|result| result.currency)
        .collect();
    currencies.sort_by_key(|code| code.to_string());
    currencies.dedup();
    let mut results = Vec::new();
    for currency in currencies {
        let now = now
            .iter()
            .find(|result| result.currency == currency)
            .cloned()
            .unwrap_or_else(|| nothing(currency));
        let then = earlier
            .iter()
            .find(|result| result.currency == currency)
            .cloned()
            .unwrap_or_else(|| nothing(currency));
        let zero = Money::from_minor_units(0, currency);
        let by_key: HashMap<RowKey, Money> =
            then.rows.iter().map(|row| (key(row), row.total)).collect();
        let mut rows: Vec<ComparedRow> = now
            .rows
            .iter()
            .map(|row| ComparedRow {
                row: row.clone(),
                earlier: by_key.get(&key(row)).copied().unwrap_or(zero),
            })
            .collect();
        // A category with spending only in the comparison still shows, with
        // nothing now; `then` is ordered largest first already.
        rows.extend(
            then.rows
                .iter()
                .filter(|row| !now.rows.iter().any(|own| key(own) == key(row)))
                .map(|row| ComparedRow {
                    row: CategoryRow {
                        total: zero,
                        ..row.clone()
                    },
                    earlier: row.total,
                }),
        );
        results.push(SpendingCompared {
            now,
            rows,
            earlier_total: then.total,
        });
    }
    Ok(results)
}

/// A comparison whose periods end before the first transaction it could
/// count (US3.1a, as D-668 does for an average): those count as zero, so
/// the comparison bar is low or empty. The figure stays as it is; the chart
/// says so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComparedEarly {
    /// How many periods the comparison counts: one, or the `n` it averages.
    pub counted: u32,
    /// How many of them end before the first transaction; at least one.
    pub before: u32,
}

/// Whether the comparison `compare` of `period` counts periods that end
/// before the first transaction `filter` keeps, found as
/// [`counted_early`](crate::counted_early) finds it: the first day with a
/// part of income or spending that passes `filter`, in the whole book.
/// `source` must have been read for [`CompareWith::read_span`]. `None` when
/// no period is early, and when nothing at all is counted.
pub fn compared_early(
    book: &Book,
    source: &Source,
    period: Period,
    filter: &Filter,
    compare: CompareWith,
) -> Result<Option<ComparedEarly>, ReportError> {
    let periods = compare.periods(period)?;
    let span = compare.read_span(period)?;
    let Some(first) = source
        .parts(span, filter)?
        .iter()
        .map(|part| part.day)
        .min()
    else {
        return Ok(None);
    };
    let before = periods.iter().filter(|shown| shown.last() < first).count();
    if before == 0 {
        return Ok(None);
    }
    // A transaction before everything read means the periods are real
    // zeros, not days before the book began.
    let earliest = book.booking_date_range()?.map(|(earliest, _)| earliest);
    if let Some(earliest) = earliest.filter(|earliest| *earliest < span.first()) {
        let day_before = span
            .first()
            .yesterday()
            .map_err(|_| ReportError::BadPeriod)?;
        let earlier = Period::new(earliest, day_before)?;
        if !Source::read(book, earlier)?
            .parts(earlier, filter)?
            .is_empty()
        {
            return Ok(None);
        }
    }
    Ok(Some(ComparedEarly {
        counted: u32::try_from(periods.len()).unwrap_or(u32::MAX),
        before: u32::try_from(before).unwrap_or(u32::MAX),
    }))
}
