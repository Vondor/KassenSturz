// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Periods: ranges of calendar dates without time zone (D-061).

use jiff::ToSpan;
use kassensturz_core::Date;

use crate::ReportError;

/// The most months a period may span, so a dashboard file with a period
/// from year 1 to year 9999 can't ask for a hundred thousand rows (D-281).
pub const MAX_MONTHS: usize = 1200;

/// The word that names a typed range of days (D-332): `FIRST..LAST` with
/// ISO dates, `..LAST` for everything up to a day, `FIRST..` for a day until
/// today. Fails with [`ReportError::BadPeriod`] for no day at all or an end
/// before the start, and with [`ReportError::PeriodTooLong`] for more than
/// [`MAX_MONTHS`] months between two days.
pub fn range_word(first: Option<Date>, last: Option<Date>) -> Result<String, ReportError> {
    match (first, last) {
        (None, None) => Err(ReportError::BadPeriod),
        (Some(first), Some(last)) => {
            Period::new(first, last)?;
            Ok(format!("{first}..{last}"))
        }
        (Some(first), None) => Ok(format!("{first}..")),
        (None, Some(last)) => Ok(format!("..{last}")),
    }
}

/// The days a typed range word names, if it is one that [`range_word`] could
/// have written: valid ISO dates, at least one of them, in order.
pub fn read_range_word(word: &str) -> Option<(Option<Date>, Option<Date>)> {
    let (first, last) = word.split_once("..")?;
    let day = |text: &str| -> Option<Option<Date>> {
        if text.is_empty() {
            return Some(None);
        }
        let ok = text.len() == 10 && text.bytes().all(|b| b.is_ascii_digit() || b == b'-');
        ok.then(|| text.parse().ok()).flatten().map(Some)
    };
    let (first, last) = (day(first)?, day(last)?);
    match (first, last) {
        (None, None) => None,
        (Some(first), Some(last)) if last < first => None,
        _ => Some((first, last)),
    }
}

/// The most months a "last N months" period counts back (D-644).
pub const MAX_ROLLING_MONTHS: u16 = 120;

/// A period that moves with today (US1.12, D-644): the last few calendar
/// months, or a salary month that starts on a day of the month. Its word
/// names it completely, so a component, a period box or the filter bar
/// stores it like any other period and every figure on the page counts the
/// same days.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Rolling {
    /// `count` calendar months up to today's. With `complete`, the month
    /// today is in is left out: the months end with the last whole one.
    /// Words `last-N-months` and `last-N-complete-months`.
    Months {
        /// How many months, 1 to [`MAX_ROLLING_MONTHS`].
        count: u16,
        /// Whether only whole months that are over count.
        complete: bool,
    },
    /// The month from one salary day to the day before the next (D-645).
    /// In a month shorter than `day` its last day counts. Words
    /// `this-salary-month-D` and `last-salary-month-D`.
    SalaryMonth {
        /// The day of the month it starts on, 1 to 31.
        day: i8,
        /// Whether it is the salary month before the one today is in.
        before: bool,
    },
}

impl Rolling {
    /// The word a period of this kind is stored as.
    pub fn word(self) -> String {
        match self {
            Self::Months {
                count,
                complete: false,
            } => format!("last-{count}-months"),
            Self::Months {
                count,
                complete: true,
            } => format!("last-{count}-complete-months"),
            Self::SalaryMonth { day, before: false } => format!("this-salary-month-{day}"),
            Self::SalaryMonth { day, before: true } => format!("last-salary-month-{day}"),
        }
    }

