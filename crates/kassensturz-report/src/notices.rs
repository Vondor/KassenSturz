// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which transactions that *Update all accounts* booked get a notice
//! (M6d.7, D-449): those no Rule sorted, and those whose amount is unusual
//! for their category. Everything else is booked silently.
//!
//! "Unusual" is worked out in whole minor units, never with floating point
//! (AGENTS.md §3.1 rule 1): the transactions already booked in the same
//! category (with its subcategories), currency and direction in the
//! [`LOOK_BACK_DAYS`] days before the new one are the comparison. With fewer
//! than [`MIN_EARLIER`] of them nothing is unusual. Otherwise their median
//! (the lower of the two middle ones for an even count, so nothing is
//! rounded) is the usual amount, and the new amount is unusual when it is at
//! least [`FACTOR`] times the usual one or at most a [`FACTOR`]th of it.
//!
//! The words never say fraud or give advice (AGENTS.md §3.1 rule 11): the
//! app says what is unusual and asks the user to check it.

use std::collections::{HashMap, HashSet};

use jiff::ToSpan;
use kassensturz_core::{AccountId, Date, Money, TransactionId};
use kassensturz_store::Book;

use crate::{Filter, Period, ReportError, Source};

/// How many times the usual amount, or which fraction of it, makes an
/// amount unusual.
pub const FACTOR: i64 = 3;

/// How many earlier transactions a category needs before anything in it is
/// called unusual.
pub const MIN_EARLIER: usize = 5;

/// How many days before a new transaction the earlier ones are taken from.
pub const LOOK_BACK_DAYS: i32 = 365;

/// Why a booked transaction gets a notice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// No Rule sorted it: a part of it has no category.
    Unsorted,
    /// Its amount in a category is unusual for it.
    Unusual {
        /// The category of the part that is unusual.
        category: AccountId,
        /// How much money it is, whichever way it went (never negative).
        amount: Money,
        /// The category's usual amount, the same way (never negative).
        usual: Money,
    },
}

/// A booked transaction the user is asked to look at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Notice {
    /// The transaction.
    pub transaction: TransactionId,
    /// Why the user is asked to look at it.
    pub reason: Reason,
}

/// The usual amount of `earlier` (each a size, never negative): their
/// median, the lower middle one for an even count; `None` with fewer than
/// [`MIN_EARLIER`]. The order of `earlier` does not matter.
pub fn usual_amount(earlier: &[i64]) -> Option<i64> {
    if earlier.len() < MIN_EARLIER {
        return None;
    }
    let mut sorted = earlier.to_vec();
    sorted.sort_unstable();
    Some(sorted[(sorted.len() - 1) / 2])
}

/// Whether `amount` is unusual against `usual` (both sizes, never
/// negative): at least [`FACTOR`] times it, or at most a [`FACTOR`]th of it.
/// Worked out in `i128`, so no product overflows.
pub fn is_unusual(amount: i64, usual: i64) -> bool {
    let (amount, usual, factor) = (i128::from(amount), i128::from(usual), i128::from(FACTOR));
    amount >= factor * usual || factor * amount <= usual
}

/// The days to read for the notices of transactions booked from `first` to
/// `last`: [`LOOK_BACK_DAYS`] days before the first, up to the last.
pub fn look_back(first: Date, last: Date) -> Result<Period, ReportError> {
    let start = first
        .checked_sub(LOOK_BACK_DAYS.days())
        .unwrap_or(Date::MIN);
    Period::new(start, last)
}

/// The notices for the transactions `new` in `book`, which were just booked.
/// The others of `new` never count as earlier ones: they are not what the
/// book "already had". One notice per transaction at most: *unsorted* comes
/// first, then the first unusual part.
pub fn notices(book: &Book, new: &[TransactionId]) -> Result<Vec<Notice>, ReportError> {
    if new.is_empty() {
        return Ok(Vec::new());
    }
    let days: Vec<Date> = new
        .iter()
        .map(|&id| book.transaction(id).map(|t| t.booking_date()))
        .collect::<Result<_, _>>()?;
    let first = days.iter().min().copied().expect("not empty");
    let last = days.iter().max().copied().expect("not empty");
    let source = Source::read(book, look_back(first, last)?)?;
    notices_in(&source, new)
}

/// One part's direction: what kind of money it is and whether it adds to
/// it (money out for spending, money in for income) or takes from it.
type Direction = (crate::source::Kind, bool);

/// [`notices`] on a `Source` that covers the days.
pub(crate) fn notices_in(
    source: &Source,
    new: &[TransactionId],
) -> Result<Vec<Notice>, ReportError> {
    let parts = source.parts(source.covers(), &Filter::default())?;
    let transactions = source.transactions();
    let new_set: HashSet<TransactionId> = new.iter().copied().collect();
    let mut inside: HashMap<AccountId, HashSet<AccountId>> = HashMap::new();
    let mut found = Vec::new();
    for &id in new {
        let Some(index) = transactions.iter().position(|t| t.id() == id) else {
            continue;
        };
        let own: Vec<_> = parts
            .iter()
            .filter(|part| part.transaction == index)
            .collect();
        if own.iter().any(|part| part.category.is_none()) {
            found.push(Notice {
                transaction: id,
                reason: Reason::Unsorted,
            });
            continue;
        }
        for part in own {
            let Some(category) = part.category else {
                continue;
            };
            let below = inside.entry(category).or_insert_with(|| {
                source
                    .inside(&HashSet::from([category]))
                    .into_keys()
                    .collect()
            });
            let direction: Direction = (part.kind, !part.amount.is_negative());
            let from = part
                .day
                .checked_sub(LOOK_BACK_DAYS.days())
                .unwrap_or(Date::MIN);
            let earlier: Vec<i64> = parts
                .iter()
                .filter(|other| {
                    other.category.is_some_and(|c| below.contains(&c))
                        && other.currency == part.currency
                        && (other.kind, !other.amount.is_negative()) == direction
                        && other.day >= from
                        && other.day < part.day
                        && !new_set.contains(&transactions[other.transaction].id())
                })
                .map(|other| size(other.amount))
                .collect();
            let amount = size(part.amount);
            if let Some(usual) = usual_amount(&earlier)
                && is_unusual(amount, usual)
            {
                found.push(Notice {
                    transaction: id,
                    reason: Reason::Unusual {
                        category,
                        amount: Money::from_minor_units(amount, part.currency),
                        usual: Money::from_minor_units(usual, part.currency),
                    },
                });
                break;
            }
        }
    }
    Ok(found)
}

/// How much money an amount is, whichever way it went. `i64::MIN` has no
/// positive twin and is taken as the largest size.
fn size(amount: Money) -> i64 {
    amount.minor_units().checked_abs().unwrap_or(i64::MAX)
}

#[cfg(test)]
#[path = "notices_tests.rs"]
mod tests;
