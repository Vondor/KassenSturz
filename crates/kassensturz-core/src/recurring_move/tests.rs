// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of monthly moves (D-749). Every name, IBAN and amount is invented;
//! the IBANs are SWIFT's examples (D-055).

use proptest::prelude::*;

use super::*;
use crate::currency::CurrencyCode;
use crate::transaction::{Split, Transaction};
use crate::transfer::join_transfer;

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};
const USD: CurrencyCode = match CurrencyCode::new("USD") {
    Ok(code) => code,
    Err(_) => panic!("USD is a valid code"),
};

/// SWIFT's example IBANs (D-055): the savings account's own, and a shop's.
const SAVINGS_IBAN: &str = "DE89 3704 0044 0532 0130 00";
const SHOP_IBAN: &str = "GB29NWBK60161331926819";

fn eur(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, EUR)
}

fn day(year: i16, month: i8, day: i8) -> Date {
    Date::new(year, month, day).expect("valid date")
}

struct Accounts {
    checking: AccountId,
    savings: AccountId,
    holiday: AccountId,
}

fn accounts() -> Accounts {
    Accounts {
        checking: AccountId::generate(),
        savings: AccountId::generate(),
        holiday: AccountId::generate(),
    }
}

fn monthly(from: AccountId, to: AccountId, minor_units: i64, on: u8) -> RecurringMove {
    RecurringMove {
        id: RecurringMoveId::generate(),
        from_account: from,
        to_account: to,
        amount: eur(minor_units),
        day_of_month: on,
        from_party_iban: None,
        from_party_name: None,
        created_on: day(2026, 9, 1),
    }
}

fn candidate(account: AccountId, minor_units: i64, on: Date) -> MoveCandidate {
    MoveCandidate {
        side: TransferSide {
            transaction: TransactionId::generate(),
            booking_date: on,
            account,
            amount: eur(minor_units),
            category: None,
        },
        party: OtherParty::default(),
    }
}

fn with_party(
    mut candidate: MoveCandidate,
    iban: Option<&str>,
    name: Option<&str>,
) -> MoveCandidate {
    candidate.party = OtherParty::new(iban, name);
    candidate
}

fn own_ibans(accounts: &Accounts) -> HashMap<AccountId, Vec<String>> {
    HashMap::from([(accounts.savings, vec![compact_iban(SAVINGS_IBAN)])])
}

fn pairs(found: &RecurringMatches) -> Vec<(TransactionId, TransactionId)> {
    found
        .matches
        .iter()
        .map(|found| {
            (
                found.pair.outgoing.transaction,
                found.pair.incoming.transaction,
            )
        })
        .collect()
}

fn id(candidate: &MoveCandidate) -> TransactionId {
    candidate.side.transaction
}

#[test]
fn the_expected_dates_are_those_of_the_design() {
    // The rows of `docs/RECURRING_MOVES.md`, 3.2.
    assert_eq!(expected_date(1, 2026, 10), Some(day(2026, 10, 1)));
    assert_eq!(day(2026, 10, 1).weekday(), Weekday::Thursday);
    // The 1st of November 2026 is a Sunday.
    assert_eq!(day(2026, 11, 1).weekday(), Weekday::Sunday);
    assert_eq!(expected_date(1, 2026, 11), Some(day(2026, 11, 2)));
    // April has 30 days.
    assert_eq!(expected_date(31, 2027, 4), Some(day(2027, 4, 30)));
    assert_eq!(day(2027, 4, 30).weekday(), Weekday::Friday);
    // Cut back to Sunday 28 February, moved to Monday.
    assert_eq!(day(2027, 2, 28).weekday(), Weekday::Sunday);
    assert_eq!(expected_date(31, 2027, 2), Some(day(2027, 3, 1)));
    assert_eq!(expected_date(29, 2027, 2), Some(day(2027, 3, 1)));
    assert_eq!(expected_date(1, 2027, 1), Some(day(2027, 1, 1)));
    assert_eq!(day(2027, 1, 1).weekday(), Weekday::Friday);
    // A leap year keeps the 29th: Tuesday 29 February 2028.
    assert_eq!(expected_date(30, 2028, 2), Some(day(2028, 2, 29)));
    // A Saturday moves two days on.
    assert_eq!(day(2026, 10, 10).weekday(), Weekday::Saturday);
    assert_eq!(expected_date(10, 2026, 10), Some(day(2026, 10, 12)));
}

