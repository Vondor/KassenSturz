// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of planned payments and the forecast (D-769, D-770). Every name,
//! IBAN and amount is invented; the IBANs are SWIFT's examples (D-055).

use proptest::prelude::*;

use super::*;
use crate::currency::CurrencyCode;

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};
const USD: CurrencyCode = match CurrencyCode::new("USD") {
    Ok(code) => code,
    Err(_) => panic!("USD is a valid code"),
};

/// SWIFT's example IBANs (D-055).
const LANDLORD_IBAN: &str = "DE89 3704 0044 0532 0130 00";
const GYM_IBAN: &str = "GB29NWBK60161331926819";

fn eur(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, EUR)
}

fn day(year: i16, month: i8, day: i8) -> Date {
    Date::new(year, month, day).expect("valid date")
}

fn plan(
    account: AccountId,
    minor_units: i64,
    unit: ScheduleUnit,
    every: u8,
    anchor: Date,
) -> Schedule {
    Schedule {
        id: ScheduleId::generate(),
        name: "Rent".to_owned(),
        account,
        amount: eur(minor_units),
        tolerance: eur(0),
        category: None,
        party_iban: None,
        party_name: None,
        unit,
        every,
        anchor,
        track_from: anchor,
        ends_on: None,
        paused: false,
    }
}

fn monthly(account: AccountId, minor_units: i64, anchor: Date) -> Schedule {
    plan(account, minor_units, ScheduleUnit::Month, 1, anchor)
}

fn dates(schedule: &Schedule, from: Date, to: Date) -> Vec<Date> {
    due_dates(schedule, from, to)
        .into_iter()
        .map(|due| due.date)
        .collect()
}

fn entry(account: AccountId, minor_units: i64, on: Date) -> ScheduleEntry {
    ScheduleEntry {
        transaction: TransactionId::generate(),
        account,
        amount: eur(minor_units),
        booking_date: on,
        party: OtherParty::default(),
    }
}

fn open(schedule: &Schedule, from: Date, to: Date) -> Vec<OpenOccurrence> {
    due_dates(schedule, from, to)
        .into_iter()
        .map(|due| OpenOccurrence {
            schedule: schedule.id,
            due,
        })
        .collect()
}

// --- when a payment is due (3.2) ---

#[test]
fn a_first_on_a_sunday_moves_to_monday() {
    let rent = monthly(AccountId::generate(), -85_000, day(2026, 10, 1));
    assert_eq!(
        dates(&rent, day(2026, 10, 1), day(2026, 12, 31)),
        [day(2026, 10, 1), day(2026, 11, 2), day(2026, 12, 1)]
    );
    let due = due_dates(&rent, day(2026, 11, 1), day(2026, 11, 30));
    assert_eq!(
        due,
        [Due {
            counted: day(2026, 11, 1),
            date: day(2026, 11, 2)
        }]
    );
}

#[test]
fn a_month_end_is_cut_back_and_never_drifts() {
    let plan = monthly(AccountId::generate(), -1_000, day(2027, 1, 31));
    // 31 Jan 2027 is a Sunday, 28 Feb 2027 too.
    assert_eq!(
        dates(&plan, day(2027, 1, 1), day(2027, 4, 30)),
        [
            day(2027, 2, 1),
            day(2027, 3, 1),
            day(2027, 3, 31),
            day(2027, 4, 30)
        ]
    );
    let counted: Vec<Date> = due_dates(&plan, day(2027, 1, 1), day(2027, 4, 30))
        .into_iter()
        .map(|due| due.counted)
        .collect();
    assert_eq!(
        counted,
        [
            day(2027, 1, 31),
            day(2027, 2, 28),
            day(2027, 3, 31),
            day(2027, 4, 30)
        ]
    );
}