    /// The rolling period `word` names, if it is one that [`Self::word`]
    /// could have written (numbers without leading zeros, in range).
    pub fn read(word: &str) -> Option<Self> {
        let number = |text: &str| -> Option<u16> {
            let plain = !text.is_empty()
                && !text.starts_with('0')
                && text.len() <= 3
                && text.bytes().all(|b| b.is_ascii_digit());
            plain.then(|| text.parse().ok()).flatten()
        };
        if let Some(rest) = word.strip_prefix("last-") {
            if let Some(count) = rest.strip_suffix("-complete-months") {
                let count = number(count).filter(|n| *n <= MAX_ROLLING_MONTHS)?;
                return Some(Self::Months {
                    count,
                    complete: true,
                });
            }
            if let Some(count) = rest.strip_suffix("-months") {
                let count = number(count).filter(|n| *n <= MAX_ROLLING_MONTHS)?;
                return Some(Self::Months {
                    count,
                    complete: false,
                });
            }
        }
        let (day, before) = if let Some(day) = word.strip_prefix("this-salary-month-") {
            (day, false)
        } else {
            (word.strip_prefix("last-salary-month-")?, true)
        };
        let day = number(day).filter(|day| *day <= 31)?;
        Some(Self::SalaryMonth {
            day: i8::try_from(day).ok()?,
            before,
        })
    }

    /// The days it covers, seen from `today`.
    pub fn period(self, today: Date) -> Result<Period, ReportError> {
        let bad = |_| ReportError::BadPeriod;
        match self {
            Self::Months { count, complete } => {
                let end = if complete {
                    today.first_of_month().checked_sub(1.month()).map_err(bad)?
                } else {
                    today.first_of_month()
                };
                let first = end
                    .checked_sub((i32::from(count) - 1).months())
                    .map_err(bad)?;
                Period::new(first, end.last_of_month())
            }
            Self::SalaryMonth { day, before } => {
                let this = salary_day(today.first_of_month(), day)?;
                let mut month = if today < this {
                    today.first_of_month().checked_sub(1.month()).map_err(bad)?
                } else {
                    today.first_of_month()
                };
                if before {
                    month = month.checked_sub(1.month()).map_err(bad)?;
                }
                salary_month(month, day)
            }
        }
    }
}

/// The salary day `day` in the month that starts on `month`: that day, or
/// the month's last day when the month is shorter (D-645).
fn salary_day(month: Date, day: i8) -> Result<Date, ReportError> {
    Date::new(month.year(), month.month(), day.min(month.days_in_month()))
        .map_err(|_| ReportError::BadPeriod)
}

/// The salary month that starts in the month beginning on `month`: from its
/// salary day to the day before the next month's.
fn salary_month(month: Date, day: i8) -> Result<Period, ReportError> {
    let bad = |_| ReportError::BadPeriod;
    let first = salary_day(month, day)?;
    let next = salary_day(month.checked_add(1.month()).map_err(bad)?, day)?;
    Period::new(first, next.yesterday().map_err(bad)?)
}

/// Whether `word` names a period a document, a component or the filter bar
/// can keep: a fixed name of [`crate::PERIODS`], a rolling period
/// ([`Rolling`]) or a typed range of days (D-332).
pub fn is_period_word(word: &str) -> bool {
    crate::PERIODS.contains(&word)
        || Rolling::read(word).is_some()
        || read_range_word(word).is_some()
}

/// A range of days, both ends included.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Period {
    first: Date,
    last: Date,
}

impl Period {
    /// The days from `first` to `last`, both included. Fails with
    /// [`ReportError::BadPeriod`] if `last` is before `first`, and with
    /// [`ReportError::PeriodTooLong`] if it touches more than
    /// [`MAX_MONTHS`] calendar months.
    pub fn new(first: Date, last: Date) -> Result<Self, ReportError> {
        if last < first {
            return Err(ReportError::BadPeriod);
        }
        let months = i64::from(last.year() - first.year()) * 12 + i64::from(last.month())
            - i64::from(first.month())
            + 1;
        if months > MAX_MONTHS as i64 {
            return Err(ReportError::PeriodTooLong);
        }
        Ok(Self { first, last })
    }

    /// The calendar month `today` is in.
    pub fn this_month(today: Date) -> Result<Self, ReportError> {
        Self::new(today.first_of_month(), today.last_of_month())
    }

    /// The calendar month before the one `today` is in.
    pub fn last_month(today: Date) -> Result<Self, ReportError> {
        let first = today
            .first_of_month()
            .checked_sub(1.month())
            .map_err(|_| ReportError::BadPeriod)?;
        Self::new(first, first.last_of_month())
    }

