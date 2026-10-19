// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Amounts of money: 64-bit integers in minor units plus an ISO 4217 currency
//! code (AGENTS.md §5, D-027).

use crate::currency::{Currency, CurrencyCode};
use crate::decimal::ExchangeRate;
use crate::rounding::RoundingMode;

/// An amount of money: a whole number of the currency's minor units (cents,
/// for the euro) and the currency.
///
/// All arithmetic is checked: it fails on overflow or when the currencies
/// differ. That's why there are no `+` and `-` operators, only `checked_*`
/// methods that return a [`Result`]. Only [`Money::checked_mul_ratio`]
/// rounds, in the mode its caller names; everything else is exact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Money {
    minor_units: i64,
    currency: CurrencyCode,
}

impl Money {
    /// An amount of `minor_units` in `currency`, e.g. 5320 EUR for €53.20.
    pub const fn from_minor_units(minor_units: i64, currency: CurrencyCode) -> Self {
        Self {
            minor_units,
            currency,
        }
    }

    /// Nothing, in `currency`.
    pub const fn zero(currency: CurrencyCode) -> Self {
        Self::from_minor_units(0, currency)
    }

    /// The amount in minor units: 5320 for €53.20.
    pub const fn minor_units(self) -> i64 {
        self.minor_units
    }

    /// The currency.
    pub const fn currency(self) -> CurrencyCode {
        self.currency
    }

    /// Whether the amount is exactly zero.
    pub const fn is_zero(self) -> bool {
        self.minor_units == 0
    }

    /// Whether the amount is below zero.
    pub const fn is_negative(self) -> bool {
        self.minor_units < 0
    }

    /// `self + other`, if both are in the same currency and the result fits.
    pub fn checked_add(self, other: Self) -> Result<Self, MoneyError> {
        self.expect_currency(other.currency)?;
        let minor_units = self
            .minor_units
            .checked_add(other.minor_units)
            .ok_or(MoneyError::Overflow)?;
        Ok(Self::from_minor_units(minor_units, self.currency))
    }

    /// `self - other`, if both are in the same currency and the result fits.
    pub fn checked_sub(self, other: Self) -> Result<Self, MoneyError> {
        self.expect_currency(other.currency)?;
        let minor_units = self
            .minor_units
            .checked_sub(other.minor_units)
            .ok_or(MoneyError::Overflow)?;
        Ok(Self::from_minor_units(minor_units, self.currency))
    }

    /// `-self`. Fails only for the most negative 64-bit amount.
    pub fn checked_neg(self) -> Result<Self, MoneyError> {
        let minor_units = self.minor_units.checked_neg().ok_or(MoneyError::Overflow)?;
        Ok(Self::from_minor_units(minor_units, self.currency))
    }

    /// `self × numerator ÷ denominator`, rounded once to whole minor units in
    /// `rounding`: for example 19 % of an amount (19 ÷ 100), or a third of it
    /// (1 ÷ 3).
    ///
    /// The calculation is exact in 128 bits, so only its result is rounded,
    /// in the mode the caller names, usually the book's (D-027). Fails if
    /// `denominator` is zero or the result doesn't fit into 64 bits.
    pub fn checked_mul_ratio(
        self,
        numerator: i64,
        denominator: i64,
        rounding: RoundingMode,
    ) -> Result<Self, MoneyError> {
        if denominator == 0 {
            return Err(MoneyError::DivisionByZero);
        }
        // Two 64-bit factors always fit into 128 bits.
        let product = i128::from(self.minor_units) * i128::from(numerator);
        let rounded = rounding
            .divide(product, i128::from(denominator))
            .ok_or(MoneyError::Overflow)?;
        let minor_units = i64::try_from(rounded).map_err(|_| MoneyError::Overflow)?;
        Ok(Self::from_minor_units(minor_units, self.currency))
    }

    /// `self × rate`, converted from `from` into `to` and rounded once to
    /// whole minor units of `to` in `rounding`: 100.00 EUR at 1.0856 USD per
    /// euro is 108.56 USD.
    ///
    /// The calculation is exact in 128 bits, so only its result is rounded
    /// (D-051). `from` must be `self`'s currency, with the decimal places the
    /// book stores for it (D-027). Fails only if the result doesn't fit into
    /// 64 bits.
    pub fn checked_mul_rate(
        self,
        from: Currency,
        rate: ExchangeRate,
        to: Currency,
        rounding: RoundingMode,
    ) -> Result<Self, MoneyError> {
        self.expect_currency(from.code())?;
        let (digits, decimal_places) = rate.normalized();
        // result = minor units × digits × 10^(to's places − from's places −
        // the rate's places). Both factors are below 2^63, so the product
        // fits into 128 bits.
        let product = i128::from(self.minor_units) * i128::from(digits);
        let exponent = i32::from(to.minor_digits())
            - i32::from(from.minor_digits())
            - i32::from(decimal_places);
        let result = if exponent >= 0 {
            // Exact; if it overflows 128 bits, the result can't fit 64 either.
            product.checked_mul(power_of_ten(exponent))
        } else {
            rounding.divide(product, power_of_ten(-exponent))
        };
        Self::from_exact(result, to.code())
    }