#[test]
fn a_leap_day_is_cut_back_in_a_common_year() {
    let plan = plan(
        AccountId::generate(),
        -9_600,
        ScheduleUnit::Year,
        1,
        day(2028, 2, 29),
    );
    // 28 Feb 2029 is a Wednesday; 29 Feb 2032 a Sunday.
    assert_eq!(
        dates(&plan, day(2028, 1, 1), day(2032, 12, 31)),
        [
            day(2028, 2, 29),
            day(2029, 2, 28),
            day(2030, 2, 28),
            day(2031, 2, 28),
            day(2032, 3, 1)
        ]
    );
}

#[test]
fn every_two_weeks_counts_from_the_anchor() {
    let gym = plan(
        AccountId::generate(),
        -2_990,
        ScheduleUnit::Week,
        2,
        day(2026, 11, 6),
    );
    assert_eq!(
        dates(&gym, day(2026, 11, 1), day(2026, 12, 10)),
        [day(2026, 11, 6), day(2026, 11, 20), day(2026, 12, 4)]
    );
    // Asked from far later, the same pattern without walking from the
    // anchor.
    assert_eq!(
        dates(&gym, day(2030, 1, 1), day(2030, 1, 31)),
        [day(2030, 1, 11), day(2030, 1, 25)]
    );
}

#[test]
fn a_saturday_moves_forward_across_the_month_end() {
    let phone = monthly(AccountId::generate(), -3_499, day(2026, 11, 28));
    let due = due_dates(&phone, day(2026, 11, 1), day(2026, 11, 30));
    assert_eq!(
        due,
        [Due {
            counted: day(2026, 11, 28),
            date: day(2026, 11, 30)
        }]
    );
    // An occurrence belongs to the month of its moved date.
    let plan = monthly(AccountId::generate(), -100, day(2026, 10, 31));
    assert_eq!(
        dates(&plan, day(2026, 10, 1), day(2026, 10, 31)),
        Vec::<Date>::new()
    );
    assert_eq!(
        dates(&plan, day(2026, 11, 1), day(2026, 11, 3)),
        [day(2026, 11, 2)]
    );
}

#[test]
fn quarterly_and_every_two_years() {
    let quarterly = plan(
        AccountId::generate(),
        -100,
        ScheduleUnit::Month,
        3,
        day(2026, 1, 15),
    );
    assert_eq!(
        dates(&quarterly, day(2026, 1, 1), day(2026, 12, 31)),
        [
            day(2026, 1, 15),
            day(2026, 4, 15),
            day(2026, 7, 15),
            day(2026, 10, 15)
        ]
    );
    let biennial = plan(
        AccountId::generate(),
        -100,
        ScheduleUnit::Year,
        2,
        day(2026, 6, 10),
    );
    assert_eq!(
        dates(&biennial, day(2026, 1, 1), day(2031, 12, 31)),
        [day(2026, 6, 10), day(2028, 6, 12), day(2030, 6, 10)]
    );
}

#[test]
fn ends_on_and_track_from_limit_the_dates() {
    let mut rent = monthly(AccountId::generate(), -85_000, day(2026, 1, 1));
    rent.track_from = day(2026, 6, 1);
    rent.ends_on = Some(day(2026, 8, 1));
    assert_eq!(
        dates(&rent, day(2026, 1, 1), day(2026, 12, 31)),
        [day(2026, 6, 1), day(2026, 7, 1), day(2026, 8, 3)]
    );
    // The end limits counted dates: 1 Aug is a Saturday, still due on
    // Monday.
    rent.ends_on = Some(day(2026, 7, 31));
    assert_eq!(
        dates(&rent, day(2026, 1, 1), day(2026, 12, 31)),
        [day(2026, 6, 1), day(2026, 7, 1)]
    );
    assert!(dates(&rent, day(2026, 12, 1), day(2026, 1, 1)).is_empty());
}

#[test]
fn due_dates_stop_at_five_years_and_a_thousand_dates() {
    let weekly = plan(
        AccountId::generate(),
        -100,
        ScheduleUnit::Week,
        1,
        day(2000, 1, 3),
    );
    let all = due_dates(&weekly, day(2000, 1, 1), day(2099, 12, 31));
    assert_eq!(all.len(), 261);
    assert!(all.last().expect("dates").date <= day(2005, 1, 1));
    let many = Schedule {
        unit: ScheduleUnit::Week,
        every: 1,
        ..weekly
    };
    assert!(due_dates(&many, day(2000, 1, 1), day(2099, 12, 31)).len() <= MAX_DUE_DATES);
}

