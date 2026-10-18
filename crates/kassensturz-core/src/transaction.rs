// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Double-entry transactions and their parts (splits).

use crate::Date;
use crate::account::AccountId;
use crate::id::define_id;
use crate::money::{CurrencyCode, Money};

define_id!(
    /// Identifies a transaction within a book.
    TransactionId
);

define_id!(
    /// Identifies one part (split) of a transaction.
    SplitId
);

/// One part of a transaction: an amount booked to one account or category.
///
/// Users see it as a "Part" (`docs/GLOSSARY.md`, D-017).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Split {
    /// Its ID.
    pub id: SplitId,
    /// The account or category the amount is booked to.
    pub account: AccountId,
    /// Positive amounts flow into the account, negative ones out of it.
    pub amount: Money,
}

/// A transaction: two or more parts that add up to exactly zero
/// (double-entry, AGENTS.md §5).
///
/// It can only be created through [`Transaction::new`], which checks this, so
/// every `Transaction` balances.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transaction {
    id: TransactionId,
    booking_date: Date,
    payee: Option<String>,
    currency: CurrencyCode,
    splits: Vec<Split>,
}

impl Transaction {
    /// Checks that `splits` form a valid transaction and creates it.
    ///
    /// All parts must be in one currency for now; parts in another currency
    /// with the rate used come in M1 (D-043).
    pub fn new(
        id: TransactionId,
        booking_date: Date,
        payee: Option<String>,
        splits: Vec<Split>,
    ) -> Result<Self, TransactionError> {
        let [first, _, ..] = splits.as_slice() else {
            return Err(TransactionError::TooFewSplits {
                found: splits.len(),
            });
        };
        let currency = first.amount.currency();
        for (index, split) in splits.iter().enumerate() {
            if split.amount.currency() != currency {
                return Err(TransactionError::MixedCurrencies {
                    expected: currency,
                    found: split.amount.currency(),
                });
            }
            if splits[..index].iter().any(|earlier| earlier.id == split.id) {
                return Err(TransactionError::DuplicateSplitId(split.id));
            }
        }
        let total = Money::checked_sum(currency, splits.iter().map(|split| split.amount))
            .map_err(|_| TransactionError::Overflow)?;
        if !total.is_zero() {
            return Err(TransactionError::Unbalanced { total });
        }
        Ok(Self {
            id,
            booking_date,
            payee,
            currency,
            splits,
        })
    }

    /// Its ID.
    pub fn id(&self) -> TransactionId {
        self.id
    }

    /// The date the bank booked it, or the user entered (D-030).
    pub fn booking_date(&self) -> Date {
        self.booking_date
    }

    /// Who the money came from or went to, as the user wants it shown.
    pub fn payee(&self) -> Option<&str> {
        self.payee.as_deref()
    }

    /// The currency all its parts are in.
    pub fn currency(&self) -> CurrencyCode {
        self.currency
    }

    /// Its parts, in the order they were entered. They add up to zero.
    pub fn splits(&self) -> &[Split] {
        &self.splits
    }
}

/// Why parts don't form a valid transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionError {
    /// A transaction needs at least two parts: where the money comes from and
    /// where it goes.
    TooFewSplits {
        /// How many parts there were.
        found: usize,
    },
    /// Not all parts are in the same currency.
    MixedCurrencies {
        /// The currency of the first part.
        expected: CurrencyCode,
        /// The currency of a later part.
        found: CurrencyCode,
    },
    /// Two parts have the same ID.
    DuplicateSplitId(SplitId),
    /// The parts don't add up to zero.
    Unbalanced {
        /// What they add up to instead.
        total: Money,
    },
    /// The parts add up to more than 64 bits can hold.
    Overflow,
}

#[cfg(test)]
mod tests {
    use super::*;

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };

    fn split(account: AccountId, minor_units: i64) -> Split {
        Split {
            id: SplitId::generate(),
            account,
            amount: Money::from_minor_units(minor_units, EUR),
        }
    }

    fn date() -> Date {
        Date::new(2026, 10, 5).expect("valid date")
    }

    #[test]
    fn accepts_parts_that_add_up_to_zero() {
        let checking = AccountId::generate();
        let groceries = AccountId::generate();
        let household = AccountId::generate();
        let parts = vec![
            split(checking, -5320),
            split(groceries, 4520),
            split(household, 800),
        ];
        let transaction = Transaction::new(
            TransactionId::generate(),
            date(),
            Some("Corner Grocery".into()),
            parts.clone(),
        )
        .expect("balanced");
        assert_eq!(transaction.splits(), parts.as_slice());
        assert_eq!(transaction.currency(), EUR);
        assert_eq!(transaction.payee(), Some("Corner Grocery"));
    }

    #[test]
    fn rejects_parts_that_do_not_add_up() {
        let result = Transaction::new(
            TransactionId::generate(),
            date(),
            None,
            vec![
                split(AccountId::generate(), -5320),
                split(AccountId::generate(), 5300),
            ],
        );
        assert_eq!(
            result,
            Err(TransactionError::Unbalanced {
                total: Money::from_minor_units(-20, EUR)
            })
        );
    }

    #[test]
    fn needs_at_least_two_parts() {
        for parts in [vec![], vec![split(AccountId::generate(), 0)]] {
            let found = parts.len();
            let result = Transaction::new(TransactionId::generate(), date(), None, parts);
            assert_eq!(result, Err(TransactionError::TooFewSplits { found }));
        }
    }

    #[test]
    fn rejects_mixed_currencies() {
        let usd = CurrencyCode::new("USD").expect("valid code");
        let mut parts = vec![
            split(AccountId::generate(), -100),
            split(AccountId::generate(), 100),
        ];
        parts[1].amount = Money::from_minor_units(100, usd);
        let result = Transaction::new(TransactionId::generate(), date(), None, parts);
        assert_eq!(
            result,
            Err(TransactionError::MixedCurrencies {
                expected: EUR,
                found: usd
            })
        );
    }

    #[test]
    fn rejects_duplicate_part_ids() {
        let first = split(AccountId::generate(), -100);
        let mut second = split(AccountId::generate(), 100);
        let id = first.id;
        second.id = id;
        let result = Transaction::new(TransactionId::generate(), date(), None, vec![first, second]);
        assert_eq!(result, Err(TransactionError::DuplicateSplitId(id)));
    }

    #[test]
    fn reports_overflow_instead_of_wrapping() {
        let result = Transaction::new(
            TransactionId::generate(),
            date(),
            None,
            vec![
                split(AccountId::generate(), i64::MAX),
                split(AccountId::generate(), i64::MAX),
                split(AccountId::generate(), 2),
            ],
        );
        assert_eq!(result, Err(TransactionError::Overflow));
    }
}
