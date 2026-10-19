// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Rounding: how a calculation turns an exact result into whole minor units
//! (AGENTS.md §5, D-027).

/// How a calculation rounds a result that falls between two whole minor
/// units, for example a third of €1.00.
///
/// KassenSturz rounds only where a calculation requires it (conversions,
/// percentages, splitting an amount), once, at the end of the calculation, and
/// always in a mode the caller names (AGENTS.md §5). Each book stores the mode
/// its calculations use; changing it affects only later calculations, never
/// amounts already stored (D-027).
///
/// The examples round to whole numbers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RoundingMode {
    /// To the nearest; halfway cases away from zero: 2.5 → 3, −2.5 → −3.
    /// Commercial rounding, and KassenSturz's default (D-027).
    #[default]
    HalfAwayFromZero,
    /// To the nearest; halfway cases to the even neighbor: 2.5 → 2, 3.5 → 4.
    /// Also called banker's rounding.
    HalfEven,
    /// To the nearest; halfway cases toward zero: 2.5 → 2, −2.5 → −2.
    HalfTowardZero,
    /// Toward zero, i.e. cut off: 2.9 → 2, −2.9 → −2.
    TowardZero,
    /// Away from zero: 2.1 → 3, −2.1 → −3.
    AwayFromZero,
    /// Down, toward negative infinity: 2.9 → 2, −2.1 → −3.
    Floor,
    /// Up, toward positive infinity: 2.1 → 3, −2.9 → −2.
    Ceiling,
}

impl RoundingMode {
    /// Every rounding mode, the default first.
    pub const ALL: [Self; 7] = [
        Self::HalfAwayFromZero,
        Self::HalfEven,
        Self::HalfTowardZero,
        Self::TowardZero,
        Self::AwayFromZero,
        Self::Floor,
        Self::Ceiling,
    ];

    /// `numerator ÷ denominator`, rounded to a whole number in this mode.
    ///
    /// Exact: the division is done in integers, so nothing but the result is
    /// rounded. `None` if `denominator` is zero, and for `i128::MIN ÷ -1`,
    /// whose result doesn't fit.
    pub(crate) fn divide(self, numerator: i128, denominator: i128) -> Option<i128> {
        // Rounded toward zero; the exact result lies between `quotient` and
        // the next whole number away from zero.
        let quotient = numerator.checked_div(denominator)?;
        let remainder = numerator.checked_rem(denominator)?;
        if remainder == 0 {
            return Some(quotient);
        }
        let negative = (numerator < 0) != (denominator < 0);
        // Twice the remainder against the divisor says whether the exact
        // result is short of, at or past the halfway point. Neither overflows:
        // |remainder| < |denominator| ≤ 2^127.
        let twice_remainder = remainder.unsigned_abs() * 2;
        let divisor = denominator.unsigned_abs();
        let away_from_zero = match self {
            Self::HalfAwayFromZero => twice_remainder >= divisor,
            Self::HalfEven => {
                twice_remainder > divisor || (twice_remainder == divisor && quotient % 2 != 0)
            }
            Self::HalfTowardZero => twice_remainder > divisor,
            Self::TowardZero => false,
            Self::AwayFromZero => true,
            Self::Floor => negative,
            Self::Ceiling => !negative,
        };
        // Can't overflow: with a remainder, |denominator| ≥ 2, so |quotient| is
        // at most half of |numerator|.
        Some(match (away_from_zero, negative) {
            (false, _) => quotient,
            (true, false) => quotient + 1,
            (true, true) => quotient - 1,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use RoundingMode::*;

    #[test]
    fn all_has_every_mode_once() {
        // Adding a mode breaks this match until it's handled here, which is
        // the reminder to add it to ALL, too.
        let position = |mode| match mode {
            HalfAwayFromZero => 0,
            HalfEven => 1,
            HalfTowardZero => 2,
            TowardZero => 3,
            AwayFromZero => 4,
            Floor => 5,
            Ceiling => 6,
        };
        for (index, mode) in RoundingMode::ALL.into_iter().enumerate() {
            assert_eq!(position(mode), index, "{mode:?}");
        }
    }

    #[test]
    fn the_default_is_half_away_from_zero() {
        assert_eq!(RoundingMode::default(), HalfAwayFromZero);
        assert_eq!(RoundingMode::ALL[0], RoundingMode::default());
    }

    #[test]
    fn rounds_like_the_textbook_table() {
        // Tenths and what each mode makes of them, in the order of ALL:
        // half away from zero, half even, half toward zero, toward zero,
        // away from zero, floor, ceiling.
        let table: [(i128, [i128; 7]); 10] = [
            (55, [6, 6, 5, 5, 6, 5, 6]),
            (25, [3, 2, 2, 2, 3, 2, 3]),
            (16, [2, 2, 2, 1, 2, 1, 2]),
            (11, [1, 1, 1, 1, 2, 1, 2]),
            (10, [1, 1, 1, 1, 1, 1, 1]),
            (-10, [-1, -1, -1, -1, -1, -1, -1]),
            (-11, [-1, -1, -1, -1, -2, -2, -1]),
            (-16, [-2, -2, -2, -1, -2, -2, -1]),
            (-25, [-3, -2, -2, -2, -3, -3, -2]),
            (-55, [-6, -6, -5, -5, -6, -6, -5]),
        ];
        for (tenths, expected) in table {
            for (mode, expected) in RoundingMode::ALL.into_iter().zip(expected) {
                assert_eq!(
                    mode.divide(tenths, 10),
                    Some(expected),
                    "{tenths}/10 in {mode:?}"
                );
                // The same value with the signs moved to the denominator.
                assert_eq!(
                    mode.divide(-tenths, -10),
                    Some(expected),
                    "-{tenths}/-10 in {mode:?}"
                );
            }
        }
    }

    #[test]
    fn exact_results_are_never_rounded() {
        for mode in RoundingMode::ALL {
            assert_eq!(mode.divide(0, 7), Some(0));
            assert_eq!(mode.divide(42, 7), Some(6));
            assert_eq!(mode.divide(-42, 7), Some(-6));
            assert_eq!(mode.divide(i128::MIN, 1), Some(i128::MIN));
            assert_eq!(mode.divide(i128::MAX, -1), Some(-i128::MAX));
        }
    }

    #[test]
    fn handles_the_extremes_of_128_bits() {
        // i128::MAX is odd and i128::MIN even, so halving them is exact or
        // exactly halfway.
        assert_eq!(
            HalfAwayFromZero.divide(i128::MAX, 2),
            Some(i128::MAX / 2 + 1)
        );
        assert_eq!(HalfTowardZero.divide(i128::MAX, 2), Some(i128::MAX / 2));
        assert_eq!(Floor.divide(i128::MIN, 2), Some(i128::MIN / 2));
        // The largest possible remainder: just short of halfway, and past it.
        assert_eq!(HalfAwayFromZero.divide(i128::MAX - 1, i128::MAX), Some(1));
        assert_eq!(HalfAwayFromZero.divide(i128::MIN, i128::MAX), Some(-1));
        assert_eq!(Ceiling.divide(i128::MIN + 1, i128::MAX), Some(-1));
        assert_eq!(TowardZero.divide(i128::MAX, i128::MIN), Some(0));
        assert_eq!(AwayFromZero.divide(i128::MAX, i128::MIN), Some(-1));
    }

    #[test]
    fn refuses_what_has_no_result() {
        for mode in RoundingMode::ALL {
            assert_eq!(mode.divide(1, 0), None, "{mode:?}");
            assert_eq!(mode.divide(i128::MIN, -1), None, "{mode:?}");
        }
    }
}
