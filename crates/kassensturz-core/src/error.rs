// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Error reports: what the UI needs to show an error in the user's language
//! (D-034).
//!
//! KassenSturz's crates never produce user-facing text. Every error turns
//! into an [`ErrorReport`]: a stable code, named and typed parameters and,
//! for an error caused by another one, that cause. The UI keeps one
//! translated message per code and fills in the parameters, formatting
//! amounts and dates for the user's locale (D-002, D-065).
//! `docs/ERROR_CODES.md` lists every code with its parameters.

use jiff::Timestamp;
use jiff::civil::Date;

use crate::account::{AccountId, AccountKind, AccountType};
use crate::currency::{CurrencyCode, CurrencyCodeError};
use crate::decimal::{DecimalError, MAX_DECIMAL_PLACES, MAX_DIGITS};
use crate::money::{Money, MoneyError};
use crate::rates::DatedRateId;
use crate::rules::{RuleError, RuleId};
use crate::transaction::{SplitId, TransactionError, TransactionId};
use crate::transfer::TransferError;

/// An error as the UI shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorReport {
    /// Which error it is, e.g. `money.overflow`: lowercase words joined by
    /// hyphens, in parts separated by dots, the first part naming where the
    /// error comes from. A code keeps its meaning: one that is no longer
    /// needed is retired, never given to another error.
    pub code: &'static str,
    /// The values the message refers to, by name, always in the same order
    /// for a code. A parameter that `docs/ERROR_CODES.md` marks as optional
    /// is left out when it isn't known.
    pub params: Vec<(&'static str, ErrorParam)>,
    /// The error that caused this one, e.g. why the backup before an update
    /// failed. The UI can show it as a detail.
    pub cause: Option<Box<ErrorReport>>,
}

impl ErrorReport {
    /// A report with the code, no parameters and no cause.
    pub const fn new(code: &'static str) -> Self {
        Self {
            code,
            params: Vec::new(),
            cause: None,
        }
    }

    /// The report with one more parameter.
    #[must_use]
    pub fn with(mut self, name: &'static str, value: ErrorParam) -> Self {
        self.params.push((name, value));
        self
    }

    /// The report with the error that caused it.
    #[must_use]
    pub fn caused_by(mut self, cause: Self) -> Self {
        self.cause = Some(Box::new(cause));
        self
    }

    /// The parameter with this name, if the report has it.
    pub fn param(&self, name: &str) -> Option<&ErrorParam> {
        self.params
            .iter()
            .find(|(param, _)| *param == name)
            .map(|(_, value)| value)
    }
}

/// A value an error message refers to. Values stay typed, so the UI can
/// format amounts and dates for the user's locale (D-002, D-065) and show an
/// account's name instead of its ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorParam {
    /// An amount of money.
    Money(Money),
    /// A currency.
    Currency(CurrencyCode),
    /// A calendar date.
    Date(Date),
    /// A moment in time, which the UI shows in the user's time zone.
    Timestamp(Timestamp),
    /// A whole number, such as a count or a limit.
    Number(i64),
    /// An account or a category.
    Account(AccountId),
    /// What an account stands for: money the user has, a category and so on.
    AccountKind(AccountKind),
    /// The sort of account: checking, savings, cash or credit card.
    AccountType(AccountType),
    /// A transaction.
    Transaction(TransactionId),
    /// A part of a transaction.
    Split(SplitId),
    /// An exchange rate.
    Rate(DatedRateId),
    /// A Rule, which the UI shows by its name.
    Rule(RuleId),
    /// One word from a fixed list that `docs/ERROR_CODES.md` gives with the
    /// code, e.g. what went wrong in the file system. The UI translates it
    /// like a code.
    Keyword(&'static str),
    /// Text as it was found in a book or a file, e.g. a damaged date or the
    /// name of the computer a book is open on. It is data, never part of a
    /// message, and may contain anything, so the UI shows it as plain text
    /// (AGENTS.md §8).
    Text(String),
}

impl ErrorParam {
    /// A count, such as how many parts there were.
    pub fn count(count: usize) -> Self {
        Self::Number(i64::try_from(count).unwrap_or(i64::MAX))
    }
}

