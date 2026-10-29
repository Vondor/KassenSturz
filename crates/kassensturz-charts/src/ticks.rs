// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Marks for the value axis, worked out in whole minor units.

use kassensturz_core::{CurrencyCode, Money};
use kassensturz_report::TextFormat;

use crate::{ChartError, Tick};

/// About how many marks an axis gets.
const WANTED: i64 = 5;

/// The smallest of 1, 2 or 5 times a power of ten that is at least `wanted`.
fn nice_step(wanted: i64) -> Option<i64> {
    let mut power: i64 = 1;
    loop {
        for factor in [1, 2, 5] {
            let step = power.checked_mul(factor)?;
            if step >= wanted {
                return Some(step);
            }
        }
        power = power.checked_mul(10)?;
    }
}

/// Marks from `low` to `high` (which are widened to include zero and to land
/// on a mark), at least `minimum_step` apart. `minimum_step` is one major unit
/// of the currency, so no mark falls between two whole euros.
pub(crate) fn ticks(
    low: i64,
    high: i64,
    minimum_step: i64,
    currency: CurrencyCode,
    format: &impl TextFormat,
) -> Result<Vec<Tick>, ChartError> {
    let low = low.min(0);
    let high = high.max(0);
    let span = high.checked_sub(low).ok_or(ChartError::Overflow)?;
    let step = nice_step((span / WANTED).max(minimum_step).max(1)).ok_or(ChartError::Overflow)?;
    // Round down to a mark below `low`, up to one above `high`.
    let first = low
        .div_euclid(step)
        .checked_mul(step)
        .ok_or(ChartError::Overflow)?;
    let last = high
        .checked_add(step - 1)
        .ok_or(ChartError::Overflow)?
        .div_euclid(step)
        .checked_mul(step)
        .ok_or(ChartError::Overflow)?;
    // An axis has at least two marks, also when there is nothing to show.
    let last = if last == first {
        first.checked_add(step).ok_or(ChartError::Overflow)?
    } else {
        last
    };
    let mut ticks = Vec::new();
    let mut at = first;
    loop {
        ticks.push(Tick {
            minor_units: at,
            label: format.amount(Money::from_minor_units(at, currency)),
        });
        if at >= last {
            break;
        }
        at = at.checked_add(step).ok_or(ChartError::Overflow)?;
    }
    Ok(ticks)
}
