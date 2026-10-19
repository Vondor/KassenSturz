// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Exchange rates, prices and quantities: fixed-point decimals (D-051).
//!
//! Each value is a whole number of digits plus a number of decimal places,
//! e.g. 1.0856 is the digits 10856 with 4 decimal places, so what the user
//! types is exactly what is stored. Inside, `rust_decimal` holds the value;
//! it never appears in KassenSturz's API, and its conversions to and from
//! floating point are blocked by Clippy (`clippy.toml`, AGENTS.md §3.1
//! rule 1).

use core::fmt;
use core::str::FromStr;

use rust_decimal::Decimal;

/// Why a number can't be an exchange rate, a price or a quantity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecimalError {
    /// The text isn't a plain decimal number such as `1.0856` or `-3`: digits,
    /// optionally a point and more digits, optionally a leading minus.
    Malformed,
    /// More than [`MAX_DIGITS`] digits, not counting leading zeros.
    TooManyDigits,
    /// More than [`MAX_DECIMAL_PLACES`] decimal places.
    TooManyDecimalPlaces,
    /// An exchange rate must be above zero.
    NotPositive,
    /// A price can't be below zero.
    Negative,
}

/// The most digits a rate, price or quantity can have, not counting leading
/// zeros. A book stores the digits in a 64-bit INTEGER, which holds any
/// 18-digit number (D-051).
pub const MAX_DIGITS: u32 = 18;

/// The most decimal places a rate, price or quantity can have.
pub const MAX_DECIMAL_PLACES: u8 = 18;

/// 10^18, the first number with more than [`MAX_DIGITS`] digits.
const DIGITS_LIMIT: i64 = 1_000_000_000_000_000_000;

/// The value shared by [`ExchangeRate`], [`Price`] and [`Quantity`]: at most
/// [`MAX_DIGITS`] digits and [`MAX_DECIMAL_PLACES`] decimal places.
///
/// Equality and order are by value: 1.50 equals 1.5. The decimal places are
/// kept as given, so 1.50 is shown and stored as 1.50.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Fixed(Decimal);

impl Fixed {
    fn from_digits(digits: i64, decimal_places: u8) -> Result<Self, DecimalError> {
        if digits <= -DIGITS_LIMIT || digits >= DIGITS_LIMIT {
            return Err(DecimalError::TooManyDigits);
        }
        if decimal_places > MAX_DECIMAL_PLACES {
            return Err(DecimalError::TooManyDecimalPlaces);
        }
        // Can't fail: rust_decimal takes up to 28 decimal places.
        Decimal::try_new(digits, u32::from(decimal_places))
            .map(Self)
            .map_err(|_| DecimalError::TooManyDecimalPlaces)
    }

    /// Reads `-?[0-9]+(\.[0-9]+)?`. Nothing else is accepted: no `+`, no
    /// exponent, no grouping, no spaces. The UI turns what the user types in
    /// their regional format into this form (D-065).
    fn parse(text: &str) -> Result<Self, DecimalError> {
        let (negative, unsigned) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let (whole, fraction) = match unsigned.split_once('.') {
            Some((whole, fraction)) => (whole, Some(fraction)),
            None => (unsigned, None),
        };
        let is_digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
        if !is_digits(whole) || fraction.is_some_and(|fraction| !is_digits(fraction)) {
            return Err(DecimalError::Malformed);
        }
        let fraction = fraction.unwrap_or("");
        let decimal_places =
            u8::try_from(fraction.len()).map_err(|_| DecimalError::TooManyDecimalPlaces)?;
        if decimal_places > MAX_DECIMAL_PLACES {
            return Err(DecimalError::TooManyDecimalPlaces);
        }
        let mut digits: i64 = 0;
        for byte in whole.bytes().chain(fraction.bytes()) {
            // Below 10^19 before the check, because the step before stayed
            // below 10^18; 128 bits hold that easily.
            let next = i128::from(digits) * 10 + i128::from(byte - b'0');
            digits = i64::try_from(next)
                .ok()
                .filter(|&next| next < DIGITS_LIMIT)
                .ok_or(DecimalError::TooManyDigits)?;
        }
        Self::from_digits(if negative { -digits } else { digits }, decimal_places)
    }