#[test]
fn an_impossible_day_or_month_has_no_expected_date() {
    assert_eq!(expected_date(0, 2026, 10), None);
    assert_eq!(expected_date(32, 2026, 10), None);
    assert_eq!(expected_date(1, 2026, 13), None);
    assert_eq!(expected_date(1, 2026, 0), None);
}

#[test]
fn a_move_must_be_between_two_accounts_with_an_amount_and_a_day() {
    let a = accounts();
    assert_eq!(monthly(a.checking, a.savings, 10_000, 1).check(), Ok(()));
    assert_eq!(
        monthly(a.checking, a.checking, 10_000, 1).check(),
        Err(RecurringMoveError::SameAccount)
    );
    assert_eq!(
        monthly(a.checking, a.savings, 0, 1).check(),
        Err(RecurringMoveError::AmountNotPositive)
    );
    assert_eq!(
        monthly(a.checking, a.savings, -10_000, 1).check(),
        Err(RecurringMoveError::AmountNotPositive)
    );
    for wrong in [0, 32] {
        assert_eq!(
            monthly(a.checking, a.savings, 10_000, wrong).check(),
            Err(RecurringMoveError::DayOutOfRange)
        );
    }
}

#[test]
fn a_pair_on_the_expected_day_is_joined() {
    let a = accounts();
    let moved = monthly(a.checking, a.savings, 10_000, 1);
    let out = candidate(a.checking, -10_000, day(2026, 10, 1));
    let into = candidate(a.savings, 10_000, day(2026, 10, 1));
    let found = match_recurring_moves(
        std::slice::from_ref(&moved),
        &[out.clone(), into.clone()],
        &own_ibans(&a),
    );
    assert_eq!(pairs(&found), vec![(id(&out), id(&into))]);
    assert_eq!(found.matches[0].recurring_move, moved.id);
    assert!(found.unclear.is_empty());
}

#[test]
fn the_incoming_side_may_come_first_and_a_pair_may_cross_a_month_end() {
    let a = accounts();
    let moved = monthly(a.checking, a.savings, 10_000, 1);
    // Expected Monday 2 November; the money arrives on Friday 30 October and
    // leaves on Monday 2 November.
    let into = candidate(a.savings, 10_000, day(2026, 10, 30));
    let out = candidate(a.checking, -10_000, day(2026, 11, 2));
    let found = match_recurring_moves(&[moved], &[into.clone(), out.clone()], &own_ibans(&a));
    assert_eq!(pairs(&found), vec![(id(&out), id(&into))]);
    assert_eq!(found.matches[0].pair.business_days, 1);
}

#[test]
fn an_entry_late_in_december_fits_the_first_of_january() {
    let a = accounts();
    let moved = monthly(a.checking, a.savings, 10_000, 1);
    // Thursday 31 December 2026, one business day before Friday 1 January.
    let out = candidate(a.checking, -10_000, day(2026, 12, 31));
    let into = candidate(a.savings, 10_000, day(2027, 1, 1));
    let found = match_recurring_moves(&[moved], &[out.clone(), into.clone()], &own_ibans(&a));
    assert_eq!(pairs(&found), vec![(id(&out), id(&into))]);
}

#[test]
fn an_entry_too_far_from_the_expected_day_is_not_joined() {
    let a = accounts();
    let moved = monthly(a.checking, a.savings, 10_000, 1);
    // Thursday 1 October expected: Tuesday 6 October is three business days
    // after it, Wednesday 7 October four.
    let near = [
        candidate(a.checking, -10_000, day(2026, 10, 6)),
        candidate(a.savings, 10_000, day(2026, 10, 6)),
    ];
    assert_eq!(
        match_recurring_moves(std::slice::from_ref(&moved), &near, &own_ibans(&a))
            .matches
            .len(),
        1
    );
    let far = [
        candidate(a.checking, -10_000, day(2026, 10, 7)),
        candidate(a.savings, 10_000, day(2026, 10, 7)),
    ];
    let found = match_recurring_moves(std::slice::from_ref(&moved), &far, &own_ibans(&a));
    assert!(found.matches.is_empty());
    assert!(found.unclear.is_empty());
    assert!(!fits_recurring_move(&moved, &far[0], &[]));
}

