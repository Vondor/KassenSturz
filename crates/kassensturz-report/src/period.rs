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
            "this-month" => Some(Self::this_month(today)),
            "last-month" => Some(Self::last_month(today)),
            "last-12-months" => Some(Self::last_12_months(today)),
            "this-year" => Some(Self::this_year(today)),
            "last-year" => Some(Self::last_year(today)),
            _ => Self::from_range_word(name, today, None),
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
    /// months, the same number of months before; otherwise the same number
    /// of days before.
    pub fn before(self) -> Result<Self, ReportError> {
        let last = self.first.yesterday().map_err(|_| ReportError::BadPeriod)?;
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
    /// same months of the year before, with the last day of the last month.
    pub fn year_before(self) -> Result<Self, ReportError> {
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