    fn digits(self) -> i64 {
        i64::try_from(self.0.mantissa()).expect("Fixed holds at most 18 digits")
    }

    fn decimal_places(self) -> u8 {
        u8::try_from(self.0.scale()).expect("Fixed holds at most 18 decimal places")
    }

    /// The same value with trailing zeros removed, e.g. 1.0850 as 10850 with
    /// 4 places becomes 1085 with 3. Calculations use this form, so their
    /// intermediate results are as small as possible.
    fn normalized(self) -> (i64, u8) {
        let normalized = Self(self.0.normalize());
        (normalized.digits(), normalized.decimal_places())
    }

    fn is_negative(self) -> bool {
        self.0.is_sign_negative() && !self.0.is_zero()
    }

    fn is_positive(self) -> bool {
        !self.0.is_sign_negative() && !self.0.is_zero()
    }
}

impl fmt::Display for Fixed {
    /// The plain form [`Fixed::parse`] reads, e.g. `1.0856`. Not for users:
    /// the UI shows values in their regional format (D-065).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// Defines one of the three public types around [`Fixed`], with the check
/// that its values must pass.
macro_rules! define_decimal {
    ($(#[$meta:meta])* $name:ident, $check:expr) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(Fixed);

        impl $name {
            /// The value `digits` × 10^−`decimal_places`, e.g. 10856 and 4
            /// for 1.0856.
            pub fn from_digits(digits: i64, decimal_places: u8) -> Result<Self, DecimalError> {
                Self::checked(Fixed::from_digits(digits, decimal_places)?)
            }

            /// The value's digits, as given: 10850 for 1.0850.
            pub fn digits(self) -> i64 {
                self.0.digits()
            }

            /// The value's number of decimal places, as given: 4 for 1.0850.
            pub fn decimal_places(self) -> u8 {
                self.0.decimal_places()
            }

            fn checked(value: Fixed) -> Result<Self, DecimalError> {
                let check: fn(Fixed) -> Result<(), DecimalError> = $check;
                check(value)?;
                Ok(Self(value))
            }
        }

        impl FromStr for $name {
            type Err = DecimalError;

            /// Reads the plain form, e.g. `1.0856`: digits, optionally a
            /// point and more digits, optionally a leading minus. Regional
            /// formats are the UI's job (D-065).
            fn from_str(text: &str) -> Result<Self, DecimalError> {
                Self::checked(Fixed::parse(text)?)
            }
        }

        impl fmt::Display for $name {
            /// The plain form `from_str` reads, e.g. `1.0856`, for logs and
            /// files; users see the regional format (D-065).
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "({})"), self.0)
            }
        }
    };
}

define_decimal!(
    /// How much one unit of one currency costs in another, e.g. 1.0856 US
    /// dollars for one euro. Always above zero.
    ///
    /// In v1, rates are entered by hand (D-028). Converting an amount with
    /// one is exact until the result is rounded once to minor units
    /// ([`Money::checked_mul_rate`](crate::Money::checked_mul_rate), D-051).
    ExchangeRate,
    |value| {
        if value.is_positive() {
            Ok(())
        } else {
            Err(DecimalError::NotPositive)
        }
    }
);

impl ExchangeRate {
    /// The digits and decimal places without trailing zeros: 1085 and 3 for
    /// 1.0850.
    pub(crate) fn normalized(self) -> (i64, u8) {
        self.0.normalized()
    }
}

define_decimal!(
    /// The price of one unit of something, e.g. one share of a fund, in a
    /// currency. Zero or above. Securities come after 1.0 (D-029); the type
    /// exists now so every such number is a fixed-point decimal from the
    /// start (D-051).
    Price,
    |value| {
        if value.is_negative() {
            Err(DecimalError::Negative)
        } else {
            Ok(())
        }
    }
);