#[test]
fn another_amount_or_currency_is_not_joined() {
    let a = accounts();
    let moved = monthly(a.checking, a.savings, 10_000, 1);
    let mut dollars = candidate(a.checking, -10_000, day(2026, 10, 1));
    dollars.side.amount = Money::from_minor_units(-10_000, USD);
    let mut dollars_in = candidate(a.savings, 10_000, day(2026, 10, 1));
    dollars_in.side.amount = Money::from_minor_units(10_000, USD);
    let other_amount = [
        candidate(a.checking, -10_001, day(2026, 10, 1)),
        candidate(a.savings, 10_001, day(2026, 10, 1)),
    ];
    for sides in [vec![dollars, dollars_in], other_amount.to_vec()] {
        let found = match_recurring_moves(std::slice::from_ref(&moved), &sides, &own_ibans(&a));
        assert!(found.matches.is_empty());
    }
}

#[test]
fn the_wrong_direction_or_other_accounts_are_not_joined() {
    let a = accounts();
    let moved = monthly(a.checking, a.savings, 10_000, 1);
    let backwards = [
        candidate(a.savings, -10_000, day(2026, 10, 1)),
        candidate(a.checking, 10_000, day(2026, 10, 1)),
    ];
    let elsewhere = [
        candidate(a.checking, -10_000, day(2026, 10, 1)),
        candidate(a.holiday, 10_000, day(2026, 10, 1)),
    ];
    for sides in [backwards, elsewhere] {
        assert!(
            match_recurring_moves(std::slice::from_ref(&moved), &sides, &own_ibans(&a))
                .matches
                .is_empty()
        );
    }
}

#[test]
fn two_entries_that_fit_equally_well_are_not_joined() {
    let a = accounts();
    let moved = monthly(a.checking, a.savings, 10_000, 1);
    let first = candidate(a.checking, -10_000, day(2026, 10, 1));
    let second = candidate(a.checking, -10_000, day(2026, 10, 1));
    let into = candidate(a.savings, 10_000, day(2026, 10, 1));
    let found = match_recurring_moves(
        std::slice::from_ref(&moved),
        &[first.clone(), second.clone(), into],
        &own_ibans(&a),
    );
    assert!(found.matches.is_empty());
    let mut expected = vec![id(&first), id(&second)];
    expected.sort_by_key(|transaction| (day(2026, 10, 1), *transaction));
    assert_eq!(
        found.unclear,
        vec![UnclearMatch {
            recurring_move: moved.id,
            transactions: expected
        }]
    );
}

#[test]
fn two_receiving_entries_on_one_day_are_not_joined() {
    let a = accounts();
    let moved = monthly(a.checking, a.savings, 10_000, 1);
    let out = candidate(a.checking, -10_000, day(2026, 10, 1));
    let first = candidate(a.savings, 10_000, day(2026, 10, 2));
    let second = candidate(a.savings, 10_000, day(2026, 10, 2));
    let found = match_recurring_moves(
        std::slice::from_ref(&moved),
        &[out.clone(), first.clone(), second.clone()],
        &own_ibans(&a),
    );
    assert!(found.matches.is_empty());
    assert_eq!(found.unclear.len(), 1);
    let mut unclear = found.unclear[0].transactions.clone();
    unclear.sort();
    let mut expected = vec![id(&out), id(&first), id(&second)];
    expected.sort();
    assert_eq!(unclear, expected);
}

