// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Budgets: an amount the user plans to spend on one spending category in a
//! stretch of time (D-344, D-345; layout 22). A budget is a way of looking
//! at the spending and nothing else: it never changes a booking, a balance
//! or a total. How a budget counts is the reporting layer's job (D-346,
//! D-347); this module only says what a budget is and which days its
//! periods cover.

use crate::id::define_id;
use crate::{AccountId, Date, Money};

define_id!(
    /// Identifies one of the user's budgets within a book.
    BudgetId
);

/// How long one period of a budget is (Q-216 = D).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BudgetEvery {
    /// Every calendar month; it repeats.
    Month,
    /// Every calendar quarter (January to March, April to June, …); it
    /// repeats.
    Quarter,
    /// Every calendar year; it repeats.
    Year,
    /// One stretch of days, from a first to a last day; it does not repeat,
    /// so it has no next period to carry anything into.
    Range,
}

impl BudgetEvery {
    /// Whether one period follows another (all but [`BudgetEvery::Range`]).
    pub const fn repeats(self) -> bool {
        !matches!(self, Self::Range)
    }

    /// The first day of the calendar month, quarter or year `day` is in, or
    /// `None` for a range, whose period is the budget's own days.
    pub fn period_start(self, day: Date) -> Option<Date> {
        match self {
            Self::Month => Some(day.first_of_month()),
            Self::Quarter => {
                let first_month = (day.month() - 1) / 3 * 3 + 1;
                // The first day of a month that exists in every year.
                Date::new(day.year(), first_month, 1).ok()
            }
            Self::Year => Some(day.first_of_year()),
            Self::Range => None,
        }
    }

    /// The last day of the calendar month, quarter or year `day` is in, or
    /// `None` for a range.
    pub fn period_end(self, day: Date) -> Option<Date> {
        match self {
            Self::Month => Some(day.last_of_month()),
            Self::Quarter => {
                let last_month = (day.month() - 1) / 3 * 3 + 3;
                Date::new(day.year(), last_month, 1)
                    .ok()
                    .map(Date::last_of_month)
            }
            Self::Year => Some(day.last_of_year()),
            Self::Range => None,
        }
    }

    /// Whether a period of this kind starts on `day`. Any day can start a
    /// range.
    pub fn is_period_start(self, day: Date) -> bool {
        self.period_start(day).is_none_or(|start| start == day)
    }

    /// Whether a period of this kind ends on `day`. Any day can end a
    /// range.
    pub fn is_period_end(self, day: Date) -> bool {
        self.period_end(day).is_none_or(|end| end == day)
    }
}

/// What a budget plans from one of its periods on, until the next
/// [`BudgetAmount`] of the same budget (D-345). Dated rows keep the past: a
/// change applies from the start of one period on, and earlier periods keep
/// what they had.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BudgetAmount {
    /// The first day of the budget's period from which this applies.
    pub from_day: Date,
    /// What the user plans to spend in each period, more than zero, in the
    /// category's currency.
    pub amount: Money,
    /// Whether what is left at the end of each period (or what was spent
    /// beyond the amount) goes into the next one (Q-266). Never for a range.
    pub carry_over: bool,
}

/// A budget as the book keeps it (D-344).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Budget {
    /// The budget's ID.
    pub id: BudgetId,
    /// The spending category it is for, counted with everything inside it
    /// (D-291).
    pub category: AccountId,
    /// How long one period is.
    pub every: BudgetEvery,
    /// The first day of its first period.
    pub first_day: Date,
    /// The last day of its last period, or `None` if it keeps going (never
    /// for a range).
    pub last_day: Option<Date>,
    /// What it plans, oldest first; the first starts on `first_day`.
    pub amounts: Vec<BudgetAmount>,
}

impl Budget {
    /// The first and last day of the budget's period that contains `day`,
    /// or `None` if the budget hasn't started by then or has ended.
    pub fn period_containing(&self, day: Date) -> Option<(Date, Date)> {
        if day < self.first_day || self.last_day.is_some_and(|last| day > last) {
            return None;
        }
        match (self.every.period_start(day), self.every.period_end(day)) {
            (Some(start), Some(end)) => Some((start, end)),
            // A range has one period: its own days.
            _ => Some((self.first_day, self.last_day.unwrap_or(day))),
        }
    }

