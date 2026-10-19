// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Amounts and currencies as files write them, read exactly: integer
//! arithmetic only, never floating point (AGENTS.md §3.1 rule 1).

use kassensturz_core::{Currency, CurrencyCode, Money};

use crate::error::ImportError;

/// The character a file separates whole units from decimal places with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DecimalMark {
    /// `12.50`, as in XML formats such as CAMT.053 and in OFX.
    Point,
    /// `12,50`, as in MT940 and many German CSV files.
    Comma,
}

impl DecimalMark {
    fn byte(self) -> u8 {
        match self {
            Self::Point => b'.',
            Self::Comma => b',',
        }
    }
}

/// The currency `code` with the decimal places KassenSturz's ISO 4217 table
/// gives it (D-062, D-077), for a [`crate::Statement`]'s currency.
///
/// Fails for a code the table doesn't list: a withdrawn currency, such as
/// the Deutsche Mark, or a code that isn't a currency. The importers that
/// convert withdrawn currencies (D-077, M2.9) give their decimal places
/// themselves.
pub fn currency(code: CurrencyCode) -> Result<Currency, ImportError> {
    code.iso_minor_digits()
        .and_then(|digits| Currency::new(code, digits))
        .ok_or(ImportError::UnknownCurrency {
            currency: code,
            line: None,
        })
}

/// Reads `text` as an amount of `currency`: an optional sign (`-` or `+`),
/// at least one digit, and optionally `mark` followed by at least one more
/// digit, such as `-1234.50` or `7`.
///
/// Nothing else is accepted, not even spaces or digit grouping; an importer
/// takes those out first where its format has them. An amount with fewer
/// decimal places than the currency is fine (`12.5` is 12.50 EUR); one with
/// more is refused, never rounded, even if the extra places are zeros, since
/// the file then counts in other units than KassenSturz thinks.
pub fn parse_amount(
    text: &str,
    mark: DecimalMark,
    currency: Currency,
) -> Result<Money, ImportError> {
    let bad = || ImportError::BadAmount {
        found: text.to_owned(),
        line: None,
    };
    let (negative, unsigned) = match text.as_bytes().first() {
        Some(b'-') => (true, &text.as_bytes()[1..]),
        Some(b'+') => (false, &text.as_bytes()[1..]),
        _ => (false, text.as_bytes()),
    };
    let (whole, fraction) = match unsigned.iter().position(|&byte| byte == mark.byte()) {
        Some(at) => (&unsigned[..at], Some(&unsigned[at + 1..])),
        None => (unsigned, None),
    };
    let digits = |part: &[u8]| !part.is_empty() && part.iter().all(u8::is_ascii_digit);
    if !digits(whole) || fraction.is_some_and(|fraction| !digits(fraction)) {
        return Err(bad());
    }
    let fraction = fraction.unwrap_or_default();
    let places = currency.minor_digits();
    if fraction.len() > usize::from(places) {
        return Err(ImportError::TooManyDecimalPlaces {
            found: text.to_owned(),
            currency: currency.code(),
            line: None,
        });
    }
    let too_large = || ImportError::AmountTooLarge {
        found: text.to_owned(),
        line: None,
    };
    // The digits as minor units: the fraction padded with zeros up to the
    // currency's decimal places. i64 holds 18 digits, so the checks stop
    // long before the u128 could overflow.
    let mut minor: u128 = 0;
    let padding = usize::from(places) - fraction.len();
    let all_digits = whole
        .iter()
        .chain(fraction)
        .map(|digit| digit - b'0')
        .chain(std::iter::repeat_n(0, padding));
    for digit in all_digits {
        minor = minor
            .checked_mul(10)
            .and_then(|minor| minor.checked_add(u128::from(digit)))
            .filter(|&minor| minor <= u128::from(i64::MAX.unsigned_abs()) + 1)
            .ok_or_else(too_large)?;
    }
    let minor = i128::try_from(minor).map_err(|_| too_large())?;
    let minor = i64::try_from(if negative { -minor } else { minor }).map_err(|_| too_large())?;
    Ok(Money::from_minor_units(minor, currency.code()))
}

#[cfg(test)]
mod tests;
