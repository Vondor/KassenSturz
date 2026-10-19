// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Transfers between the user's own accounts: finding the two transactions
//! that are the two sides of one, and joining them into one Transfer
//! (D-021, D-030, D-087).
//!
//! When money moves from the checking account to the savings account, each
//! bank's statement shows one side: 100 out of checking, 100 into savings.
//! Imported, each side is a transaction of its own whose other part has no
//! category yet (D-083), so the move would count as spending and as income.
//! Joined into one transaction with both accounts, it is a Transfer, which
//! counts as neither. KassenSturz only suggests which sides belong together;
//! the user confirms every suggestion (D-030).

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use jiff::civil::Weekday;

use crate::Date;
use crate::account::AccountId;
use crate::currency::CurrencyCode;
use crate::money::Money;
use crate::transaction::{Split, Transaction, TransactionError, TransactionId};

/// How many business days the two sides of a suggested Transfer may be
/// apart (D-030). Saturdays and Sundays don't count; public holidays aren't
/// considered yet.
pub const MAX_BUSINESS_DAYS: u32 = 3;

/// One side of a possible Transfer: a transaction that moves money into or
/// out of one of the user's accounts, and whose other part has no category
/// yet.
///
/// Only such a transaction can be one: a part with a category says where
/// the money went, and manual choices win (D-031), and a transaction the
/// user broke into more Parts (D-017), or a part in another currency (Q-020),
/// is left alone too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferSide {
    /// The transaction.
    pub transaction: TransactionId,
    /// Its booking date, the one the user sees.
    pub booking_date: Date,
    /// The user's account the money moves into or out of.
    pub account: AccountId,
    /// What it does to that account: positive amounts come in, negative ones
    /// go out. Never zero.
    pub amount: Money,
}

impl TransferSide {
    /// The side `transaction` is, or [`TransferError::NotASide`] if it can't
    /// be one: it has exactly two parts, one booked to one of the user's
    /// accounts, which `is_account` tells apart from categories, and one
    /// without a category, in the transaction's currency, and its amount
    /// isn't zero.
    pub fn of(
        transaction: &Transaction,
        is_account: impl Fn(AccountId) -> bool,
    ) -> Result<Self, TransferError> {
        let not_a_side = TransferError::NotASide(transaction.id());
        let [first, second] = transaction.splits() else {
            return Err(not_a_side);
        };
        let (booked, open) = match (first.account, second.account) {
            (Some(_), None) => (first, second),
            (None, Some(_)) => (second, first),
            _ => return Err(not_a_side),
        };
        let account = booked.account.ok_or(not_a_side)?;
        if !is_account(account)
            || booked.foreign.is_some()
            || open.foreign.is_some()
            || booked.amount.is_zero()
        {
            return Err(not_a_side);
        }
        Ok(Self {
            transaction: transaction.id(),
            booking_date: transaction.booking_date(),
            account,
            amount: booked.amount,
        })
    }
}

/// A suggested Transfer: two sides with the same amount, one out of an
/// account and one into another, at most [`MAX_BUSINESS_DAYS`] apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferMatch {
    /// The side the money left from: its amount is negative.
    pub outgoing: TransferSide,
    /// The side the money arrived in: the same amount, positive.
    pub incoming: TransferSide,
    /// How many business days lie between their booking dates.
    pub business_days: u32,
}