/// An error the UI can show the user.
pub trait Report {
    /// The code, parameters and cause the UI needs to show this error.
    fn report(&self) -> ErrorReport;
}

impl Report for MoneyError {
    fn report(&self) -> ErrorReport {
        match *self {
            Self::CurrencyMismatch { expected, found } => {
                ErrorReport::new("money.currency-mismatch")
                    .with("expected", ErrorParam::Currency(expected))
                    .with("found", ErrorParam::Currency(found))
            }
            Self::Overflow => ErrorReport::new("money.overflow"),
            Self::DivisionByZero => ErrorReport::new("money.division-by-zero"),
        }
    }
}

impl Report for DecimalError {
    fn report(&self) -> ErrorReport {
        match self {
            Self::Malformed => ErrorReport::new("decimal.malformed"),
            Self::TooManyDigits => ErrorReport::new("decimal.too-many-digits")
                .with("max", ErrorParam::Number(MAX_DIGITS.into())),
            Self::TooManyDecimalPlaces => ErrorReport::new("decimal.too-many-decimal-places")
                .with("max", ErrorParam::Number(MAX_DECIMAL_PLACES.into())),
            Self::NotPositive => ErrorReport::new("decimal.not-positive"),
            Self::Negative => ErrorReport::new("decimal.negative"),
        }
    }
}

impl Report for CurrencyCodeError {
    fn report(&self) -> ErrorReport {
        match self {
            Self::Malformed => ErrorReport::new("currency.malformed-code"),
        }
    }
}

impl Report for TransactionError {
    fn report(&self) -> ErrorReport {
        let split = |code, id| ErrorReport::new(code).with("split", ErrorParam::Split(id));
        match *self {
            Self::TooFewSplits { found } => ErrorReport::new("transaction.too-few-splits")
                .with("found", ErrorParam::count(found)),
            Self::MixedCurrencies { expected, found } => {
                ErrorReport::new("transaction.mixed-currencies")
                    .with("expected", ErrorParam::Currency(expected))
                    .with("found", ErrorParam::Currency(found))
            }
            Self::DuplicateSplitId(id) => split("transaction.duplicate-split-id", id),
            Self::Unbalanced { total } => {
                ErrorReport::new("transaction.unbalanced").with("total", ErrorParam::Money(total))
            }
            Self::Overflow => ErrorReport::new("transaction.overflow"),
            Self::UnknownSplit(id) => split("transaction.unknown-split", id),
            Self::ImportedWithoutAccount(id) => split("transaction.imported-without-account", id),
            Self::ForeignWithoutAccount(id) => split("transaction.foreign-without-account", id),
            Self::ForeignInTransactionCurrency(id) => {
                split("transaction.foreign-in-transaction-currency", id)
            }
            Self::ForeignSignMismatch(id) => split("transaction.foreign-sign-mismatch", id),
            Self::RateCurrencyMismatch(id) => split("transaction.rate-currency-mismatch", id),
            Self::OriginalInPartCurrency(id) => split("transaction.original-in-part-currency", id),
            Self::OriginalSignMismatch(id) => split("transaction.original-sign-mismatch", id),
            Self::OriginalWithForeignRate(id) => {
                split("transaction.original-with-foreign-rate", id)
            }
        }
    }
}

impl Report for TransferError {
    fn report(&self) -> ErrorReport {
        match *self {
            Self::NotASide(transaction) => ErrorReport::new("transfer.not-a-side")
                .with("transaction", ErrorParam::Transaction(transaction)),
            Self::SameTransaction(transaction) => ErrorReport::new("transfer.same-transaction")
                .with("transaction", ErrorParam::Transaction(transaction)),
            Self::SameAccount(account) => ErrorReport::new("transfer.same-account")
                .with("account", ErrorParam::Account(account)),
            Self::AmountsDiffer { first, second } => ErrorReport::new("transfer.amounts-differ")
                .with("first", ErrorParam::Money(first))
                .with("second", ErrorParam::Money(second)),
            // A transaction's own error says what is wrong.
            Self::Transaction(error) => error.report(),
        }
    }
}