proptest! {
    #[test]
    fn due_dates_increase_skip_weekends_and_respect_track_from(
        unit in 0usize..3,
        every in 1u8..=52,
        anchor_offset in 0i64..3_000,
        track_offset in 0i64..800,
        from_offset in -400i64..4_000,
        length in 0i64..900,
    ) {
        let unit = ScheduleUnit::ALL[unit];
        let anchor = day(2020, 1, 1).checked_add(anchor_offset.days()).expect("in range");
        let mut schedule = plan(AccountId::generate(), -100, unit, every, anchor);
        schedule.track_from = anchor.checked_add(track_offset.days()).expect("in range");
        let from = day(2020, 1, 1).checked_add(from_offset.days()).expect("in range");
        let to = from.checked_add(length.days()).expect("in range");
        let found = due_dates(&schedule, from, to);
        for pair in found.windows(2) {
            prop_assert!(pair[0].date < pair[1].date);
            prop_assert!(pair[0].counted < pair[1].counted);
        }
        for due in &found {
            prop_assert!(!matches!(due.date.weekday(), Weekday::Saturday | Weekday::Sunday));
            prop_assert!(due.counted >= schedule.track_from);
            prop_assert!(due.date >= from && due.date <= to);
            prop_assert_eq!(banking_day(due.counted), Some(due.date));
        }
        // Asking in two halves gives the same dates as asking once.
        let middle = from.checked_add((length / 2).days()).expect("in range");
        let mut halves = due_dates(&schedule, from, middle);
        if let Ok(after) = middle.tomorrow() {
            halves.extend(due_dates(&schedule, after, to));
        }
        prop_assert_eq!(halves, found);
    }
}

// --- checks (3.1) ---

#[test]
fn a_plan_checks_what_it_must_be() {
    let good = monthly(AccountId::generate(), -85_000, day(2026, 10, 1));
    assert_eq!(good.check(), Ok(()));
    let error = |change: fn(&mut Schedule)| {
        let mut changed = good.clone();
        change(&mut changed);
        changed.check().err()
    };
    assert_eq!(error(|p| p.name = String::new()), Some(ScheduleError::Name));
    assert_eq!(
        error(|p| p.name = " Rent".to_owned()),
        Some(ScheduleError::Name)
    );
    assert_eq!(
        error(|p| p.name = "Rent\nflat".to_owned()),
        Some(ScheduleError::Name)
    );
    assert_eq!(error(|p| p.name = "é".repeat(60)), None);
    assert_eq!(
        error(|p| p.name = "é".repeat(61)),
        Some(ScheduleError::Name)
    );
    assert_eq!(
        error(|p| p.amount = eur(0)),
        Some(ScheduleError::AmountZero)
    );
    assert_eq!(error(|p| p.every = 0), Some(ScheduleError::EveryRange));
    assert_eq!(error(|p| p.every = 53), Some(ScheduleError::EveryRange));
    assert_eq!(error(|p| p.every = 52), None);
    assert_eq!(
        error(|p| p.ends_on = Some(day(2026, 9, 30))),
        Some(ScheduleError::EndBeforeStart)
    );
    assert_eq!(
        error(|p| p.track_from = day(2026, 9, 30)),
        Some(ScheduleError::EndBeforeStart)
    );
    assert_eq!(error(|p| p.ends_on = Some(day(2026, 10, 1))), None);
    assert_eq!(
        error(|p| p.tolerance = eur(-1)),
        Some(ScheduleError::ToleranceRange)
    );
    assert_eq!(
        error(|p| p.tolerance = eur(85_000)),
        Some(ScheduleError::ToleranceRange)
    );
    assert_eq!(error(|p| p.tolerance = eur(84_999)), None);
    assert_eq!(
        error(|p| p.tolerance = Money::from_minor_units(5, USD)),
        Some(ScheduleError::ToleranceRange)
    );
    assert_eq!(
        error(|p| p.party_name = Some(" ".to_owned())),
        Some(ScheduleError::Party)
    );
    assert_eq!(
        error(|p| p.party_name = Some("x".repeat(141))),
        Some(ScheduleError::Party)
    );
    assert_eq!(error(|p| p.party_name = Some("x".repeat(140))), None);
}

