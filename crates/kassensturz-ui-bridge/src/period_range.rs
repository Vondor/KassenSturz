// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A date range typed by the user (M6b.R6, D-332): read in the regional date
//! format into the word a period is stored as, and written back as text to
//! edit. No Qt in this file: `period_range_model.rs` hands the results to
//! QML, which only ever sees text (D-065).

use kassensturz_core::Date;
use kassensturz_report::{ReportError, range_word, read_range_word};

use crate::format::Locale;

/// Why typed text isn't a period.
#[derive(Debug)]
pub enum RangeError {
    /// The text isn't a range of dates in a form KassenSturz reads.
    NotUnderstood,
    /// The dates are fine but don't make a period: the end is before the
    /// start, or the range is longer than 100 years.
    Report(ReportError),
}

/// The word that names the range `text` says, for example
/// `01.01.2000-01.01.2020`, `<01.01.2025` (the days before one),
/// `<=01.01.2025` (up to and including it), `>01.01.2020` or `>=01.01.2020`.
/// Dates are read in the user's regional format or as ISO 8601; between two
/// dates a hyphen, a dash, `..` or `to` may stand.
pub fn parse(text: &str, locale: &impl Locale) -> Result<String, RangeError> {
    let text = text.trim();
    let day = |text: &str| {
        locale
            .read_date(text.trim())
            .ok_or(RangeError::NotUnderstood)
    };
    let (first, last) = if let Some(rest) = text.strip_prefix("<=") {
        (None, Some(day(rest)?))
    } else if let Some(rest) = text.strip_prefix('<') {
        let before = day(rest)?
            .yesterday()
            .map_err(|_| RangeError::Report(ReportError::BadPeriod))?;
        (None, Some(before))
    } else if let Some(rest) = text.strip_prefix(">=") {
        (Some(day(rest)?), None)
    } else if let Some(rest) = text.strip_prefix('>') {
        let after = day(rest)?
            .tomorrow()
            .map_err(|_| RangeError::Report(ReportError::BadPeriod))?;
        (Some(after), None)
    } else {
        both_days(text, locale)?
    };
    range_word(first, last).map_err(RangeError::Report)
}

/// Splits `text` at the separator that leaves a date on each side. A hyphen
/// is also part of ISO dates, so every place is tried.
fn both_days(text: &str, locale: &impl Locale) -> Result<(Option<Date>, Option<Date>), RangeError> {
    let mut places: Vec<(usize, usize)> = text
        .char_indices()
        .filter(|(_, c)| matches!(c, '-' | '–' | '—'))
        .map(|(at, c)| (at, c.len_utf8()))
        .collect();
    for separator in ["..", " to "] {
        places.extend(text.match_indices(separator).map(|(at, s)| (at, s.len())));
    }
    places.sort_unstable();
    for (at, len) in places {
        let (left, right) = (&text[..at], &text[at + len..]);
        if let (Some(first), Some(last)) = (
            locale.read_date(left.trim()),
            locale.read_date(right.trim()),
        ) {
            return Ok((Some(first), Some(last)));
        }
    }
    Err(RangeError::NotUnderstood)
}

/// A range word as the text the user edits, in the regional date format;
/// `None` for a word that is no range (a fixed name or a month).
pub fn describe(word: &str, locale: &impl Locale) -> Option<String> {
    let (first, last) = read_range_word(word)?;
    Some(match (first, last) {
        (Some(first), Some(last)) => {
            format!("{} – {}", locale.short_date(first), locale.short_date(last))
        }
        (Some(first), None) => format!(">={}", locale.short_date(first)),
        (None, Some(last)) => format!("<={}", locale.short_date(last)),
        (None, None) => return None,
    })
}

