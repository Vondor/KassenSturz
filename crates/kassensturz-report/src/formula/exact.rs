// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Exact decimals for formulas (D-292, D-317).
//!
//! A value is a whole number of units in 128 bits and a number of decimal
//! places: 312.40 is 31240 units with 2 places. Adding, subtracting and
//! multiplying are exact and overflow-checked; a result that doesn't fit, or
//! would need more than [`MAX_SCALE`] decimal places, is refused, never
//! rounded on the quiet. Division is the one rounding step inside a formula:
//! to [`DIVISION_PLACES`] places, half away from zero. No floating point is
//! involved anywhere (AGENTS.md §3.1 rule 1).

use core::cmp::Ordering;

use kassensturz_core::RoundingMode;

/// The most decimal places a value keeps.
pub(crate) const MAX_SCALE: u32 = 28;

/// The decimal places a division rounds to (D-292).
pub(crate) const DIVISION_PLACES: u32 = 10;

/// A result is too large, or too precise, to be worked out exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TooLarge;

/// `10^power`.
fn ten_to(power: u32) -> Result<i128, TooLarge> {
    10i128.checked_pow(power).ok_or(TooLarge)
}

/// An exact decimal. It is always kept without trailing zeros in its
/// decimal places, so two equal values have the same units and places.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Exact {
    units: i128,
    scale: u32,
}

impl Exact {
    /// Zero.
    pub(crate) const ZERO: Self = Self { units: 0, scale: 0 };

    /// `units ÷ 10^scale`.
    pub(crate) fn new(units: i128, scale: u32) -> Result<Self, TooLarge> {
        let mut value = Self { units, scale };
        while value.scale > 0 && value.units % 10 == 0 {
            value.units /= 10;
            value.scale -= 1;
        }
        if value.scale > MAX_SCALE {
            return Err(TooLarge);
        }
        Ok(value)
    }

    /// A whole number.
    pub(crate) fn whole(number: i64) -> Self {
        Self {
            units: i128::from(number),
            scale: 0,
        }
    }

    /// Whether it is zero.
    pub(crate) fn is_zero(self) -> bool {
        self.units == 0
    }

    /// −1, 0 or 1.
    pub(crate) fn signum(self) -> i64 {
        // A sign always fits.
        self.units.signum() as i64
    }

    /// Both values' units at the larger of their scales, and that scale.
    fn aligned(self, other: Self) -> Result<(i128, i128, u32), TooLarge> {
        let scale = self.scale.max(other.scale);
        let left = self
            .units
            .checked_mul(ten_to(scale - self.scale)?)
            .ok_or(TooLarge)?;
        let right = other
            .units
            .checked_mul(ten_to(scale - other.scale)?)
            .ok_or(TooLarge)?;
        Ok((left, right, scale))
    }

    /// `self + other`.
    pub(crate) fn add(self, other: Self) -> Result<Self, TooLarge> {
        let (left, right, scale) = self.aligned(other)?;
        Self::new(left.checked_add(right).ok_or(TooLarge)?, scale)
    }

    /// `self − other`.
    pub(crate) fn sub(self, other: Self) -> Result<Self, TooLarge> {
        let (left, right, scale) = self.aligned(other)?;
        Self::new(left.checked_sub(right).ok_or(TooLarge)?, scale)
    }

    /// `self × other`, exact.
    pub(crate) fn mul(self, other: Self) -> Result<Self, TooLarge> {
        Self::new(
            self.units.checked_mul(other.units).ok_or(TooLarge)?,
            self.scale + other.scale,
        )
    }

    /// `−self`.
    pub(crate) fn neg(self) -> Result<Self, TooLarge> {
        Self::new(self.units.checked_neg().ok_or(TooLarge)?, self.scale)
    }

    /// Without its sign.
    pub(crate) fn abs(self) -> Result<Self, TooLarge> {
        Self::new(self.units.checked_abs().ok_or(TooLarge)?, self.scale)
    }