#[test]
fn party_text_is_cut_never_refused() {
    let long = "ä".repeat(200);
    let cut = cut_party(Some(&long)).expect("kept");
    assert_eq!(cut.chars().count(), MAX_SCHEDULE_PARTY_CHARS);
    assert_eq!(cut_party(Some("  ")), None);
    assert_eq!(cut_party(None), None);
    // A plan made from a long name still fits an entry with the full name.
    let mut gym = monthly(AccountId::generate(), -2_990, day(2026, 11, 25));
    gym.party_name = Some(cut);
    assert!(gym.party_fits(&OtherParty::new(None, Some(&long))));
}

// --- states (3.3) ---

#[test]
fn an_occurrence_is_due_for_three_banking_days_then_overdue() {
    let monday = day(2026, 11, 2);
    let transaction = TransactionId::generate();
    let state = |today| OccurrenceState::of(monday, None, today);
    assert_eq!(state(day(2026, 11, 1)), OccurrenceState::Upcoming);
    assert_eq!(state(monday), OccurrenceState::Due);
    assert_eq!(state(day(2026, 11, 5)), OccurrenceState::Due);
    assert_eq!(state(day(2026, 11, 6)), OccurrenceState::Overdue);
    // Friday to the next Wednesday is 3 banking days.
    let friday = day(2026, 11, 6);
    assert_eq!(
        OccurrenceState::of(friday, None, day(2026, 11, 11)),
        OccurrenceState::Due
    );
    assert_eq!(
        OccurrenceState::of(friday, None, day(2026, 11, 12)),
        OccurrenceState::Overdue
    );
    assert_eq!(
        OccurrenceState::of(monday, Some(Decided::Matched(transaction)), day(2027, 1, 1)),
        OccurrenceState::Matched
    );
    assert_eq!(
        OccurrenceState::of(monday, Some(Decided::Skipped), day(2026, 1, 1)),
        OccurrenceState::Skipped
    );
    let words: Vec<&str> = [
        OccurrenceState::Matched,
        OccurrenceState::Skipped,
        OccurrenceState::Upcoming,
        OccurrenceState::Due,
        OccurrenceState::Overdue,
    ]
    .map(OccurrenceState::word)
    .to_vec();
    assert_eq!(words, ["matched", "skipped", "upcoming", "due", "overdue"]);
}

#[test]
fn banking_days_are_added_and_taken_off() {
    assert_eq!(
        add_banking_days(day(2026, 11, 13), 3),
        Some(day(2026, 11, 18))
    );
    assert_eq!(
        add_banking_days(day(2026, 11, 14), 1),
        Some(day(2026, 11, 16))
    );
    assert_eq!(
        add_banking_days(day(2026, 11, 13), 0),
        Some(day(2026, 11, 13))
    );
    // The earliest day at most 3 banking days before Wednesday 18 Nov:
    // Friday to Wednesday is 3 (Monday, Tuesday, Wednesday).
    assert_eq!(
        sub_banking_days(day(2026, 11, 18), 3),
        Some(day(2026, 11, 13))
    );
    assert_eq!(
        sub_banking_days(day(2026, 11, 16), 1),
        Some(day(2026, 11, 13))
    );
    assert_eq!(
        sub_banking_days(day(2026, 11, 17), 0),
        Some(day(2026, 11, 17))
    );
}

// --- matching (3.4) ---

