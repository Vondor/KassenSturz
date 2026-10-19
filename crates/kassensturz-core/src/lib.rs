// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! KassenSturz's domain model: money, currencies, accounts and transactions.
//!
//! This crate knows nothing about storage or user interfaces and depends on no
//! other KassenSturz crate (`docs/ARCHITECTURE.md`). It never produces
//! user-facing text: errors are structured codes with parameters, which the UI
//! turns into translated messages (D-034).

#![forbid(unsafe_code)]

mod account;
mod currency;
mod decimal;
mod id;
mod money;
mod rates;
mod rounding;
mod transaction;

pub use account::{Account, AccountId, AccountKind};
pub use currency::{Currency, CurrencyCode, CurrencyCodeError};
pub use decimal::{DecimalError, ExchangeRate, MAX_DECIMAL_PLACES, MAX_DIGITS, Price, Quantity};
pub use money::{Money, MoneyError};
pub use rates::{DatedRate, DatedRateId, RateSource};
pub use rounding::RoundingMode;
pub use transaction::{Split, SplitId, Transaction, TransactionError, TransactionId};

/// A calendar date without time or time zone, used for booking dates
/// (AGENTS.md §5, D-061).
pub use jiff::civil::Date;
