// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The user's regional format, read from Qt's locale data (D-065): the
//! system's regional settings, or the user's choice (D-002).

use kassensturz_core::{CurrencyCode, Date};

use crate::format::{Affixes, AmountFormat, Locale};

#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt's string type.
        type QString = cxx_qt_lib::QString;
    }

    #[namespace = "kassensturz::locale"]
    unsafe extern "C++" {
        include!("kassensturz-ui-bridge/cpp/locale.h");

        #[cxx_name = "decimalPoint"]
        fn decimal_point() -> QString;

        #[cxx_name = "zeroDigit"]
        fn zero_digit() -> QString;

        #[cxx_name = "currencyIsoCode"]
        fn currency_iso_code() -> QString;

        #[cxx_name = "currencySymbol"]
        fn currency_symbol() -> QString;

        #[cxx_name = "groupDigits"]
        fn group_digits(value: u64) -> QString;

        #[cxx_name = "currencyPrefix"]
        fn currency_prefix(negative: bool, symbol: &QString) -> QString;

        #[cxx_name = "currencySuffix"]
        fn currency_suffix(negative: bool, symbol: &QString) -> QString;

        #[cxx_name = "shortDate"]
        fn short_date(year: i32, month: i32, day: i32) -> QString;

        #[cxx_name = "setDefaultLocale"]
        fn set_default_locale(name: &QString);
    }
}

/// The regional format Qt uses for this app.
pub struct QtLocale;

impl QtLocale {
    /// Makes `name` (e.g. "de_DE") the locale used from now on.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by tests until Settings exist (D-002)")
    )]
    pub fn set_default(name: &str) {
        ffi::set_default_locale(&ffi::QString::from(name));
    }
}

impl Locale for QtLocale {
    fn amount_format(&self, currency: CurrencyCode) -> AmountFormat {
        // Qt knows the symbol of the locale's own currency only, so other
        // currencies are written with their code, e.g. "12,00 USD".
        let symbol = if ffi::currency_iso_code().to_string() == currency.as_str() {
            ffi::currency_symbol()
        } else {
            ffi::QString::from(currency.as_str())
        };
        let affixes = |negative: bool| {
            Affixes::spaced(
                ffi::currency_prefix(negative, &symbol).to_string(),
                ffi::currency_suffix(negative, &symbol).to_string(),
            )
        };
        AmountFormat {
            decimal_point: ffi::decimal_point().to_string(),
            zero_digit: ffi::zero_digit().to_string().chars().next().unwrap_or('0'),
            positive: affixes(false),
            negative: affixes(true),
        }
    }

    fn group_digits(&self, value: u64) -> String {
        ffi::group_digits(value).to_string()
    }

    fn short_date(&self, date: Date) -> String {
        let (year, month, day) = (date.year(), date.month(), date.day());
        ffi::short_date(year.into(), month.into(), day.into()).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kassensturz_core::Money;

    fn format(locale: &QtLocale, minor_units: i64, currency: &str) -> String {
        let currency = CurrencyCode::new(currency).expect("valid code");
        locale.amount_format(currency).format(
            Money::from_minor_units(minor_units, currency),
            2,
            |value| locale.group_digits(value),
        )
    }

    // One test only: the default locale is global, so tests that change it
    // must not run in parallel.
    #[test]
    fn formats_with_qts_locale_data() {
        let locale = QtLocale;

        QtLocale::set_default("de_DE");
        assert_eq!(format(&locale, 123_456, "EUR"), "1.234,56\u{a0}€");
        assert_eq!(format(&locale, -5320, "EUR"), "-53,20\u{a0}€");
        assert_eq!(format(&locale, 1200, "USD"), "12,00\u{a0}USD");
        let date = Date::new(2026, 10, 5).expect("valid date");
        assert_eq!(locale.short_date(date), "05.10.2026");

        QtLocale::set_default("en_US");
        assert_eq!(format(&locale, 123_456, "USD"), "$1,234.56");
        // Qt's American English writes negative amounts in parentheses.
        assert_eq!(format(&locale, -5320, "USD"), "($53.20)");
        assert_eq!(format(&locale, 123_456, "EUR"), "EUR\u{a0}1,234.56");
        assert_eq!(locale.short_date(date), "10/5/2026");
    }
}
