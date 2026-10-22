// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reads amounts the user typed, in their regional format (D-002, D-065).
//!
//! QML hands over the text as typed; this module turns it into an exact
//! amount with integer arithmetic only (AGENTS.md §3.1 rule 1). It is strict
//! where a mistake could change an amount: "12.50" typed in German, where the
//! point groups thousands, is refused instead of being read as 1250, and
//! more decimal places than the currency has are refused, never rounded.

use kassensturz_core::{CurrencyCode, DecimalError, ExchangeRate, MAX_DECIMAL_PLACES, Money};

use crate::format::AmountFormat;

/// Why typed text isn't an amount. The UI shows a message per case (D-034).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AmountInputError {
    /// Nothing was typed.
    Empty,
    /// The text isn't a number in the user's regional format.
    Malformed,
    /// More decimal places than the currency has.
    TooManyDecimalPlaces {
        /// How many the currency has.
        max: u8,
    },
    /// Too large to fit into an amount.
    TooLarge,
}

/// What the user's regional format says about typed amounts.
pub struct AmountReader<'a> {
    /// Separators, digits and currency symbol of the user's locale.
    pub format: &'a AmountFormat,
    /// What separates groups of digits, e.g. "." in German.
    pub group_separator: &'a str,
    /// The currency of the amount.
    pub currency: CurrencyCode,
    /// How many decimal places it has (2 for the euro).
    pub minor_digits: u8,
}

impl AmountReader<'_> {
    /// Reads `text` as an amount, e.g. "-1.234,56 €" in German or "$1,234.56"
    /// in American English. The currency's symbol or code may be typed or
    /// left out, a minus sign or parentheses make the amount negative.
    pub fn read(&self, text: &str) -> Result<Money, AmountInputError> {
        let (negative, number) = self.strip_sign_and_symbol(text)?;
        let number: String = number.chars().map(|c| self.ascii_digit(c)).collect();
        let (whole, fraction) = match number.split_once(self.format.decimal_point.as_str()) {
            Some((whole, fraction)) => (whole, Some(fraction)),
            None => (number.as_str(), None),
        };
        let whole = self.ungrouped(whole)?;
        let fraction = fraction.unwrap_or_default();
        let digits_only = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
        // "12," and "," are unfinished; ",50" is clear enough.
        let unfinished = number.contains(self.format.decimal_point.as_str()) && fraction.is_empty();
        if !digits_only(fraction) || unfinished || (whole.is_empty() && fraction.is_empty()) {
            return Err(AmountInputError::Malformed);
        }
        if fraction.len() > usize::from(self.minor_digits) {
            return Err(AmountInputError::TooManyDecimalPlaces {
                max: self.minor_digits,
            });
        }

        // Padding the fraction to the currency's decimal places makes the
        // digits the number of minor units.
        let padded = format!(
            "{fraction:0<width$}",
            width = usize::from(self.minor_digits)
        );
        let mut minor: i128 = 0;
        for digit in whole.bytes().chain(padded.bytes()) {
            minor = minor
                .checked_mul(10)
                .and_then(|minor| minor.checked_add(i128::from(digit - b'0')))
                .ok_or(AmountInputError::TooLarge)?;
        }
        if negative {
            minor = -minor;
        }
        let minor = i64::try_from(minor).map_err(|_| AmountInputError::TooLarge)?;
        Ok(Money::from_minor_units(minor, self.currency))
    }

    /// Takes the sign and the currency's symbol or code off `text`, in
    /// whichever order and on whichever side they were typed, and returns
    /// whether the amount is negative and what is left.
    fn strip_sign_and_symbol<'t>(
        &self,
        text: &'t str,
    ) -> Result<(bool, &'t str), AmountInputError> {
        let mut rest = text.trim();
        if rest.is_empty() {
            return Err(AmountInputError::Empty);
        }
        let mut negative = false;
        // Qt's American English writes negative amounts in parentheses.
        if let Some(inner) = rest
            .strip_prefix('(')
            .and_then(|rest| rest.strip_suffix(')'))
        {
            negative = true;
            rest = inner.trim();
        }
        let symbols = self.symbols();
        loop {
            let before = rest;
            for symbol in &symbols {
                rest = rest.strip_prefix(symbol.as_str()).unwrap_or(rest).trim();
                rest = rest.strip_suffix(symbol.as_str()).unwrap_or(rest).trim();
            }
            if let Some(unsigned) = rest.strip_prefix(['-', '\u{2212}']) {
                if negative {
                    return Err(AmountInputError::Malformed);
                }
                negative = true;
                rest = unsigned.trim();
            } else if let Some(unsigned) = rest.strip_prefix('+') {
                rest = unsigned.trim();
            }
            if rest == before {
                return Ok((negative, rest));
            }
        }
    }

    /// The currency's symbols as the locale writes them, e.g. "€", and its
    /// ISO 4217 code.
    fn symbols(&self) -> Vec<String> {
        let mut symbols = vec![self.currency.as_str().to_owned()];
        for affixes in [&self.format.positive, &self.format.negative] {
            for affix in [&affixes.prefix, &affixes.suffix] {
                let symbol: String = affix
                    .chars()
                    .filter(|c| !c.is_whitespace() && !matches!(c, '-' | '\u{2212}' | '(' | ')'))
                    .collect();
                if !symbol.is_empty() && !symbols.contains(&symbol) {
                    symbols.push(symbol);
                }
            }
        }
        symbols
    }

    /// `c` as an ASCII digit if it is one of the locale's digits.
    fn ascii_digit(&self, c: char) -> char {
        let zero = u32::from(self.format.zero_digit);
        match u32::from(c).checked_sub(zero) {
            Some(value @ 0..=9) => char::from_digit(value, 10).unwrap_or(c),
            _ => c,
        }
    }

    /// The whole part's digits without group separators, see [`ungrouped`].
    fn ungrouped(&self, whole: &str) -> Result<String, AmountInputError> {
        ungrouped(whole, self.group_separator)
    }
}

