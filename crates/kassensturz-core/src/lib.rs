// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! KassenSturz's domain model: money, currencies, accounts, transactions and
//! Rules.
//!
//! This crate knows nothing about storage or user interfaces and depends on no
//! other KassenSturz crate (`docs/ARCHITECTURE.md`). It never produces
//! user-facing text: errors are structured codes with parameters
//! ([`ErrorReport`]), which the UI turns into translated messages (D-034).

#![forbid(unsafe_code)]

mod account;
mod bank_link;
mod changeover;
mod currency;
mod decimal;
mod details;
mod error;
mod id;
mod money;
mod rates;
mod rounding;
mod rules;
mod transaction;
mod transfer;
mod withdrawn;

pub use account::{Account, AccountId, AccountKind, AccountType};
pub use bank_link::BankLink;
pub use changeover::{Changeover, ConvertedPart};
pub use currency::{Currency, CurrencyCode, CurrencyCodeError};
pub use decimal::{DecimalError, ExchangeRate, MAX_DECIMAL_PLACES, MAX_DIGITS, Price, Quantity};
pub use details::{Institution, InstitutionId, Payee, PayeeId, PayeeRecognition, Tag, TagId};
pub use error::{ErrorParam, ErrorReport, Report};
pub use money::{Money, MoneyError};
pub use rates::{DatedRate, DatedRateId, RateSource};
pub use rounding::RoundingMode;
pub use rules::{
    Actions, AmountTest, BookTo, Condition, MatchMode, Rule, RuleError, RuleFacts, RuleId,
    RuleNode, RuleOutcome, RuleSet, TextField, TextOperator,
};
pub use transaction::{
    Attachment, ForeignAmount, ImportedDetails, OriginalAmount, PartStatus, Split, SplitId,
    Transaction, TransactionError, TransactionId,
};
pub use transfer::{
    MAX_BUSINESS_DAYS, TransferError, TransferMatch, TransferSide, business_days_apart,
    find_transfers, join_transfer,
};
pub use withdrawn::Withdrawn;

/// A calendar date without time or time zone, used for booking dates
/// (AGENTS.md §5, D-061).
pub use jiff::civil::Date;

/// A moment in time, kept in UTC, e.g. since when a book is open on another
/// computer (D-038). The UI shows it in the user's time zone.
pub use jiff::Timestamp;
