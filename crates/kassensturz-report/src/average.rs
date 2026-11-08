// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Spending shown as an average per month, quarter or year (US1.13,
//! D-648). The dataset stays exact; an average divides an exact amount
//! once, by a whole number of months, quarters or years, in the book's
//! rounding mode (AGENTS.md §5, D-027). A total is divided from the exact
//! total, never added up from rounded rows.

use kassensturz_core::{Date, Money, RoundingMode};

use crate::{Period, ReportError};

/// The words of `params.average`, the default first: the total, or an
/// average per month, quarter or year.
pub const AVERAGE_WORDS: [&str; 4] = ["total", "per-month", "per-quarter", "per-year"];

/// Days in 400 Gregorian years: 4800 months, 1600 quarters, 400 years.
const DAYS_IN_400_YEARS: u64 = 146_097;

/// What an average is taken per.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Per {
    /// An average per month.
    Month,
    /// An average per quarter (three months).
    Quarter,
    /// An average per year.
    Year,
}

impl Per {
    /// The `params.average` word: `None` for `total` and for a word that
    /// is not one of [`AVERAGE_WORDS`].
    pub fn read(word: &str) -> Option<Self> {
        Some(match word {
            "per-month" => Self::Month,
            "per-quarter" => Self::Quarter,
            "per-year" => Self::Year,
            _ => return None,
        })
    }

    /// The word [`Per::read`] reads.
    pub const fn word(self) -> &'static str {
        match self {
            Self::Month => "per-month",
            Self::Quarter => "per-quarter",
            Self::Year => "per-year",
        }
    }

    /// How many of these 400 Gregorian years hold.
    const fn in_400_years(self) -> u64 {
        match self {
            Self::Month => 4800,
            Self::Quarter => 1600,
            Self::Year => 400,
        }
    }

    /// How many months (quarters, years) `period` counts, as of `today`
    /// (D-648): its days up to `today` (all of them for a period that has
    /// not begun), in months of the average Gregorian length, rounded half
    /// up and at least one. So a whole month, a salary month and a quarter
    /// count as what they are whatever their days, and a period that is not
    /// over yet counts only what has passed: this year on 7 November is 10
    /// months.
    pub fn count(self, period: Period, today: Date) -> Result<u32, ReportError> {
        let last = if period.contains(today) {
            today
        } else {
            period.last()
        };
        let days = period
            .first()
            .until(last)
            .map_err(|_| ReportError::BadPeriod)?
            .get_days();
        let days = u64::try_from(days).map_err(|_| ReportError::BadPeriod)? + 1;
        // Half up in integers: (2 · days · n + whole) / (2 · whole).
        let count = (2 * days * self.in_400_years() + DAYS_IN_400_YEARS) / (2 * DAYS_IN_400_YEARS);
        Ok(u32::try_from(count.max(1)).unwrap_or(u32::MAX))
    }
}

/// An average: what it is taken per, by how many it divides, and how it
/// rounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Average {
    /// What the average is taken per.
    pub per: Per,
    /// How many months (quarters, years) the period counts; never zero.
    pub count: u32,
    /// How the division rounds: the book's mode (D-027).
    pub rounding: RoundingMode,
}

impl Average {
    /// The average per `per` over `period` as of `today`, rounded in
    /// `rounding` (the book's mode, D-027).
    pub fn new(
        per: Per,
        period: Period,
        today: Date,
        rounding: RoundingMode,
    ) -> Result<Self, ReportError> {
        Ok(Self {
            per,
            count: per.count(period, today)?,
            rounding,
        })
    }

    /// `money` divided by the count, rounded once to whole minor units.
    pub fn of(&self, money: Money) -> Money {
        let minor = self
            .rounding
            .divide(
                i128::from(money.minor_units()),
                i128::from(self.count.max(1)),
            )
            .and_then(|minor| i64::try_from(minor).ok())
            // Dividing by one or more never grows an amount.
            .unwrap_or(money.minor_units());
        Money::from_minor_units(minor, money.currency())
    }

