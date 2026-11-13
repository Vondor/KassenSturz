// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Monthly moves between the user's own accounts that the user asked to be
//! joined by themselves (D-749, `docs/RECURRING_MOVES.md`).
//!
//! A standing order from the checking account to the savings account shows
//! up on both statements every month. Once the user has confirmed such a
//! move and ticked "join this move by itself", the book remembers it as a
//! [`RecurringMove`]. This module decides which later pairs of sides fit a
//! remembered move ([`match_recurring_moves`]) and which single side waits
//! for its other half ([`fits_recurring_move`]). It changes nothing: the
//! store joins the pairs with [`join_transfer`](crate::join_transfer), so a
//! join keeps both account parts exactly as their banks sent them, and
//! neither balance changes (D-691).
//!
//! What never decides a match: the order of the entries, their IDs or the
//! time they were imported. Where two entries fit a move equally well,
//! neither is joined and both are reported as [`UnclearMatch`], for the user.

use std::collections::HashMap;

use jiff::civil::Weekday;

use crate::Date;
use crate::account::AccountId;
use crate::id::define_id;
use crate::money::Money;
use crate::transaction::TransactionId;
use crate::transfer::{MAX_BUSINESS_DAYS, TransferMatch, TransferSide, days_between};

define_id!(
    /// Identifies one remembered monthly move within a book.
    RecurringMoveId
);

/// How many business days a receiving side that waits for its paying side
/// may be from a month's expected date ([`fits_recurring_move`]): the
/// paying side may be [`MAX_BUSINESS_DAYS`] from it, and the receiving side
/// that many again from the paying one.
pub const MAX_WAITING_BUSINESS_DAYS: u32 = 2 * MAX_BUSINESS_DAYS;

/// A move between two of the user's own accounts that repeats every month
/// and that the user asked to be joined by itself (D-749).
///
/// It is not a schedule and creates nothing: it only recognises the two
/// sides statements bring.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecurringMove {
    /// Its ID.
    pub id: RecurringMoveId,
    /// The account the money leaves.
    pub from_account: AccountId,
    /// The account the money arrives at.
    pub to_account: AccountId,
    /// The exact amount that leaves, greater than zero, in the currency of
    /// both accounts.
    pub amount: Money,
    /// The day of the month the paying side was booked on when the user
    /// remembered the move, 1 to 31.
    pub day_of_month: u8,
    /// The other party's IBAN on the paying side, as the bank delivered it
    /// (D-067), if it named one.
    pub from_party_iban: Option<String>,
    /// The other party's name on the paying side, as the bank delivered it,
    /// if it named one.
    pub from_party_name: Option<String>,
    /// The day the user remembered it.
    pub created_on: Date,
}

/// Why a move can't be remembered as it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecurringMoveError {
    /// The money would leave and arrive in the same account.
    SameAccount,
    /// The amount is zero or negative.
    AmountNotPositive,
    /// The day of the month is not 1 to 31.
    DayOutOfRange,
    /// The amount isn't in the currency of both accounts, so the move would
    /// cross currencies (Q-020).
    OtherCurrency,
}

impl RecurringMoveError {
    /// The word `docs/ERROR_CODES.md` gives for it.
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::SameAccount => "same-account",
            Self::AmountNotPositive => "amount",
            Self::DayOutOfRange => "day",
            Self::OtherCurrency => "currency",
        }
    }
}

impl RecurringMove {
    /// Checks what a remembered move must be by itself: two different
    /// accounts, an amount greater than zero and a day from 1 to 31.
    /// Whether the accounts are the user's own and in the amount's currency
    /// is for the book to check.
    pub fn check(&self) -> Result<(), RecurringMoveError> {
        if self.from_account == self.to_account {
            return Err(RecurringMoveError::SameAccount);
        }
        if self.amount.is_zero() || self.amount.is_negative() {
            return Err(RecurringMoveError::AmountNotPositive);
        }
        if !(1..=31).contains(&self.day_of_month) {
            return Err(RecurringMoveError::DayOutOfRange);
        }
        Ok(())
    }