/// The Transfers among `sides` to suggest to the user (D-021, D-030).
///
/// Two sides fit if they are in different accounts, in the same currency,
/// one takes out exactly what the other puts in, and their booking dates are
/// at most [`MAX_BUSINESS_DAYS`] business days apart. Every side is in at
/// most one suggestion: where several fit, the closest dates win, i.e. the
/// fewest business days, then the fewest calendar days, then the earliest
/// date, then the IDs, so the same book always gets the same suggestions.
/// They come sorted by the outgoing side's date.
///
/// It only suggests: the user confirms each Transfer (D-030), which joins
/// the two transactions ([`join_transfer`]).
pub fn find_transfers(sides: &[TransferSide]) -> Vec<TransferMatch> {
    // Only sides with the same currency and size can fit.
    let mut groups: HashMap<(CurrencyCode, u64), Vec<&TransferSide>> = HashMap::new();
    for side in sides {
        let size = side.amount.minor_units().unsigned_abs();
        groups
            .entry((side.amount.currency(), size))
            .or_default()
            .push(side);
    }
    let mut candidates = Vec::new();
    for group in groups.values_mut() {
        group.sort_by(|a, b| side_order(a, b));
        for (index, earlier) in group.iter().enumerate() {
            for later in &group[index + 1..] {
                // Sorted by date, so every later side is further away.
                let Some((business_days, calendar_days)) =
                    days_apart(earlier.booking_date, later.booking_date, MAX_BUSINESS_DAYS)
                else {
                    break;
                };
                if earlier.account == later.account
                    || earlier.transaction == later.transaction
                    || earlier.amount.is_negative() == later.amount.is_negative()
                {
                    continue;
                }
                let (outgoing, incoming) = if earlier.amount.is_negative() {
                    (**earlier, **later)
                } else {
                    (**later, **earlier)
                };
                candidates.push((
                    calendar_days,
                    TransferMatch {
                        outgoing,
                        incoming,
                        business_days,
                    },
                ));
            }
        }
    }
    candidates.sort_by(|(a_days, a), (b_days, b)| {
        a.business_days
            .cmp(&b.business_days)
            .then(a_days.cmp(b_days))
            .then_with(|| match_order(a, b))
    });
    let mut used = HashSet::new();
    let mut matches: Vec<TransferMatch> = Vec::new();
    for (_, candidate) in candidates {
        let (outgoing, incoming) = (
            candidate.outgoing.transaction,
            candidate.incoming.transaction,
        );
        if used.contains(&outgoing) || used.contains(&incoming) {
            continue;
        }
        used.insert(outgoing);
        used.insert(incoming);
        matches.push(candidate);
    }
    matches.sort_by(match_order);
    matches
}

/// Joins two sides into one Transfer: the transaction `a` or `b` the money
/// left from, with its other part replaced by the part of the other
/// transaction that books the money into its account (D-087).
///
/// The joined transaction keeps the outgoing side's ID, booking date and
/// payee, or the incoming side's payee if the outgoing one has none. Both
/// accounts' parts stay exactly as they were, with what their banks
/// delivered (D-083), so neither account's balance changes; the two parts
/// without a category go. The incoming transaction is the one to remove.
///
/// The user decides which sides to join, so the dates may be any number of
/// days apart; the two sides must be in different accounts and take out and
/// put in the same amount in the same currency. `is_account` tells the
/// user's accounts apart from categories, as for [`TransferSide::of`].
pub fn join_transfer(
    a: &Transaction,
    b: &Transaction,
    is_account: impl Fn(AccountId) -> bool,
) -> Result<Transaction, TransferError> {
    if a.id() == b.id() {
        return Err(TransferError::SameTransaction(a.id()));
    }
    let side_a = TransferSide::of(a, &is_account)?;
    let side_b = TransferSide::of(b, &is_account)?;
    if side_a.account == side_b.account {
        return Err(TransferError::SameAccount(side_a.account));
    }
    let cancels = side_a.amount.currency() == side_b.amount.currency()
        && side_a
            .amount
            .checked_neg()
            .is_ok_and(|neg| neg == side_b.amount);
    if !cancels {
        return Err(TransferError::AmountsDiffer {
            first: side_a.amount,
            second: side_b.amount,
        });
    }
    let (outgoing, incoming, incoming_account) = if side_a.amount.is_negative() {
        (a, b, side_b.account)
    } else {
        (b, a, side_a.account)
    };
    let incoming_part = incoming
        .splits()
        .iter()
        .find(|split| split.account == Some(incoming_account))
        .cloned()
        .ok_or(TransferError::NotASide(incoming.id()))?;
    let splits: Vec<Split> = outgoing
        .splits()
        .iter()
        .map(|split| match split.account {
            Some(_) => split.clone(),
            None => incoming_part.clone(),
        })
        .collect();
    let payee = outgoing.payee().or(incoming.payee()).map(str::to_owned);
    Transaction::new(outgoing.id(), outgoing.booking_date(), payee, splits)
        .map_err(TransferError::Transaction)
}

