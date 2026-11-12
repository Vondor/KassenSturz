// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Spending shown as an average per month, quarter or year (US1.13,
//! D-648). The dataset stays exact; an average divides an exact amount
//! once, by a whole number of months, quarters or years, in the book's
//! rounding mode (AGENTS.md §5, D-027). A total is divided from the exact
//! total, never added up from rounded rows.

use kassensturz_core::{Date, Money, RoundingMode};
use kassensturz_store::Book;

use crate::{Filter, Period, ReportError, Source};

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

/// An average that counts months (quarters, years) before the first
/// transaction it could count (M6-B207, #590, D-668): those count as zero,
/// so the average comes out lower than what was spent in the months that
/// have transactions. The figure stays as it is; the tile and the AI's
/// answer say so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CountedEarly {
    /// What the average is taken per.
    pub per: Per,
    /// How many months (quarters, years) the average divides by: its
    /// [`Average::count`].
    pub counted: u32,
    /// How many of them lie before the first transaction; at least one and
    /// at most `counted`.
    pub before: u32,
}

/// Whether an average per `per` over `period` as of `today` counts months
/// (quarters, years) before the first transaction that `filter` keeps
/// (M6-B207, #590, D-668), and how many.
///
/// The first transaction is the first day with a part of income or
/// spending that passes `filter` (its accounts and categories), in the
/// whole book. The months before it are counted the way [`Per::count`]
/// counts, so a few days are not a month. `source` must have been read
/// for at least `period`; the book is read again only when the period's
/// first transaction is late enough to matter, to see whether one comes
/// before the period. `None` when nothing is counted early, and when the
/// period has no transaction at all (then nothing is averaged).
pub fn counted_early(
    book: &Book,
    source: &Source,
    period: Period,
    filter: &Filter,
    per: Per,
    today: Date,
) -> Result<Option<CountedEarly>, ReportError> {
    let Some(first) = source
        .parts(period, filter)?
        .iter()
        .map(|part| part.day)
        .min()
    else {
        return Ok(None);
    };
    if first <= period.first() {
        return Ok(None);
    }
    let counted = per.count(period, today)?;
    // The days the average counts end today in a period that is not over.
    let end = if period.contains(today) {
        today
    } else {
        period.last()
    };
    let from_first = if first > end {
        0
    } else {
        per.count(Period::new(first, end)?, end)?
    };
    let before = counted.saturating_sub(from_first);
    if before == 0 {
        return Ok(None);
    }
    // A transaction before the period means the months are real zeros.
    let earliest = book.booking_date_range()?.map(|(earliest, _)| earliest);
    if let Some(earliest) = earliest.filter(|earliest| *earliest < period.first()) {
        let day_before = period
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
    Ok(Some(CountedEarly {
        per,
        counted,
        before,
    }))
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

    /// A synthetic book: a checking account with groceries from 3 August
    /// 2026, and a savings account (`old`) with one payment on 10 January
    /// 2025, so the book itself starts long before.
    fn late_start() -> (
        Book,
        kassensturz_core::AccountId,
        kassensturz_core::AccountId,
    ) {
        use crate::tests::{EUR, account, money};
        use kassensturz_core::{AccountKind, Split, Transaction, TransactionId};
        let mut book = Book::create_in_memory().unwrap();
        book.use_currency(EUR).unwrap();
        let checking = account(&mut book, "Checking", AccountKind::Asset, EUR, None);
        let old = account(&mut book, "Savings", AccountKind::Asset, EUR, None);
        let food = account(&mut book, "Groceries", AccountKind::Expense, EUR, None);
        let mut add = |day: Date, from| {
            let splits = vec![
                Split::new(from, money(-4_000)),
                Split::new(food, money(4_000)),
            ];
            let transaction =
                Transaction::new(TransactionId::generate(), day, None, splits).expect("balanced");
            book.add_transaction(&transaction).expect("added");
        };
        add(day(2025, 1, 10), old);
        for month in 8..=10 {
            add(day(2026, month, 3), checking);
        }
        (book, checking, old)
    }

    fn early(
        book: &Book,
        shown: Period,
        filter: &Filter,
        per: Per,
        today: Date,
    ) -> Option<CountedEarly> {
        let source = Source::read(book, shown).unwrap();
        counted_early(book, &source, shown, filter, per, today).unwrap()
    }

    #[test]
    fn months_before_the_first_transaction_of_the_chosen_accounts_are_named() {
        let (book, checking, _) = late_start();
        let checking_only = Filter {
            accounts: Some(vec![checking]),
            categories: None,
            tags: None,
        };
        let year = period(day(2025, 11, 1), day(2026, 10, 31));
        let today = day(2026, 11, 9);
        // Twelve months, nine of them (November to July) before 3 August.
        assert_eq!(
            early(&book, year, &checking_only, Per::Month, today),
            Some(CountedEarly {
                per: Per::Month,
                counted: 12,
                before: 9,
            })
        );
        // Per quarter: four, three of them before.
        assert_eq!(
            early(&book, year, &checking_only, Per::Quarter, today).map(|e| (e.counted, e.before)),
            Some((4, 3))
        );
        // The whole book has a transaction before the period: its zeros are
        // real, so nothing is named.
        assert_eq!(
            early(&book, year, &Filter::default(), Per::Month, today),
            None
        );
        // A period that starts with the first transaction names nothing,
        // and a few days late are not a month.
        let from_august = period(day(2026, 8, 1), day(2026, 10, 31));
        assert_eq!(
            early(&book, from_august, &checking_only, Per::Month, today),
            None
        );
        // A period with no transaction averages nothing.
        let empty = period(day(2025, 3, 1), day(2025, 5, 31));
        assert_eq!(early(&book, empty, &checking_only, Per::Month, today), None);
    }

    #[test]
    fn a_period_not_over_yet_counts_its_early_months_up_to_today() {
        let (book, checking, _) = late_start();
        let checking_only = Filter {
            accounts: Some(vec![checking]),
            categories: None,
            tags: None,
        };
        let this_year = period(day(2026, 1, 1), day(2026, 12, 31));
        // On 9 November: ten months counted, seven (January to July) early.
        assert_eq!(
            early(
                &book,
                this_year,
                &checking_only,
                Per::Month,
                day(2026, 11, 9)
            )
            .map(|e| (e.counted, e.before)),
            Some((10, 7))
        );
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
