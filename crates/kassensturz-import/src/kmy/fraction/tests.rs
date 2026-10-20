// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use kassensturz_core::CurrencyCode;
use proptest::prelude::*;

use super::*;

fn currency(code: &str, digits: u8) -> Currency {
    Currency::new(CurrencyCode::new(code).expect("a valid code"), digits).expect("valid digits")
}

fn eur() -> Currency {
    currency("EUR", 2)
}

fn fraction(text: &str) -> Fraction {
    Fraction::parse(text).unwrap_or_else(|| panic!("{text} is a fraction"))
}

#[test]
fn only_plain_fractions_are_read() {
    for text in ["-6004/25", "0/1", "1/25", "-0/1", "44251/50"] {
        assert!(Fraction::parse(text).is_some(), "{text}");
    }
    for text in [
        "", "12", "1/", "/1", "1/0", "1/-5", "+1/2", "1.5/1", " 1/2", "1/2 ", "--1/2", "1//2",
        "1/2/3", "١/٢",
    ] {
        assert_eq!(Fraction::parse(text), None, "{text:?}");
    }
    // Larger than the arithmetic holds.
    assert_eq!(Fraction::parse(&format!("{}/1", "9".repeat(40))), None);
}

#[test]
fn amounts_come_out_in_whole_minor_units() {
    let amount = |text: &str, currency: Currency| fraction(text).money(currency, text);
    assert_eq!(
        amount("-6004/25", eur()),
        Ok(Money::from_minor_units(-24_016, eur().code()))
    );
    assert_eq!(
        amount("44251/50", eur()),
        Ok(Money::from_minor_units(88_502, eur().code()))
    );
    assert_eq!(
        amount("0/1", eur()),
        Ok(Money::from_minor_units(0, eur().code()))
    );
    let yen = currency("JPY", 0);
    assert_eq!(
        amount("1500/1", yen),
        Ok(Money::from_minor_units(1_500, yen.code()))
    );
    // A cent of a currency without cents, or a third of a cent, is never
    // rounded.
    assert_eq!(
        amount("1501/100", yen),
        Err(ImportError::TooManyDecimalPlaces {
            found: "15.01".to_owned(),
            currency: yen.code(),
            line: None,
        })
    );
    assert_eq!(
        amount("1/300", eur()),
        Err(ImportError::TooManyDecimalPlaces {
            found: "1/300".to_owned(),
            currency: eur().code(),
            line: None,
        })
    );
    let huge = format!("{}/1", i64::MAX);
    assert_eq!(
        amount(&huge, eur()),
        Err(ImportError::AmountTooLarge {
            found: i64::MAX.to_string(),
            line: None,
        })
    );
}

#[test]
fn equal_fractions_are_recognized_however_written() {
    assert!(fraction("6004/25").equals(fraction("24016/100")));
    assert!(fraction("-1/2").equals(fraction("-50/100")));
    assert!(!fraction("1/2").equals(fraction("-1/2")));
    assert!(!fraction("1/3").equals(fraction("33/100")));
}

#[test]
fn a_rate_is_kept_only_if_it_is_exact_and_above_zero() {
    let rate = |text: &str| fraction(text).rate().map(|rate| rate.to_string());
    assert_eq!(rate("10811/10000").as_deref(), Some("1.0811"));
    assert_eq!(rate("1/1000").as_deref(), Some("0.001"));
    assert_eq!(rate("3/1").as_deref(), Some("3"));
    // KMyMoney's rate from the Deutsche Mark to the euro has no finite
    // decimal form.
    assert_eq!(rate("100000/195583"), None);
    assert_eq!(rate("0/1"), None);
    assert_eq!(rate("-1/2"), None);
    // More places than a rate holds.
    assert_eq!(rate(&format!("1/{}", 10_i128.pow(19))), None);
}

proptest! {
    #[test]
    fn any_cent_amount_reads_back_exactly(minor_units in any::<i64>(), factor in 1_i64..=100) {
        // The same amount written over any denominator that divides it.
        let text = format!("{}/{}", i128::from(minor_units) * i128::from(factor), 100 * factor);
        prop_assert_eq!(
            fraction(&text).money(eur(), &text),
            Ok(Money::from_minor_units(minor_units, eur().code()))
        );
    }

    #[test]
    fn no_text_makes_reading_panic(text in "\\PC{0,40}") {
        if let Some(fraction) = Fraction::parse(&text) {
            let _ = fraction.money(eur(), &text);
            let _ = fraction.rate();
            let _ = fraction.equals(fraction);
        }
    }
}