    /// What the budget plans for the period that starts on `period_start`:
    /// the last of its amounts that applies from that day or earlier.
    pub fn amount_for(&self, period_start: Date) -> Option<&BudgetAmount> {
        self.amounts
            .iter()
            .filter(|amount| amount.from_day <= period_start)
            .max_by_key(|amount| amount.from_day)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CurrencyCode;

    fn day(year: i16, month: i8, day: i8) -> Date {
        Date::new(year, month, day).expect("valid date")
    }

    fn eur(minor_units: i64) -> Money {
        Money::from_minor_units(minor_units, CurrencyCode::new("EUR").expect("valid code"))
    }

    #[test]
    fn periods_are_calendar_months_quarters_and_years() {
        let in_august = day(2026, 8, 14);
        assert_eq!(
            BudgetEvery::Month.period_start(in_august),
            Some(day(2026, 8, 1))
        );
        assert_eq!(
            BudgetEvery::Month.period_end(in_august),
            Some(day(2026, 8, 31))
        );
        assert_eq!(
            BudgetEvery::Quarter.period_start(in_august),
            Some(day(2026, 7, 1))
        );
        assert_eq!(
            BudgetEvery::Quarter.period_end(in_august),
            Some(day(2026, 9, 30))
        );
        assert_eq!(
            BudgetEvery::Year.period_start(in_august),
            Some(day(2026, 1, 1))
        );
        assert_eq!(
            BudgetEvery::Year.period_end(in_august),
            Some(day(2026, 12, 31))
        );
        assert_eq!(BudgetEvery::Range.period_start(in_august), None);
        // Every quarter, including a leap year's first one.
        for (month, start, end) in [
            (1, day(2028, 1, 1), day(2028, 3, 31)),
            (2, day(2028, 1, 1), day(2028, 3, 31)),
            (4, day(2028, 4, 1), day(2028, 6, 30)),
            (9, day(2028, 7, 1), day(2028, 9, 30)),
            (12, day(2028, 10, 1), day(2028, 12, 31)),
        ] {
            let inside = day(2028, month, 15);
            assert_eq!(BudgetEvery::Quarter.period_start(inside), Some(start));
            assert_eq!(BudgetEvery::Quarter.period_end(inside), Some(end));
        }
        assert_eq!(
            BudgetEvery::Month.period_end(day(2028, 2, 3)),
            Some(day(2028, 2, 29))
        );
    }

    #[test]
    fn a_period_starts_and_ends_only_on_its_own_days() {
        assert!(BudgetEvery::Month.is_period_start(day(2026, 11, 1)));
        assert!(!BudgetEvery::Month.is_period_start(day(2026, 11, 2)));
        assert!(BudgetEvery::Quarter.is_period_start(day(2026, 10, 1)));
        assert!(!BudgetEvery::Quarter.is_period_start(day(2026, 11, 1)));
        assert!(BudgetEvery::Year.is_period_end(day(2026, 12, 31)));
        assert!(!BudgetEvery::Year.is_period_end(day(2026, 11, 30)));
        assert!(BudgetEvery::Range.is_period_start(day(2026, 11, 17)));
        assert!(BudgetEvery::Range.is_period_end(day(2026, 11, 17)));
        assert!(!BudgetEvery::Range.repeats());
        assert!(BudgetEvery::Quarter.repeats());
    }

    #[test]
    fn a_budget_finds_its_period_and_its_amount() {
        let budget = Budget {
            id: BudgetId::generate(),
            category: AccountId::generate(),
            every: BudgetEvery::Month,
            first_day: day(2026, 9, 1),
            last_day: Some(day(2026, 12, 31)),
            amounts: vec![
                BudgetAmount {
                    from_day: day(2026, 9, 1),
                    amount: eur(40_000),
                    carry_over: false,
                },
                BudgetAmount {
                    from_day: day(2026, 11, 1),
                    amount: eur(45_000),
                    carry_over: true,
                },
            ],
        };
        assert_eq!(budget.period_containing(day(2026, 8, 31)), None);
        assert_eq!(
            budget.period_containing(day(2026, 10, 9)),
            Some((day(2026, 10, 1), day(2026, 10, 31)))
        );
        assert_eq!(budget.period_containing(day(2027, 1, 1)), None);
        assert_eq!(
            budget.amount_for(day(2026, 10, 1)).map(|row| row.amount),
            Some(eur(40_000))
        );
        assert_eq!(
            budget.amount_for(day(2026, 12, 1)).map(|row| row.amount),
            Some(eur(45_000))
        );
        assert_eq!(budget.amount_for(day(2026, 8, 1)), None);

        let trip = Budget {
            every: BudgetEvery::Range,
            first_day: day(2026, 7, 10),
            last_day: Some(day(2026, 7, 24)),
            ..budget
        };
        assert_eq!(
            trip.period_containing(day(2026, 7, 12)),
            Some((day(2026, 7, 10), day(2026, 7, 24)))
        );
    }
}