#[test]
fn an_entry_matches_within_tolerance_sign_and_window() {
    let checking = AccountId::generate();
    let mut power = monthly(checking, -6_000, day(2026, 11, 16));
    power.tolerance = eur(500);
    let due = due_dates(&power, day(2026, 11, 1), day(2026, 11, 30))[0];
    assert_eq!(due.date, day(2026, 11, 16));
    let fits = |minor_units, on| entry_fits(&power, due, &entry(checking, minor_units, on));
    assert_eq!(fits(-6_000, day(2026, 11, 16)), Some((0, 0, 0)));
    assert_eq!(fits(-6_500, day(2026, 11, 19)), Some((500, 3, 3)));
    assert_eq!(fits(-5_500, day(2026, 11, 11)), Some((500, 3, 5)));
    assert_eq!(fits(-6_501, day(2026, 11, 16)), None);
    assert_eq!(fits(6_000, day(2026, 11, 16)), None);
    assert_eq!(fits(-6_000, day(2026, 11, 20)), None);
    assert_eq!(fits(-6_000, day(2026, 11, 10)), None);
    assert_eq!(
        entry_fits(
            &power,
            due,
            &entry(AccountId::generate(), -6_000, day(2026, 11, 16))
        ),
        None
    );
    let mut dollars = entry(checking, -6_000, day(2026, 11, 16));
    dollars.amount = Money::from_minor_units(-6_000, USD);
    assert_eq!(entry_fits(&power, due, &dollars), None);
    // Exact when the tolerance is 0.
    power.tolerance = eur(0);
    assert_eq!(
        entry_fits(&power, due, &entry(checking, -6_001, day(2026, 11, 16))),
        None
    );
}

#[test]
fn the_party_must_fit_when_the_plan_names_one() {
    let checking = AccountId::generate();
    let mut rent = monthly(checking, -85_000, day(2026, 11, 2));
    let due = due_dates(&rent, day(2026, 11, 1), day(2026, 11, 30))[0];
    let with = |rent: &Schedule, iban: Option<&str>, name: Option<&str>| {
        let mut found = entry(checking, -85_000, day(2026, 11, 2));
        found.party = OtherParty::new(iban, name);
        entry_fits(rent, due, &found).is_some()
    };
    // Neither: any party fits.
    assert!(with(&rent, None, None));
    assert!(with(&rent, Some(GYM_IBAN), Some("Gym")));
    rent.party_iban = Some(LANDLORD_IBAN.to_owned());
    rent.party_name = Some("Landlord Ltd".to_owned());
    assert!(with(
        &rent,
        Some("de89370400440532013000"),
        Some("Someone else")
    ));
    assert!(!with(&rent, Some(GYM_IBAN), Some("Landlord Ltd")));
    assert!(!with(&rent, None, Some("Landlord Ltd")));
    rent.party_iban = None;
    assert!(with(&rent, None, Some("landlord  ltd")));
    assert!(!with(&rent, Some(LANDLORD_IBAN), Some("Gym")));
}

#[test]
fn the_closest_entry_wins_and_ties_need_the_user() {
    let checking = AccountId::generate();
    let rent = monthly(checking, -85_000, day(2026, 11, 2));
    let occurrences = open(&rent, day(2026, 11, 1), day(2026, 11, 30));
    let close = entry(checking, -85_000, day(2026, 11, 2));
    let far = entry(checking, -85_000, day(2026, 11, 4));
    let found = match_schedules(
        std::slice::from_ref(&rent),
        &occurrences,
        &[far.clone(), close.clone()],
    );
    assert_eq!(found.matches.len(), 1);
    assert_eq!(found.matches[0].transaction, close.transaction);
    assert!(found.unclear.is_empty());

    // Two equally close entries: neither is matched.
    let twin = entry(checking, -85_000, day(2026, 11, 2));
    let found = match_schedules(
        std::slice::from_ref(&rent),
        &occurrences,
        &[close.clone(), twin.clone(), far],
    );
    assert!(found.matches.is_empty());
    assert_eq!(found.unclear.len(), 1);
    let mut expected = vec![close.transaction, twin.transaction];
    expected.sort();
    assert_eq!(found.unclear[0].transactions, expected);
}

