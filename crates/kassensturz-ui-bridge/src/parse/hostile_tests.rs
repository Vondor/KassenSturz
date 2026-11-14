// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Typed amounts in the German and American formats, and what both refuse
//! (D-787, US7 Phase 3 round 1, section C). An amount typed in the wrong
//! region's format must be refused, never read as another amount: that is
//! money (AGENTS.md §2.1). All names and numbers are made up.

use kassensturz_core::{CurrencyCode, Money};

use super::{AmountInputError, AmountReader};
use crate::format::{Affixes, AmountFormat};

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
        positive: Affixes::spaced("$".into(), String::new()),
        negative: Affixes::spaced("($".into(), ")".into()),
    }
}

fn read(format: &AmountFormat, group: &str, minor_digits: u8, text: &str) -> Result<i64, String> {
    AmountReader {
        format,
        group_separator: group,
        currency: EUR,
        minor_digits,
    }
    .read(text)
    .map(Money::minor_units)
    .map_err(|error| format!("{error:?}"))
}

fn de(text: &str) -> Result<i64, String> {
    read(&german(), ".", 2, text)
}

fn us(text: &str) -> Result<i64, String> {
    read(&american(), ",", 2, text)
}

#[test]
fn the_german_ways_to_type_an_amount_are_read() {
    for (text, cents) in [
        ("1.234,56", 123_456),
        ("1234,56", 123_456),
        ("1234", 123_400),
        ("-1.234,56", -123_456),
        ("-1.234,56 €", -123_456),
        ("1.234,56\u{a0}€", 123_456),
        ("€ 1.234,56", 123_456),
        ("EUR 12,5", 1_250),
        ("  12,50  ", 1_250),
        (",5", 50),
        ("+12,00", 1_200),
        ("\u{2212}3,10", -310),
        ("1.000", 100_000),
    ] {
        assert_eq!(de(text), Ok(cents), "{text:?}");
    }
}

#[test]
fn a_minus_zero_is_zero_not_a_negative_amount() {
    for text in ["-0,00", "-0", "0,00", "+0,00", "-0,0"] {
        assert_eq!(de(text), Ok(0), "{text:?}");
    }
}

#[test]
fn an_american_amount_typed_in_german_is_refused_not_read_as_another_amount() {
    for text in [
        "1,234.56",
        "12.50",
        "1.5",
        "0.99",
        "1,234,567.89",
        "12.345,6.7",
        "1,2,3",
    ] {
        assert!(de(text).is_err(), "{text:?} was read as {:?}", de(text));
    }
    // The one that looks right and is the user's own: a thousands point.
    assert_eq!(de("12.500"), Ok(1_250_000));
}

#[test]
fn a_german_amount_typed_in_american_is_refused_not_read_as_another_amount() {
    for text in ["1.234,56", "12,50", "1,23", "0,99", "12,5"] {
        assert!(us(text).is_err(), "{text:?} was read as {:?}", us(text));
    }
    assert_eq!(us("1,234.56"), Ok(123_456));
    assert_eq!(us("($53.20)"), Ok(-5_320));
    assert_eq!(us("-$53.20"), Ok(-5_320));
}

#[test]
fn text_that_is_no_amount_is_refused() {
    let mut wrongly = Vec::new();
    for text in [
        "",
        " ",
        "-",
        "+",
        ",",
        ".",
        "€",
        "1,",
        "1,,5",
        "--5",
        "-+5",
        "+-5",
        "1e3",
        "0x10",
        "NaN",
        "inf",
        "1 2",
        "1_000",
        "١٢٣",
        "１２３",
        "1'234,56",
        "(12,50)(",
        "((12,50))",
        "12\n,50",
        "12,50\0",
        "\u{202e}12,50",
        "12,50\u{200b}",
        "--",
        "−−3",
    ] {
        let result = de(text);
        if result.is_ok() {
            wrongly.push(format!("{text:?} was read as {result:?}"));
        }
    }
    assert!(wrongly.is_empty(), "{wrongly:#?}");
}

#[test]
fn too_many_decimal_places_are_refused_never_rounded() {
    assert_eq!(
        read(&german(), ".", 2, "12,505"),
        Err(format!(
            "{:?}",
            AmountInputError::TooManyDecimalPlaces { max: 2 }
        ))
    );
    assert!(de("0,001").is_err());
    // A currency without decimals takes none.
    assert!(read(&german(), ".", 0, "12,5").is_err());
    assert_eq!(read(&german(), ".", 0, "1.234"), Ok(1_234));
    // One with three keeps all three.
    assert_eq!(read(&german(), ".", 3, "1,234"), Ok(1_234));
}

#[test]
fn an_amount_too_large_is_refused_at_the_edge_and_never_wraps() {
    // i64::MAX is 9 223 372 036 854 775 807 minor units.
    assert_eq!(de("92.233.720.368.547.758,07"), Ok(i64::MAX));
    for text in [
        "92.233.720.368.547.758,08",
        "92.233.720.368.547.759,00",
        "100.000.000.000.000.000,00",
        "-92.233.720.368.547.758,09",
        "999.999.999.999.999.999.999.999.999,99",
    ] {
        assert_eq!(
            de(text),
            Err(format!("{:?}", AmountInputError::TooLarge)),
            "{text:?}"
        );
    }
    // The negative edge: -i64::MAX is fine, i64::MIN has no positive twin.
    assert_eq!(de("-92.233.720.368.547.758,07"), Ok(-i64::MAX));
    let long = "9".repeat(5_000);
    assert!(de(&long).is_err());
    assert!(de(&format!("{long},99")).is_err());
}

#[test]
fn a_pasted_amount_with_other_currency_symbols_is_not_read_as_euro() {
    for text in ["$12,50", "12,50 USD", "£12,50", "12,50 CHF", "¥1250"] {
        assert!(de(text).is_err(), "{text:?} was read as {:?}", de(text));
    }
}
