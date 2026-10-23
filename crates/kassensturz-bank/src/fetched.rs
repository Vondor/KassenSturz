// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a bank delivers, as it delivers it: amounts as exact fractions in
//! text, dates that may be missing, texts in lines. Nothing is interpreted
//! here; [`crate::statement`] turns it into a statement.
//!
//! Amounts are exact fractions because AqBanking keeps them that way
//! (`AB_VALUE`, read with `AB_Value_GetNumDenomString`, D-141). A fraction
//! becomes [`Money`] only when it is a whole number of the currency's minor
//! units; anything else is refused, never rounded (threat T-10).

use kassensturz_core::{Currency, CurrencyCode, Date, Money};

use crate::backend::BankAccount;
use crate::error::BankError;

/// The most digits a numerator or denominator may have. Far above any
/// amount: 128-bit arithmetic below holds 38 digits, and a currency's minor
/// units add at most [`kassensturz_core::MAX_DECIMAL_PLACES`] of them.
const MAX_DIGITS: usize = 19;

/// An amount as the bank sent it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchedAmount {
    /// The amount as a fraction of whole numbers, `numerator/denominator`,
    /// e.g. `-1234/100` for −12.34, or just a whole number, e.g. `50`. An
    /// optional sign comes first.
    pub fraction: String,
    /// The currency's code as the bank sent it, if it sent one; otherwise
    /// the account's currency applies (D-141).
    pub currency: Option<String>,
}

impl FetchedAmount {
    /// `minor_units` of `currency` as the fraction `minor_units/10^digits`,
    /// the way a bank that counts in minor units delivers them. For the fake
    /// bank and for tests.
    pub fn from_minor_units(minor_units: i64, currency: Currency) -> Self {
        let denominator = 10_i128.pow(u32::from(currency.minor_digits()));
        Self {
            fraction: format!("{minor_units}/{denominator}"),
            currency: Some(currency.code().as_str().to_owned()),
        }
    }

    /// The amount as [`Money`] in `account_currency`, the currency of the
    /// account it belongs to, with that currency's decimal places (D-027).
    ///
    /// Fails when the fraction isn't one of whole numbers
    /// (`bank.malformed-amount`), isn't a whole number of minor units
    /// (`bank.inexact-amount`), doesn't fit (`bank.amount-too-large`), or
    /// names another currency than the account's (`bank.currency-mismatch`).
    pub fn money(&self, account_currency: Currency) -> Result<Money, BankError> {
        let currency = match &self.currency {
            Some(code) => CurrencyCode::new(code).map_err(|_| self.malformed())?,
            None => account_currency.code(),
        };
        if currency != account_currency.code() {
            return Err(BankError::CurrencyMismatch {
                expected: account_currency.code(),
                found: currency,
            });
        }
        let (numerator, denominator) = self.parse_fraction()?;
        let scale = 10_i128.pow(u32::from(account_currency.minor_digits()));
        let scaled = numerator
            .checked_mul(scale)
            .ok_or_else(|| self.too_large())?;
        if scaled % denominator != 0 {
            return Err(BankError::InexactAmount {
                amount: self.fraction.clone(),
            });
        }
        let minor_units = i64::try_from(scaled / denominator).map_err(|_| self.too_large())?;
        Ok(Money::from_minor_units(minor_units, currency))
    }

    /// The numerator and the denominator (1 for a whole number), as the
    /// text says.
    fn parse_fraction(&self) -> Result<(i128, i128), BankError> {
        let text = self.fraction.trim();
        let (numerator, denominator) = match text.split_once('/') {
            Some((numerator, denominator)) => (numerator, Some(denominator)),
            None => (text, None),
        };
        let numerator = self.parse_integer(numerator, true)?;
        let denominator = match denominator {
            Some(denominator) => self.parse_integer(denominator, false)?,
            None => 1,
        };
        if denominator == 0 {
            return Err(self.malformed());
        }
        Ok((numerator, denominator))
    }

    /// Digits, with a sign in front where allowed, into a number. Leading
    /// zeros are fine; anything else isn't.
    fn parse_integer(&self, text: &str, signed: bool) -> Result<i128, BankError> {
        let (negative, digits) = match text.strip_prefix('-') {
            Some(digits) if signed => (true, digits),
            Some(_) => return Err(self.malformed()),
            None => (
                false,
                text.strip_prefix('+').filter(|_| signed).unwrap_or(text),
            ),
        };
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(self.malformed());
        }
        if digits.len() > MAX_DIGITS {
            return Err(self.too_large());
        }
        let value: i128 = digits.parse().map_err(|_| self.malformed())?;
        Ok(if negative { -value } else { value })
    }

    fn malformed(&self) -> BankError {
        BankError::MalformedAmount {
            amount: self.fraction.clone(),
        }
    }

    fn too_large(&self) -> BankError {
        BankError::AmountTooLarge {
            amount: self.fraction.clone(),
        }
    }
}

/// A balance the bank reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchedBalance {
    /// The amount. For a credit card the bank's sign is kept as sent.
    pub amount: FetchedAmount,
    /// The day it is for, if the bank says.
    pub date: Option<Date>,
}

