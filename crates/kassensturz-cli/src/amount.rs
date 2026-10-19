// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Amounts as plain text: `-12.50`, always with a point and the currency's
//! decimal places, whatever the user's region. Regional formats are the UI's
//! job (D-065); this tool is for testing the core, so it reads and writes the
//! one form that can't be misread. Everything is exact integer arithmetic
//! (AGENTS.md §3.1 rule 1).

use kassensturz_core::{CurrencyCode, Money, MoneyError};
use kassensturz_store::Book;

use crate::error::CliError;

/// `minor_units` written with `digits` decimal places: 1234 and 2 give
/// `12.34`, -5 and 2 give `-0.05`.
pub(crate) fn format_minor(minor_units: i64, digits: u8) -> String {
    let magnitude = u128::from(minor_units.unsigned_abs());
    let sign = if minor_units < 0 { "-" } else { "" };
    if digits == 0 {
        return format!("{sign}{magnitude}");
    }
    let scale = 10_u128.pow(u32::from(digits));
    format!(
        "{sign}{}.{:0width$}",
        magnitude / scale,
        magnitude % scale,
        width = usize::from(digits)
    )
}

/// `money` with its currency, e.g. `-53.20 EUR`. The decimal places are the
/// book's, or ISO 4217's if the book doesn't use the currency yet.
pub(crate) fn money_text(book: Option<&Book>, money: Money) -> String {
    let currency = money.currency();
    let digits = book
        .and_then(|book| book.currency_minor_digits(currency).ok())
        .or_else(|| currency.iso_minor_digits());
    match digits {
        Some(digits) => format!("{} {currency}", format_minor(money.minor_units(), digits)),
        // Better no decimal point than one in the wrong place.
        None => format!("{} minor units of {currency}", money.minor_units()),
    }
}

/// Reads an amount typed as `[+-]digits[.digits]` in `currency`, which has
/// `digits` decimal places. More decimal places than that are refused, never
/// rounded away.
pub(crate) fn parse_amount(
    text: &str,
    currency: CurrencyCode,
    digits: u8,
) -> Result<Money, CliError> {
    let malformed = || {
        CliError::message(format!(
            "`{text}` isn't an amount. Type digits with an optional decimal point, \
             such as 12.50 or -3"
        ))
    };
    let (negative, unsigned) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let (whole, fraction) = match unsigned.split_once('.') {
        Some((whole, fraction)) => (whole, fraction),
        None => (unsigned, ""),
    };
    let all_digits = |part: &str| part.bytes().all(|b| b.is_ascii_digit());
    let point_without_fraction = unsigned.contains('.') && fraction.is_empty();
    if whole.is_empty() || !all_digits(whole) || !all_digits(fraction) || point_without_fraction {
        return Err(malformed());
    }
    if fraction.len() > usize::from(digits) {
        return Err(CliError::message(format!(
            "`{text}` has more decimal places than {currency} has ({digits}). \
             Leave out the extra places; nothing is rounded for you"
        )));
    }
    // Padding the fraction to `digits` places makes the integer the number of
    // minor units. At most 18 places and a 64-bit whole part fit into i128
    // unless the whole part is absurdly long, which `checked_mul` catches.
    let padded = format!("{fraction:0<width$}", width = usize::from(digits));
    let mut minor: i128 = 0;
    for digit in whole.bytes().chain(padded.bytes()) {
        minor = minor
            .checked_mul(10)
            .and_then(|minor| minor.checked_add(i128::from(digit - b'0')))
            .ok_or(MoneyError::Overflow)?;
    }
    if negative {
        minor = -minor;
    }
    let minor = i64::try_from(minor).map_err(|_| MoneyError::Overflow)?;
    Ok(Money::from_minor_units(minor, currency))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn eur() -> CurrencyCode {
        CurrencyCode::new("EUR").expect("valid code")
    }

    fn parsed(text: &str, digits: u8) -> Result<i64, String> {
        parse_amount(text, eur(), digits)
            .map(Money::minor_units)
            .map_err(|error| format!("{error:?}"))
    }

    #[test]
    fn formats_with_the_currencys_decimal_places() {
        assert_eq!(format_minor(1234, 2), "12.34");
        assert_eq!(format_minor(-5, 2), "-0.05");
        assert_eq!(format_minor(0, 2), "0.00");
        assert_eq!(format_minor(700, 0), "700");
        assert_eq!(format_minor(-1234, 3), "-1.234");
        assert_eq!(format_minor(i64::MIN, 2), "-92233720368547758.08");
    }

    #[test]
    fn parses_amounts() {
        assert_eq!(parsed("12.50", 2), Ok(1250));
        assert_eq!(parsed("-12.5", 2), Ok(-1250));
        assert_eq!(parsed("+3", 2), Ok(300));
        assert_eq!(parsed("0.05", 2), Ok(5));
        assert_eq!(parsed("700", 0), Ok(700));
        assert_eq!(parsed("-0", 2), Ok(0));
        assert_eq!(parsed("92233720368547758.07", 2), Ok(i64::MAX));
        assert_eq!(parsed("-92233720368547758.08", 2), Ok(i64::MIN));
    }

    #[test]
    fn refuses_what_is_not_an_amount() {
        for text in [
            "", "-", ".5", "1.", "1,5", "1 000", "1e3", "abc", "--1", "1.2.3", "€5",
        ] {
            assert!(parsed(text, 2).is_err(), "{text:?} should be refused");
        }
    }

    #[test]
    fn refuses_to_round() {
        assert!(parsed("12.345", 2).is_err());
        assert!(parsed("12.5", 0).is_err());
        assert!(parsed("12.0", 0).is_err());
    }

    #[test]
    fn refuses_amounts_that_do_not_fit() {
        assert!(parsed("92233720368547758.08", 2).is_err());
        assert!(parsed("-92233720368547758.09", 2).is_err());
        assert!(parsed(&"9".repeat(60), 2).is_err());
    }

    proptest! {
        /// What the tool writes, it reads back unchanged.
        #[test]
        fn formatting_then_parsing_gives_the_same_amount(minor in any::<i64>(), digits in 0_u8..=6) {
            let text = format_minor(minor, digits);
            prop_assert_eq!(parsed(&text, digits), Ok(minor));
        }
    }
}