#[test]
fn one_pair_per_expected_date_the_closest_one() {
    let a = accounts();
    let moved = monthly(a.checking, a.savings, 10_000, 1);
    // Expected Thursday 1 October: Friday 2 October is closer than Monday 5.
    let closest = candidate(a.checking, -10_000, day(2026, 10, 2));
    let later = candidate(a.checking, -10_000, day(2026, 10, 5));
    let into = candidate(a.savings, 10_000, day(2026, 10, 2));
    let into_later = candidate(a.savings, 10_000, day(2026, 10, 5));
    let found = match_recurring_moves(
        &[moved],
        &[later, into_later, closest.clone(), into.clone()],
        &own_ibans(&a),
    );
    assert_eq!(pairs(&found), vec![(id(&closest), id(&into))]);
    assert!(found.unclear.is_empty());
}

#[test]
fn equally_close_on_different_days_takes_the_earlier() {
    let a = accounts();
    let moved = monthly(a.checking, a.savings, 10_000, 7);
    // Expected Wednesday 7 October: Tuesday 6 and Thursday 8 are one day
    // from it each.
    let earlier = candidate(a.checking, -10_000, day(2026, 10, 6));
    let later = candidate(a.checking, -10_000, day(2026, 10, 8));
    let into = candidate(a.savings, 10_000, day(2026, 10, 6));
    let found = match_recurring_moves(
        &[moved],
        &[later, earlier.clone(), into.clone()],
        &own_ibans(&a),
    );
    assert_eq!(pairs(&found), vec![(id(&earlier), id(&into))]);
}

#[test]
fn the_closest_paying_side_without_its_other_half_joins_nothing() {
    let a = accounts();
    let moved = monthly(a.checking, a.savings, 10_000, 1);
    let closest = candidate(a.checking, -10_000, day(2026, 10, 1));
    let further = candidate(a.checking, -10_000, day(2026, 10, 5));
    let into = candidate(a.savings, 10_000, day(2026, 10, 6));
    // Thursday 1 to Tuesday 6 is three business days, so `into` is the
    // closest side's.
    let found = match_recurring_moves(
        std::slice::from_ref(&moved),
        &[closest.clone(), further.clone(), into.clone()],
        &own_ibans(&a),
    );
    assert_eq!(pairs(&found), vec![(id(&closest), id(&into))]);
    // Without a receiving side near the closest one, `further` isn't taken
    // in its place.
    let late_into = candidate(a.savings, 10_000, day(2026, 10, 8));
    let found = match_recurring_moves(&[moved], &[closest, further, late_into], &own_ibans(&a));
    assert!(found.matches.is_empty());
}

#[test]
fn every_month_has_its_own_pair() {
    let a = accounts();
    let moved = monthly(a.checking, a.savings, 10_000, 1);
    let sides = [
        candidate(a.checking, -10_000, day(2026, 10, 1)),
        candidate(a.savings, 10_000, day(2026, 10, 1)),
        candidate(a.checking, -10_000, day(2026, 11, 2)),
        candidate(a.savings, 10_000, day(2026, 11, 3)),
        candidate(a.checking, -10_000, day(2026, 12, 1)),
        candidate(a.savings, 10_000, day(2026, 12, 1)),
    ];
    let found = match_recurring_moves(&[moved], &sides, &own_ibans(&a));
    assert_eq!(
        pairs(&found),
        vec![
            (id(&sides[0]), id(&sides[1])),
            (id(&sides[2]), id(&sides[3])),
            (id(&sides[4]), id(&sides[5])),
        ]
    );
}

#[test]
fn a_remembered_iban_must_be_named() {
    let a = accounts();
    let moved = RecurringMove {
        from_party_iban: Some(SAVINGS_IBAN.to_owned()),
        ..monthly(a.checking, a.savings, 10_000, 1)
    };
    let into = candidate(a.savings, 10_000, day(2026, 10, 1));
    let named = with_party(
        candidate(a.checking, -10_000, day(2026, 10, 1)),
        Some("de89370400440532013000"),
        Some("Savings"),
    );
    let found = match_recurring_moves(
        std::slice::from_ref(&moved),
        &[named.clone(), into.clone()],
        &own_ibans(&a),
    );
    assert_eq!(pairs(&found), vec![(id(&named), id(&into))]);
    for party in [
        (None, Some("Savings")),
        (Some(SHOP_IBAN), None),
        (None, None),
    ] {
        let other = with_party(
            candidate(a.checking, -10_000, day(2026, 10, 1)),
            party.0,
            party.1,
        );
        let found = match_recurring_moves(
            std::slice::from_ref(&moved),
            &[other, into.clone()],
            &own_ibans(&a),
        );
        assert!(found.matches.is_empty(), "{party:?}");
    }
}