/// One transaction as the bank delivered it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchedTransaction {
    /// Whether the bank has booked it. Only booked ones are imported (D-030).
    pub booked: bool,
    /// The bank's own ID for it, if it gives one (D-030).
    pub bank_id: Option<String>,
    /// The day the bank booked it, if given.
    pub booking_date: Option<Date>,
    /// The value date, if given.
    pub value_date: Option<Date>,
    /// The amount: money in positive, money out negative.
    pub amount: FetchedAmount,
    /// The other party's name, if given.
    pub other_party_name: Option<String>,
    /// The other party's IBAN or account number, if given.
    pub other_party_iban: Option<String>,
    /// The payment's text, in the lines the bank sent (German:
    /// Verwendungszweck).
    pub purpose: Vec<String>,
}

/// What one fetch delivered for one account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fetched {
    /// The account, as the backend lists it.
    pub account: BankAccount,
    /// The booked balance the bank reported, if it did.
    pub balance: Option<FetchedBalance>,
    /// The transactions, in the order the bank sent them.
    pub transactions: Vec<FetchedTransaction>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };
    const USD: CurrencyCode = match CurrencyCode::new("USD") {
        Ok(code) => code,
        Err(_) => panic!("USD is a valid code"),
    };
    const JPY: CurrencyCode = match CurrencyCode::new("JPY") {
        Ok(code) => code,
        Err(_) => panic!("JPY is a valid code"),
    };

    fn eur() -> Currency {
        Currency::new(EUR, 2).expect("two decimal places")
    }

    fn amount(fraction: &str) -> FetchedAmount {
        FetchedAmount {
            fraction: fraction.to_owned(),
            currency: Some("EUR".to_owned()),
        }
    }

    #[test]
    fn a_fraction_of_minor_units_becomes_money() {
        for (fraction, minor_units) in [
            ("-1234/100", -1234),
            ("1234/100", 1234),
            ("+1234/100", 1234),
            ("50", 5000),
            ("-50/1", -5000),
            ("617/50", 1234),
            ("0/100", 0),
            ("-0/100", 0),
            ("00012/100", 12),
            (" 7/100 ", 7),
        ] {
            assert_eq!(
                amount(fraction).money(eur()),
                Ok(Money::from_minor_units(minor_units, EUR)),
                "{fraction}"
            );
        }
    }

    #[test]
    fn the_decimal_places_come_from_the_currency() {
        let yen = Currency::new(JPY, 0).expect("no decimal places");
        let fetched = FetchedAmount {
            fraction: "1500".to_owned(),
            currency: Some("JPY".to_owned()),
        };
        assert_eq!(fetched.money(yen), Ok(Money::from_minor_units(1500, JPY)));
        let fetched = FetchedAmount {
            fraction: "3001/2".to_owned(),
            currency: Some("JPY".to_owned()),
        };
        assert_eq!(
            fetched.money(yen),
            Err(BankError::InexactAmount {
                amount: "3001/2".to_owned()
            })
        );
    }

    #[test]
    fn a_fraction_that_is_not_whole_minor_units_is_refused_not_rounded() {
        for fraction in ["1/3", "12345/1000", "1/200"] {
            assert_eq!(
                amount(fraction).money(eur()),
                Err(BankError::InexactAmount {
                    amount: fraction.to_owned()
                }),
                "{fraction}"
            );
        }
    }

    #[test]
    fn text_that_is_not_a_fraction_is_refused() {
        for fraction in [
            "",
            "/",
            "1/",
            "/100",
            "12.34",
            "1,234/100",
            "1/-100",
            "--1/100",
            "1e3",
            "1/0",
            "abc",
            "1/100/10",
            "- 1/100",
        ] {
            assert_eq!(
                amount(fraction).money(eur()),
                Err(BankError::MalformedAmount {
                    amount: fraction.to_owned()
                }),
                "{fraction}"
            );
        }
        let bad_currency = FetchedAmount {
            fraction: "1/100".to_owned(),
            currency: Some("Euro".to_owned()),
        };
        assert_eq!(
            bad_currency.money(eur()),
            Err(BankError::MalformedAmount {
                amount: "1/100".to_owned()
            })
        );
    }

    #[test]
    fn too_many_digits_are_refused_before_arithmetic() {
        let huge = "1".repeat(MAX_DIGITS + 1);
        assert_eq!(
            amount(&huge).money(eur()),
            Err(BankError::AmountTooLarge { amount: huge })
        );
        // Fits 128 bits scaled, but not 64 bits as minor units.
        let large = "9223372036854775807";
        assert_eq!(
            amount(large).money(eur()),
            Err(BankError::AmountTooLarge {
                amount: large.to_owned()
            })
        );
        // The largest amount that fits.
        let largest = "9223372036854775807/100";
        assert_eq!(
            amount(largest).money(eur()),
            Ok(Money::from_minor_units(i64::MAX, EUR))
        );
    }

    #[test]
    fn the_currency_must_be_the_accounts() {
        let dollars = FetchedAmount {
            fraction: "1/100".to_owned(),
            currency: Some("USD".to_owned()),
        };
        assert_eq!(
            dollars.money(eur()),
            Err(BankError::CurrencyMismatch {
                expected: EUR,
                found: USD
            })
        );
        let unsaid = FetchedAmount {
            fraction: "1/100".to_owned(),
            currency: None,
        };
        assert_eq!(unsaid.money(eur()), Ok(Money::from_minor_units(1, EUR)));
    }

    #[test]
    fn minor_units_round_trip() {
        let fetched = FetchedAmount::from_minor_units(-1234, eur());
        assert_eq!(fetched.fraction, "-1234/100");
        assert_eq!(
            fetched.money(eur()),
            Ok(Money::from_minor_units(-1234, EUR))
        );
    }
}