    /// The month `today` is in and the eleven before it: twelve whole
    /// calendar months.
    pub fn last_12_months(today: Date) -> Result<Self, ReportError> {
        let first = today
            .first_of_month()
            .checked_sub(11.months())
            .map_err(|_| ReportError::BadPeriod)?;
        Self::new(first, today.last_of_month())
    }

    /// The calendar year `today` is in, January to December.
    pub fn this_year(today: Date) -> Result<Self, ReportError> {
        Self::new(today.first_of_year(), today.last_of_year())
    }

    /// The calendar year before the one `today` is in.
    pub fn last_year(today: Date) -> Result<Self, ReportError> {
        let first = today
            .first_of_year()
            .checked_sub(1.year())
            .map_err(|_| ReportError::BadPeriod)?;
        Self::new(first, first.last_of_year())
    }

    /// The period a component shows (`props.period`, D-289): the `page`
    /// period for `page`, otherwise the fixed period of that name as seen
    /// from `today`. `None` for a name that is no period.
    pub fn for_component(name: &str, page: Self, today: Date) -> Option<Result<Self, ReportError>> {
        match name {
            "page" => Some(Ok(page)),
            _ => Self::named(name, today, None),
        }
    }

    /// The period a stored word names, seen from `today`: a fixed name, a
    /// rolling period (D-644) or a typed range, whose open start is
    /// `earliest` (see [`Self::from_range_word`]). `None` for a word that is
    /// none of them.
    pub fn named(
        word: &str,
        today: Date,
        earliest: Option<Date>,
    ) -> Option<Result<Self, ReportError>> {
        match word {
            "this-month" => Some(Self::this_month(today)),
            "last-month" => Some(Self::last_month(today)),
            "last-12-months" => Some(Self::last_12_months(today)),
            "this-year" => Some(Self::this_year(today)),
            "last-year" => Some(Self::last_year(today)),
            _ => match Rolling::read(word) {
                Some(rolling) => Some(rolling.period(today)),
                None => Self::from_range_word(word, today, earliest),
            },
        }
    }

    /// The period a typed range names (D-332), as written by
    /// [`range_word`]: `2000-01-01..2020-01-01`, `..2025-01-01` (everything
    /// up to a day) or `2025-01-01..` (a day until `today`). An open start is
    /// the first day of the book, `earliest`, or the end when the book has
    /// no transaction. `None` for a word that is no range.
    pub fn from_range_word(
        word: &str,
        today: Date,
        earliest: Option<Date>,
    ) -> Option<Result<Self, ReportError>> {
        let (first, last) = read_range_word(word)?;
        let last = last.unwrap_or_else(|| today.max(first.unwrap_or(today)));
        let first = first.or(earliest).map_or(last, |first| first.min(last));
        Some(Self::new(first, last))
    }

    /// The period right before this one, to compare with: for whole calendar
    /// months, the same number of months before; for a salary month, the
    /// salary month before (D-645); otherwise the same number of days
    /// before.
    pub fn before(self) -> Result<Self, ReportError> {
        let last = self.first.yesterday().map_err(|_| ReportError::BadPeriod)?;
        if let Some(day) = self.salary_day() {
            let month = self
                .first
                .first_of_month()
                .checked_sub(1.month())
                .map_err(|_| ReportError::BadPeriod)?;
            return salary_month(month, day);
        }
        let first = if self.is_whole_months() {
            let months = self.months().len() as i32;
            self.first
                .checked_sub(months.months())
                .map_err(|_| ReportError::BadPeriod)?
        } else {
            let days = self
                .first
                .until(self.last)
                .map_err(|_| ReportError::BadPeriod)?
                .get_days();
            last.checked_sub(days.days())
                .map_err(|_| ReportError::BadPeriod)?
        };
        Self::new(first, last)
    }