/// The whole part's digits without group separators. Groups are checked:
/// the last one has three digits, the others two or three (Indian formats
/// group by two), so a decimal point typed in the wrong format is refused
/// instead of read as a group separator.
fn ungrouped(whole: &str, separator: &str) -> Result<String, AmountInputError> {
    // Some locales group with a (narrow) no-break space; any space will do.
    let is_space = !separator.is_empty() && separator.chars().all(char::is_whitespace);
    let groups: Vec<&str> = if is_space {
        whole.split(char::is_whitespace).collect()
    } else if separator.is_empty() {
        vec![whole]
    } else {
        whole.split(separator).collect()
    };
    let digits_only = |group: &str| group.bytes().all(|byte| byte.is_ascii_digit());
    if !groups.iter().all(|group| digits_only(group)) {
        return Err(AmountInputError::Malformed);
    }
    if let [first, middle @ .., last] = groups.as_slice() {
        let fits = (1..=3).contains(&first.len())
            && middle.iter().all(|group| (2..=3).contains(&group.len()))
            && last.len() == 3;
        if !fits {
            return Err(AmountInputError::Malformed);
        }
    }
    Ok(groups.concat())
}

/// Why typed text isn't an exchange rate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RateInputError {
    /// Nothing was typed.
    Empty,
    /// The text isn't a number in the user's regional format.
    Malformed,
    /// More decimal places than a rate can have.
    TooManyDecimalPlaces {
        /// How many it can have.
        max: u8,
    },
    /// More digits than a rate can have.
    TooLarge,
    /// A rate is above zero.
    NotPositive,
}