    /// Whether `party`, the other party a paying side names, is compatible
    /// with the one this move remembers (D-749, 3.3 point 2): the same IBAN
    /// if it remembers one, spaces and capitals ignored; otherwise the same
    /// name if it remembers one, spaces and capitals ignored; otherwise no
    /// other party at all, or one of the receiving account's own IBANs
    /// (`receiving_ibans`, already without spaces and in capitals), never
    /// another IBAN. So a payment of the same amount to a shop on the same
    /// day can't be taken for the move.
    pub fn party_fits(&self, party: &OtherParty, receiving_ibans: &[String]) -> bool {
        if let Some(iban) = &self.from_party_iban {
            return party
                .iban
                .as_deref()
                .is_some_and(|given| compact_iban(given) == compact_iban(iban));
        }
        if let Some(name) = &self.from_party_name {
            return party
                .name
                .as_deref()
                .is_some_and(|given| compact_name(given) == compact_name(name));
        }
        match party.iban.as_deref().map(compact_iban) {
            Some(iban) => receiving_ibans.contains(&iban),
            None => party.name.is_none(),
        }
    }
}

/// An IBAN without spaces and in capitals, the way it is compared.
pub fn compact_iban(iban: &str) -> String {
    iban.chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// A name without spaces and in small letters, the way a remembered
/// party's name is compared.
fn compact_name(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The other party a bank named on a side's account part (D-067): an empty
/// text counts as none.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OtherParty {
    /// Its IBAN or account number, as delivered.
    pub iban: Option<String>,
    /// Its name, as delivered.
    pub name: Option<String>,
}

impl OtherParty {
    /// The party with `iban` and `name`, where text of nothing but spaces
    /// counts as none.
    pub fn new(iban: Option<&str>, name: Option<&str>) -> Self {
        let given = |text: Option<&str>| {
            text.filter(|text| !text.trim().is_empty())
                .map(str::to_owned)
        };
        Self {
            iban: given(iban),
            name: given(name),
        }
    }
}

/// One side that may belong to a remembered move: the side and the other
/// party its bank named on the account part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveCandidate {
    /// The side. The caller passes only sides that may be joined by
    /// themselves: open ones and ones a Rule sorted, never one the user
    /// sorted by hand (D-031).
    pub side: TransferSide,
    /// The other party on its account part.
    pub party: OtherParty,
}

/// A pair that fits a remembered move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecurringMatch {
    /// The move it fits.
    pub recurring_move: RecurringMoveId,
    /// The two sides.
    pub pair: TransferMatch,
}

/// Sides that fit a remembered move but can't be joined by themselves,
/// because two of them fit equally well, or one fits two moves: the user
/// decides.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnclearMatch {
    /// The move.
    pub recurring_move: RecurringMoveId,
    /// The sides in question, sorted by date, then ID.
    pub transactions: Vec<TransactionId>,
}

/// What [`match_recurring_moves`] found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecurringMatches {
    /// The pairs to join, sorted by the paying side's date, then the IDs.
    pub matches: Vec<RecurringMatch>,
    /// The sides that need the user, sorted by move, then by the first
    /// side's date.
    pub unclear: Vec<UnclearMatch>,
}

/// The day a move remembered for `day` (1 to 31) is expected in `month` of
/// `year` (D-749, 3.2): the day, cut back to the month's last day when the
/// month is shorter, and moved forward to Monday when it falls on a Saturday
/// or Sunday, because a bank books a standing order on a banking day.
/// Public holidays aren't considered, as in D-030. `None` for a day outside
/// 1 to 31, a month outside 1 to 12, or a date outside the calendar
/// KassenSturz knows.
pub fn expected_date(day: u8, year: i16, month: i8) -> Option<Date> {
    if !(1..=31).contains(&day) {
        return None;
    }
    let first = Date::new(year, month, 1).ok()?;
    let day = i8::try_from(day).ok()?.min(first.days_in_month());
    let date = Date::new(year, month, day).ok()?;
    match date.weekday() {
        Weekday::Saturday => date.tomorrow().ok()?.tomorrow().ok(),
        Weekday::Sunday => date.tomorrow().ok(),
        _ => Some(date),
    }
}