    /// The same days one year earlier. For whole calendar months it is the
    /// same months of the year before, with the last day of the last month;
    /// for a salary month, the salary month that starts in the same month a
    /// year before (D-645).
    pub fn year_before(self) -> Result<Self, ReportError> {
        if let Some(day) = self.salary_day() {
            let month = self
                .first
                .first_of_month()
                .checked_sub(1.year())
                .map_err(|_| ReportError::BadPeriod)?;
            return salary_month(month, day);
        }
        let first = self
            .first
            .checked_sub(1.year())
            .map_err(|_| ReportError::BadPeriod)?;
        let mut last = self
            .last
            .checked_sub(1.year())
            .map_err(|_| ReportError::BadPeriod)?;
        if self.is_whole_months() {
            last = last.last_of_month();
        }
        Self::new(first, last)
    }

    /// The salary day, if the period is one salary month that does not start
    /// on the 1st (D-645): it starts on a day of one month and ends the day
    /// before the same day of the next, where a short month's last day
    /// stands in for a day it lacks. Two months in a row are never both
    /// shorter than the day, so the larger of the two days is that day.
    fn salary_day(self) -> Option<i8> {
        let next = self.last.tomorrow().ok()?;
        let day = self.first.day().max(next.day());
        let following = self.first.first_of_month().checked_add(1.month()).ok()?;
        let fits = day > 1
            && salary_day(self.first.first_of_month(), day).ok()? == self.first
            && salary_day(following, day).ok()? == next;
        fits.then_some(day)
    }

    /// Whether it starts on the first of a month and ends on the last of one.
    fn is_whole_months(self) -> bool {
        self.first == self.first.first_of_month() && self.last == self.last.last_of_month()
    }

    /// The first day.
    pub fn first(self) -> Date {
        self.first
    }

    /// The last day.
    pub fn last(self) -> Date {
        self.last
    }

    /// Whether `day` is in the period.
    pub fn contains(self, day: Date) -> bool {
        self.first <= day && day <= self.last
    }

    /// The first day of every calendar month the period touches, oldest
    /// first. A first or last month the period covers only in part is
    /// listed too; datasets count only the days inside the period.
    pub fn months(self) -> Vec<Date> {
        let mut months = Vec::new();
        let mut month = self.first.first_of_month();
        while month <= self.last {
            months.push(month);
            match month.checked_add(1.month()) {
                Ok(next) => month = next,
                Err(_) => break,
            }
        }
        months
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i16, month: i8, day: i8) -> Date {
        Date::new(year, month, day).expect("valid date")
    }

    #[test]
    fn the_named_periods_are_whole_calendar_months_and_years() {
        let today = date(2026, 3, 31);
        let range = |period: Result<Period, ReportError>| {
            let period = period.expect("a period");
            (period.first(), period.last())
        };
        assert_eq!(
            range(Period::this_month(today)),
            (date(2026, 3, 1), date(2026, 3, 31))
        );
        // February is shorter than the day of the month `today` is.
        assert_eq!(
            range(Period::last_month(today)),
            (date(2026, 2, 1), date(2026, 2, 28))
        );
        assert_eq!(
            range(Period::last_month(date(2026, 1, 15))),
            (date(2025, 12, 1), date(2025, 12, 31))
        );
        assert_eq!(
            range(Period::last_12_months(today)),
            (date(2025, 4, 1), date(2026, 3, 31))
        );
        assert_eq!(
            range(Period::this_year(today)),
            (date(2026, 1, 1), date(2026, 12, 31))
        );
        assert_eq!(
            range(Period::last_year(date(2028, 2, 29))),
            (date(2027, 1, 1), date(2027, 12, 31))
        );
        let year = Period::last_12_months(today).expect("a period");
        assert_eq!(year.months().len(), 12);
    }

    #[test]
    fn a_period_must_run_forward_and_stay_short() {
        let first = date(2026, 1, 1);
        assert!(matches!(
            Period::new(date(2026, 1, 2), first),
            Err(ReportError::BadPeriod)
        ));
        assert!(Period::new(first, first).is_ok());
        assert!(Period::new(first, date(2125, 12, 31)).is_ok());
        assert!(matches!(
            Period::new(first, date(2126, 1, 1)),
            Err(ReportError::PeriodTooLong)
        ));
        assert!(matches!(
            Period::new(date(1, 1, 1), date(9999, 12, 31)),
            Err(ReportError::PeriodTooLong)
        ));
    }