/// Why two transactions can't be joined into a Transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferError {
    /// The transaction isn't one side of a Transfer: it needs exactly two
    /// parts, one booked to one of the user's accounts and one without a
    /// category yet, in one currency, and an amount other than zero.
    NotASide(TransactionId),
    /// Both are the same transaction.
    SameTransaction(TransactionId),
    /// Both sides are in the same account, so no money moves between
    /// accounts.
    SameAccount(AccountId),
    /// One side doesn't take out exactly what the other puts in, in the
    /// same currency.
    AmountsDiffer {
        /// What the first side does to its account.
        first: Money,
        /// What the second side does to its account.
        second: Money,
    },
    /// The joined parts don't form a valid transaction.
    Transaction(TransactionError),
}

/// Sorts sides by date, then by transaction ID.
fn side_order(a: &TransferSide, b: &TransferSide) -> Ordering {
    (a.booking_date, a.transaction).cmp(&(b.booking_date, b.transaction))
}

/// Sorts matches by the outgoing side, then by the incoming one.
fn match_order(a: &TransferMatch, b: &TransferMatch) -> Ordering {
    side_order(&a.outgoing, &b.outgoing).then_with(|| side_order(&a.incoming, &b.incoming))
}

/// How far `earlier` and `later` are apart, as business days and calendar
/// days, if at most `limit` business days: the number of Mondays to Fridays
/// after `earlier` up to and including `later`. Friday to Monday is one
/// business day, Saturday to Sunday none.
fn days_apart(earlier: Date, later: Date, limit: u32) -> Option<(u32, u32)> {
    let mut business_days = 0;
    let mut calendar_days = 0;
    let mut day = earlier;
    while day < later {
        day = day.tomorrow().ok()?;
        calendar_days += 1;
        if !matches!(day.weekday(), Weekday::Saturday | Weekday::Sunday) {
            business_days += 1;
            if business_days > limit {
                return None;
            }
        }
    }
    Some((business_days, calendar_days))
}

