// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Writes exact amounts and dates as text in the user's regional format
//! (D-002, D-065).
//!
//! QML only ever gets the finished text. The locale supplies separators,
//! grouping and the currency's position; the digits come straight from the
//! integer amount, so no floating point is involved. Amounts are rounded only
//! when the user chose to see fewer decimal places than the currency has,
//! and only for display: totals are added up from the exact amounts first.

use kassensturz_core::{CurrencyCode, Date, Money};

/// Where the regional format comes from: Qt's locale data in the app, a fixed
/// stand-in in tests.
pub trait Locale {
    /// How amounts in `currency` are written.
    fn amount_format(&self, currency: CurrencyCode) -> AmountFormat;
    /// What separates the whole part from the decimal places, e.g. "," in
    /// German.
    fn decimal_point(&self) -> String;
    /// `value` with the locale's digit grouping, e.g. "1.234.567" in German.
    fn group_digits(&self, value: u64) -> String;
    /// What separates groups of digits, e.g. "." in German.
    fn group_separator(&self) -> String;
    /// `date` in the locale's short format.
    fn short_date(&self, date: Date) -> String;
    /// The month `date` is in, with its year, e.g. "October 2026".
    fn month_and_year(&self, date: Date) -> String;
    /// A date the user typed in the locale's short format, or as ISO 8601
    /// (2026-10-05); `None` if it isn't a date.
    fn read_date(&self, text: &str) -> Option<Date>;
    /// Today's date on this computer.
    fn today(&self) -> Date;
    /// How many of a currency's `minor_digits` decimal places amounts show
    /// (D-002). All of them unless the user chose fewer.
    fn shown_decimal_places(&self, minor_digits: u8) -> u8 {
        minor_digits
    }
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
        self.format_rounded(amount, minor_digits, minor_digits, group_digits)
    }

    /// `amount`, which has `minor_digits` decimal places, rounded to
    /// `shown_digits` of them for display, e.g. "-1.235 €" for -1.234,56 €
    /// with none shown (D-002). Halfway cases round away from zero, the
    /// books' default (D-027), so an amount and its negative look alike.
    /// An amount that rounds to zero is shown without a sign.
    pub fn format_rounded(
        &self,
        amount: Money,
        minor_digits: u8,
        shown_digits: u8,
        group_digits: impl Fn(u64) -> String,
    ) -> String {
        // Books allow at most 18 decimal places. The cap only guards against
        // overflow: 10^19 is the largest power of ten that fits into u64.
        let minor_digits = minor_digits.min(19);
        let shown_digits = shown_digits.min(minor_digits);
        // unsigned_abs: -i64::MIN doesn't fit into i64, but into u64. In 128
        // bits, adding half of what is cut off can't overflow either.
        let magnitude = u128::from(amount.minor_units().unsigned_abs());
        let cut = 10_u128.pow(u32::from(minor_digits - shown_digits));
        let rounded = (magnitude + cut / 2) / cut;
        let scale = 10_u128.pow(u32::from(shown_digits));
        let affixes = if amount.is_negative() && rounded != 0 {
            &self.negative
        } else {
            &self.positive
        };

        let mut text = affixes.prefix.clone();
        // `rounded` is at most 2^63, so the whole part fits into u64.
        text.push_str(&group_digits(
            u64::try_from(rounded / scale).unwrap_or(u64::MAX),
        ));
        if shown_digits > 0 {
            text.push_str(&self.decimal_point);
            let fraction = format!(
                "{:0width$}",
                rounded % scale,
                width = usize::from(shown_digits)
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
    use proptest::prelude::*;

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
    fn rounds_for_display_half_away_from_zero() {
        let format = german();
        let group = group_by_three(".");
        let whole = |minor_units| format.format_rounded(eur(minor_units), 2, 0, &group);
        assert_eq!(whole(123_456), "1.235\u{a0}€");
        assert_eq!(whole(123_449), "1.234\u{a0}€");
        assert_eq!(whole(150), "2\u{a0}€");
        assert_eq!(whole(-150), "-2\u{a0}€");
        assert_eq!(whole(-149), "-1\u{a0}€");
        assert_eq!(whole(99_950), "1.000\u{a0}€");
        // No "-0 €" for an amount that rounds to zero.
        assert_eq!(whole(-49), "0\u{a0}€");
        assert_eq!(whole(i64::MIN), "-92.233.720.368.547.758\u{a0}€");
        assert_eq!(whole(i64::MAX), "92.233.720.368.547.758\u{a0}€");
        // Three decimal places shown as one; more than the currency has
        // shows what it has.
        assert_eq!(
            format.format_rounded(eur(1_250), 3, 1, &group),
            "1,3\u{a0}€"
        );
        assert_eq!(
            format.format_rounded(eur(1_250), 2, 4, &group),
            "12,50\u{a0}€"
        );
    }

    /// `minor_units` rounded to whole units of `10^cut` the way the display
    /// should, computed differently: from the remainder, with signed numbers.
    fn expected_rounding(minor_units: i64, cut: u32) -> i128 {
        let unit = 10_i128.pow(cut);
        let value = i128::from(minor_units);
        let toward_zero = value / unit;
        let remainder = (value % unit).abs();
        if remainder * 2 >= unit {
            toward_zero + value.signum()
        } else {
            toward_zero
        }
    }

    proptest! {
        #[test]
        fn display_rounding_is_exact_rounding(
            minor_units in any::<i64>(),
            minor_digits in 0_u8..=4,
            shown_digits in 0_u8..=4,
        ) {
            // Read back without separators, the shown text is the exact
            // amount rounded half away from zero to the shown places.
            let format = AmountFormat {
                decimal_point: ".".into(),
                zero_digit: '0',
                positive: Affixes::default(),
                negative: Affixes { prefix: "-".into(), suffix: String::new() },
            };
            let shown = shown_digits.min(minor_digits);
            let text = format.format_rounded(
                eur(minor_units), minor_digits, shown_digits, |value| value.to_string());
            let digits: String = text.chars().filter(|c| *c != '.').collect();
            let read: i128 = digits.parse().expect("digits and a sign");
            prop_assert_eq!(
                read,
                expected_rounding(minor_units, u32::from(minor_digits - shown))
            );
            let decimals = text.split_once('.').map_or(0, |(_, fraction)| fraction.len());
            prop_assert_eq!(decimals, usize::from(shown));
        }

        #[test]
        fn rounding_treats_an_amount_and_its_negative_alike(
            minor_units in 1_i64..=i64::MAX,
            shown_digits in 0_u8..=2,
        ) {
            let format = german();
            let group = group_by_three(".");
            let positive = format.format_rounded(eur(minor_units), 2, shown_digits, &group);
            let negative = format.format_rounded(eur(-minor_units), 2, shown_digits, &group);
            if positive.starts_with("0\u{a0}") || positive.starts_with("0,0\u{a0}") {
                prop_assert_eq!(negative, positive);
            } else {
                prop_assert_eq!(negative, format!("-{positive}"));
            }
        }

        #[test]
        fn showing_every_place_is_exact(minor_units in any::<i64>()) {
            let format = german();
            let group = group_by_three(".");
            prop_assert_eq!(
                format.format_rounded(eur(minor_units), 2, 2, &group),
                format.format(eur(minor_units), 2, &group)
            );
        }
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
