// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A date range typed in the Statistic Builder or in a widget's properties
//! (M6b.R6, D-332), as a QML object. The work is in `period_range.rs`; QML
//! only gets text (D-065).

use cxx_qt_lib::{QString, QVariant};
use kassensturz_report::ReportError;

use crate::errors::ErrorText;
use crate::format::Locale;
use crate::number_format::app_locale;
use crate::period_range::{self, RangeError};
use crate::variant::{error_object, map, text};

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
        /// regional date format (empty for the fixed names).
        #[qinvokable]
        fn describe(self: &PeriodRange, word: &QString) -> QVariant;

        /// An example of what to type, in the regional date format.
        #[qinvokable]
        fn example(self: &PeriodRange) -> QString;
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

/// What `describe` gives QML.
fn describe_object(word: &str, locale: &impl Locale) -> QVariant {
    let described = period_range::describe(word, locale);
    map([
        ("custom", QVariant::from(&described.is_some())),
        ("text", text(&described.unwrap_or_default())),
    ])
}

impl qobject::PeriodRange {
    fn parse(&self, input: &QString) -> QVariant {
        parse_object(&input.to_string(), &app_locale())
    }

    fn describe(&self, word: &QString) -> QVariant {
        describe_object(&word.to_string(), &app_locale())
    }

    fn example(&self) -> QString {
        QString::from(period_range::example(&app_locale()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::register::tests::TestLocale;
    use crate::variant::read_back::{flag_of, keys, text_of};

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
        assert_eq!(keys(&range), ["custom", "text"]);
        assert!(flag_of(&range, "custom"));
        assert_eq!(text_of(&range, "text"), "<=2025-01-01");
        let fixed = describe_object("this-year", &TestLocale);
        assert!(!flag_of(&fixed, "custom"));
        assert_eq!(text_of(&fixed, "text"), "");
    }
}