/// Reads an exchange rate the user typed, in their regional format: digits,
/// optionally grouped, and a decimal point (D-065, D-106). As for amounts,
/// "10.000" typed in German, where the point groups thousands, is read as
/// ten thousand, and "10.5" is refused rather than read as 105.
pub fn read_rate(
    text: &str,
    decimal_point: &str,
    group_separator: &str,
) -> Result<ExchangeRate, RateInputError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(RateInputError::Empty);
    }
    let (whole, fraction) = text.split_once(decimal_point).unwrap_or((text, ""));
    let whole = ungrouped(whole, group_separator).map_err(|_| RateInputError::Malformed)?;
    let digits_only = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    // "10," is unfinished; ",5" is clear enough.
    let unfinished = text.contains(decimal_point) && fraction.is_empty();
    if !digits_only(fraction) || unfinished || (whole.is_empty() && fraction.is_empty()) {
        return Err(RateInputError::Malformed);
    }
    let plain = format!(
        "{}{}{fraction}",
        if whole.is_empty() { "0" } else { &whole },
        if fraction.is_empty() { "" } else { "." }
    );
    plain.parse::<ExchangeRate>().map_err(|error| match error {
        DecimalError::TooManyDigits => RateInputError::TooLarge,
        DecimalError::TooManyDecimalPlaces => RateInputError::TooManyDecimalPlaces {
            max: MAX_DECIMAL_PLACES,
        },
        DecimalError::NotPositive => RateInputError::NotPositive,
        _ => RateInputError::Malformed,
    })
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::format::Affixes;

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };

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
            negative: Affixes::spaced("(€".into(), ")".into()),
        }
    }

    fn read(
        format: &AmountFormat,
        separator: &str,
        digits: u8,
        text: &str,
    ) -> Result<i64, AmountInputError> {
        AmountReader {
            format,
            group_separator: separator,
            currency: EUR,
            minor_digits: digits,
        }
        .read(text)
        .map(Money::minor_units)
    }

    fn de(text: &str) -> Result<i64, AmountInputError> {
        read(&german(), ".", 2, text)
    }

    fn us(text: &str) -> Result<i64, AmountInputError> {
        read(&american(), ",", 2, text)
    }

    #[test]
    fn reads_amounts_the_german_way() {
        assert_eq!(de("12,50"), Ok(1250));
        assert_eq!(de("12,5"), Ok(1250));
        assert_eq!(de("12"), Ok(1200));
        assert_eq!(de("1.234,56"), Ok(123_456));
        assert_eq!(de("1234,56"), Ok(123_456));
        assert_eq!(de("1.234.567"), Ok(123_456_700));
        assert_eq!(de(",50"), Ok(50));
        assert_eq!(de("  -53,20 € "), Ok(-5320));
        assert_eq!(de("€ 5"), Ok(500));
        assert_eq!(de("5 EUR"), Ok(500));
        assert_eq!(de("\u{2212}5"), Ok(-500));
        assert_eq!(de("+5"), Ok(500));
        assert_eq!(de("-0"), Ok(0));
    }

    #[test]
    fn reads_amounts_the_american_way() {
        assert_eq!(us("1,234.56"), Ok(123_456));
        assert_eq!(us("€1,234.56"), Ok(123_456));
        assert_eq!(us("(€53.20)"), Ok(-5320));
        assert_eq!(us("-€53.20"), Ok(-5320));
        assert_eq!(us("€-53.20"), Ok(-5320));
    }

    #[test]
    fn refuses_a_decimal_point_from_another_format() {
        // In German the point groups thousands: "12.50" can't be meant as
        // 1250, so it is refused rather than misread.
        assert_eq!(de("12.50"), Err(AmountInputError::Malformed));
        assert_eq!(de("1.5"), Err(AmountInputError::Malformed));
        assert_eq!(us("12,50"), Err(AmountInputError::Malformed));
    }

    #[test]
    fn refuses_what_is_not_an_amount() {
        for text in [
            "-", "abc", "1,2,3", "12,", ",", "--5", "-(5)", "1e3", "5 USD", "1.23.4,5",
        ] {
            assert_eq!(de(text), Err(AmountInputError::Malformed), "{text:?}");
        }
        assert_eq!(de(""), Err(AmountInputError::Empty));
        assert_eq!(de("   "), Err(AmountInputError::Empty));
    }

    #[test]
    fn refuses_to_round() {
        assert_eq!(
            de("12,345"),
            Err(AmountInputError::TooManyDecimalPlaces { max: 2 })
        );
        assert_eq!(
            read(&german(), ".", 0, "12,5"),
            Err(AmountInputError::TooManyDecimalPlaces { max: 0 })
        );
        assert_eq!(read(&german(), ".", 3, "1,234"), Ok(1234));
    }

    #[test]
    fn refuses_amounts_that_do_not_fit() {
        assert_eq!(de("92233720368547758,07"), Ok(i64::MAX));
        assert_eq!(de("-92233720368547758,08"), Ok(i64::MIN));
        assert_eq!(de("92233720368547758,08"), Err(AmountInputError::TooLarge));
        assert_eq!(de(&"9".repeat(60)), Err(AmountInputError::TooLarge));
    }

    #[test]
    fn accepts_space_grouping_and_the_locales_digits() {
        let french = AmountFormat {
            decimal_point: ",".into(),
            zero_digit: '0',
            positive: Affixes::spaced(String::new(), "\u{a0}€".into()),
            negative: Affixes::spaced("-".into(), "\u{a0}€".into()),
        };
        assert_eq!(read(&french, "\u{202f}", 2, "1\u{202f}234,56"), Ok(123_456));
        assert_eq!(read(&french, "\u{202f}", 2, "1 234,56"), Ok(123_456));

        let arabic = AmountFormat {
            zero_digit: '\u{660}',
            ..german()
        };
        assert_eq!(read(&arabic, ".", 2, "\u{661}\u{662},\u{665}"), Ok(1250));
    }

    #[test]
    fn accepts_indian_grouping() {
        assert_eq!(us("1,00,000.00"), Ok(10_000_000));
    }

    proptest! {
        /// Whatever the app shows, it reads back as the same amount.
        #[test]
        fn formatting_then_reading_gives_the_same_amount(minor in any::<i64>(), digits in 0_u8..=4) {
            let group = |value: u64| {
                let digits = value.to_string();
                let mut grouped = String::new();
                for (index, digit) in digits.chars().enumerate() {
                    if index > 0 && (digits.len() - index).is_multiple_of(3) {
                        grouped.push('.');
                    }
                    grouped.push(digit);
                }
                grouped
            };
            let format = german();
            let text = format.format(Money::from_minor_units(minor, EUR), digits, group);
            prop_assert_eq!(read(&format, ".", digits, &text), Ok(minor));
        }
    }

    fn rate(
        text: &str,
        decimal_point: &str,
        group_separator: &str,
    ) -> Result<String, RateInputError> {
        read_rate(text, decimal_point, group_separator).map(|rate| rate.to_string())
    }

    #[test]
    fn rates_are_read_in_the_regional_format() {
        assert_eq!(rate("1,95583", ",", "."), Ok("1.95583".into()));
        assert_eq!(rate("10.000", ",", "."), Ok("10000".into()));
        assert_eq!(rate("10.000,5", ",", "."), Ok("10000.5".into()));
        assert_eq!(rate(" ,5 ", ",", "."), Ok("0.5".into()));
        assert_eq!(rate("1.95583", ".", ","), Ok("1.95583".into()));
        assert_eq!(rate("10,000", ".", ","), Ok("10000".into()));
        assert_eq!(rate("1\u{a0}000", ",", "\u{a0}"), Ok("1000".into()));
    }

    #[test]
    fn a_rate_in_the_wrong_format_is_refused_not_guessed() {
        // A point groups thousands in German: "10.5" is not ten and a half.
        assert_eq!(rate("10.5", ",", "."), Err(RateInputError::Malformed));
    }

    #[test]
    fn a_rate_that_cannot_be_one_says_why() {
        assert_eq!(rate("", ",", "."), Err(RateInputError::Empty));
        assert_eq!(rate("  ", ",", "."), Err(RateInputError::Empty));
        assert_eq!(rate("abc", ",", "."), Err(RateInputError::Malformed));
        assert_eq!(rate("10,", ",", "."), Err(RateInputError::Malformed));
        assert_eq!(rate("-2", ",", "."), Err(RateInputError::Malformed));
        assert_eq!(rate("0", ",", "."), Err(RateInputError::NotPositive));
        assert_eq!(
            rate("0,1234567890123456789", ",", "."),
            Err(RateInputError::TooManyDecimalPlaces { max: 18 })
        );
        assert_eq!(
            rate("1.000.000.000.000.000.000.000", ",", "."),
            Err(RateInputError::TooLarge)
        );
    }
}