#[test]
fn an_entry_that_fits_two_plans_is_matched_by_neither() {
    let checking = AccountId::generate();
    let weekly = plan(checking, -2_000, ScheduleUnit::Week, 1, day(2026, 11, 2));
    let monthly = monthly(checking, -2_000, day(2026, 11, 2));
    let mut occurrences = open(&weekly, day(2026, 11, 1), day(2026, 11, 8));
    occurrences.extend(open(&monthly, day(2026, 11, 1), day(2026, 11, 8)));
    let paid = entry(checking, -2_000, day(2026, 11, 2));
    let found = match_schedules(
        &[weekly.clone(), monthly.clone()],
        &occurrences,
        std::slice::from_ref(&paid),
    );
    assert!(found.matches.is_empty());
    assert_eq!(found.unclear.len(), 2);
    // The order of plans, occurrences and entries never decides.
    let mut reversed = occurrences.clone();
    reversed.reverse();
    assert_eq!(
        match_schedules(&[monthly, weekly], &reversed, &[paid]),
        found
    );
}

#[test]
fn a_late_file_matches_the_old_month_once() {
    let checking = AccountId::generate();
    let rent = monthly(checking, -85_000, day(2026, 10, 1));
    let occurrences = open(&rent, day(2026, 10, 1), day(2026, 11, 30));
    let october = entry(checking, -85_000, day(2026, 10, 1));
    let november = entry(checking, -85_000, day(2026, 11, 2));
    let found = match_schedules(
        std::slice::from_ref(&rent),
        &occurrences,
        &[november.clone(), october.clone()],
    );
    let pairs: Vec<(Date, TransactionId)> = found
        .matches
        .iter()
        .map(|found| (found.occurrence.due.date, found.transaction))
        .collect();
    assert_eq!(
        pairs,
        [
            (day(2026, 10, 1), october.transaction),
            (day(2026, 11, 2), november.transaction)
        ]
    );
}

proptest! {
    #[test]
    fn matching_does_not_depend_on_order_and_takes_each_entry_once(
        amounts in prop::collection::vec(prop::sample::select(vec![-2_000i64, -2_100, -85_000, 220_000]), 0..10),
        offsets in prop::collection::vec(0i64..40, 0..10),
        seed in any::<u64>(),
    ) {
        let checking = AccountId::generate();
        let mut gym = plan(checking, -2_000, ScheduleUnit::Week, 1, day(2026, 11, 2));
        gym.tolerance = eur(100);
        let rent = monthly(checking, -85_000, day(2026, 11, 2));
        let salary = monthly(checking, 220_000, day(2026, 11, 27));
        let plans = vec![gym, rent, salary];
        let mut occurrences = Vec::new();
        for schedule in &plans {
            occurrences.extend(open(schedule, day(2026, 10, 20), day(2026, 12, 20)));
        }
        let entries: Vec<ScheduleEntry> = amounts
            .iter()
            .zip(offsets.iter())
            .map(|(amount, offset)| {
                entry(checking, *amount, day(2026, 11, 1).checked_add(offset.days()).expect("in range"))
            })
            .collect();
        let found = match_schedules(&plans, &occurrences, &entries);
        let mut shuffled = entries.clone();
        let rotate = usize::try_from(seed % 10).unwrap_or(0).min(shuffled.len());
        shuffled.rotate_left(rotate);
        shuffled.reverse();
        let mut reversed_plans = plans.clone();
        reversed_plans.reverse();
        let mut reversed = occurrences.clone();
        reversed.reverse();
        prop_assert_eq!(match_schedules(&reversed_plans, &reversed, &shuffled), found.clone());
        let mut used = std::collections::HashSet::new();
        for matched in &found.matches {
            prop_assert!(used.insert(matched.transaction));
            let schedule = plans.iter().find(|p| p.id == matched.occurrence.schedule).expect("a plan");
            let entry = entries.iter().find(|e| e.transaction == matched.transaction).expect("an entry");
            prop_assert!(entry_fits(schedule, matched.occurrence.due, entry).is_some());
        }
        for unclear in &found.unclear {
            for transaction in &unclear.transactions {
                prop_assert!(!used.contains(transaction));
            }
        }
    }
}

