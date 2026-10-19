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
mod id;
mod money;
mod transaction;

pub use account::{Account, AccountId, AccountKind};
pub use currency::{CurrencyCode, CurrencyCodeError};
pub use money::{Money, MoneyError};
pub use transaction::{Split, SplitId, Transaction, TransactionError, TransactionId};

/// A calendar date without time or time zone, used for booking dates
/// (AGENTS.md §5, D-061).
pub use jiff::civil::Date;
