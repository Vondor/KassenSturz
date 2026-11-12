// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A date range typed in the Statistic Builder or in a widget's properties
//! (M6b.R6, D-332), as a QML object. The work is in `period_range.rs`; QML
//! only gets text (D-065).

use cxx_qt_lib::{QString, QVariant};
use kassensturz_core::Date;
use kassensturz_report::{CANDLE_WORDS, CandleLength, Period, ReportError, Rolling};

use crate::errors::ErrorText;
use crate::format::Locale;
use crate::number_format::app_locale;
use crate::period_range::{self, RangeError};
use crate::variant::{error_object, list, map, text};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt's string type.
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        /// A value of any Qt type.
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// Reads a typed range of dates into the word a period is stored
        /// as.
        #[qobject]
        #[qml_element]
        type PeriodRange = super::PeriodRangeRust;
    }

    extern "RustQt" {
        /// The period the text names. Gives `word`, or the error: the text is
        /// no range of dates, the end is before the start, or the range is
        /// longer than 100 years.
        #[qinvokable]
        fn parse(self: &PeriodRange, input: &QString) -> QVariant;

        /// What to show for a period word: `custom` says whether it is a
        /// typed range, `text` is the range as the user types it, in their
        /// regional date format (empty for the fixed names). `rolling` is
        /// `months`, `salary` or `days` for a rolling period (D-644,
        /// D-717), else empty; then `count` and `complete` (months), `day`
        /// and `before` (salary month) or `count` (days) say which one.
        #[qinvokable]
        fn describe(self: &PeriodRange, word: &QString) -> QVariant;

        /// The word for the last `count` months, with or without the month
        /// today is in (D-644); empty for a count that can't be kept.
        #[qinvokable]
        fn months(self: &PeriodRange, count: i32, complete: bool) -> QString;

        /// The word for the salary month from `day`, the one today is in or
        /// the one before (D-645); empty for a day that is no day of a
        /// month.
        #[qinvokable]
        #[cxx_name = "salaryMonth"]
        fn salary_month(self: &PeriodRange, day: i32, before: bool) -> QString;

        /// The word for the last `count` days, today included (D-717);
        /// empty for a count that can't be kept.
        #[qinvokable]
        fn days(self: &PeriodRange, count: i32) -> QString;

        /// An example of what to type, in the regional date format.
        #[qinvokable]
        fn example(self: &PeriodRange) -> QString;

        /// The words of a candle chart's *One candle per* that fit the
        /// period `word` (D-747): `auto`, then `day`, `week` and `month`
        /// where they give at least two candles. Every word for a period
        /// whose days aren't known here (an open start, an unknown word).
        #[qinvokable]
        #[cxx_name = "candleWords"]
        fn candle_words(self: &PeriodRange, word: &QString) -> QVariant;
    }
}

/// The Rust side of [`qobject::PeriodRange`]; it holds nothing.
#[derive(Default)]
pub struct PeriodRangeRust;

/// What `parse` gives QML: the `word`, or the error.
fn parse_object(input: &str, locale: &impl Locale) -> QVariant {
    match period_range::parse(input, locale) {
        Ok(word) => map([("word", text(&word))]),
        Err(error) => error_object(&ErrorText::plain(match error {
            RangeError::NotUnderstood => "input.bad-range-text",
            RangeError::Report(ReportError::PeriodTooLong) => "report.period-too-long",
            RangeError::Report(_) => "report.bad-period",
        })),
    }
}

/// The candle lengths that fit the period `word`, seen from `today`, as
/// the chart reads it (`charts::period`): `today`, a month as `YYYY-MM` or a
/// stored period word. A typed range with an open start begins at the
/// book's first day, which isn't known here, so it offers every length, and
/// so does a word that is no period; the chart still draws only a length
/// that fits (`CandleLength::fits`).
fn candle_words_of(word: &str, today: Date) -> Vec<&'static str> {
    let open_start =
        kassensturz_report::read_range_word(word).is_some_and(|(first, _)| first.is_none());
    let period = if word == "today" {
        Period::new(today, today).ok()
    } else if let Some(month) = crate::charts::month_period(word) {
        Some(month)
    } else if open_start {
        None
    } else {
        Period::named(word, today, None).and_then(Result::ok)
    };
    period.map_or_else(|| CANDLE_WORDS.to_vec(), CandleLength::offered)
}

/// What `describe` gives QML.
fn describe_object(word: &str, locale: &impl Locale) -> QVariant {
    let described = period_range::describe(word, locale);
    let (rolling, count, complete, day, before) = match Rolling::read(word) {
        Some(Rolling::Months { count, complete }) => {
            ("months", i32::from(count), complete, 0, false)
        }
        Some(Rolling::SalaryMonth { day, before }) => ("salary", 0, false, i32::from(day), before),
        Some(Rolling::Days { count }) => ("days", i32::from(count), false, 0, false),
        None => ("", 0, false, 0, false),
    };
    map([
        ("custom", QVariant::from(&described.is_some())),
        ("text", text(&described.unwrap_or_default())),
        ("rolling", text(rolling)),
        ("count", QVariant::from(&count)),
        ("complete", QVariant::from(&complete)),
        ("day", QVariant::from(&day)),
        ("before", QVariant::from(&before)),
    ])
}

impl qobject::PeriodRange {
    fn parse(&self, input: &QString) -> QVariant {
        parse_object(&input.to_string(), &app_locale())
    }

