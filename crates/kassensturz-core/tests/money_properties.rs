// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Property-based tests for `Money` and balanced transactions (AGENTS.md §9).
//!
//! Every result is compared with the exact answer computed in 128 bits, so
//! these tests catch wrapping, silent truncation and lost overflow.

use kassensturz_core::{
    AccountId, CurrencyCode, Date, Money, MoneyError, Split, Transaction, TransactionError,
    TransactionId,
};
use proptest::prelude::*;

fn currency() -> impl Strategy<Value = CurrencyCode> {
    prop::sample::select(vec!["EUR", "USD", "JPY", "BHD"])
        .prop_map(|code| CurrencyCode::new(code).expect("valid code"))
}

/// The exact result as `Money`, or `Overflow` if it doesn't fit into 64 bits.
fn exact(minor_units: i128, currency: CurrencyCode) -> Result<Money, MoneyError> {
    i64::try_from(minor_units)
        .map(|minor_units| Money::from_minor_units(minor_units, currency))
        .map_err(|_| MoneyError::Overflow)
}

fn transaction(amounts: &[i64], currency: CurrencyCode) -> Result<Transaction, TransactionError> {
    let splits = amounts
        .iter()
        .map(|&minor_units| {
            Split::new(
                AccountId::generate(),
                Money::from_minor_units(minor_units, currency),
            )
        })
        .collect();
    let date = Date::new(2026, 10, 18).expect("valid date");
    Transaction::new(TransactionId::generate(), date, None, splits)
}

proptest! {
    #[test]
    fn add_is_exact_or_overflows(a: i64, b: i64, currency in currency()) {
        let sum = Money::from_minor_units(a, currency)
            .checked_add(Money::from_minor_units(b, currency));
        prop_assert_eq!(sum, exact(i128::from(a) + i128::from(b), currency));
    }

    #[test]
    fn sub_is_exact_or_overflows(a: i64, b: i64, currency in currency()) {
        let difference = Money::from_minor_units(a, currency)
            .checked_sub(Money::from_minor_units(b, currency));
        prop_assert_eq!(difference, exact(i128::from(a) - i128::from(b), currency));
    }

    #[test]
    fn neg_is_exact_or_overflows(a: i64, currency in currency()) {
        let negated = Money::from_minor_units(a, currency).checked_neg();
        prop_assert_eq!(negated, exact(-i128::from(a), currency));
    }

    #[test]
    fn add_is_commutative(a: i64, b: i64, currency in currency()) {
        let a = Money::from_minor_units(a, currency);
        let b = Money::from_minor_units(b, currency);
        prop_assert_eq!(a.checked_add(b), b.checked_add(a));
    }

    #[test]
    fn zero_changes_nothing(a: i64, currency in currency()) {
        let a = Money::from_minor_units(a, currency);
        prop_assert_eq!(a.checked_add(Money::zero(currency)), Ok(a));
        prop_assert_eq!(a.checked_sub(Money::zero(currency)), Ok(a));
    }

    #[test]
    fn subtracting_is_adding_the_negation(a: i64, b in (i64::MIN + 1)..=i64::MAX, currency in currency()) {
        let a = Money::from_minor_units(a, currency);
        let b = Money::from_minor_units(b, currency);
        let negated = b.checked_neg().expect("only i64::MIN can't be negated");
        prop_assert_eq!(a.checked_sub(b), a.checked_add(negated));
    }

    #[test]
    fn sum_is_exact_or_overflows(amounts in prop::collection::vec(any::<i64>(), 0..20), currency in currency()) {
        let sum = Money::checked_sum(
            currency,
            amounts.iter().map(|&a| Money::from_minor_units(a, currency)),
        );
        let exact_sum: i128 = amounts.iter().map(|&a| i128::from(a)).sum();
        prop_assert_eq!(sum, exact(exact_sum, currency));
    }

    #[test]
    fn different_currencies_never_mix(a: i64, b: i64) {
        let euros = Money::from_minor_units(a, CurrencyCode::new("EUR").expect("valid"));
        let dollars = Money::from_minor_units(b, CurrencyCode::new("USD").expect("valid"));
        let mismatch = Err(MoneyError::CurrencyMismatch {
            expected: euros.currency(),
            found: dollars.currency(),
        });
        prop_assert_eq!(euros.checked_add(dollars), mismatch);
        prop_assert_eq!(euros.checked_sub(dollars), mismatch);
    }

    #[test]
    fn balanced_parts_make_a_transaction(
        amounts in prop::collection::vec(-1_000_000_000_000_i64..1_000_000_000_000, 1..10),
        currency in currency(),
    ) {
        // The last part takes whatever is left, as the Parts editor does (D-017).
        let mut amounts = amounts;
        let rest: i64 = amounts.iter().sum();
        amounts.push(-rest);
        let transaction = transaction(&amounts, currency);
        prop_assert!(transaction.is_ok(), "{:?}", transaction);
    }

    #[test]
    fn the_remainder_balances_the_parts_or_overflows(
        amounts in prop::collection::vec(any::<i64>(), 1..10),
        open in any::<prop::sample::Index>(),
        currency in currency(),
    ) {
        // Whatever the open part holds before, the remainder replaces it
        // (D-017).
        let mut parts: Vec<Split> = amounts
            .iter()
            .map(|&minor_units| {
                Split::new(AccountId::generate(), Money::from_minor_units(minor_units, currency))
            })
            .collect();
        parts.push(Split::uncategorized(Money::zero(currency)));
        let open = open.index(parts.len());
        let id = parts[open].id;
        let others: i128 = parts
            .iter()
            .filter(|part| part.id != id)
            .map(|part| i128::from(part.amount.minor_units()))
            .sum();
        let remainder = Transaction::remainder(&parts, id);
        match exact(-others, currency) {
            Ok(expected) => {
                prop_assert_eq!(remainder, Ok(expected));
                parts[open].amount = expected;
                let date = Date::new(2026, 10, 19).expect("valid date");
                let transaction = Transaction::new(TransactionId::generate(), date, None, parts);
                prop_assert!(transaction.is_ok(), "{:?}", transaction);
            }
            Err(_) => prop_assert_eq!(remainder, Err(TransactionError::Overflow)),
        }
    }

    #[test]
    fn unbalanced_parts_are_rejected(
        amounts in prop::collection::vec(-1_000_000_000_000_i64..1_000_000_000_000, 1..10),
        difference in prop_oneof![-1_000_000_i64..=-1, 1_i64..=1_000_000],
        currency in currency(),
    ) {
        let mut amounts = amounts;
        let rest: i64 = amounts.iter().sum();
        amounts.push(difference - rest);
        prop_assert_eq!(
            transaction(&amounts, currency).map(|_| ()),
            Err(TransactionError::Unbalanced {
                total: Money::from_minor_units(difference, currency),
            })
        );
    }
}