#[cfg(test)]
mod tests {
    use super::*;

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };
    const USD: CurrencyCode = match CurrencyCode::new("USD") {
        Ok(code) => code,
        Err(_) => panic!("USD is a valid code"),
    };

    fn eur(minor_units: i64) -> Money {
        Money::from_minor_units(minor_units, EUR)
    }

    /// A day in October 2026: the 5th is a Monday, the 10th a Saturday.
    fn october(day: i8) -> Date {
        Date::new(2026, 10, day).expect("valid date")
    }

    fn side(account: AccountId, minor_units: i64, day: i8) -> TransferSide {
        TransferSide {
            transaction: TransactionId::generate(),
            booking_date: october(day),
            account,
            amount: eur(minor_units),
        }
    }

    /// A transaction as an import adds it: the account's part and one
    /// without a category (D-083).
    fn imported(account: AccountId, minor_units: i64, day: i8, payee: Option<&str>) -> Transaction {
        let booked = Split::new(account, eur(minor_units));
        let open = Split::uncategorized(eur(-minor_units));
        Transaction::new(
            TransactionId::generate(),
            october(day),
            payee.map(str::to_owned),
            vec![booked, open],
        )
        .expect("balanced")
    }

    fn pairs(matches: &[TransferMatch]) -> Vec<(TransactionId, TransactionId)> {
        matches
            .iter()
            .map(|found| (found.outgoing.transaction, found.incoming.transaction))
            .collect()
    }

    #[test]
    fn the_days_used_in_these_tests_are_the_weekdays_they_claim() {
        assert_eq!(october(5).weekday(), Weekday::Monday);
        assert_eq!(october(10).weekday(), Weekday::Saturday);
    }

    #[test]
    fn business_days_skip_weekends() {
        // Same day, then Monday to Thursday.
        assert_eq!(days_apart(october(5), october(5), 3), Some((0, 0)));
        assert_eq!(days_apart(october(5), october(8), 3), Some((3, 3)));
        assert_eq!(days_apart(october(5), october(9), 3), None);
        // Friday to Monday is one business day, and Friday to Wednesday
        // three, five calendar days later.
        assert_eq!(days_apart(october(9), october(12), 3), Some((1, 3)));
        assert_eq!(days_apart(october(9), october(14), 3), Some((3, 5)));
        assert_eq!(days_apart(october(9), october(15), 3), None);
        // A weekend on its own is no business day.
        assert_eq!(days_apart(october(10), october(11), 3), Some((0, 1)));
        assert_eq!(days_apart(october(10), october(14), 3), Some((3, 4)));
        // Far apart is quickly too far, and the last date there is ends it.
        assert_eq!(days_apart(Date::MIN, Date::MAX, 3), None);
        assert_eq!(days_apart(Date::MAX, Date::MAX, 3), Some((0, 0)));
    }

    #[test]
    fn pairs_money_out_of_one_account_with_the_same_money_into_another() {
        let checking = AccountId::generate();
        let savings = AccountId::generate();
        let out = side(checking, -10_000, 9);
        let into = side(savings, 10_000, 12);
        let found = find_transfers(&[into, out]);
        assert_eq!(
            found,
            vec![TransferMatch {
                outgoing: out,
                incoming: into,
                business_days: 1,
            }]
        );
    }

    #[test]
    fn suggests_nothing_that_does_not_fit() {
        let checking = AccountId::generate();
        let savings = AccountId::generate();
        // Thursday the 8th.
        let out = side(checking, -10_000, 8);
        let cases = [
            ("another amount", side(savings, 9_999, 8)),
            ("the same direction", side(savings, -10_000, 8)),
            ("the same account", side(checking, 10_000, 8)),
            ("four business days later", side(savings, 10_000, 14)),
            ("four business days earlier", side(savings, 10_000, 2)),
            (
                "another currency",
                TransferSide {
                    amount: Money::from_minor_units(10_000, USD),
                    ..side(savings, 10_000, 8)
                },
            ),
        ];
        for (what, other) in cases {
            assert_eq!(find_transfers(&[out, other]), Vec::new(), "{what}");
        }
        // A side never fits itself.
        let both_ways = TransferSide {
            account: savings,
            amount: eur(10_000),
            ..out
        };
        assert_eq!(find_transfers(&[out, both_ways]), Vec::new());
    }

    #[test]
    fn the_incoming_side_may_come_first() {
        // A credit card payment can show on the card before the checking
        // account: 300 into the card on Monday, out of checking on Tuesday.
        let checking = AccountId::generate();
        let card = AccountId::generate();
        let into = side(card, 30_000, 5);
        let out = side(checking, -30_000, 6);
        let found = find_transfers(&[into, out]);
        assert_eq!(pairs(&found), vec![(out.transaction, into.transaction)]);
    }

    #[test]
    fn each_side_goes_with_the_closest_one() {
        // Two equal transfers, on Monday and Thursday, each booked into
        // savings a day later. Thursday's could also be Tuesday's other
        // side, but that is two business days away.
        let checking = AccountId::generate();
        let savings = AccountId::generate();
        let monday_out = side(checking, -5_000, 5);
        let tuesday_in = side(savings, 5_000, 6);
        let thursday_out = side(checking, -5_000, 8);
        let friday_in = side(savings, 5_000, 9);
        let sides = [friday_in, thursday_out, tuesday_in, monday_out];
        let found = find_transfers(&sides);
        assert_eq!(
            pairs(&found),
            vec![
                (monday_out.transaction, tuesday_in.transaction),
                (thursday_out.transaction, friday_in.transaction),
            ]
        );
        // The order the sides come in doesn't matter.
        let mut reversed = sides;
        reversed.reverse();
        assert_eq!(find_transfers(&reversed), found);

        // The same day is closer than the next one.
        let same_day_in = side(savings, 5_000, 5);
        let found = find_transfers(&[tuesday_in, same_day_in, monday_out]);
        assert_eq!(
            pairs(&found),
            vec![(monday_out.transaction, same_day_in.transaction)]
        );
    }

    #[test]
    fn one_side_is_in_at_most_one_suggestion() {
        // One withdrawal, two equal deposits in two accounts on the same
        // day: only one can be its other side.
        let checking = AccountId::generate();
        let out = side(checking, -2_000, 5);
        let first = side(AccountId::generate(), 2_000, 5);
        let second = side(AccountId::generate(), 2_000, 5);
        let found = find_transfers(&[out, first, second]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].outgoing, out);
        // The same one every time: the lower ID wins the tie.
        let expected = if first.transaction < second.transaction {
            first
        } else {
            second
        };
        assert_eq!(found[0].incoming, expected);
        assert_eq!(find_transfers(&[second, first, out]), found);
    }

    #[test]
    fn closer_calendar_days_win_between_equal_business_days() {
        // Friday out; in on Saturday (0 business days, 1 calendar day) or
        // on Sunday (0 business days, 2 calendar days).
        let out = side(AccountId::generate(), -1_000, 9);
        let saturday = side(AccountId::generate(), 1_000, 10);
        let sunday = side(AccountId::generate(), 1_000, 11);
        let found = find_transfers(&[sunday, saturday, out]);
        assert_eq!(pairs(&found), vec![(out.transaction, saturday.transaction)]);
    }

    #[test]
    fn a_side_is_a_transaction_between_an_account_and_no_category_yet() {
        let checking = AccountId::generate();
        let groceries = AccountId::generate();
        let is_account = |id: AccountId| id == checking;
        let transaction = imported(checking, -5_320, 5, Some("Savings"));
        assert_eq!(
            TransferSide::of(&transaction, is_account),
            Ok(TransferSide {
                transaction: transaction.id(),
                booking_date: october(5),
                account: checking,
                amount: eur(-5_320),
            })
        );
        // The part without a category may come first.
        let flipped = Transaction::new(
            TransactionId::generate(),
            october(5),
            None,
            transaction.splits().iter().rev().cloned().collect(),
        )
        .expect("balanced");
        assert_eq!(
            TransferSide::of(&flipped, is_account).map(|side| side.account),
            Ok(checking)
        );

        let not_sides = [
            // The other side has a category already.
            vec![
                Split::new(checking, eur(-5_320)),
                Split::new(groceries, eur(5_320)),
            ],
            // A category instead of an account.
            vec![
                Split::new(groceries, eur(5_320)),
                Split::uncategorized(eur(-5_320)),
            ],
            // Nothing booked to an account.
            vec![
                Split::uncategorized(eur(5_320)),
                Split::uncategorized(eur(-5_320)),
            ],
            // Broken into more parts.
            vec![
                Split::new(checking, eur(-5_320)),
                Split::uncategorized(eur(4_000)),
                Split::uncategorized(eur(1_320)),
            ],
            // Nothing moves.
            vec![Split::new(checking, eur(0)), Split::uncategorized(eur(0))],
            // In another currency.
            vec![
                Split {
                    foreign: Some(crate::ForeignAmount {
                        amount: Money::from_minor_units(-6_000, USD),
                        rate: None,
                    }),
                    ..Split::new(checking, eur(-5_320))
                },
                Split::uncategorized(eur(5_320)),
            ],
        ];
        for parts in not_sides {
            let transaction =
                Transaction::new(TransactionId::generate(), october(5), None, parts.clone())
                    .expect("balanced");
            assert_eq!(
                TransferSide::of(&transaction, is_account),
                Err(TransferError::NotASide(transaction.id())),
                "{parts:?}"
            );
        }
    }

    #[test]
    fn joining_keeps_both_accounts_parts_and_the_outgoing_transaction() {
        let checking = AccountId::generate();
        let savings = AccountId::generate();
        let is_account = |id: AccountId| id == checking || id == savings;
        let mut into = imported(savings, 10_000, 12, Some("From checking"));
        // What the bank delivered stays with its part.
        let delivered = crate::ImportedDetails {
            bank_id: Some("SAV-0001".into()),
            ..Default::default()
        };
        let mut parts = into.splits().to_vec();
        parts[0].imported = Some(delivered.clone());
        into = Transaction::new(into.id(), october(12), Some("From checking".into()), parts)
            .expect("balanced");
        let out = imported(checking, -10_000, 9, None);

        for (a, b) in [(&out, &into), (&into, &out)] {
            let joined = join_transfer(a, b, is_account).expect("joined");
            assert_eq!(joined.id(), out.id());
            assert_eq!(joined.booking_date(), october(9));
            // The outgoing side has no payee, so the incoming one's is kept.
            assert_eq!(joined.payee(), Some("From checking"));
            assert_eq!(
                joined.splits(),
                [out.splits()[0].clone(), into.splits()[0].clone()]
            );
            assert_eq!(joined.splits()[1].imported, Some(delivered.clone()));
        }

        // The outgoing side's payee wins, and its parts keep their order.
        let out = Transaction::new(
            out.id(),
            october(9),
            Some("To savings".into()),
            out.splits().iter().rev().cloned().collect(),
        )
        .expect("balanced");
        let joined = join_transfer(&into, &out, is_account).expect("joined");
        assert_eq!(joined.payee(), Some("To savings"));
        assert_eq!(
            joined.splits(),
            [into.splits()[0].clone(), out.splits()[1].clone()]
        );
    }

    #[test]
    fn joining_any_two_days_apart_is_up_to_the_user() {
        let checking = AccountId::generate();
        let savings = AccountId::generate();
        let is_account = |id: AccountId| id == checking || id == savings;
        let out = imported(checking, -10_000, 1, None);
        let into = imported(savings, 10_000, 14, None);
        assert!(join_transfer(&out, &into, is_account).is_ok());
    }

    #[test]
    fn joining_refuses_what_is_no_transfer() {
        let checking = AccountId::generate();
        let savings = AccountId::generate();
        let groceries = AccountId::generate();
        let is_account = |id: AccountId| id == checking || id == savings;
        let out = imported(checking, -10_000, 5, None);

        assert_eq!(
            join_transfer(&out, &out, is_account),
            Err(TransferError::SameTransaction(out.id()))
        );
        let same_account = imported(checking, 10_000, 5, None);
        assert_eq!(
            join_transfer(&out, &same_account, is_account),
            Err(TransferError::SameAccount(checking))
        );
        let less = imported(savings, 9_999, 5, None);
        assert_eq!(
            join_transfer(&out, &less, is_account),
            Err(TransferError::AmountsDiffer {
                first: eur(-10_000),
                second: eur(9_999),
            })
        );
        let same_way = imported(savings, -10_000, 5, None);
        assert_eq!(
            join_transfer(&out, &same_way, is_account),
            Err(TransferError::AmountsDiffer {
                first: eur(-10_000),
                second: eur(-10_000),
            })
        );
        let in_dollars = Transaction::new(
            TransactionId::generate(),
            october(5),
            None,
            vec![
                Split::new(savings, Money::from_minor_units(10_000, USD)),
                Split::uncategorized(Money::from_minor_units(-10_000, USD)),
            ],
        )
        .expect("balanced");
        assert_eq!(
            join_transfer(&out, &in_dollars, is_account),
            Err(TransferError::AmountsDiffer {
                first: eur(-10_000),
                second: Money::from_minor_units(10_000, USD),
            })
        );
        let categorized = Transaction::new(
            TransactionId::generate(),
            october(5),
            None,
            vec![
                Split::new(savings, eur(10_000)),
                Split::new(groceries, eur(-10_000)),
            ],
        )
        .expect("balanced");
        assert_eq!(
            join_transfer(&out, &categorized, is_account),
            Err(TransferError::NotASide(categorized.id()))
        );
    }

    #[test]
    fn joining_parts_that_share_an_id_is_refused() {
        // Two transactions can't share a part's ID in a book, but nothing
        // stops two values from doing so.
        let checking = AccountId::generate();
        let savings = AccountId::generate();
        let is_account = |id: AccountId| id == checking || id == savings;
        let out = imported(checking, -10_000, 5, None);
        let mut parts = imported(savings, 10_000, 5, None).splits().to_vec();
        parts[0].id = out.splits()[0].id;
        let into =
            Transaction::new(TransactionId::generate(), october(5), None, parts).expect("balanced");
        assert_eq!(
            join_transfer(&out, &into, is_account),
            Err(TransferError::Transaction(
                TransactionError::DuplicateSplitId(out.splits()[0].id)
            ))
        );
    }
}
