// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Writes exact amounts and dates as text in the user's regional format
//! (D-002, D-065).
//!
//! QML only ever gets the finished text. The locale supplies separators,
//! grouping and the currency's position; the digits come straight from the
//! integer amount, so nothing is rounded and no floating point is involved.

use kassensturz_core::{CurrencyCode, Date, Money};

/// Where the regional format comes from: Qt's locale data in the app, a fixed
/// stand-in in tests.
pub trait Locale {
    /// How amounts in `currency` are written.
    fn amount_format(&self, currency: CurrencyCode) -> AmountFormat;
    /// `value` with the locale's digit grouping, e.g. "1.234.567" in German.
    fn group_digits(&self, value: u64) -> String;
    /// `date` in the locale's short format.
    fn short_date(&self, date: Date) -> String;
}

/// The text before and after an amount's digits, e.g. "-" and " €".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Affixes {
    /// Before the digits.
    pub prefix: String,
    /// After the digits.
    pub suffix: String,
}

impl Affixes {
    /// Puts a no-break space between letters and digits, so "EUR1.00"
    /// becomes "EUR 1.00", as CLDR's currency spacing rule does. Symbols such
    /// as € and $ stay attached.
    pub fn spaced(prefix: String, suffix: String) -> Self {
        let mut affixes = Self { prefix, suffix };
        if affixes
            .prefix
            .chars()
            .next_back()
            .is_some_and(char::is_alphabetic)
        {
            affixes.prefix.push(NO_BREAK_SPACE);
        }
        if affixes
            .suffix
            .chars()
            .next()
            .is_some_and(char::is_alphabetic)
        {
            affixes.suffix.insert(0, NO_BREAK_SPACE);
        }
        affixes
    }
}

const NO_BREAK_SPACE: char = '\u{a0}';

/// How amounts of one currency are written in one locale.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AmountFormat {
    /// Between the whole and the fractional part, e.g. "," in German.
    pub decimal_point: String,
    /// The locale's digit zero. In Unicode the digits one to nine follow it.
    pub zero_digit: char,
    /// Around positive amounts and zero.
    pub positive: Affixes,
    /// Around negative amounts.
    pub negative: Affixes,
}

impl AmountFormat {
    /// `amount` with `minor_digits` decimal places (2 for the euro), e.g.
    /// "-1.234,56 €". `group_digits` writes the whole part.
    pub fn format(
        &self,
        amount: Money,
        minor_digits: u8,
        group_digits: impl Fn(u64) -> String,
    ) -> String {
        // Books allow at most 18 decimal places. The cap only guards against
        // overflow: 10^19 is the largest power of ten that fits into u64.
        let minor_digits = minor_digits.min(19);
        let scale = 10_u64.pow(u32::from(minor_digits));
        // unsigned_abs: -i64::MIN doesn't fit into i64, but into u64.
        let magnitude = amount.minor_units().unsigned_abs();
        let affixes = if amount.is_negative() {
            &self.negative
        } else {
            &self.positive
        };

        let mut text = affixes.prefix.clone();
        text.push_str(&group_digits(magnitude / scale));
        if minor_digits > 0 {
            text.push_str(&self.decimal_point);
            let fraction = format!(
                "{:0width$}",
                magnitude % scale,
                width = usize::from(minor_digits)
            );
            text.extend(fraction.chars().map(|digit| self.localized(digit)));
        }
        text.push_str(&affixes.suffix);
        text
    }

    /// The locale's version of an ASCII digit.
    fn localized(&self, digit: char) -> char {
        digit
            .to_digit(10)
            .and_then(|value| char::from_u32(u32::from(self.zero_digit) + value))
            .unwrap_or(digit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };

    /// Groups of three, separated by `separator`, like most locales.
    fn group_by_three(separator: &str) -> impl Fn(u64) -> String {
        move |value| {
            let digits = value.to_string();
            let mut grouped = String::new();
            for (index, digit) in digits.chars().enumerate() {
                if index > 0 && (digits.len() - index) % 3 == 0 {
                    grouped.push_str(separator);
                }
                grouped.push(digit);
            }
            grouped
        }
    }

    fn german() -> AmountFormat {
        AmountFormat {
            decimal_point: ",".into(),
            zero_digit: '0',
            positive: Affixes::spaced(String::new(), "\u{a0}€".into()),
            negative: Affixes::spaced("-".into(), "\u{a0}€".into()),
        }
    }

    fn american() -> AmountFormat {
        AmountFormat {
            decimal_point: ".".into(),
            zero_digit: '0',
            positive: Affixes::spaced("€".into(), String::new()),
            negative: Affixes::spaced("-€".into(), String::new()),
        }
    }

    fn eur(minor_units: i64) -> Money {
        Money::from_minor_units(minor_units, EUR)
    }

    #[test]
    fn writes_amounts_the_german_way() {
        let format = german();
        let group = group_by_three(".");
        assert_eq!(format.format(eur(123_456), 2, &group), "1.234,56\u{a0}€");
        assert_eq!(format.format(eur(-5320), 2, &group), "-53,20\u{a0}€");
        assert_eq!(format.format(eur(5), 2, &group), "0,05\u{a0}€");
        assert_eq!(format.format(eur(0), 2, &group), "0,00\u{a0}€");
    }

    #[test]
    fn writes_amounts_the_american_way() {
        let format = american();
        let group = group_by_three(",");
        assert_eq!(format.format(eur(123_456), 2, &group), "€1,234.56");
        assert_eq!(format.format(eur(-5320), 2, &group), "-€53.20");
    }

    #[test]
    fn follows_the_currencys_decimal_places() {
        let format = german();
        let group = group_by_three(".");
        // Yen have none, Bahraini dinar three (ISO 4217).
        assert_eq!(format.format(eur(1234), 0, &group), "1.234\u{a0}€");
        assert_eq!(format.format(eur(1234), 3, &group), "1,234\u{a0}€");
    }

    #[test]
    fn handles_the_largest_amounts_exactly() {
        let format = german();
        let group = group_by_three(".");
        assert_eq!(
            format.format(eur(i64::MIN), 2, &group),
            "-92.233.720.368.547.758,08\u{a0}€"
        );
        assert_eq!(
            format.format(eur(i64::MAX), 2, &group),
            "92.233.720.368.547.758,07\u{a0}€"
        );
    }

    #[test]
    fn uses_the_locales_digits() {
        let format = AmountFormat {
            zero_digit: '\u{660}',
            ..german()
        };
        // The whole part comes from the locale's grouping, here a stand-in.
        assert_eq!(
            format.format(eur(105), 2, |_| "١".into()),
            "١,\u{660}\u{665}\u{a0}€"
        );
    }

    #[test]
    fn spaces_currency_codes_but_not_symbols() {
        assert_eq!(
            Affixes::spaced("EUR".into(), String::new()),
            Affixes {
                prefix: "EUR\u{a0}".into(),
                suffix: String::new()
            }
        );
        assert_eq!(
            Affixes::spaced(String::new(), "USD".into()),
            Affixes {
                prefix: String::new(),
                suffix: "\u{a0}USD".into()
            }
        );
        assert_eq!(
            Affixes::spaced("-€".into(), String::new()),
            Affixes {
                prefix: "-€".into(),
                suffix: String::new()
            }
        );
        assert_eq!(
            Affixes::spaced(String::new(), "\u{a0}€".into()),
            Affixes {
                prefix: String::new(),
                suffix: "\u{a0}€".into()
            }
        );
    }
}
