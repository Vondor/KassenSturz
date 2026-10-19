// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Property-based tests for rounding (AGENTS.md §9, D-027).
//!
//! `Money::checked_mul_ratio` is compared with a reference that rounds by a
//! different route: it finds the whole numbers on either side of the exact
//! result with Euclidean division, then picks one by comparing the exact
//! distances to both.

use kassensturz_core::{CurrencyCode, Money, MoneyError, RoundingMode};
use proptest::prelude::*;

fn currency() -> impl Strategy<Value = CurrencyCode> {
    prop::sample::select(vec!["EUR", "USD", "JPY", "BHD"])
        .prop_map(|code| CurrencyCode::new(code).expect("valid code"))
}

fn mode() -> impl Strategy<Value = RoundingMode> {
    prop::sample::select(RoundingMode::ALL.to_vec())
}

/// Mostly everyday amounts, sometimes any 64-bit amount.
fn amount() -> impl Strategy<Value = i64> {
    prop_oneof![
        3 => -1_000_000_000_000_i64..=1_000_000_000_000,
        1 => any::<i64>(),
    ]
}

/// Mostly small numerators such as percentages, sometimes any.
fn numerator() -> impl Strategy<Value = i64> {
    prop_oneof![3 => -1000_i64..=1000, 1 => any::<i64>()]
}

/// Any denominator but zero, mostly small ones.
fn denominator() -> impl Strategy<Value = i64> {
    prop_oneof![
        3 => 1_i64..=1000,
        1 => -1000_i64..=-1,
        1 => any::<i64>().prop_filter("not zero", |&d| d != 0),
    ]
}

/// `numerator ÷ denominator` rounded in `mode`, computed independently of
/// KassenSturz's own code.
fn reference(numerator: i128, denominator: i128, mode: RoundingMode) -> i128 {
    // Same value, positive denominator.
    let (numerator, denominator) = if denominator < 0 {
        (-numerator, -denominator)
    } else {
        (numerator, denominator)
    };
    let floor = numerator.div_euclid(denominator);
    // The exact result is floor + rest ÷ denominator, with 0 ≤ rest < denominator.
    let rest = numerator.rem_euclid(denominator);
    if rest == 0 {
        return floor;
    }
    let ceiling = floor + 1;
    let negative = floor < 0;
    let (toward_zero, away_from_zero) = if negative {
        (ceiling, floor)
    } else {
        (floor, ceiling)
    };
    // Distances to floor and ceiling, both times `denominator`.
    let (to_floor, to_ceiling) = (rest, denominator - rest);
    match mode {
        RoundingMode::Floor => floor,
        RoundingMode::Ceiling => ceiling,
        RoundingMode::TowardZero => toward_zero,
        RoundingMode::AwayFromZero => away_from_zero,
        _ if to_floor < to_ceiling => floor,
        _ if to_ceiling < to_floor => ceiling,
        // Exactly halfway.
        RoundingMode::HalfAwayFromZero => away_from_zero,
        RoundingMode::HalfTowardZero => toward_zero,
        RoundingMode::HalfEven if floor.rem_euclid(2) == 0 => floor,
        RoundingMode::HalfEven => ceiling,
    }
}

/// The exact result as `Money`, or `Overflow` if it doesn't fit into 64 bits.
fn exact(minor_units: i128, currency: CurrencyCode) -> Result<Money, MoneyError> {
    i64::try_from(minor_units)
        .map(|minor_units| Money::from_minor_units(minor_units, currency))
        .map_err(|_| MoneyError::Overflow)
}

fn rounds_halfway_cases(mode: RoundingMode) -> bool {
    matches!(
        mode,
        RoundingMode::HalfAwayFromZero | RoundingMode::HalfEven | RoundingMode::HalfTowardZero
    )
}