#[test]
fn a_remembered_name_must_be_named_capitals_and_spaces_aside() {
    let a = accounts();
    let moved = RecurringMove {
        from_party_name: Some("Ada Example".to_owned()),
        ..monthly(a.checking, a.savings, 10_000, 1)
    };
    let into = candidate(a.savings, 10_000, day(2026, 10, 1));
    let named = with_party(
        candidate(a.checking, -10_000, day(2026, 10, 1)),
        Some(SHOP_IBAN),
        Some("ADA  example"),
    );
    let found = match_recurring_moves(
        std::slice::from_ref(&moved),
        &[named.clone(), into.clone()],
        &own_ibans(&a),
    );
    assert_eq!(pairs(&found), vec![(id(&named), id(&into))]);
    let stranger = with_party(
        candidate(a.checking, -10_000, day(2026, 10, 1)),
        None,
        Some("Corner Shop"),
    );
    let found = match_recurring_moves(&[moved], &[stranger, into], &own_ibans(&a));
    assert!(found.matches.is_empty());
}

#[test]
fn without_a_remembered_party_a_stranger_is_never_taken_for_the_move() {
    let a = accounts();
    let moved = monthly(a.checking, a.savings, 10_000, 1);
    let into = candidate(a.savings, 10_000, day(2026, 10, 1));
    // No other party, or the savings account's own IBAN: the move.
    for party in [(None, None), (Some(SAVINGS_IBAN), Some("Me"))] {
        let out = with_party(
            candidate(a.checking, -10_000, day(2026, 10, 1)),
            party.0,
            party.1,
        );
        let found = match_recurring_moves(
            std::slice::from_ref(&moved),
            &[out.clone(), into.clone()],
            &own_ibans(&a),
        );
        assert_eq!(pairs(&found), vec![(id(&out), id(&into))], "{party:?}");
    }
    // A shop's IBAN, or a name alone: not the move.
    for party in [(Some(SHOP_IBAN), None), (None, Some("Corner Shop"))] {
        let out = with_party(
            candidate(a.checking, -10_000, day(2026, 10, 1)),
            party.0,
            party.1,
        );
        let found = match_recurring_moves(
            std::slice::from_ref(&moved),
            &[out.clone(), into.clone()],
            &own_ibans(&a),
        );
        assert!(found.matches.is_empty(), "{party:?}");
        assert!(!fits_recurring_move(
            &moved,
            &out,
            &[compact_iban(SAVINGS_IBAN)]
        ));
    }
}

#[test]
fn a_stranger_on_the_same_day_does_not_stop_the_move() {
    let a = accounts();
    let moved = RecurringMove {
        from_party_iban: Some(SAVINGS_IBAN.to_owned()),
        ..monthly(a.checking, a.savings, 10_000, 1)
    };
    let shop = with_party(
        candidate(a.checking, -10_000, day(2026, 10, 1)),
        Some(SHOP_IBAN),
        Some("Corner Shop"),
    );
    let out = with_party(
        candidate(a.checking, -10_000, day(2026, 10, 1)),
        Some(SAVINGS_IBAN),
        None,
    );
    let into = candidate(a.savings, 10_000, day(2026, 10, 1));
    let found = match_recurring_moves(&[moved], &[shop, out.clone(), into.clone()], &own_ibans(&a));
    assert_eq!(pairs(&found), vec![(id(&out), id(&into))]);
}