/// The expected date of the month before, of and after `date`'s month for
/// `day`, and how far `date` is from the one closest to it, as business
/// days and calendar days (counted as for Transfers, D-030), if at most
/// `limit` business days.
fn nearest_expected(day: u8, date: Date, limit: u32) -> Option<(Date, u32, u32)> {
    let (year, month) = (date.year(), date.month());
    let months = [
        if month == 1 {
            (year.checked_sub(1)?, 12)
        } else {
            (year, month - 1)
        },
        (year, month),
        if month == 12 {
            (year.checked_add(1)?, 1)
        } else {
            (year, month + 1)
        },
    ];
    months
        .into_iter()
        .filter_map(|(year, month)| expected_date(day, year, month))
        .map(|expected| {
            let (business, calendar) = days_between(date, expected);
            (expected, business, calendar)
        })
        .filter(|(_, business, _)| *business <= limit)
        .min_by_key(|(expected, business, calendar)| (*business, *calendar, *expected))
}

/// Whether `candidate` fits `recurring_move` as its paying side (D-749, 3.2
/// and 3.3): out of its account, exactly its amount, a compatible other
/// party, and at most [`MAX_BUSINESS_DAYS`] from a month's expected date.
/// Returns that expected date and how far the side is from it.
fn as_paying(
    recurring_move: &RecurringMove,
    candidate: &MoveCandidate,
    receiving_ibans: &[String],
) -> Option<(Date, u32, u32)> {
    let side = &candidate.side;
    let leaves = recurring_move.amount.checked_neg().ok()?;
    if side.account != recurring_move.from_account
        || side.amount != leaves
        || !recurring_move.party_fits(&candidate.party, receiving_ibans)
    {
        return None;
    }
    nearest_expected(
        recurring_move.day_of_month,
        side.booking_date,
        MAX_BUSINESS_DAYS,
    )
}

/// Whether `candidate` could be the receiving side of `recurring_move`:
/// into its account, exactly its amount.
fn as_receiving(recurring_move: &RecurringMove, candidate: &MoveCandidate) -> bool {
    candidate.side.account == recurring_move.to_account
        && candidate.side.amount == recurring_move.amount
}

/// Whether `candidate` is a side of `recurring_move` whose other half may
/// still be missing (D-749, 3.5): a paying side as [`match_recurring_moves`]
/// takes one, or a receiving side into its account with exactly its amount
/// at most [`MAX_WAITING_BUSINESS_DAYS`] from a month's expected date. A
/// Rule leaves such a side alone, so the move isn't booked twice.
/// `receiving_ibans` are the receiving account's own IBANs, without spaces
/// and in capitals.
pub fn fits_recurring_move(
    recurring_move: &RecurringMove,
    candidate: &MoveCandidate,
    receiving_ibans: &[String],
) -> bool {
    if as_paying(recurring_move, candidate, receiving_ibans).is_some() {
        return true;
    }
    as_receiving(recurring_move, candidate)
        && nearest_expected(
            recurring_move.day_of_month,
            candidate.side.booking_date,
            MAX_WAITING_BUSINESS_DAYS,
        )
        .is_some()
}

