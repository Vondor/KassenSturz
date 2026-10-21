// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Converting a book's amounts in withdrawn currencies into the currencies
//! that replaced them, e.g. a book another program kept in German marks
//! before the euro (D-104).
//!
//! Each account's converted balance follows its balance in the withdrawn
//! currency, converted at the fixed rate and rounded once, as a bank
//! converted the balance of each account it kept: a part's converted amount
//! is the step between the balances before and after it. Rounding each part
//! on its own instead would leave cents in an account the old book had
//! emptied.

use std::cmp::Reverse;
use std::collections::HashMap;

use crate::account::AccountId;
use crate::currency::{Currency, CurrencyCode};
use crate::money::{Money, MoneyError};
use crate::rounding::RoundingMode;
use crate::withdrawn::Withdrawn;

/// Converts the parts of a book's transactions in withdrawn currencies, one
/// transaction after the other in date order (D-104).
///
/// It keeps, for each account or category in a withdrawn currency, what its
/// parts so far add up to in that currency and in the successor.
#[derive(Debug)]
pub struct Changeover {
    rounding: RoundingMode,
    balances: HashMap<AccountId, Balances>,
}

/// What an account's parts so far add up to.
#[derive(Clone, Copy, Debug)]
struct Balances {
    /// In the withdrawn currency, as the old book had them.
    original: Money,
    /// In the successor, as converted.
    converted: Money,
}

/// One part of a transaction as [`Changeover::balance`] sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConvertedPart {
    /// Its amount in the transaction's currency after the changeover, which
    /// [`Changeover::balance`] may change.
    pub amount: Money,
    /// The amount in a withdrawn currency that `amount` was converted from,
    /// or `None` if `amount` was given, e.g. by the file, and stays as it is.
    pub original: Option<Money>,
    /// The account in a withdrawn currency whose converted balance `amount`
    /// is a step of ([`Changeover::book`]), so a change of it is noted there
    /// and that account's next part catches up.
    pub account: Option<AccountId>,
    /// Whether `amount` is what the part books to one of the user's
    /// accounts, whose balances must match the bank's: such a part takes
    /// what rounding left over only if no other part can.
    pub to_user_account: bool,
}

impl Changeover {
    /// A changeover that rounds in `rounding`, usually the book's (D-027).
    pub fn new(rounding: RoundingMode) -> Self {
        Self {
            rounding,
            balances: HashMap::new(),
        }
    }

    /// The amount in `to`, `withdrawn`'s successor with the book's decimal
    /// places, that a part booking `original` to `account` comes to: the
    /// account's balance after it, converted, minus what its parts so far
    /// came to in `to`.
    ///
    /// The step never points the other way than `original`: if an earlier
    /// part's amount was changed so that it would, the step is zero, and a
    /// later part catches up. A part of zero stays zero. Fails if `original`
    /// isn't in `withdrawn`'s currency, or isn't in the currency the
    /// account's earlier parts were in, or on overflow.
    pub fn book(
        &mut self,
        account: AccountId,
        original: Money,
        withdrawn: Withdrawn,
        to: Currency,
    ) -> Result<Money, MoneyError> {
        let code = withdrawn.currency().code();
        if original.currency() != code {
            return Err(MoneyError::CurrencyMismatch {
                expected: code,
                found: original.currency(),
            });
        }
        let balances = self.balances_of(account, code, to.code());
        let after = balances.original.checked_add(original)?;
        let target = withdrawn.convert(after, to, self.rounding)?;
        let mut step = target.checked_sub(balances.converted)?;
        if original.is_zero() || opposite(step, original) {
            step = Money::zero(to.code());
        }
        self.note(account, original, step)?;
        Ok(step)
    }

    /// Notes that a part booked `original` to `account`, in a withdrawn
    /// currency, and that its amount in the successor, `converted`, was
    /// given, e.g. by the file, rather than converted. The account's next
    /// converted part catches up with any difference.
    pub fn book_given(
        &mut self,
        account: AccountId,
        original: Money,
        converted: Money,
    ) -> Result<(), MoneyError> {
        self.balances_of(account, original.currency(), converted.currency());
        self.note(account, original, converted)
    }