    fn describe(&self, word: &QString) -> QVariant {
        describe_object(&word.to_string(), &app_locale())
    }

    fn months(&self, count: i32, complete: bool) -> QString {
        QString::from(period_range::months_word(count, complete).unwrap_or_default())
    }

    fn salary_month(&self, day: i32, before: bool) -> QString {
        QString::from(period_range::salary_month_word(day, before).unwrap_or_default())
    }

    fn days(&self, count: i32) -> QString {
        QString::from(period_range::days_word(count).unwrap_or_default())
    }

    fn example(&self) -> QString {
        QString::from(period_range::example(&app_locale()))
    }

    fn candle_words(&self, word: &QString) -> QVariant {
        list(
            candle_words_of(&word.to_string(), app_locale().today())
                .into_iter()
                .map(text),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::register::tests::TestLocale;
    use crate::variant::read_back::{flag_of, keys, number_of, text_of};

    #[test]
    fn the_candle_lengths_fit_the_period_and_read_back_the_way_qml_reads_them() {
        // D-747: seven days offer no week and no month, three whole months
        // all three lengths, one day only the automatic one.
        let today = TestLocale.today();
        assert_eq!(candle_words_of("last-7-days", today), ["auto", "day"]);
        assert_eq!(
            candle_words_of("last-3-complete-months", today),
            ["auto", "day", "week", "month"]
        );
        assert_eq!(candle_words_of("today", today), ["auto"]);
        assert_eq!(candle_words_of("2026-02", today), ["auto", "day", "week"]);
        assert_eq!(
            candle_words_of("2026-10-01..2026-10-05", today),
            ["auto", "day"]
        );
        // Days not known here: every word, as before.
        assert_eq!(candle_words_of("..2026-10-05", today), CANDLE_WORDS);
        assert_eq!(candle_words_of("page", today), CANDLE_WORDS);
        // QML gets a list of texts.
        let words = list(candle_words_of("last-7-days", today).into_iter().map(text));
        let read: Vec<String> = crate::variant::read_back::elements(&words)
            .expect("a list")
            .iter()
            .map(|word| word.value::<QString>().expect("text").to_string())
            .collect();
        assert_eq!(read, ["auto", "day"]);
    }

    #[test]
    fn a_typed_range_reads_back_as_a_word_or_an_error_code() {
        let ok = parse_object("2000-01-01 – 2020-01-01", &TestLocale);
        assert_eq!(keys(&ok), ["word"]);
        assert_eq!(text_of(&ok, "word"), "2000-01-01..2020-01-01");
        for (input, code) in [
            ("sometime", "input.bad-range-text"),
            ("2020-01-01 – 2000-01-01", "report.bad-period"),
            ("1800-01-01 – 2000-01-01", "report.period-too-long"),
        ] {
            let error = parse_object(input, &TestLocale);
            assert_eq!(keys(&error), ["code"], "{input}");
            assert_eq!(text_of(&error, "code"), code, "{input}");
        }
    }

    #[test]
    fn a_word_is_described_for_the_field_the_way_qml_reads_it() {
        let range = describe_object("..2025-01-01", &TestLocale);
        assert_eq!(
            keys(&range),
            [
                "before", "complete", "count", "custom", "day", "rolling", "text"
            ]
        );
        assert!(flag_of(&range, "custom"));
        assert_eq!(text_of(&range, "text"), "<=2025-01-01");
        assert_eq!(text_of(&range, "rolling"), "");
        let fixed = describe_object("this-year", &TestLocale);
        assert!(!flag_of(&fixed, "custom"));
        assert_eq!(text_of(&fixed, "text"), "");
        assert_eq!(text_of(&fixed, "rolling"), "");
    }

    #[test]
    fn a_rolling_word_is_described_the_way_qml_reads_it() {
        let months = describe_object("last-6-complete-months", &TestLocale);
        assert!(!flag_of(&months, "custom"));
        assert_eq!(text_of(&months, "rolling"), "months");
        assert_eq!(number_of(&months, "count"), 6);
        assert!(flag_of(&months, "complete"));
        let open = describe_object("last-3-months", &TestLocale);
        assert_eq!(number_of(&open, "count"), 3);
        assert!(!flag_of(&open, "complete"));
        let salary = describe_object("last-salary-month-28", &TestLocale);
        assert_eq!(text_of(&salary, "rolling"), "salary");
        assert_eq!(number_of(&salary, "day"), 28);
        assert!(flag_of(&salary, "before"));
        let this = describe_object("this-salary-month-1", &TestLocale);
        assert_eq!(number_of(&this, "day"), 1);
        assert!(!flag_of(&this, "before"));
        assert!(!flag_of(&this, "custom"));
        let days = describe_object("last-7-days", &TestLocale);
        assert!(!flag_of(&days, "custom"));
        assert_eq!(text_of(&days, "rolling"), "days");
        assert_eq!(number_of(&days, "count"), 7);
    }

    #[test]
    fn the_last_days_word_keeps_one_to_366_days() {
        assert_eq!(period_range::days_word(7).as_deref(), Some("last-7-days"));
        assert_eq!(
            period_range::days_word(366).as_deref(),
            Some("last-366-days")
        );
        assert_eq!(period_range::days_word(0), None);
        assert_eq!(period_range::days_word(367), None);
        assert_eq!(period_range::days_word(-1), None);
    }
}