#[test]
fn two_moves_alike_but_for_the_receiving_account_are_told_apart() {
    // The case of #784: the same amount on the same day to two accounts.
    let a = accounts();
    let holiday_iban = "NL91ABNA0417164300";
    let to_savings = RecurringMove {
        from_party_iban: Some(SAVINGS_IBAN.to_owned()),
        ..monthly(a.checking, a.savings, 5_000, 1)
    };
    let to_holiday = RecurringMove {
        from_party_iban: Some(holiday_iban.to_owned()),
        ..monthly(a.checking, a.holiday, 5_000, 1)
    };
    let out_savings = with_party(
        candidate(a.checking, -5_000, day(2026, 10, 1)),
        Some(SAVINGS_IBAN),
        None,
    );
    let out_holiday = with_party(
        candidate(a.checking, -5_000, day(2026, 10, 1)),
        Some(holiday_iban),
        None,
    );
    let into_savings = candidate(a.savings, 5_000, day(2026, 10, 1));
    let into_holiday = candidate(a.holiday, 5_000, day(2026, 10, 2));
    let sides = [
        out_holiday.clone(),
        into_savings.clone(),
        out_savings.clone(),
        into_holiday.clone(),
    ];
    let found = match_recurring_moves(
        &[to_savings.clone(), to_holiday.clone()],
        &sides,
        &own_ibans(&a),
    );
    let mut joined: Vec<(RecurringMoveId, TransactionId, TransactionId)> = found
        .matches
        .iter()
        .map(|found| {
            (
                found.recurring_move,
                found.pair.outgoing.transaction,
                found.pair.incoming.transaction,
            )
        })
        .collect();
    joined.sort();
    let mut expected = vec![
        (to_savings.id, id(&out_savings), id(&into_savings)),
        (to_holiday.id, id(&out_holiday), id(&into_holiday)),
    ];
    expected.sort();
    assert_eq!(joined, expected);
}

#[test]
fn two_moves_that_both_want_one_side_join_neither() {
    let a = accounts();
    // Neither remembers a party, so one paying side without one fits both.
    let to_savings = monthly(a.checking, a.savings, 5_000, 1);
    let to_holiday = monthly(a.checking, a.holiday, 5_000, 1);
    let out = candidate(a.checking, -5_000, day(2026, 10, 1));
    let into_savings = candidate(a.savings, 5_000, day(2026, 10, 1));
    let into_holiday = candidate(a.holiday, 5_000, day(2026, 10, 1));
    let found = match_recurring_moves(
        &[to_savings, to_holiday],
        &[out, into_savings, into_holiday],
        &HashMap::new(),
    );
    assert!(found.matches.is_empty());
    assert_eq!(found.unclear.len(), 2);
}

#[test]
fn a_side_that_waits_for_its_other_half_fits_the_move() {
    let a = accounts();
    let moved = monthly(a.checking, a.savings, 10_000, 1);
    let out = candidate(a.checking, -10_000, day(2026, 10, 2));
    assert!(fits_recurring_move(&moved, &out, &[]));
    // A receiving side may be up to six business days from the expected
    // Thursday 1 October: Friday 9 October is six, Monday 12 seven.
    let into = candidate(a.savings, 10_000, day(2026, 10, 9));
    assert!(fits_recurring_move(&moved, &into, &[]));
    let too_late = candidate(a.savings, 10_000, day(2026, 10, 12));
    assert!(!fits_recurring_move(&moved, &too_late, &[]));
    let elsewhere = candidate(a.holiday, 10_000, day(2026, 10, 1));
    assert!(!fits_recurring_move(&moved, &elsewhere, &[]));
    let other_amount = candidate(a.checking, -9_999, day(2026, 10, 1));
    assert!(!fits_recurring_move(&moved, &other_amount, &[]));
}

#[test]
fn an_empty_party_counts_as_none() {
    assert_eq!(OtherParty::new(Some("  "), Some("")), OtherParty::default());
    assert_eq!(
        OtherParty::new(Some(SHOP_IBAN), None).iban.as_deref(),
        Some(SHOP_IBAN)
    );
}

/// Sides on days around the turn of October 2026, in three accounts, with
/// two amounts, and moves between them.
/// Moves as (from, to, amount, day) and sides as (account, amount, day
/// from 1 September, out), accounts counted from 0.
type ArbitraryMoves = Vec<(usize, usize, i64, u8)>;
type ArbitrarySides = Vec<(usize, i64, i8, bool)>;