    #[test]
    fn the_period_before_has_the_same_length_in_months_or_days() {
        let before = |first, last| {
            let period = Period::new(first, last).expect("a period").before();
            let period = period.expect("a period before");
            (period.first(), period.last())
        };
        // A month is followed by the month before, whatever its length.
        assert_eq!(
            before(date(2026, 3, 1), date(2026, 3, 31)),
            (date(2026, 2, 1), date(2026, 2, 28))
        );
        assert_eq!(
            before(date(2026, 1, 1), date(2026, 3, 31)),
            (date(2025, 10, 1), date(2025, 12, 31))
        );
        // Days that are not whole months: the same number of days.
        assert_eq!(
            before(date(2026, 3, 11), date(2026, 3, 20)),
            (date(2026, 3, 1), date(2026, 3, 10))
        );
        assert_eq!(
            before(date(2026, 3, 5), date(2026, 3, 5)),
            (date(2026, 3, 4), date(2026, 3, 4))
        );
        // The calendar ends somewhere.
        let first_day = Period::new(date(-9999, 1, 1), date(-9999, 1, 31)).expect("a period");
        assert!(matches!(first_day.before(), Err(ReportError::BadPeriod)));
    }

    #[test]
    fn the_same_days_a_year_earlier() {
        let year_before = |first, last| {
            let period = Period::new(first, last).expect("a period").year_before();
            let period = period.expect("a period a year before");
            (period.first(), period.last())
        };
        assert_eq!(
            year_before(date(2028, 2, 1), date(2028, 2, 29)),
            (date(2027, 2, 1), date(2027, 2, 28))
        );
        assert_eq!(
            year_before(date(2028, 2, 10), date(2028, 2, 29)),
            (date(2027, 2, 10), date(2027, 2, 28))
        );
        assert_eq!(
            year_before(date(2026, 1, 1), date(2026, 12, 31)),
            (date(2025, 1, 1), date(2025, 12, 31))
        );
    }

    #[test]
    fn a_typed_range_is_written_read_and_resolved() {
        let (first, last) = (date(2000, 1, 1), date(2020, 1, 1));
        let both = range_word(Some(first), Some(last)).unwrap();
        assert_eq!(both, "2000-01-01..2020-01-01");
        assert_eq!(read_range_word(&both), Some((Some(first), Some(last))));
        let today = date(2026, 10, 30);
        let period = |word: &str, earliest| {
            Period::from_range_word(word, today, earliest)
                .unwrap()
                .map(|p| (p.first(), p.last()))
        };
        assert_eq!(period(&both, None).unwrap(), (first, last));
        // An open start is the book's first day; an open end is today.
        let up_to = range_word(None, Some(last)).unwrap();
        assert_eq!(up_to, "..2020-01-01");
        let book_start = date(2018, 5, 3);
        assert_eq!(
            period(&up_to, Some(book_start)).unwrap(),
            (book_start, last)
        );
        // A book that starts later than the end has just that day.
        assert_eq!(
            period(&up_to, Some(date(2021, 1, 1))).unwrap(),
            (last, last)
        );
        assert_eq!(period(&up_to, None).unwrap(), (last, last));
        let from = range_word(Some(first), None).unwrap();
        assert_eq!(from, "2000-01-01..");
        assert_eq!(period(&from, None).unwrap(), (first, today));
        // A start in the future is that one day, never a backwards period.
        let future = range_word(Some(date(2030, 1, 1)), None).unwrap();
        assert_eq!(
            period(&future, None).unwrap(),
            (date(2030, 1, 1), date(2030, 1, 1))
        );
        // Mistakes are refused, never guessed.
        assert!(matches!(
            range_word(Some(last), Some(first)),
            Err(ReportError::BadPeriod)
        ));
        assert!(matches!(
            range_word(None, None),
            Err(ReportError::BadPeriod)
        ));
        assert!(matches!(
            range_word(Some(date(1800, 1, 1)), Some(last)),
            Err(ReportError::PeriodTooLong)
        ));
        for word in [
            "..",
            "2020-01-01..2000-01-01",
            "2020-13-01..",
            "x..",
            "2020-1-1..",
            "this-year",
            "1.1.2000..",
        ] {
            assert_eq!(read_range_word(word), None, "{word}");
            assert!(
                Period::from_range_word(word, today, None).is_none(),
                "{word}"
            );
        }
        // The names a component can use work for a range too.
        assert!(Period::for_component(&both, Period::this_month(today).unwrap(), today).is_some());
    }

