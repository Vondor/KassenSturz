// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use proptest::prelude::*;

use super::*;

const EUR: Currency = currency_with("EUR", 2);
const JPY: Currency = currency_with("JPY", 0);
const BHD: Currency = currency_with("BHD", 3);

const fn currency_with(code: &str, digits: u8) -> Currency {
    let Ok(code) = CurrencyCode::new(code) else {
        panic!("a valid code");
    };
    match Currency::new(code, digits) {
        Some(currency) => currency,
        None => panic!("valid decimal places"),
    }
}

fn eur(text: &str) -> Result<Money, ImportError> {
    parse_amount(text, DecimalMark::Point, EUR)
}

fn cents(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, EUR.code())
}

#[test]
fn amounts_are_read_in_minor_units() {
    assert_eq!(eur("1234.56"), Ok(cents(123_456)));
    assert_eq!(eur("-1234.56"), Ok(cents(-123_456)));
    assert_eq!(eur("+0.05"), Ok(cents(5)));
    assert_eq!(eur("7"), Ok(cents(700)));
    assert_eq!(eur("12.5"), Ok(cents(1250)));
    assert_eq!(eur("0007.10"), Ok(cents(710)));
    assert_eq!(eur("-0.00"), Ok(cents(0)));
}

#[test]
fn the_decimal_mark_is_the_one_given() {
    assert_eq!(
        parse_amount("1234,56", DecimalMark::Comma, EUR),
        Ok(cents(123_456))
    );
    // With a comma as the mark, a point is just a wrong character.
    assert!(matches!(
        parse_amount("1.234,56", DecimalMark::Comma, EUR),
        Err(ImportError::BadAmount { .. })
    ));
    assert!(matches!(eur("1234,56"), Err(ImportError::BadAmount { .. })));
}

#[test]
fn the_currency_gives_the_decimal_places() {
    assert_eq!(
        parse_amount("1500", DecimalMark::Point, JPY),
        Ok(Money::from_minor_units(1500, JPY.code()))
    );
    assert_eq!(
        parse_amount("1.5", DecimalMark::Point, BHD),
        Ok(Money::from_minor_units(1500, BHD.code()))
    );
}

#[test]
fn extra_decimal_places_are_refused_never_rounded() {
    for text in ["1.005", "1.000", "0.001"] {
        assert_eq!(
            eur(text),
            Err(ImportError::TooManyDecimalPlaces {
                found: text.to_owned(),
                currency: EUR.code(),
                line: None,
            }),
            "{text}"
        );
    }
    assert!(matches!(
        parse_amount("1.0", DecimalMark::Point, JPY),
        Err(ImportError::TooManyDecimalPlaces { .. })
    ));
}

#[test]
fn anything_else_is_not_an_amount() {
    for text in [
        "", "-", "+", ".", "1.", ".5", "-.5", "1.2.3", "1 234.00", " 12.00", "12.00 ", "1e3",
        "--1", "+-1", "12.-5", "١٢", "12.00EUR", "0x10",
    ] {
        assert_eq!(
            eur(text),
            Err(ImportError::BadAmount {
                found: text.to_owned(),
                line: None,
            }),
            "{text:?}"
        );
    }
}

#[test]
fn amounts_beyond_64_bits_are_refused() {
    // The largest and smallest amounts there are, in cents.
    assert_eq!(eur("92233720368547758.07"), Ok(cents(i64::MAX)));
    assert_eq!(eur("-92233720368547758.08"), Ok(cents(i64::MIN)));
    for text in [
        "92233720368547758.08",
        "-92233720368547758.09",
        "99999999999999999999999999999999999999999",
    ] {
        assert_eq!(
            eur(text),
            Err(ImportError::AmountTooLarge {
                found: text.to_owned(),
                line: None,
            }),
            "{text}"
        );
    }
    // Leading zeros don't count.
    assert_eq!(eur("000000000000000000000000000001.00"), Ok(cents(100)));
}

#[test]
fn currencies_come_from_the_table() {
    let code = |text| CurrencyCode::new(text).expect("a valid code");
    assert_eq!(currency(code("EUR")), Ok(EUR));
    assert_eq!(currency(code("JPY")), Ok(JPY));
    // The Deutsche Mark is withdrawn: the table doesn't list it, the table
    // of withdrawn currencies does (D-077, D-105).
    assert_eq!(currency(code("DEM")), Ok(currency_with("DEM", 2)));
    assert_eq!(
        currency(code("XYZ")),
        Err(ImportError::UnknownCurrency {
            currency: code("XYZ"),
            line: None,
        })
    );
}

/// `minor_units` written as a file would, with `digits` decimal places.
fn written(minor_units: i64, digits: u8, mark: char) -> String {
    let magnitude = minor_units.unsigned_abs();
    let sign = if minor_units < 0 { "-" } else { "" };
    if digits == 0 {
        return format!("{sign}{magnitude}");
    }
    let scale = 10_u64.pow(u32::from(digits));
    format!(
        "{sign}{}{mark}{:0width$}",
        magnitude / scale,
        magnitude % scale,
        width = usize::from(digits)
    )
}

proptest! {
    /// Every amount there is reads back exactly as it was written, with
    /// either mark and up to four decimal places, as ISO 4217 has.
    #[test]
    fn every_amount_reads_back_exactly(
        minor_units in any::<i64>(),
        digits in 0_u8..=4,
        comma in any::<bool>(),
    ) {
        let currency = currency_with("XYZ", digits);
        let (mark, character) = if comma {
            (DecimalMark::Comma, ',')
        } else {
            (DecimalMark::Point, '.')
        };
        let text = written(minor_units, digits, character);
        prop_assert_eq!(
            parse_amount(&text, mark, currency),
            Ok(Money::from_minor_units(minor_units, currency.code()))
        );
    }

    /// Fewer decimal places than the currency's mean the same amount.
    #[test]
    fn missing_decimal_places_are_zeros(whole in 0_i64..1_000_000, tenth in 0_i64..10) {
        prop_assert_eq!(
            eur(&format!("{whole}.{tenth}")),
            Ok(cents(whole * 100 + tenth * 10))
        );
    }

    /// No text makes the reader panic.
    #[test]
    fn any_text_is_read_or_refused(text in any::<String>()) {
        let _ = eur(&text);
    }
}