fn arbitrary_book() -> impl Strategy<Value = (ArbitraryMoves, ArbitrarySides)> {
    let moves = prop::collection::vec(
        (
            0..3usize,
            0..3usize,
            prop::sample::select(vec![5_000i64, 10_000]),
            1..=31u8,
        ),
        0..4,
    );
    let sides = prop::collection::vec(
        (
            0..3usize,
            prop::sample::select(vec![5_000i64, 10_000]),
            1..=60i8,
            any::<bool>(),
        ),
        0..14,
    );
    (moves, sides)
}

proptest! {
    /// Whatever the book holds: a side is joined at most once, never also
    /// unclear, every pair fits its move, the order of the input changes
    /// nothing, and joining the pairs leaves every account's balance as it
    /// was (principle 1).
    #[test]
    fn matching_is_safe_for_the_money((moves, sides) in arbitrary_book()) {
        let ids = [AccountId::generate(), AccountId::generate(), AccountId::generate()];
        let moves: Vec<RecurringMove> = moves
            .into_iter()
            .filter(|(from, to, _, _)| from != to)
            .map(|(from, to, amount, on)| monthly(ids[from], ids[to], amount, on))
            .collect();
        let start = day(2026, 9, 1);
        let transactions: Vec<Transaction> = sides
            .iter()
            .map(|&(account, amount, offset, out)| {
                let amount = if out { -amount } else { amount };
                let on = start.checked_add(jiff::Span::new().days(i64::from(offset))).expect("in range");
                Transaction::new(
                    TransactionId::generate(),
                    on,
                    None,
                    vec![Split::new(ids[account], eur(amount)), Split::uncategorized(eur(-amount))],
                )
                .expect("balanced")
            })
            .collect();
        let is_account = |id: AccountId| ids.contains(&id);
        let candidates: Vec<MoveCandidate> = transactions
            .iter()
            .map(|transaction| MoveCandidate {
                side: TransferSide::of(transaction, is_account).expect("a side"),
                party: OtherParty::default(),
            })
            .collect();
        let found = match_recurring_moves(&moves, &candidates, &HashMap::new());

        let mut reversed_moves = moves.clone();
        reversed_moves.reverse();
        let mut reversed = candidates.clone();
        reversed.reverse();
        prop_assert_eq!(&match_recurring_moves(&reversed_moves, &reversed, &HashMap::new()), &found);

        let mut used = std::collections::HashSet::new();
        for found in &found.matches {
            let moved = moves.iter().find(|moved| moved.id == found.recurring_move).expect("its move");
            prop_assert_eq!(found.pair.outgoing.account, moved.from_account);
            prop_assert_eq!(found.pair.incoming.account, moved.to_account);
            prop_assert_eq!(found.pair.incoming.amount, moved.amount);
            prop_assert_eq!(found.pair.outgoing.amount, moved.amount.checked_neg().expect("small"));
            prop_assert!(found.pair.business_days <= MAX_BUSINESS_DAYS);
            prop_assert!(used.insert(found.pair.outgoing.transaction));
            prop_assert!(used.insert(found.pair.incoming.transaction));
        }
        for unclear in &found.unclear {
            for transaction in &unclear.transactions {
                prop_assert!(!used.contains(transaction));
            }
        }

        // Join every pair and compare each account's balance.
        let balance = |transactions: &[Transaction], account: AccountId| -> i64 {
            transactions
                .iter()
                .flat_map(Transaction::splits)
                .filter(|split| split.account == Some(account))
                .map(|split| split.amount.minor_units())
                .sum()
        };
        let mut book = transactions.clone();
        for found in &found.matches {
            let take = |book: &mut Vec<Transaction>, id: TransactionId| {
                let at = book.iter().position(|t| t.id() == id).expect("still there");
                book.remove(at)
            };
            let out = take(&mut book, found.pair.outgoing.transaction);
            let into = take(&mut book, found.pair.incoming.transaction);
            book.push(join_transfer(&out, &into, is_account).expect("joins"));
        }
        for account in ids {
            prop_assert_eq!(balance(&book, account), balance(&transactions, account));
        }
    }
}