// --- the forecast (5.2, 5.4) ---

/// The worked example of `docs/SCHEDULES.md` 5.4.
fn worked_example() -> (Vec<Schedule>, HashMap<(ScheduleId, Date), Decided>) {
    let checking = AccountId::generate();
    let named = |name: &str, mut schedule: Schedule| {
        schedule.name = name.to_owned();
        schedule
    };
    let mut rent = named("Rent", monthly(checking, -85_000, day(2026, 10, 1)));
    rent.track_from = day(2026, 11, 1);
    let insurance = named(
        "Insurance",
        plan(checking, -9_600, ScheduleUnit::Year, 1, day(2026, 11, 20)),
    );
    let gym = named("Gym", monthly(checking, -2_990, day(2026, 11, 25)));
    let phone = named("Phone", monthly(checking, -3_499, day(2026, 11, 28)));
    let salary = named("Salary", monthly(checking, 220_000, day(2026, 11, 27)));
    let mut decided = HashMap::new();
    decided.insert(
        (rent.id, day(2026, 11, 1)),
        Decided::Matched(TransactionId::generate()),
    );
    (vec![rent, insurance, gym, phone, salary], decided)
}

#[test]
fn the_worked_example_gives_the_designs_figures() {
    let (plans, decided) = worked_example();
    let today = day(2026, 11, 13);
    let result = forecast(eur(234_000), &plans, &decided, today, 2).expect("a forecast");
    assert_eq!(result.balance_now, eur(234_000));
    assert_eq!(result.out_due, eur(16_089));
    assert_eq!(result.in_due, eur(220_000));
    assert_eq!(result.left, eur(217_911));
    assert_eq!(result.expected_end, eur(437_911));
    assert!(result.overdue.is_empty());
    assert_eq!(
        result.months[0],
        MonthPlan {
            month: day(2026, 12, 1),
            last: day(2026, 12, 31),
            start: eur(437_911),
            planned_in: eur(220_000),
            planned_out: eur(91_489),
            end: eur(566_422),
        }
    );
    assert_eq!(result.months[1].start, eur(566_422));
    assert_eq!(result.months[1].month, day(2027, 1, 1));
}

/// The worked example with months that start on the salary day, the 27th
/// (#921): "this month" is 27 Oct to 26 Nov, so the phone of the 28th and
/// the salary of the 27th belong to the next one.
#[test]
fn months_that_start_on_the_salary_day_end_the_day_before_it() {
    let (plans, decided) = worked_example();
    let today = day(2026, 11, 13);
    let result =
        forecast_from_day(eur(234_000), &plans, &decided, today, 2, 27).expect("a forecast");
    // Insurance 96,00 on the 20th and gym 29,90 on the 25th.
    assert_eq!(result.out_due, eur(9_600 + 2_990));
    assert_eq!(result.in_due, eur(0));
    assert_eq!(result.left, eur(234_000 - 12_590));
    assert_eq!(result.expected_end, result.left);
    // 27 Nov to 26 Dec: the salary (Fri 27 Nov) comes in; rent (1 Dec),
    // the phone (Sat 28 Nov, so Mon 30 Nov) and the gym (Fri 25 Dec) go out.
    assert_eq!(
        result.months[0],
        MonthPlan {
            month: day(2026, 11, 27),
            last: day(2026, 12, 26),
            start: eur(221_410),
            planned_in: eur(220_000),
            planned_out: eur(85_000 + 3_499 + 2_990),
            end: eur(221_410 + 220_000 - 91_489),
        }
    );
    assert_eq!(result.months[1].month, day(2026, 12, 27));
    assert_eq!(result.months[1].last, day(2027, 1, 26));
    // Day 1 is the calendar month: the same as the plain forecast.
    assert_eq!(
        forecast_from_day(eur(234_000), &plans, &decided, today, 2, 1),
        forecast(eur(234_000), &plans, &decided, today, 2)
    );
    // A day that is no day of a month is refused, not guessed.
    assert_eq!(
        forecast_from_day(eur(234_000), &plans, &decided, today, 2, 32),
        Err(ForecastError::Calendar)
    );
}