define_decimal!(
    /// A number of units of something, e.g. 0.1234 shares bought by a fund
    /// savings plan. Negative quantities leave an account. Securities come
    /// after 1.0 (D-029); the type exists now so every such number is a
    /// fixed-point decimal from the start (D-051).
    Quantity,
    |_| Ok(())
);

#[cfg(test)]
mod tests {
    use super::*;

    fn rate(text: &str) -> ExchangeRate {
        text.parse().expect("valid rate")
    }

    #[test]
    fn reads_and_writes_the_plain_form() {
        for text in [
            "1.0856",
            "1.95583",
            "0.5",
            "7",
            "1.0850",
            "0.000000000000000001",
        ] {
            assert_eq!(rate(text).to_string(), text);
        }
        for text in ["-0.25", "0", "-12"] {
            assert_eq!(
                text.parse::<Quantity>().map(|q| q.to_string()),
                Ok(text.into())
            );
        }
    }

    #[test]
    fn keeps_the_decimal_places_as_given() {
        let rate = rate("1.0850");
        assert_eq!((rate.digits(), rate.decimal_places()), (10850, 4));
        assert_eq!(rate.normalized(), (1085, 3));
        // Trailing zeros of a whole number stay: there are no negative
        // decimal places.
        let whole = ExchangeRate::from_digits(1200, 0).expect("valid rate");
        assert_eq!(whole.normalized(), (1200, 0));
    }

    #[test]
    fn compares_by_value() {
        assert_eq!(rate("1.50"), rate("1.5"));
        assert!(rate("1.0856") < rate("1.1"));
        let quantity = |text: &str| text.parse::<Quantity>().expect("valid quantity");
        assert!(quantity("-1") < quantity("0.5"));
    }

    #[test]
    fn refuses_anything_but_the_plain_form() {
        for text in [
            "", "-", ".5", "5.", "1,5", "+1", "1e3", " 1", "1 ", "1_000", "--1", "1.2.3", "١",
        ] {
            assert_eq!(
                text.parse::<Quantity>(),
                Err(DecimalError::Malformed),
                "{text:?}"
            );
        }
    }

    #[test]
    fn holds_at_most_18_digits_and_18_decimal_places() {
        let most = "999999999999999999";
        assert_eq!(
            most.parse::<Quantity>().map(|q| q.digits()),
            Ok(999_999_999_999_999_999)
        );
        assert_eq!(
            format!("{most}9").parse::<Quantity>(),
            Err(DecimalError::TooManyDigits)
        );
        assert_eq!(
            "99999999999999999999999999999999999999".parse::<Quantity>(),
            Err(DecimalError::TooManyDigits)
        );
        // Leading zeros don't count.
        assert!(format!("000{most}").parse::<Quantity>().is_ok());
        assert_eq!(
            "0.0000000000000000001".parse::<Quantity>(),
            Err(DecimalError::TooManyDecimalPlaces)
        );
        assert_eq!(
            Quantity::from_digits(DIGITS_LIMIT, 0),
            Err(DecimalError::TooManyDigits)
        );
        assert_eq!(
            Quantity::from_digits(-DIGITS_LIMIT, 0),
            Err(DecimalError::TooManyDigits)
        );
        assert_eq!(
            Quantity::from_digits(1, MAX_DECIMAL_PLACES + 1),
            Err(DecimalError::TooManyDecimalPlaces)
        );
    }

    #[test]
    fn a_rate_is_above_zero() {
        for text in ["0", "0.000", "-1.5"] {
            assert_eq!(
                text.parse::<ExchangeRate>(),
                Err(DecimalError::NotPositive),
                "{text}"
            );
        }
        assert_eq!(
            ExchangeRate::from_digits(0, 2),
            Err(DecimalError::NotPositive)
        );
    }

    #[test]
    fn a_price_is_zero_or_above() {
        assert!("0".parse::<Price>().is_ok());
        assert!("12.34".parse::<Price>().is_ok());
        assert_eq!("-0.01".parse::<Price>(), Err(DecimalError::Negative));
        // Minus zero is zero.
        assert_eq!(
            "-0.00".parse::<Price>().map(|p| p.to_string()),
            Ok("0.00".into())
        );
    }
}