    /// Changes the converted amounts of `parts`, whose amounts are in
    /// `currency`, so that they add up to zero, if the transaction's
    /// amounts in its withdrawn currency did: each part was rounded on its
    /// own, so the converted ones can be a few minor units off.
    ///
    /// What is left over goes to the converted part with the largest
    /// amount among those that don't book to one of the user's accounts,
    /// so the user's balances stay as converted; only if there is none, to
    /// one of the user's accounts. No part's amount comes to point the
    /// other way than its original amount: one that would becomes zero and
    /// the next part takes the rest. A part with an original amount of zero
    /// and a given one are left as they are.
    ///
    /// Returns what the parts add up to afterwards: zero, unless no part
    /// could take what is left, which a part without a category then can.
    /// Fails if a part isn't in `currency`, or on overflow.
    pub fn balance(
        &mut self,
        currency: CurrencyCode,
        parts: &mut [ConvertedPart],
    ) -> Result<Money, MoneyError> {
        let mut left = Money::checked_sum(currency, parts.iter().map(|part| part.amount))?;
        let mut order: Vec<usize> = (0..parts.len())
            .filter(|&index| {
                parts[index]
                    .original
                    .is_some_and(|original| !original.is_zero())
            })
            .collect();
        // Stable, so of two equal amounts the earlier part comes first.
        order.sort_by_key(|&index| {
            let part = &parts[index];
            (
                part.to_user_account,
                Reverse(part.amount.minor_units().unsigned_abs()),
            )
        });
        for index in order {
            if left.is_zero() {
                break;
            }
            let part = &mut parts[index];
            let original = part
                .original
                .expect("only converted parts are in the order");
            let wanted = part.amount.checked_sub(left)?;
            let new = if opposite(wanted, original) {
                Money::zero(currency)
            } else {
                wanted
            };
            let change = new.checked_sub(part.amount)?;
            left = left.checked_add(change)?;
            part.amount = new;
            if let Some(account) = part.account
                && let Some(balances) = self.balances.get_mut(&account)
            {
                balances.converted = balances.converted.checked_add(change)?;
            }
        }
        Ok(left)
    }

    /// The balances of `account`, zero in `original` and `converted` if it
    /// had no part yet.
    fn balances_of(
        &mut self,
        account: AccountId,
        original: CurrencyCode,
        converted: CurrencyCode,
    ) -> Balances {
        *self.balances.entry(account).or_insert(Balances {
            original: Money::zero(original),
            converted: Money::zero(converted),
        })
    }

    /// Adds a part's two amounts to `account`'s balances.
    fn note(
        &mut self,
        account: AccountId,
        original: Money,
        converted: Money,
    ) -> Result<(), MoneyError> {
        let balances = self
            .balances
            .get_mut(&account)
            .expect("balances_of added the account");
        let new = Balances {
            original: balances.original.checked_add(original)?,
            converted: balances.converted.checked_add(converted)?,
        };
        *balances = new;
        Ok(())
    }
}