#[test]
fn overdue_paused_and_other_currency_plans_count_for_nothing() {
    let (mut plans, decided) = worked_example();
    let today = day(2026, 11, 26);
    // The insurance of 20 Nov is overdue on 26 Nov (Friday to Thursday is
    // 4 banking days); the gym of 25 Nov is still due.
    let result = forecast(eur(100_000), &plans, &decided, today, 0).expect("a forecast");
    assert_eq!(result.overdue.len(), 1);
    assert_eq!(result.overdue[0].due.date, day(2026, 11, 20));
    assert_eq!(result.out_due, eur(2_990 + 3_499));
    assert!(result.months.is_empty());
    for plan in &mut plans {
        plan.paused = true;
    }
    let paused = forecast(eur(100_000), &plans, &decided, today, 3).expect("a forecast");
    assert_eq!(paused.left, eur(100_000));
    assert_eq!(paused.expected_end, eur(100_000));
    assert!(paused.overdue.is_empty());
    assert!(paused.months.iter().all(|month| month.end == eur(100_000)));
    let dollars = forecast(
        Money::from_minor_units(100_000, USD),
        &worked_example().0,
        &decided,
        today,
        1,
    )
    .expect("a forecast");
    assert_eq!(dollars.out_due, Money::zero(USD));
}

#[test]
fn a_skipped_occurrence_is_not_counted_and_months_stop_at_twelve() {
    let (plans, mut decided) = worked_example();
    let gym = plans.iter().find(|plan| plan.name == "Gym").expect("gym");
    decided.insert((gym.id, day(2026, 11, 25)), Decided::Skipped);
    let result =
        forecast(eur(234_000), &plans, &decided, day(2026, 11, 13), 40).expect("a forecast");
    assert_eq!(result.out_due, eur(9_600 + 3_499));
    assert_eq!(result.months.len(), MAX_FORECAST_MONTHS);
}

#[test]
fn a_sum_too_large_is_an_error_not_a_wrong_figure() {
    let checking = AccountId::generate();
    let huge = monthly(checking, i64::MIN + 1, day(2026, 11, 20));
    let result = forecast(
        eur(i64::MIN),
        &[huge],
        &HashMap::new(),
        day(2026, 11, 13),
        0,
    );
    assert!(matches!(result, Err(ForecastError::Money(_))));
}

proptest! {
    #[test]
    fn left_plus_what_goes_out_is_the_balance(
        amounts in prop::collection::vec(prop::sample::select(vec![-1i64, -2_990, -85_000, 220_000, 7]), 1..8),
        days in prop::collection::vec(1i8..=28, 1..8),
        balance in -1_000_000i64..1_000_000,
        today in 1i8..=30,
        starts_on in 1i8..=31,
    ) {
        let checking = AccountId::generate();
        let plans: Vec<Schedule> = amounts
            .iter()
            .zip(days.iter())
            .map(|(amount, on)| monthly(checking, *amount, day(2026, 11, *on)))
            .collect();
        let today = day(2026, 11, today);
        let result = forecast_from_day(eur(balance), &plans, &HashMap::new(), today, 3, starts_on)
            .expect("a forecast");
        prop_assert_eq!(result.left.checked_add(result.out_due).expect("fits"), eur(balance));
        prop_assert_eq!(result.expected_end.checked_sub(result.in_due).expect("fits"), result.left);
        prop_assert!(!result.out_due.is_negative() && !result.in_due.is_negative());
        let mut start = result.expected_end;
        // Each month follows the one before without a gap, the first right
        // after the one today is in.
        let (_, mut last) = crate::salary_month_of(today, starts_on).expect("a salary month");
        prop_assert!(last >= today);
        for month in &result.months {
            prop_assert_eq!(month.start, start);
            prop_assert_eq!(month.month, last.tomorrow().expect("a day"));
            prop_assert!(month.last >= month.month);
            start = month.end;
            last = month.last;
        }
    }
}
