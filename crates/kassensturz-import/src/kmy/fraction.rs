// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The fractions a `.kmy` file writes amounts and prices as, such as
//! `-6004/25` for -240.16, read exactly with integer arithmetic only
//! (AGENTS.md §3.1 rule 1).

use kassensturz_core::{Currency, ExchangeRate, Money};

use crate::error::ImportError;

/// A fraction as the file writes it: a numerator, maybe negative, over a
/// denominator above zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Fraction {
    numerator: i128,
    denominator: i128,
}

impl Fraction {
    /// Reads `text`, such as `-6004/25`: an optional minus, digits, a
    /// slash and digits that aren't all zeros. `None` for anything else,
    /// including numbers too large to hold.
    pub fn parse(text: &str) -> Option<Self> {
        let (numerator, denominator) = text.split_once('/')?;
        let digits = numerator.strip_prefix('-').unwrap_or(numerator);
        let plain = |digits: &str| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit());
        if !plain(digits) || !plain(denominator) {
            return None;
        }
        let numerator: i128 = numerator.parse().ok()?;
        let denominator: i128 = denominator.parse().ok()?;
        (denominator > 0).then_some(Self {
            numerator,
            denominator,
        })
    }

    /// Whether this is the same number as `other`, however each is written.
    pub fn equals(self, other: Self) -> bool {
        match (
            self.numerator.checked_mul(other.denominator),
            other.numerator.checked_mul(self.denominator),
        ) {
            (Some(left), Some(right)) => left == right,
            // Too large to compare: then they're not the plain amounts a
            // file holds, and the caller refuses them.
            _ => false,
        }
    }

    /// The fraction as an amount of `currency`. It must come out in whole
    /// minor units: KassenSturz never rounds what a file holds.
    /// `text` is the fraction as written, for the error.
    pub fn money(self, currency: Currency, text: &str) -> Result<Money, ImportError> {
        let too_large = || ImportError::AmountTooLarge {
            found: self.shown(text),
            line: None,
        };
        let scaled = self
            .numerator
            .checked_mul(10_i128.pow(u32::from(currency.minor_digits())))
            .ok_or_else(too_large)?;
        if scaled % self.denominator != 0 {
            return Err(ImportError::TooManyDecimalPlaces {
                found: self.shown(text),
                currency: currency.code(),
                line: None,
            });
        }
        let minor_units = i64::try_from(scaled / self.denominator).map_err(|_| too_large())?;
        Ok(Money::from_minor_units(minor_units, currency.code()))
    }

    /// The fraction as an exchange rate, if it is above zero and has a
    /// finite decimal form that a rate can hold exactly: `100000/195583`,
    /// KMyMoney's rate from the Deutsche Mark to the euro, has none.
    pub fn rate(self) -> Option<ExchangeRate> {
        let (digits, places) = self.decimal(18)?;
        ExchangeRate::from_digits(i64::try_from(digits).ok()?, places).ok()
    }

    /// The fraction's digits and decimal places, such as 24016 and 2 for
    /// `6004/25`, if it has a finite decimal form with at most `max`
    /// places.
    fn decimal(self, max: u8) -> Option<(i128, u8)> {
        let divisor = gcd(
            self.numerator.unsigned_abs(),
            self.denominator.unsigned_abs(),
        );
        let divisor = i128::try_from(divisor).ok()?;
        let (numerator, denominator) = (self.numerator / divisor, self.denominator / divisor);
        // A finite decimal form needs a denominator of only twos and fives.
        let mut rest = denominator;
        let (mut twos, mut fives) = (0_u8, 0_u8);
        while rest % 2 == 0 {
            rest /= 2;
            twos = twos.checked_add(1)?;
        }
        while rest % 5 == 0 {
            rest /= 5;
            fives = fives.checked_add(1)?;
        }
        let places = twos.max(fives);
        if rest != 1 || places > max {
            return None;
        }
        let scale = 10_i128.checked_pow(u32::from(places))?;
        let digits = numerator.checked_mul(scale / denominator)?;
        Some((digits, places))
    }

    /// The fraction as people read it: `-240.16` for `-6004/25`, or as the
    /// file wrote it if it has no short decimal form.
    fn shown(self, text: &str) -> String {
        let Some((digits, places)) = self.decimal(18) else {
            return text.to_owned();
        };
        let sign = if digits < 0 { "-" } else { "" };
        let digits = digits.unsigned_abs().to_string();
        let places = usize::from(places);
        if places == 0 {
            return format!("{sign}{digits}");
        }
        let padded = format!("{digits:0>width$}", width = places + 1);
        let (whole, decimals) = padded.split_at(padded.len() - places);
        format!("{sign}{whole}.{decimals}")
    }
}

/// The greatest common divisor of `a` and `b`, at least 1.
fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}

#[cfg(test)]
mod tests;