    /// `self ÷ rate`, converted from `from` into `to` and rounded once to
    /// whole minor units of `to` in `rounding`: 108.56 USD at 1.0856 USD per
    /// euro is 100.00 EUR. This converts back with the same rate, so no
    /// inverse rate is ever stored.
    ///
    /// The calculation is exact in 128 bits, so only its result is rounded
    /// (D-051). `from` must be `self`'s currency, with the decimal places the
    /// book stores for it (D-027). Fails if the result doesn't fit into 64
    /// bits, and for amounts beyond any household's when the rate has many
    /// decimal places: the minor units times 10^(the rate's places + `to`'s
    /// places − `from`'s places) must fit into 128 bits. Between currencies
    /// with the same places that always holds; the worst case among ISO 4217
    /// currencies, from yen (0 places) into one with 4 at a rate with 18
    /// places, handles up to about 1.7 × 10^16 yen.
    pub fn checked_div_rate(
        self,
        from: Currency,
        rate: ExchangeRate,
        to: Currency,
        rounding: RoundingMode,
    ) -> Result<Self, MoneyError> {
        self.expect_currency(from.code())?;
        let (digits, decimal_places) = rate.normalized();
        // result = minor units × 10^(the rate's places + to's places −
        // from's places) ÷ digits.
        let exponent = i32::from(decimal_places) + i32::from(to.minor_digits())
            - i32::from(from.minor_digits());
        let minor_units = i128::from(self.minor_units);
        let digits = i128::from(digits);
        let result = if exponent >= 0 {
            minor_units
                .checked_mul(power_of_ten(exponent))
                .and_then(|numerator| rounding.divide(numerator, digits))
        } else {
            // Below 10^18 × 10^18, so it fits into 128 bits.
            rounding.divide(minor_units, digits * power_of_ten(-exponent))
        };
        Self::from_exact(result, to.code())
    }

    /// The sum of `amounts`, all of which must be in `currency`.
    ///
    /// The sum is computed exactly, so it only fails if the final result
    /// doesn't fit, not if a running total along the way wouldn't have.
    pub fn checked_sum<I>(currency: CurrencyCode, amounts: I) -> Result<Self, MoneyError>
    where
        I: IntoIterator<Item = Self>,
    {
        let mut total: i128 = 0;
        for amount in amounts {
            if amount.currency != currency {
                return Err(MoneyError::CurrencyMismatch {
                    expected: currency,
                    found: amount.currency,
                });
            }
            total = total
                .checked_add(i128::from(amount.minor_units))
                .ok_or(MoneyError::Overflow)?;
        }
        let minor_units = i64::try_from(total).map_err(|_| MoneyError::Overflow)?;
        Ok(Self::from_minor_units(minor_units, currency))
    }

    /// A whole number of minor units computed in 128 bits, or `Overflow` if
    /// there is none or it doesn't fit into 64 bits.
    fn from_exact(minor_units: Option<i128>, currency: CurrencyCode) -> Result<Self, MoneyError> {
        let minor_units = minor_units
            .and_then(|minor_units| i64::try_from(minor_units).ok())
            .ok_or(MoneyError::Overflow)?;
        Ok(Self::from_minor_units(minor_units, currency))
    }

    fn expect_currency(self, found: CurrencyCode) -> Result<(), MoneyError> {
        if self.currency == found {
            Ok(())
        } else {
            Err(MoneyError::CurrencyMismatch {
                expected: self.currency,
                found,
            })
        }
    }
}

/// 10^`exponent`, for exponents from 0 to 36: the most the conversions need,
/// with at most 18 decimal places in a rate and in a currency.
fn power_of_ten(exponent: i32) -> i128 {
    let exponent = u32::try_from(exponent).expect("never negative");
    debug_assert!(exponent <= 36, "10^{exponent} isn't needed");
    10_i128.pow(exponent)
}