/// Whether `a` and `b` point in opposite directions: one above zero, the
/// other below.
fn opposite(a: Money, b: Money) -> bool {
    (a.is_negative() && b.minor_units() > 0) || (a.minor_units() > 0 && b.is_negative())
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };
    const DEM: CurrencyCode = match CurrencyCode::new("DEM") {
        Ok(code) => code,
        Err(_) => panic!("DEM is a valid code"),
    };

    fn eur(minor_units: i64) -> Money {
        Money::from_minor_units(minor_units, EUR)
    }

    fn dem(minor_units: i64) -> Money {
        Money::from_minor_units(minor_units, DEM)
    }

    fn mark() -> Withdrawn {
        DEM.withdrawn().expect("the table lists the mark")
    }

    fn euro() -> Currency {
        Currency::new(EUR, 2).expect("2 decimal places")
    }

    fn changeover() -> Changeover {
        Changeover::new(RoundingMode::HalfAwayFromZero)
    }

    /// `amount` in marks converted on its own.
    fn converted(minor_units: i64) -> Money {
        mark()
            .convert(dem(minor_units), euro(), RoundingMode::HalfAwayFromZero)
            .expect("fits")
    }

    #[test]
    fn an_account_emptied_in_marks_is_empty_in_euros() {
        // Three times 0.01 DEM in, then 0.03 DEM out. On its own, 0.01 DEM
        // is 0.01 EUR (0.0051...), but 0.03 DEM is 0.02 EUR (0.0153...).
        let mut changeover = changeover();
        let account = AccountId::generate();
        let steps: Vec<i64> = [1, 1, 1, -3]
            .into_iter()
            .map(|minor_units| {
                changeover
                    .book(account, dem(minor_units), mark(), euro())
                    .expect("converted")
                    .minor_units()
            })
            .collect();
        assert_eq!(steps, [1, 0, 1, -2]);
        assert_eq!(steps.iter().sum::<i64>(), 0);
    }

    #[test]
    fn a_given_amount_counts_and_the_next_part_catches_up() {
        let mut changeover = changeover();
        let account = AccountId::generate();
        // The file said 195.58 DEM are 100.01 EUR, one cent more than the
        // fixed rate gives (99.9985...).
        changeover
            .book_given(account, dem(19_558), eur(10_001))
            .expect("noted");
        // 295.58 DEM in all are 151.1276... EUR, so the next 100.00 DEM
        // come to 151.13 - 100.01 = 51.12 EUR, one cent less than on their
        // own.
        let step = changeover
            .book(account, dem(10_000), mark(), euro())
            .expect("converted");
        assert_eq!(step, eur(5112));
    }

    #[test]
    fn a_step_never_points_the_other_way() {
        let mut changeover = changeover();
        let account = AccountId::generate();
        changeover
            .book_given(account, dem(100), eur(100))
            .expect("noted");
        // 1.01 DEM are 0.52 EUR, below the 1.00 EUR booked so far, so the
        // next 0.01 DEM in comes to zero, not to -0.48 EUR.
        assert_eq!(changeover.book(account, dem(1), mark(), euro()), Ok(eur(0)));
        assert_eq!(changeover.book(account, dem(0), mark(), euro()), Ok(eur(0)));
        // Money going out catches up.
        assert_eq!(
            changeover.book(account, dem(-101), mark(), euro()),
            Ok(eur(-100))
        );
    }

    #[test]
    fn booking_needs_the_withdrawn_currency() {
        let mut changeover = changeover();
        assert_eq!(
            changeover.book(AccountId::generate(), eur(100), mark(), euro()),
            Err(MoneyError::CurrencyMismatch {
                expected: DEM,
                found: EUR
            })
        );
    }

    #[test]
    fn what_rounding_leaves_goes_to_a_category_first() {
        // Three times 0.01 DEM spent from a checking account in marks:
        // 0.03 DEM out of it, 0.01 DEM into each of three categories.
        let mut changeover = changeover();
        let checking = AccountId::generate();
        let out = changeover
            .book(checking, dem(-3), mark(), euro())
            .expect("converted");
        assert_eq!(out, eur(-2));
        let mut parts = vec![ConvertedPart {
            amount: out,
            original: Some(dem(-3)),
            account: Some(checking),
            to_user_account: true,
        }];
        for _ in 0..3 {
            let category = AccountId::generate();
            parts.push(ConvertedPart {
                amount: changeover
                    .book(category, dem(1), mark(), euro())
                    .expect("converted"),
                original: Some(dem(1)),
                account: Some(category),
                to_user_account: false,
            });
        }
        assert_eq!(changeover.balance(EUR, &mut parts), Ok(eur(0)));
        let amounts: Vec<i64> = parts.iter().map(|part| part.amount.minor_units()).collect();
        assert_eq!(amounts, [-2, 0, 1, 1]);
        // The first category's balance was noted: its next 0.01 DEM is
        // 0.01 EUR again.
        let first = parts[1].account.expect("a category");
        assert_eq!(changeover.book(first, dem(1), mark(), euro()), Ok(eur(1)));
    }

    #[test]
    fn the_users_accounts_take_what_is_left_only_without_another_part() {
        let mut changeover = changeover();
        let mut parts = [
            ConvertedPart {
                amount: eur(-5113),
                original: Some(dem(-10_000)),
                account: None,
                to_user_account: true,
            },
            ConvertedPart {
                amount: eur(5113),
                original: Some(dem(10_000)),
                account: None,
                to_user_account: true,
            },
            ConvertedPart {
                amount: eur(7),
                original: None,
                account: None,
                to_user_account: false,
            },
        ];
        assert_eq!(changeover.balance(EUR, &mut parts), Ok(eur(0)));
        let amounts: Vec<i64> = parts.iter().map(|part| part.amount.minor_units()).collect();
        // Of two equal amounts, the earlier part takes it; the given one
        // stays.
        assert_eq!(amounts, [-5120, 5113, 7]);
    }

    #[test]
    fn what_no_part_can_take_is_returned() {
        let mut changeover = changeover();
        // 0.99 EUR too few: the converted part goes up, the way its original
        // amount points, so it takes all.
        let mut parts = [
            ConvertedPart {
                amount: eur(-100),
                original: None,
                account: None,
                to_user_account: true,
            },
            ConvertedPart {
                amount: eur(1),
                original: Some(dem(1)),
                account: None,
                to_user_account: false,
            },
        ];
        assert_eq!(changeover.balance(EUR, &mut parts), Ok(eur(0)));
        assert_eq!(parts[1].amount, eur(100));
        // 1.01 EUR too many: the converted part drops to zero, the one
        // converted from zero stays, and 1.00 EUR is left.
        let mut parts = [
            ConvertedPart {
                amount: eur(101),
                original: None,
                account: None,
                to_user_account: true,
            },
            ConvertedPart {
                amount: eur(1),
                original: Some(dem(1)),
                account: None,
                to_user_account: false,
            },
            ConvertedPart {
                amount: eur(-1),
                original: Some(dem(0)),
                account: None,
                to_user_account: false,
            },
        ];
        assert_eq!(changeover.balance(EUR, &mut parts), Ok(eur(100)));
        assert_eq!(parts[1].amount, eur(0));
        assert_eq!(parts[2].amount, eur(-1));
    }

    proptest! {
        /// However an account's parts go, its converted balance after each
        /// one is its balance in marks converted and rounded once, and no
        /// part points the other way.
        #[test]
        fn the_converted_balance_follows_the_balance_in_marks(
            parts in prop::collection::vec(-1_000_000_i64..1_000_000, 1..50),
        ) {
            let mut changeover = changeover();
            let account = AccountId::generate();
            let mut original = 0;
            let mut total = 0;
            for minor_units in parts {
                let step = changeover
                    .book(account, dem(minor_units), mark(), euro())
                    .expect("converted");
                prop_assert!(!opposite(step, dem(minor_units)));
                original += minor_units;
                total += step.minor_units();
                prop_assert_eq!(eur(total), converted(original));
            }
        }

        /// Balancing leaves the given parts and the signs alone, and what
        /// the parts add up to afterwards is what it returns: zero whenever
        /// a converted part points against what is left over.
        #[test]
        fn balancing_moves_only_converted_amounts(
            parts in prop::collection::vec(
                (-10_000_i64..10_000, prop::option::of(-20_000_i64..20_000), any::<bool>()),
                1..8,
            ),
        ) {
            let mut changeover = changeover();
            let mut parts: Vec<ConvertedPart> = parts
                .into_iter()
                .map(|(amount, original, to_user_account)| ConvertedPart {
                    // A converted amount points the way its original does.
                    amount: eur(match original {
                        Some(original) => amount.abs() * original.signum(),
                        None => amount,
                    }),
                    original: original.map(dem),
                    account: None,
                    to_user_account,
                })
                .collect();
            let before = parts.clone();
            let excess = before.iter().map(|part| part.amount.minor_units()).sum::<i64>();
            let left = changeover.balance(EUR, &mut parts).expect("fits");
            let total: i64 = parts.iter().map(|part| part.amount.minor_units()).sum();
            prop_assert_eq!(left, eur(total));
            for (part, old) in parts.iter().zip(&before) {
                match old.original {
                    Some(original) if !original.is_zero() => {
                        prop_assert!(!opposite(part.amount, original));
                    }
                    _ => prop_assert_eq!(part, old),
                }
            }
            let can_take_all = before.iter().any(|part| {
                part.original
                    .is_some_and(|original| opposite(original, eur(excess)))
            });
            if can_take_all {
                prop_assert_eq!(left, eur(0));
            }
        }
    }
}
