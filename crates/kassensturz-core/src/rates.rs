// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Exchange rates on a date: the rows of a book's price table (AGENTS.md §5,
//! D-028).

use crate::Date;
use crate::currency::CurrencyCode;
use crate::decimal::ExchangeRate;
use crate::id::define_id;

define_id!(
    /// Identifies an exchange rate in a book's price table.
    DatedRateId
);

/// Where an exchange rate came from, so a conversion can always say which
/// rate it used, from when and from where (AGENTS.md §5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RateSource {
    /// Entered by hand. The only source in v1: downloading rates would be
    /// network access, which needs its own approval first (D-028).
    Manual,
}

/// An exchange rate that applies on a date: one unit of `from` costs `rate`
/// units of `to`, e.g. 1 EUR = 1.0856 USD.
///
/// Converting the other way divides by the same rate
/// ([`Money::checked_div_rate`](crate::Money::checked_div_rate)), so a book
/// doesn't store the inverse as well.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DatedRate {
    /// Its ID.
    pub id: DatedRateId,
    /// The day it applies to.
    pub date: Date,
    /// The currency one unit of which is priced.
    pub from: CurrencyCode,
    /// The currency the price is in.
    pub to: CurrencyCode,
    /// How many units of `to` one unit of `from` costs.
    pub rate: ExchangeRate,
    /// Where it came from.
    pub source: RateSource,
}