/// Why a calculation with money failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoneyError {
    /// The amounts are in different currencies. Combining them needs an
    /// exchange rate (D-043).
    CurrencyMismatch {
        /// The currency the calculation is in.
        expected: CurrencyCode,
        /// The currency of the amount that didn't match.
        found: CurrencyCode,
    },
    /// The result doesn't fit into 64 bits.
    Overflow,
    /// A calculation would divide by zero, e.g. split an amount into zero
    /// shares.
    DivisionByZero,
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

    #[test]
    fn adds_and_subtracts_exactly() {
        let groceries = Money::from_minor_units(5320, EUR);
        let household = Money::from_minor_units(800, EUR);
        assert_eq!(
            groceries.checked_add(household),
            Ok(Money::from_minor_units(6120, EUR))
        );
        assert_eq!(
            groceries.checked_sub(household),
            Ok(Money::from_minor_units(4520, EUR))
        );
    }

    #[test]
    fn refuses_to_mix_currencies() {
        let euros = Money::from_minor_units(100, EUR);
        let dollars = Money::from_minor_units(100, USD);
        let mismatch = Err(MoneyError::CurrencyMismatch {
            expected: EUR,
            found: USD,
        });
        assert_eq!(euros.checked_add(dollars), mismatch);
        assert_eq!(euros.checked_sub(dollars), mismatch);
        assert_eq!(Money::checked_sum(EUR, [euros, dollars]), mismatch);
    }

    #[test]
    fn reports_overflow_instead_of_wrapping() {
        let max = Money::from_minor_units(i64::MAX, EUR);
        let min = Money::from_minor_units(i64::MIN, EUR);
        let one = Money::from_minor_units(1, EUR);
        assert_eq!(max.checked_add(one), Err(MoneyError::Overflow));
        assert_eq!(min.checked_sub(one), Err(MoneyError::Overflow));
        assert_eq!(min.checked_neg(), Err(MoneyError::Overflow));
        assert_eq!(
            Money::checked_sum(EUR, [max, one]),
            Err(MoneyError::Overflow)
        );
    }

    #[test]
    fn sum_only_fails_if_the_result_does_not_fit() {
        let max = Money::from_minor_units(i64::MAX, EUR);
        let one = Money::from_minor_units(1, EUR);
        let minus_one = Money::from_minor_units(-1, EUR);
        assert_eq!(Money::checked_sum(EUR, [max, one, minus_one]), Ok(max));
    }

    #[test]
    fn sum_of_nothing_is_zero() {
        assert_eq!(Money::checked_sum(EUR, []), Ok(Money::zero(EUR)));
    }

    #[test]
    fn takes_a_share_rounded_once() {
        let rounding = RoundingMode::HalfAwayFromZero;
        let hundred = Money::from_minor_units(10_000, EUR);
        assert_eq!(
            hundred.checked_mul_ratio(1, 3, rounding),
            Ok(Money::from_minor_units(3333, EUR))
        );
        assert_eq!(
            hundred.checked_mul_ratio(2, 3, rounding),
            Ok(Money::from_minor_units(6667, EUR))
        );
        // 19 % of €1.50 is 28.5 cents: exactly halfway.
        let price = Money::from_minor_units(150, EUR);
        let refund = Money::from_minor_units(-150, EUR);
        for (rounding, cents) in [
            (RoundingMode::HalfAwayFromZero, 29),
            (RoundingMode::HalfEven, 28),
            (RoundingMode::HalfTowardZero, 28),
        ] {
            assert_eq!(
                price.checked_mul_ratio(19, 100, rounding),
                Ok(Money::from_minor_units(cents, EUR)),
                "{rounding:?}"
            );
            assert_eq!(
                refund.checked_mul_ratio(19, 100, rounding),
                Ok(Money::from_minor_units(-cents, EUR)),
                "{rounding:?}"
            );
        }
    }

    #[test]
    fn a_ratio_overflows_only_if_the_result_does() {
        let max = Money::from_minor_units(i64::MAX, EUR);
        let rounding = RoundingMode::default();
        // Doubling and then halving stays exact, although the doubled amount
        // doesn't fit into 64 bits.
        assert_eq!(max.checked_mul_ratio(2, 2, rounding), Ok(max));
        assert_eq!(
            max.checked_mul_ratio(2, 1, rounding),
            Err(MoneyError::Overflow)
        );
        let min = Money::from_minor_units(i64::MIN, EUR);
        assert_eq!(
            min.checked_mul_ratio(-1, 1, rounding),
            Err(MoneyError::Overflow)
        );
    }

    fn currency(code: CurrencyCode) -> Currency {
        let digits = code.iso_minor_digits().expect("listed currency");
        Currency::new(code, digits).expect("valid currency")
    }

    fn rate(text: &str) -> ExchangeRate {
        text.parse().expect("valid rate")
    }

    #[test]
    fn converts_with_a_rate_and_back() {
        let rounding = RoundingMode::default();
        let (eur, usd) = (currency(EUR), currency(USD));
        let hundred_euros = Money::from_minor_units(10_000, EUR);
        let dollars = Money::from_minor_units(10_856, USD);
        assert_eq!(
            hundred_euros.checked_mul_rate(eur, rate("1.0856"), usd, rounding),
            Ok(dollars)
        );
        assert_eq!(
            dollars.checked_div_rate(usd, rate("1.0856"), eur, rounding),
            Ok(hundred_euros)
        );
        // Trailing zeros in the rate change nothing.
        assert_eq!(
            hundred_euros.checked_mul_rate(eur, rate("1.085600"), usd, rounding),
            Ok(dollars)
        );
    }

    #[test]
    fn converts_between_different_decimal_places() {
        let rounding = RoundingMode::default();
        let jpy = currency(CurrencyCode::new("JPY").expect("valid code"));
        let bhd = currency(CurrencyCode::new("BHD").expect("valid code"));
        let eur = currency(EUR);
        let euros = Money::from_minor_units(10_000, EUR);
        assert_eq!(
            euros.checked_mul_rate(eur, rate("160.25"), jpy, rounding),
            Ok(Money::from_minor_units(16_025, jpy.code()))
        );
        assert_eq!(
            Money::from_minor_units(16_025, jpy.code()).checked_div_rate(
                jpy,
                rate("160.25"),
                eur,
                rounding
            ),
            Ok(euros)
        );
        // 100.00 EUR at 0.4105 BHD is 41.050 BHD, three decimal places.
        assert_eq!(
            euros.checked_mul_rate(eur, rate("0.4105"), bhd, rounding),
            Ok(Money::from_minor_units(41_050, bhd.code()))
        );
    }

    #[test]
    fn rounds_a_conversion_once() {
        let (eur, usd) = (currency(EUR), currency(USD));
        // 0.01 EUR × 1.5 = 0.015 USD: exactly halfway.
        let cent = Money::from_minor_units(1, EUR);
        for (rounding, cents) in [
            (RoundingMode::HalfAwayFromZero, 2),
            (RoundingMode::HalfEven, 2),
            (RoundingMode::HalfTowardZero, 1),
            (RoundingMode::Floor, 1),
        ] {
            assert_eq!(
                cent.checked_mul_rate(eur, rate("1.5"), usd, rounding),
                Ok(Money::from_minor_units(cents, USD)),
                "{rounding:?}"
            );
        }
        // 1.00 USD ÷ 3 = 0.333… EUR.
        let dollar = Money::from_minor_units(100, USD);
        assert_eq!(
            dollar.checked_div_rate(usd, rate("3"), eur, RoundingMode::default()),
            Ok(Money::from_minor_units(33, EUR))
        );
    }

    #[test]
    fn a_conversion_checks_the_currency_and_overflow() {
        let rounding = RoundingMode::default();
        let (eur, usd) = (currency(EUR), currency(USD));
        let dollars = Money::from_minor_units(100, USD);
        let mismatch = Err(MoneyError::CurrencyMismatch {
            expected: USD,
            found: EUR,
        });
        assert_eq!(
            dollars.checked_mul_rate(eur, rate("2"), usd, rounding),
            mismatch
        );
        assert_eq!(
            dollars.checked_div_rate(eur, rate("2"), usd, rounding),
            mismatch
        );
        let max = Money::from_minor_units(i64::MAX, EUR);
        assert_eq!(
            max.checked_mul_rate(eur, rate("2"), usd, rounding),
            Err(MoneyError::Overflow)
        );
        assert_eq!(
            max.checked_div_rate(eur, rate("0.5"), usd, rounding),
            Err(MoneyError::Overflow)
        );
        // Halving the largest amount fits, although doubling it wouldn't.
        assert_eq!(
            max.checked_mul_rate(eur, rate("0.5"), usd, rounding),
            Ok(Money::from_minor_units(i64::MAX / 2 + 1, USD))
        );
    }

    #[test]
    fn a_ratio_never_divides_by_zero() {
        let amount = Money::from_minor_units(100, EUR);
        assert_eq!(
            amount.checked_mul_ratio(1, 0, RoundingMode::default()),
            Err(MoneyError::DivisionByZero)
        );
    }
}