/// The pairs among `candidates` that fit the remembered `moves`, and the
/// sides that need the user (D-749, 3.2 and 3.3).
///
/// For each move, every paying side is placed at the month's expected date
/// closest to it. Per expected date, the closest paying side is taken
/// (fewest business days, then calendar days, then the earlier date); its
/// receiving side is the side into the move's other account, with exactly
/// the amount, at most [`MAX_BUSINESS_DAYS`] business days from it, the
/// closest again, and the incoming side may come first. Two sides that are
/// equally close on the same day can't be told apart, so neither is joined
/// and the move's sides of that date are [`UnclearMatch`]es. A closest
/// paying side without its receiving side yet joins nothing for that date;
/// a side further away is never taken in its place.
///
/// Pairs are made per move. A side that two pairs would use, of one move
/// or of two, is joined by neither: those sides are unclear too. So the
/// result never depends on the order of `moves` or `candidates`.
/// `own_ibans` are each account's own IBANs, without spaces and in
/// capitals, for [`RecurringMove::party_fits`].
pub fn match_recurring_moves(
    moves: &[RecurringMove],
    candidates: &[MoveCandidate],
    own_ibans: &HashMap<AccountId, Vec<String>>,
) -> RecurringMatches {
    let none = Vec::new();
    let mut proposed: Vec<RecurringMatch> = Vec::new();
    let mut unclear: Vec<UnclearMatch> = Vec::new();
    for recurring_move in moves {
        let receiving_ibans = own_ibans.get(&recurring_move.to_account).unwrap_or(&none);
        // The paying sides, per expected date.
        let mut by_date: HashMap<Date, Vec<(u32, u32, &TransferSide)>> = HashMap::new();
        for candidate in candidates {
            if let Some((expected, business, calendar)) =
                as_paying(recurring_move, candidate, receiving_ibans)
            {
                by_date
                    .entry(expected)
                    .or_default()
                    .push((business, calendar, &candidate.side));
            }
        }
        let receiving: Vec<&TransferSide> = candidates
            .iter()
            .filter(|candidate| as_receiving(recurring_move, candidate))
            .map(|candidate| &candidate.side)
            .collect();
        let mut dates: Vec<Date> = by_date.keys().copied().collect();
        dates.sort();
        for expected in dates {
            let mut paying = by_date.remove(&expected).unwrap_or_default();
            paying.sort_by_key(|(business, calendar, side)| {
                (*business, *calendar, side.booking_date, side.transaction)
            });
            let closest = |(business, calendar, side): &(u32, u32, &TransferSide)| {
                (*business, *calendar, side.booking_date)
            };
            let Some(first) = paying.first() else {
                continue;
            };
            let tied: Vec<TransactionId> = paying
                .iter()
                .filter(|other| closest(other) == closest(first))
                .map(|(_, _, side)| side.transaction)
                .collect();
            if tied.len() > 1 {
                unclear.push(UnclearMatch {
                    recurring_move: recurring_move.id,
                    transactions: tied,
                });
                continue;
            }
            let outgoing = *first.2;
            let mut partners: Vec<(u32, u32, &TransferSide)> = receiving
                .iter()
                .filter(|side| side.transaction != outgoing.transaction)
                .filter_map(|side| {
                    let (business, calendar) =
                        days_between(outgoing.booking_date, side.booking_date);
                    (business <= MAX_BUSINESS_DAYS).then_some((business, calendar, *side))
                })
                .collect();
            partners.sort_by_key(|(business, calendar, side)| {
                (*business, *calendar, side.booking_date, side.transaction)
            });
            let Some(partner) = partners.first() else {
                continue;
            };
            let tied: Vec<TransactionId> = partners
                .iter()
                .filter(|other| closest(other) == closest(partner))
                .map(|(_, _, side)| side.transaction)
                .collect();
            if tied.len() > 1 {
                let mut transactions = vec![outgoing.transaction];
                transactions.extend(tied);
                unclear.push(UnclearMatch {
                    recurring_move: recurring_move.id,
                    transactions,
                });
                continue;
            }
            proposed.push(RecurringMatch {
                recurring_move: recurring_move.id,
                pair: TransferMatch {
                    outgoing,
                    incoming: *partner.2,
                    business_days: partner.0,
                },
            });
        }
    }

    // A side two pairs would use is joined by neither.
    let mut uses: HashMap<TransactionId, usize> = HashMap::new();
    for found in &proposed {
        for side in [found.pair.outgoing, found.pair.incoming] {
            *uses.entry(side.transaction).or_default() += 1;
        }
    }
    let (mut matches, shared): (Vec<RecurringMatch>, Vec<RecurringMatch>) =
        proposed.into_iter().partition(|found| {
            uses[&found.pair.outgoing.transaction] == 1
                && uses[&found.pair.incoming.transaction] == 1
        });
    for found in shared {
        unclear.push(UnclearMatch {
            recurring_move: found.recurring_move,
            transactions: vec![
                found.pair.outgoing.transaction,
                found.pair.incoming.transaction,
            ],
        });
    }

    let dates: HashMap<TransactionId, Date> = candidates
        .iter()
        .map(|candidate| (candidate.side.transaction, candidate.side.booking_date))
        .collect();
    for found in &mut unclear {
        found
            .transactions
            .sort_by_key(|transaction| (dates.get(transaction).copied(), *transaction));
        found.transactions.dedup();
    }
    unclear.sort_by_key(|found| {
        (
            found.recurring_move,
            found
                .transactions
                .first()
                .and_then(|transaction| dates.get(transaction).copied()),
            found.transactions.clone(),
        )
    });
    unclear.dedup();
    matches.sort_by_key(|found| {
        (
            found.pair.outgoing.booking_date,
            found.pair.outgoing.transaction,
            found.pair.incoming.transaction,
        )
    });
    RecurringMatches { matches, unclear }
}

#[cfg(test)]
mod tests;