impl Report for RuleError {
    fn report(&self) -> ErrorReport {
        let rule = |code, id| ErrorReport::new(code).with("rule", ErrorParam::Rule(id));
        // Counted from 1, as the user sees the list of conditions.
        let condition = |code, id, index: usize| {
            rule(code, id).with("condition", ErrorParam::count(index.saturating_add(1)))
        };
        match *self {
            Self::NoConditions(id) => rule("rule.no-conditions", id),
            Self::EmptyText {
                rule,
                condition: index,
            } => condition("rule.empty-text", rule, index),
            Self::BadPattern {
                rule,
                condition: index,
            } => condition("rule.bad-pattern", rule, index),
            Self::AmountRangeReversed {
                rule,
                condition: index,
            } => condition("rule.amount-range-reversed", rule, index),
            Self::AmountRangeCurrencies {
                rule,
                condition: index,
            } => condition("rule.amount-range-currencies", rule, index),
            Self::DateRangeReversed {
                rule,
                condition: index,
            } => condition("rule.date-range-reversed", rule, index),
            Self::EmptyPayee(id) => rule("rule.empty-payee", id),
        }
    }
}

/// Whether `code` has the shape of an error code (`docs/ERROR_CODES.md`):
/// lowercase words joined by hyphens, in at least two parts separated by
/// dots, at most 64 characters long.
pub fn is_error_code(code: &str) -> bool {
    code.len() <= 64
        && code.split('.').count() >= 2
        && code.split('.').all(|part| {
            part.starts_with(|c: char| c.is_ascii_lowercase())
                && !part.ends_with('-')
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_have_the_listed_shape() {
        assert!(is_error_code("ai.timeout"));
        assert!(is_error_code("ai-tools.bad-argument"));
        assert!(is_error_code("store.corrupt.missing-table"));
        assert!(!is_error_code("ai"));
        assert!(!is_error_code("AI.timeout"));
        assert!(!is_error_code("ai.time out"));
        assert!(!is_error_code("ai.-x"));
        assert!(!is_error_code("ai.x-"));
        assert!(!is_error_code(&format!("ai.{}", "x".repeat(70))));
    }

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };
    const USD: CurrencyCode = match CurrencyCode::new("USD") {
        Ok(code) => code,
        Err(_) => panic!("USD is a valid code"),
    };

    #[test]
    fn a_report_keeps_its_parameters_by_name_and_in_order() {
        let report = MoneyError::CurrencyMismatch {
            expected: EUR,
            found: USD,
        }
        .report();
        assert_eq!(report.code, "money.currency-mismatch");
        assert_eq!(
            report.params,
            vec![
                ("expected", ErrorParam::Currency(EUR)),
                ("found", ErrorParam::Currency(USD)),
            ]
        );
        assert_eq!(report.param("found"), Some(&ErrorParam::Currency(USD)));
        assert_eq!(report.param("total"), None);
        assert_eq!(report.cause, None);
    }

    #[test]
    fn amounts_stay_exact_in_a_report() {
        let total = Money::from_minor_units(-1, EUR);
        let report = TransactionError::Unbalanced { total }.report();
        assert_eq!(report.param("total"), Some(&ErrorParam::Money(total)));
    }

    #[test]
    fn limits_come_from_the_constants() {
        assert_eq!(
            DecimalError::TooManyDigits.report().param("max"),
            Some(&ErrorParam::Number(18))
        );
        assert_eq!(
            DecimalError::TooManyDecimalPlaces.report().param("max"),
            Some(&ErrorParam::Number(18))
        );
    }

    #[test]
    fn a_count_too_large_for_64_bits_is_capped() {
        assert_eq!(ErrorParam::count(3), ErrorParam::Number(3));
        assert_eq!(ErrorParam::count(usize::MAX), ErrorParam::Number(i64::MAX));
    }

    #[test]
    fn conditions_are_counted_from_one() {
        let rule = RuleId::generate();
        let report = RuleError::EmptyText { rule, condition: 0 }.report();
        assert_eq!(report.param("rule"), Some(&ErrorParam::Rule(rule)));
        assert_eq!(report.param("condition"), Some(&ErrorParam::Number(1)));
    }

    #[test]
    fn a_cause_is_kept() {
        let report =
            ErrorReport::new("store.backup-failed").caused_by(MoneyError::Overflow.report());
        assert_eq!(report.cause.map(|cause| cause.code), Some("money.overflow"));
    }
}