proptest! {
    #[test]
    fn matches_the_reference(
        a in amount(),
        n in numerator(),
        d in denominator(),
        mode in mode(),
        currency in currency(),
    ) {
        let result = Money::from_minor_units(a, currency).checked_mul_ratio(n, d, mode);
        let expected = reference(i128::from(a) * i128::from(n), i128::from(d), mode);
        prop_assert_eq!(result, exact(expected, currency));
    }

    #[test]
    fn lands_next_to_the_exact_result(
        a in amount(),
        n in numerator(),
        d in denominator(),
        mode in mode(),
        currency in currency(),
    ) {
        if let Ok(result) = Money::from_minor_units(a, currency).checked_mul_ratio(n, d, mode) {
            // How far the result is from the exact value, times |d|.
            let exact_times_d = i128::from(a) * i128::from(n);
            let distance = (exact_times_d - i128::from(result.minor_units()) * i128::from(d)).abs();
            let d = i128::from(d).abs();
            prop_assert!(distance < d, "more than one minor unit off");
            if rounds_halfway_cases(mode) {
                prop_assert!(2 * distance <= d, "not the nearest whole number");
            }
        }
    }

    #[test]
    fn halfway_cases_follow_the_mode(
        whole in -1_000_000_000_000_i64..=1_000_000_000_000,
        half in 1_i64..=1000,
        currency in currency(),
    ) {
        // Random ratios rarely land exactly halfway, so this builds them:
        // (whole × d + half) ÷ d with d = 2 × half is whole + ½.
        let d = 2 * half;
        let amount = Money::from_minor_units(whole * d + half, currency);
        let (toward_zero, away_from_zero) = if whole < 0 {
            (whole + 1, whole)
        } else {
            (whole, whole + 1)
        };
        let even = if whole % 2 == 0 { whole } else { whole + 1 };
        for (mode, expected) in [
            (RoundingMode::HalfAwayFromZero, away_from_zero),
            (RoundingMode::HalfEven, even),
            (RoundingMode::HalfTowardZero, toward_zero),
        ] {
            let expected = Ok(Money::from_minor_units(expected, currency));
            prop_assert_eq!(amount.checked_mul_ratio(1, d, mode), expected, "{:?}", mode);
            prop_assert_eq!(amount.checked_mul_ratio(-1, -d, mode), expected, "{:?}", mode);
        }
    }

    #[test]
    fn exact_results_are_never_rounded(
        a in amount(),
        d in denominator(),
        mode in mode(),
        currency in currency(),
    ) {
        // a × d ÷ d is exactly a, even where a × d needs more than 64 bits.
        let amount = Money::from_minor_units(a, currency);
        prop_assert_eq!(amount.checked_mul_ratio(d, d, mode), Ok(amount));
        prop_assert_eq!(amount.checked_mul_ratio(1, 1, mode), Ok(amount));
    }

    #[test]
    fn negative_amounts_round_like_positive_ones(
        a in (i64::MIN + 1)..=i64::MAX,
        n in numerator(),
        d in denominator(),
        currency in currency(),
    ) {
        let positive = Money::from_minor_units(a, currency);
        let negative = Money::from_minor_units(-a, currency);
        // Every mode but floor and ceiling ignores the sign; floor of a
        // negative value mirrors ceiling of the positive one.
        let mirrored = [
            (RoundingMode::HalfAwayFromZero, RoundingMode::HalfAwayFromZero),
            (RoundingMode::HalfEven, RoundingMode::HalfEven),
            (RoundingMode::HalfTowardZero, RoundingMode::HalfTowardZero),
            (RoundingMode::TowardZero, RoundingMode::TowardZero),
            (RoundingMode::AwayFromZero, RoundingMode::AwayFromZero),
            (RoundingMode::Floor, RoundingMode::Ceiling),
            (RoundingMode::Ceiling, RoundingMode::Floor),
        ];
        for (mode, mirror) in mirrored {
            // Only i64::MIN can't be negated; then one side overflows.
            if let (Ok(up), Ok(down)) = (
                positive.checked_mul_ratio(n, d, mode),
                negative.checked_mul_ratio(n, d, mirror),
            ) {
                prop_assert_eq!(down.minor_units(), -up.minor_units(), "{:?}", mode);
            }
        }
    }

    #[test]
    fn larger_amounts_never_give_smaller_results(
        a in amount(),
        b in amount(),
        n in 0..=i64::MAX,
        d in 1..=i64::MAX,
        mode in mode(),
        currency in currency(),
    ) {
        let (low, high) = (a.min(b), a.max(b));
        let low = Money::from_minor_units(low, currency).checked_mul_ratio(n, d, mode);
        let high = Money::from_minor_units(high, currency).checked_mul_ratio(n, d, mode);
        if let (Ok(low), Ok(high)) = (low, high) {
            prop_assert!(low.minor_units() <= high.minor_units());
        }
    }

    #[test]
    fn dividing_by_zero_is_refused(
        a in amount(),
        n in numerator(),
        mode in mode(),
        currency in currency(),
    ) {
        prop_assert_eq!(
            Money::from_minor_units(a, currency).checked_mul_ratio(n, 0, mode),
            Err(MoneyError::DivisionByZero)
        );
    }
}