    /// `minor` minor units divided by the count, as [`Average::of`].
    pub fn of_minor(&self, minor: i64) -> i64 {
        self.rounding
            .divide(i128::from(minor), i128::from(self.count.max(1)))
            .and_then(|minor| i64::try_from(minor).ok())
            .unwrap_or(minor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kassensturz_core::CurrencyCode;

    fn day(year: i16, month: i8, day: i8) -> Date {
        Date::new(year, month, day).expect("a date")
    }

    fn period(first: Date, last: Date) -> Period {
        Period::new(first, last).expect("a period")
    }

    #[test]
    fn a_month_a_salary_month_and_a_quarter_count_as_what_they_are() {
        let later = day(2027, 6, 1);
        let count = |first, last, per: Per| per.count(period(first, last), later).unwrap();
        assert_eq!(count(day(2026, 2, 1), day(2026, 2, 28), Per::Month), 1);
        assert_eq!(count(day(2026, 1, 31), day(2026, 2, 27), Per::Month), 1);
        assert_eq!(count(day(2026, 2, 28), day(2026, 3, 30), Per::Month), 1);
        assert_eq!(count(day(2026, 8, 1), day(2026, 10, 31), Per::Month), 3);
        assert_eq!(count(day(2026, 8, 1), day(2026, 10, 31), Per::Quarter), 1);
        assert_eq!(count(day(2026, 1, 1), day(2026, 12, 31), Per::Month), 12);
        assert_eq!(count(day(2026, 1, 1), day(2026, 12, 31), Per::Quarter), 4);
        assert_eq!(count(day(2026, 1, 1), day(2026, 12, 31), Per::Year), 1);
        assert_eq!(count(day(2024, 1, 1), day(2026, 12, 31), Per::Year), 3);
        // A few days are still one; half a month and more rounds up.
        assert_eq!(count(day(2026, 3, 1), day(2026, 3, 3), Per::Month), 1);
        assert_eq!(count(day(2026, 3, 1), day(2026, 4, 14), Per::Month), 1);
        assert_eq!(count(day(2026, 3, 1), day(2026, 4, 15), Per::Month), 2);
    }

    #[test]
    fn a_period_not_over_yet_counts_up_to_today() {
        let year = period(day(2026, 1, 1), day(2026, 12, 31));
        assert_eq!(Per::Month.count(year, day(2026, 11, 7)).unwrap(), 10);
        assert_eq!(Per::Month.count(year, day(2026, 1, 2)).unwrap(), 1);
        // A period that has not begun counts all its days.
        assert_eq!(Per::Month.count(year, day(2025, 6, 1)).unwrap(), 12);
    }

    #[test]
    fn an_average_divides_once_in_the_books_rounding_mode() {
        let eur = CurrencyCode::new("EUR").unwrap();
        let average = |rounding| Average {
            per: Per::Month,
            count: 3,
            rounding,
        };
        let money = Money::from_minor_units(10_000, eur);
        assert_eq!(
            average(RoundingMode::HalfAwayFromZero).of(money),
            Money::from_minor_units(3_333, eur)
        );
        assert_eq!(
            average(RoundingMode::Ceiling).of(money),
            Money::from_minor_units(3_334, eur)
        );
        // 0,05 € in three months: half a cent and more rounds away.
        assert_eq!(average(RoundingMode::HalfAwayFromZero).of_minor(5), 2);
        assert_eq!(average(RoundingMode::HalfAwayFromZero).of_minor(-5), -2);
    }

    #[test]
    fn the_words_read_back() {
        for per in [Per::Month, Per::Quarter, Per::Year] {
            assert_eq!(Per::read(per.word()), Some(per));
            assert!(AVERAGE_WORDS.contains(&per.word()));
        }
        assert_eq!(Per::read("total"), None);
        assert_eq!(AVERAGE_WORDS[0], "total");
    }
}