    #[test]
    fn the_last_months_count_back_with_or_without_this_one() {
        let range = |word: &str, today| {
            let period = Period::named(word, today, None).expect("a word");
            let period = period.expect("a period");
            (period.first(), period.last())
        };
        let today = date(2026, 11, 7);
        assert_eq!(
            range("last-3-months", today),
            (date(2026, 9, 1), date(2026, 11, 30))
        );
        assert_eq!(
            range("last-3-complete-months", today),
            (date(2026, 8, 1), date(2026, 10, 31))
        );
        assert_eq!(
            range("last-1-complete-months", date(2026, 1, 31)),
            (date(2025, 12, 1), date(2025, 12, 31))
        );
        assert_eq!(
            range("last-6-complete-months", date(2026, 3, 31)),
            (date(2025, 9, 1), date(2026, 2, 28))
        );
        // The word written by hand before is the same period.
        assert_eq!(
            range("last-12-months", today),
            (
                Period::last_12_months(today).unwrap().first(),
                Period::last_12_months(today).unwrap().last()
            )
        );
        assert_eq!(
            Rolling::read("last-12-months"),
            Some(Rolling::Months {
                count: 12,
                complete: false
            })
        );
        // Complete months compare with the same number of months before.
        let three = Period::named("last-3-complete-months", today, None)
            .unwrap()
            .unwrap();
        let before = three.before().unwrap();
        assert_eq!(
            (before.first(), before.last()),
            (date(2026, 5, 1), date(2026, 7, 31))
        );
    }

    #[test]
    fn a_salary_month_runs_from_its_day_to_the_day_before_the_next() {
        let range = |word: &str, today| {
            let period = Period::named(word, today, None).expect("a word");
            let period = period.expect("a period");
            (period.first(), period.last())
        };
        // Before the salary day, the salary month began in the month before.
        assert_eq!(
            range("this-salary-month-28", date(2026, 11, 7)),
            (date(2026, 10, 28), date(2026, 11, 27))
        );
        assert_eq!(
            range("this-salary-month-28", date(2026, 11, 28)),
            (date(2026, 11, 28), date(2026, 12, 27))
        );
        assert_eq!(
            range("last-salary-month-28", date(2026, 11, 7)),
            (date(2026, 9, 28), date(2026, 10, 27))
        );
        // Across the turn of the year.
        assert_eq!(
            range("this-salary-month-15", date(2026, 1, 3)),
            (date(2025, 12, 15), date(2026, 1, 14))
        );
        // In a short month its last day counts.
        assert_eq!(
            range("this-salary-month-31", date(2026, 2, 28)),
            (date(2026, 2, 28), date(2026, 3, 30))
        );
        assert_eq!(
            range("this-salary-month-31", date(2026, 2, 27)),
            (date(2026, 1, 31), date(2026, 2, 27))
        );
        assert_eq!(
            range("this-salary-month-30", date(2028, 3, 1)),
            (date(2028, 2, 29), date(2028, 3, 29))
        );
        assert_eq!(
            range("last-salary-month-31", date(2026, 5, 31)),
            (date(2026, 4, 30), date(2026, 5, 30))
        );
        // The 1st is the calendar month.
        assert_eq!(
            range("this-salary-month-1", date(2026, 2, 14)),
            (date(2026, 2, 1), date(2026, 2, 28))
        );
    }