/// An example of what to type, in the regional format.
pub fn example(locale: &impl Locale) -> String {
    format!(
        "{} – {}",
        locale.short_date(Date::constant(2000, 1, 1)),
        locale.short_date(Date::constant(2020, 1, 1))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::register::tests::TestLocale;

    /// Day.month.year, as in German; reads ISO too, like the app.
    struct Dotted;

    impl Locale for Dotted {
        fn amount_format(
            &self,
            currency: kassensturz_core::CurrencyCode,
        ) -> crate::format::AmountFormat {
            TestLocale.amount_format(currency)
        }
        fn decimal_point(&self) -> String {
            TestLocale.decimal_point()
        }
        fn group_digits(&self, value: u64) -> String {
            TestLocale.group_digits(value)
        }
        fn group_separator(&self) -> String {
            TestLocale.group_separator()
        }
        fn short_date(&self, date: Date) -> String {
            format!("{:02}.{:02}.{}", date.day(), date.month(), date.year())
        }
        fn month_and_year(&self, date: Date) -> String {
            TestLocale.month_and_year(date)
        }
        fn read_date(&self, text: &str) -> Option<Date> {
            if let Ok(date) = text.parse() {
                return Some(date);
            }
            let mut parts = text.split('.');
            let (day, month, year) = (parts.next()?, parts.next()?, parts.next()?);
            if parts.next().is_some() {
                return None;
            }
            Date::new(year.parse().ok()?, month.parse().ok()?, day.parse().ok()?).ok()
        }
        fn today(&self) -> Date {
            TestLocale.today()
        }
    }

    fn word(text: &str) -> Result<String, RangeError> {
        parse(text, &Dotted)
    }

    #[test]
    fn the_ranges_the_maintainer_typed_are_read() {
        assert_eq!(
            word("01.01.2000-01.01.2020").unwrap(),
            "2000-01-01..2020-01-01"
        );
        assert_eq!(
            word(" 01.01.2000 – 01.01.2020 ").unwrap(),
            "2000-01-01..2020-01-01"
        );
        assert_eq!(
            word("01.01.2000 to 01.01.2020").unwrap(),
            "2000-01-01..2020-01-01"
        );
        assert_eq!(
            word("01.01.2000..01.01.2020").unwrap(),
            "2000-01-01..2020-01-01"
        );
        // Before and after leave the day itself out; with = it is in.
        assert_eq!(word("<01.01.2025").unwrap(), "..2024-12-31");
        assert_eq!(word("<=01.01.2025").unwrap(), "..2025-01-01");
        assert_eq!(word(">01.01.2020").unwrap(), "2020-01-02..");
        assert_eq!(word(">=01.01.2020").unwrap(), "2020-01-01..");
    }

    #[test]
    fn iso_dates_with_their_own_hyphens_are_split_where_both_sides_are_dates() {
        assert_eq!(
            word("2000-01-01-2020-01-01").unwrap(),
            "2000-01-01..2020-01-01"
        );
        assert_eq!(
            word("2000-01-01 – 2020-01-01").unwrap(),
            "2000-01-01..2020-01-01"
        );
        assert_eq!(
            parse("2000-01-01-2020-01-01", &TestLocale).unwrap(),
            "2000-01-01..2020-01-01"
        );
    }

    #[test]
    fn mistakes_are_said_not_guessed() {
        for text in [
            "",
            "last year",
            "01.01.2000",
            "01.01.2000-",
            "<",
            "32.01.2000-01.01.2001",
            ">>01.01.2000",
        ] {
            assert!(
                matches!(word(text), Err(RangeError::NotUnderstood)),
                "{text:?}"
            );
        }
        assert!(matches!(
            word("01.01.2020-01.01.2000"),
            Err(RangeError::Report(ReportError::BadPeriod))
        ));
        assert!(matches!(
            word("01.01.1800-01.01.2000"),
            Err(RangeError::Report(ReportError::PeriodTooLong))
        ));
    }

    #[test]
    fn a_word_is_written_back_in_a_form_that_reads_the_same() {
        for word_in in ["2000-01-01..2020-01-01", "..2025-01-01", "2020-01-01.."] {
            let text = describe(word_in, &Dotted).unwrap();
            assert_eq!(word(&text).unwrap(), word_in, "{text}");
        }
        assert_eq!(
            describe("2000-01-01..2020-01-01", &Dotted).unwrap(),
            "01.01.2000 – 01.01.2020"
        );
        assert_eq!(describe("this-year", &Dotted), None);
        assert_eq!(describe("2026-10", &Dotted), None);
        assert_eq!(example(&Dotted), "01.01.2000 – 01.01.2020");
        assert_eq!(word(&example(&Dotted)).unwrap(), "2000-01-01..2020-01-01");
    }
}