    /// `self ÷ 100`, exact: what `%` does.
    pub(crate) fn percent(self) -> Result<Self, TooLarge> {
        Self::new(self.units, self.scale + 2)
    }

    /// `self ÷ other`, rounded to [`DIVISION_PLACES`] decimal places, half
    /// away from zero (D-292). `None` if `other` is zero: a division by zero
    /// has no value.
    pub(crate) fn div(self, other: Self) -> Result<Option<Self>, TooLarge> {
        if other.is_zero() {
            return Ok(None);
        }
        // a / 10^sa ÷ b / 10^sb, in units of 10^-places:
        // a × 10^(sb + places − sa) ÷ b.
        let power = i64::from(other.scale) + i64::from(DIVISION_PLACES) - i64::from(self.scale);
        let shift = |units: i128, power: i64| -> Result<i128, TooLarge> {
            let power = u32::try_from(power).map_err(|_| TooLarge)?;
            units.checked_mul(ten_to(power)?).ok_or(TooLarge)
        };
        let (numerator, denominator) = if power >= 0 {
            (shift(self.units, power)?, other.units)
        } else {
            (self.units, shift(other.units, -power)?)
        };
        let units = RoundingMode::HalfAwayFromZero
            .divide(numerator, denominator)
            .ok_or(TooLarge)?;
        Self::new(units, DIVISION_PLACES).map(Some)
    }

    /// The value times `10^places`, rounded to a whole number in `mode`:
    /// 312.405 with 2 places is 31241 half away from zero.
    pub(crate) fn scaled(self, places: u32, mode: RoundingMode) -> Result<i128, TooLarge> {
        if self.scale <= places {
            self.units
                .checked_mul(ten_to(places - self.scale)?)
                .ok_or(TooLarge)
        } else {
            mode.divide(self.units, ten_to(self.scale - places)?)
                .ok_or(TooLarge)
        }
    }

    /// Rounded to `places` decimal places in `mode`; a negative `places`
    /// rounds to tens, hundreds and so on (−2 rounds 1250 to 1300 half away
    /// from zero).
    pub(crate) fn round(self, places: i32, mode: RoundingMode) -> Result<Self, TooLarge> {
        match u32::try_from(places) {
            Ok(places) if places >= self.scale => Ok(self),
            Ok(places) => Self::new(self.scaled(places, mode)?, places),
            Err(_) => {
                let tens = places.unsigned_abs();
                let divisor = ten_to(self.scale + tens)?;
                let rounded = mode.divide(self.units, divisor).ok_or(TooLarge)?;
                Self::new(rounded.checked_mul(ten_to(tens)?).ok_or(TooLarge)?, 0)
            }
        }
    }

    /// The value as a whole number, if it is one and fits into 64 bits.
    pub(crate) fn as_whole(self) -> Option<i64> {
        (self.scale == 0)
            .then(|| i64::try_from(self.units).ok())
            .flatten()
    }
}

impl Ord for Exact {
    fn cmp(&self, other: &Self) -> Ordering {
        // Whole parts first, then the decimal places at the larger scale;
        // neither step can overflow, whatever the units are.
        let parts = |value: &Self| {
            // Can't fail: the scale is at most MAX_SCALE.
            let one = 10i128.pow(value.scale);
            (value.units.div_euclid(one), value.units.rem_euclid(one))
        };
        let (left_whole, left_rest) = parts(self);
        let (right_whole, right_rest) = parts(other);
        let scale = self.scale.max(other.scale);
        // Each rest is below 10^its scale, so at most 10^MAX_SCALE here.
        let left_rest = left_rest * 10i128.pow(scale - self.scale);
        let right_rest = right_rest * 10i128.pow(scale - other.scale);
        left_whole
            .cmp(&right_whole)
            .then(left_rest.cmp(&right_rest))
    }
}

impl PartialOrd for Exact {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