    #[test]
    fn a_salary_month_compares_with_the_salary_month_before() {
        let compare = |word: &str, today| {
            let period = Period::named(word, today, None).unwrap().unwrap();
            let (before, year) = (period.before().unwrap(), period.year_before().unwrap());
            ((before.first(), before.last()), (year.first(), year.last()))
        };
        assert_eq!(
            compare("this-salary-month-28", date(2026, 11, 7)),
            (
                (date(2026, 9, 28), date(2026, 10, 27)),
                (date(2025, 10, 28), date(2025, 11, 27))
            )
        );
        // The day before a short month's stand-in is the real day again.
        assert_eq!(
            compare("this-salary-month-31", date(2026, 3, 10)),
            (
                (date(2026, 1, 31), date(2026, 2, 27)),
                (date(2025, 2, 28), date(2025, 3, 30))
            )
        );
        assert_eq!(
            compare("this-salary-month-31", date(2026, 4, 1)),
            (
                (date(2026, 2, 28), date(2026, 3, 30)),
                (date(2025, 3, 31), date(2025, 4, 29))
            )
        );
        // Every salary month the word names compares with the one the
        // "last" word names.
        for day in 1..=31 {
            let mut today = date(2025, 1, 1);
            while today < date(2027, 1, 1) {
                let this = Period::named(&format!("this-salary-month-{day}"), today, None)
                    .unwrap()
                    .unwrap();
                let last = Period::named(&format!("last-salary-month-{day}"), today, None)
                    .unwrap()
                    .unwrap();
                assert!(this.contains(today), "{day} {today}");
                assert_eq!(this.before().unwrap(), last, "{day} {today}");
                assert_eq!(last.last().tomorrow().unwrap(), this.first());
                today = today.tomorrow().unwrap();
            }
        }
    }

    #[test]
    fn rolling_words_are_written_and_read_exactly() {
        for rolling in [
            Rolling::Months {
                count: 1,
                complete: false,
            },
            Rolling::Months {
                count: MAX_ROLLING_MONTHS,
                complete: true,
            },
            Rolling::SalaryMonth {
                day: 31,
                before: false,
            },
            Rolling::SalaryMonth {
                day: 1,
                before: true,
            },
        ] {
            let word = rolling.word();
            assert_eq!(Rolling::read(&word), Some(rolling), "{word}");
            assert!(is_period_word(&word), "{word}");
        }
        assert_eq!(
            Rolling::SalaryMonth {
                day: 28,
                before: false
            }
            .word(),
            "this-salary-month-28"
        );
        assert_eq!(
            Rolling::Months {
                count: 3,
                complete: true
            }
            .word(),
            "last-3-complete-months"
        );
        for word in [
            "last-0-months",
            "last-03-months",
            "last-121-months",
            "last--months",
            "last-3-month",
            "last-x-complete-months",
            "this-salary-month-0",
            "this-salary-month-32",
            "this-salary-month-07",
            "next-salary-month-1",
            "this-salary-month-",
            "last-salary-month-+1",
        ] {
            assert_eq!(Rolling::read(word), None, "{word}");
            assert!(!is_period_word(word), "{word}");
        }
        for word in crate::PERIODS {
            assert!(is_period_word(word), "{word}");
        }
        assert!(is_period_word("2025-01-01.."));
        assert!(!is_period_word("page"));
    }

    #[test]
    fn months_lists_every_month_touched() {
        let period = Period::new(date(2025, 11, 20), date(2026, 2, 3)).expect("a period");
        assert_eq!(
            period.months(),
            [
                date(2025, 11, 1),
                date(2025, 12, 1),
                date(2026, 1, 1),
                date(2026, 2, 1)
            ]
        );
        assert!(period.contains(date(2025, 11, 20)));
        assert!(period.contains(date(2026, 2, 3)));
        assert!(!period.contains(date(2025, 11, 19)));
        assert!(!period.contains(date(2026, 2, 4)));
    }
}
