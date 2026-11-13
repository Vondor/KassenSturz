// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of planned payments and the forecast (D-769, D-770), on books in
//! memory. All data is made up (AGENTS.md §3.1 rule 2); the IBANs are the
//! examples in SWIFT's IBAN Registry (D-055).

use kassensturz_core::{Account, ImportedDetails, MAX_SCHEDULE_NAME_CHARS};

use super::*;
use crate::history::RECORDED;
use crate::tests::{EUR, USD, account, date, eur};

/// SWIFT's example IBAN for Germany, the landlord's.
const LANDLORD_IBAN: &str = "DE89370400440532013000";

/// Every row of every recorded table as SQL text, so two moments can be
/// compared exactly.
fn contents(book: &Book) -> Vec<String> {
    let mut rows = Vec::new();
    for table in &RECORDED {
        let values = table
            .columns
            .iter()
            .map(|(name, _)| format!("quote({name})"))
            .collect::<Vec<_>>()
            .join(" || ', ' || ");
        let mut statement = book
            .conn
            .prepare(&format!(
                "SELECT '{name}: ' || {values} FROM main.{name} ORDER BY 1",
                name = table.name
            ))
            .expect("rows read");
        rows.extend(
            statement
                .query_map([], |row| row.get::<_, String>(0))
                .expect("rows read")
                .collect::<Result<Vec<_>, _>>()
                .expect("rows read"),
        );
    }
    rows
}

struct Household {
    book: Book,
    checking: AccountId,
    savings: AccountId,
    rent: AccountId,
    salary: AccountId,
}

/// A checking account, a savings account and two categories.
fn household() -> Household {
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    let checking = AccountId::generate();
    book.add_account(&Account {
        id: checking,
        name: "Checking account".into(),
        kind: AccountKind::Asset,
        currency: EUR,
        account_type: Some(AccountType::Checking),
        closed: false,
        parent: None,
        number: None,
        notes: None,
        institution: None,
    })
    .expect("account added");
    let savings = account(&mut book, "Savings", AccountKind::Asset, EUR);
    let rent = account(&mut book, "Rent", AccountKind::Expense, EUR);
    let salary = account(&mut book, "Salary", AccountKind::Income, EUR);
    Household {
        book,
        checking,
        savings,
        rent,
        salary,
    }
}

/// The rent: 750 EUR out on the 1st of each month from October 2026,
/// give or take 50 cents, to the landlord's IBAN, booked as rent.
fn rent_plan(household: &Household) -> NewSchedule {
    NewSchedule {
        name: "  Rent  ".into(),
        account: household.checking,
        amount: eur(-75_000),
        tolerance: eur(50),
        category: Some(household.rent),
        party_iban: Some(LANDLORD_IBAN.into()),
        party_name: None,
        unit: ScheduleUnit::Month,
        every: 1,
        anchor: date(2026, 10, 1),
        track_from: date(2026, 10, 1),
        ends_on: None,
    }
}

/// One bank's entry, as an import adds it: the account's part with what
/// the bank delivered, and a part without a category.
fn entry(
    book: &mut Book,
    account: AccountId,
    minor_units: i64,
    on: Date,
    iban: Option<&str>,
) -> TransactionId {
    let delivered = ImportedDetails {
        bank_id: Some(format!("{on}-{minor_units}-{}", TransactionId::generate())),
        booking_date: Some(on),
        value_date: Some(on),
        other_party_name: Some("Example Party".into()),
        other_party_iban: iban.map(str::to_owned),
        remittance_information: None,
    };
    let booked = Split {
        imported: Some(delivered),
        ..Split::new(account, eur(minor_units))
    };
    let transaction = Transaction::new(
        TransactionId::generate(),
        on,
        Some("Example Party".into()),
        vec![booked, Split::uncategorized(eur(-minor_units))],
    )
    .expect("balanced");
    book.add_transaction(&transaction).expect("added");
    transaction.id()
}

/// The category of the part of `transaction` that isn't in `account`.
fn other_part(book: &Book, transaction: TransactionId, account: AccountId) -> Option<AccountId> {
    book.transaction(transaction)
        .expect("transaction")
        .splits()
        .iter()
        .find(|split| split.account != Some(account))
        .and_then(|split| split.account)
}

#[test]
fn a_plan_is_added_edited_paused_and_removed_and_each_step_undone() {
    let household = household();
    let new = rent_plan(&household);
    let mut book = household.book;
    let empty = contents(&book);
    let id = book.add_schedule(&new).expect("planned");
    let plan = book.schedule(id).expect("plan");
    assert_eq!(plan.name, "Rent");
    assert_eq!(plan.amount, eur(-75_000));
    assert!(!plan.paused);
    assert_eq!(book.schedules().expect("plans"), std::slice::from_ref(&plan));
    let added = contents(&book);

    let edited = Schedule {
        name: "Flat".into(),
        every: 3,
        ends_on: Some(date(2027, 12, 31)),
        ..plan.clone()
    };
    book.edit_schedule(&edited).expect("edited");
    assert_eq!(book.schedule(id).expect("plan"), edited);
    let after_edit = contents(&book);
    book.set_schedule_paused(id, true).expect("paused");
    assert!(book.schedule(id).expect("plan").paused);
    let paused = contents(&book);
    book.remove_schedule(id).expect("removed");
    assert!(matches!(
        book.schedule(id),
        Err(StoreError::UnknownSchedule(unknown)) if unknown == id
    ));

    for expected in [&paused, &after_edit, &added, &empty] {
        book.undo().expect("undone");
        assert_eq!(&contents(&book), expected);
    }
    for expected in [&added, &after_edit, &paused] {
        book.redo().expect("redone");
        assert_eq!(&contents(&book), expected);
    }
}

#[test]
fn plans_that_do_not_fit_the_book_are_refused() {
    let household = household();
    let good = rent_plan(&household);
    let mut book = household.book;
    book.use_currency(USD).expect("currency used");
    let usd_account = account(&mut book, "Dollar account", AccountKind::Asset, USD);
    let food_usd = account(&mut book, "Food in dollars", AccountKind::Expense, USD);
    let before = contents(&book);
    for (plan, check) in [
        (
            NewSchedule {
                name: " ".into(),
                ..good.clone()
            },
            "name" as &str,
        ),
        (
            NewSchedule {
                name: "x".repeat(MAX_SCHEDULE_NAME_CHARS + 1),
                ..good.clone()
            },
            "name",
        ),
        (
            NewSchedule {
                amount: eur(0),
                ..good.clone()
            },
            "amount",
        ),
        (
            NewSchedule {
                every: 0,
                ..good.clone()
            },
            "every",
        ),
        (
            NewSchedule {
                ends_on: Some(date(2026, 9, 30)),
                ..good.clone()
            },
            "end",
        ),
        (
            NewSchedule {
                account: usd_account,
                ..good.clone()
            },
            "currency",
        ),
        (
            NewSchedule {
                category: Some(food_usd),
                ..good.clone()
            },
            "category",
        ),
        (
            NewSchedule {
                category: Some(household.savings),
                ..good.clone()
            },
            "category",
        ),
        (
            NewSchedule {
                account: household.rent,
                category: None,
                ..good.clone()
            },
            "account",
        ),
    ] {
        let refused = book.add_schedule(&plan).expect_err(check);
        let expected = match check {
            "name" => matches!(refused, StoreError::Schedule(ScheduleError::Name)),
            "amount" => matches!(refused, StoreError::Schedule(ScheduleError::AmountZero)),
            "every" => matches!(refused, StoreError::Schedule(ScheduleError::EveryRange)),
            "end" => matches!(refused, StoreError::Schedule(ScheduleError::EndBeforeStart)),
            "currency" => matches!(refused, StoreError::ScheduleCurrencyDiffers { .. }),
            "category" => matches!(refused, StoreError::ScheduleCategory(_)),
            _ => matches!(refused, StoreError::NotAnAccount(_)),
        };
        assert!(expected, "{check}: {refused:?}");
    }
    assert_eq!(contents(&book), before);

    // The other party's name and IBAN are cut, never refused.
    let long = book
        .add_schedule(&NewSchedule {
            party_name: Some("ä".repeat(200)),
            ..good.clone()
        })
        .expect("planned");
    assert_eq!(
        book.schedule(long)
            .expect("plan")
            .party_name
            .map(|name| name.chars().count()),
        Some(kassensturz_core::MAX_SCHEDULE_PARTY_CHARS)
    );
    // A closed account takes no new plans.
    let closed = account(&mut book, "Old account", AccountKind::Asset, EUR);
    book.close_account(closed).expect("closed");
    assert!(matches!(
        book.add_schedule(&NewSchedule {
            account: closed,
            ..good.clone()
        }),
        Err(StoreError::ScheduleAccountClosed(account)) if account == closed
    ));
}

#[test]
fn a_book_holds_at_most_200_plans() {
    let household = household();
    let plan = rent_plan(&household);
    let mut book = household.book;
    for _ in 1..MAX_SCHEDULES {
        book.add_schedule(&plan).expect("planned");
    }
    let last = book.add_schedule(&plan).expect("the 200th");
    assert!(matches!(
        book.add_schedule(&plan),
        Err(StoreError::TooManySchedules { max: 200 })
    ));
    // Editing one keeps the count.
    let mut edited = book.schedule(last).expect("plan");
    edited.name = "Still rent".into();
    book.edit_schedule(&edited).expect("edited");
}

#[test]
fn entries_are_matched_after_an_import_and_one_undo_takes_the_matches_back() {
    let household = household();
    let plan = rent_plan(&household);
    let (checking, rent) = (household.checking, household.rent);
    let mut book = household.book;
    let id = book.add_schedule(&plan).expect("planned");
    // October's rent came 30 cents dearer on Monday the 5th, November's on
    // the 2nd; a payment to someone else on the 1st doesn't fit.
    let october = entry(
        &mut book,
        checking,
        -75_030,
        date(2026, 10, 5),
        Some(LANDLORD_IBAN),
    );
    let november = entry(
        &mut book,
        checking,
        -75_000,
        date(2026, 11, 2),
        Some("DE89 3704 0044 0532 0130 00"),
    );
    let other = entry(
        &mut book,
        checking,
        -75_000,
        date(2026, 12, 1),
        Some("GB29NWBK60161331926819"),
    );
    let before = contents(&book);
    let report = book
        .match_schedules(date(2026, 12, 1), None)
        .expect("matched");
    assert!(report.needs_you.is_empty());
    let matched: Vec<_> = report
        .matched
        .iter()
        .map(|done| {
            (
                done.occurrence.due.counted,
                done.transaction,
                done.category_set,
            )
        })
        .collect();
    assert_eq!(
        matched,
        [
            (date(2026, 10, 1), october, true),
            (date(2026, 11, 1), november, true)
        ]
    );
    assert_eq!(other_part(&book, october, checking), Some(rent));
    assert_eq!(other_part(&book, other, checking), None);
    let decided = book.decided_occurrences().expect("decided");
    assert_eq!(
        decided.get(&(id, date(2026, 10, 1))),
        Some(&Decided::Matched(october))
    );
    // Running it again matches nothing new and records nothing.
    let after = contents(&book);
    let again = book
        .match_schedules(date(2026, 12, 1), None)
        .expect("matched");
    assert!(again.matched.is_empty());
    assert_eq!(contents(&book), after);
    // December's 1st is still due; the payment to someone else is no match.
    let states: Vec<_> = book
        .planned_occurrences(date(2026, 10, 1), date(2026, 12, 31), date(2026, 12, 1))
        .expect("occurrences")
        .iter()
        .map(|occurrence| occurrence.state)
        .collect();
    assert_eq!(
        states,
        [
            OccurrenceState::Matched,
            OccurrenceState::Matched,
            OccurrenceState::Due
        ]
    );

    book.undo().expect("undone");
    assert_eq!(contents(&book), before);
    book.redo().expect("redone");
    assert_eq!(contents(&book), after);
}

#[test]
fn only_the_entries_an_import_added_are_matched_and_a_users_category_stays() {
    let household = household();
    let plan = rent_plan(&household);
    let checking = household.checking;
    let mut book = household.book;
    book.add_schedule(&plan).expect("planned");
    let old = entry(
        &mut book,
        checking,
        -75_000,
        date(2026, 10, 1),
        Some(LANDLORD_IBAN),
    );
    let new = entry(
        &mut book,
        checking,
        -75_000,
        date(2026, 11, 2),
        Some(LANDLORD_IBAN),
    );
    // The user booked the new one to a category of their own.
    let own = account(&mut book, "Housing", AccountKind::Expense, EUR);
    let stored = book.transaction(new).expect("entry");
    let splits: Vec<Split> = stored
        .splits()
        .iter()
        .map(|split| {
            let mut split = split.clone();
            if split.account.is_none() {
                split.account = Some(own);
            }
            split
        })
        .collect();
    let changed = Transaction::new(new, stored.booking_date(), None, splits)
        .expect("balanced")
        .with_details_of(&stored);
    book.update_transaction(&changed).expect("changed");
    let involving: HashSet<TransactionId> = [new].into();
    let report = book
        .match_schedules(date(2026, 11, 13), Some(&involving))
        .expect("matched");
    assert_eq!(report.matched.len(), 1);
    assert_eq!(report.matched[0].transaction, new);
    assert!(!report.matched[0].category_set);
    assert_eq!(other_part(&book, new, checking), Some(own));
    // The old entry fits October, but wasn't among the new ones.
    assert_eq!(other_part(&book, old, checking), None);
    assert_eq!(book.decided_occurrences().expect("decided").len(), 1);
}

#[test]
fn an_entry_that_fits_two_payments_waits_for_the_user() {
    let household = household();
    let checking = household.checking;
    let mut book = household.book;
    // Every week, on Thursdays from 1 October 2026; an entry on Monday the
    // 5th is 2 business days from the 1st and 3 from the 8th, so it fits
    // both, and neither is matched by itself.
    let id = book
        .add_schedule(&NewSchedule {
            name: "Cleaner".into(),
            account: checking,
            amount: eur(-4_000),
            tolerance: eur(0),
            category: None,
            party_iban: None,
            party_name: None,
            unit: ScheduleUnit::Week,
            every: 1,
            anchor: date(2026, 10, 1),
            track_from: date(2026, 10, 1),
            ends_on: Some(date(2026, 10, 8)),
        })
        .expect("planned");
    let monday = entry(&mut book, checking, -4_000, date(2026, 10, 5), None);
    let before = contents(&book);
    let report = book
        .match_schedules(date(2026, 10, 9), None)
        .expect("matched");
    assert!(report.matched.is_empty());
    let unclear: Vec<_> = report
        .needs_you
        .iter()
        .map(|needs| (needs.occurrence.due.counted, needs.transactions.clone()))
        .collect();
    assert_eq!(
        unclear,
        [
            (date(2026, 10, 1), vec![monday]),
            (date(2026, 10, 8), vec![monday])
        ]
    );
    assert_eq!(contents(&book), before);
    // The second payment comes on its day: it is the 8th's, and then the
    // Monday entry fits only the 1st.
    let thursday = entry(&mut book, checking, -4_000, date(2026, 10, 8), None);
    let report = book
        .match_schedules(date(2026, 10, 9), None)
        .expect("matched");
    assert_eq!(report.matched.len(), 1);
    assert_eq!(report.matched[0].transaction, thursday);
    assert_eq!(report.needs_you.len(), 1);
    let report = book
        .match_schedules(date(2026, 10, 9), None)
        .expect("matched");
    assert_eq!(report.matched.len(), 1);
    assert_eq!(report.matched[0].transaction, monday);
    assert_eq!(report.matched[0].occurrence.due.counted, date(2026, 10, 1));
    assert_eq!(book.next_due(id, date(2026, 10, 1)).expect("next"), None);
}

#[test]
fn removing_a_matched_entry_opens_its_payment_again_and_undo_brings_both_back() {
    let household = household();
    let plan = rent_plan(&household);
    let checking = household.checking;
    let mut book = household.book;
    let id = book.add_schedule(&plan).expect("planned");
    let october = entry(
        &mut book,
        checking,
        -75_000,
        date(2026, 10, 1),
        Some(LANDLORD_IBAN),
    );
    book.match_schedules(date(2026, 10, 2), None)
        .expect("matched");
    let matched = contents(&book);
    book.remove_transaction(october).expect("removed");
    assert!(book.decided_occurrences().expect("decided").is_empty());
    assert_eq!(
        book.next_due(id, date(2026, 10, 1)).expect("next"),
        Some(Due {
            counted: date(2026, 10, 1),
            date: date(2026, 10, 1)
        })
    );
    book.undo().expect("undone");
    assert_eq!(contents(&book), matched);
    // Removing the plan keeps the entry and its category.
    book.remove_schedule(id).expect("removed");
    assert_eq!(
        other_part(&book, october, checking),
        Some(plan.category.expect("category"))
    );
    book.undo().expect("undone");
    assert_eq!(contents(&book), matched);
}

#[test]
fn a_category_a_plan_uses_is_not_removed() {
    let household = household();
    let plan = rent_plan(&household);
    let rent = household.rent;
    let mut book = household.book;
    let id = book.add_schedule(&plan).expect("planned");
    assert!(matches!(
        book.remove_category(rent),
        Err(StoreError::CategoryInUse {
            used_by: "schedules",
            ..
        })
    ));
    book.remove_schedule(id).expect("removed");
    book.remove_category(rent).expect("removed");
}

#[test]
fn a_plan_on_a_closed_account_is_neither_expected_nor_matched() {
    let household = household();
    let savings = household.savings;
    let mut book = household.book;
    let id = book
        .add_schedule(&NewSchedule {
            name: "Savings plan".into(),
            account: savings,
            amount: eur(10_000),
            tolerance: eur(0),
            category: None,
            party_iban: None,
            party_name: None,
            unit: ScheduleUnit::Month,
            every: 1,
            anchor: date(2026, 10, 1),
            track_from: date(2026, 10, 1),
            ends_on: None,
        })
        .expect("planned");
    book.close_account(savings).expect("closed");
    assert!(
        book.planned_occurrences(date(2026, 10, 1), date(2027, 1, 1), date(2026, 11, 1))
            .expect("occurrences")
            .is_empty()
    );
    assert_eq!(book.next_due(id, date(2026, 11, 1)).expect("next"), None);
    assert!(
        book.match_schedules(date(2026, 11, 1), None)
            .expect("matched")
            .matched
            .is_empty()
    );
    assert!(matches!(
        book.book_schedule_occurrence(id, date(2026, 11, 1), date(2026, 11, 1), eur(10_000)),
        Err(StoreError::ScheduleAccountClosed(_))
    ));
    // Reopened, the plan goes on.
    book.reopen_account(savings).expect("reopened");
    assert_eq!(
        book.next_due(id, date(2026, 11, 1))
            .expect("next")
            .map(|due| due.counted),
        Some(date(2026, 11, 1))
    );
}

#[test]
fn the_user_matches_skips_and_books_payments() {
    let household = household();
    let plan = rent_plan(&household);
    let (checking, savings, rent) = (household.checking, household.savings, household.rent);
    let mut book = household.book;
    let id = book.add_schedule(&plan).expect("planned");
    // Far too small for the plan, but the user says it is October's.
    let partly = entry(&mut book, checking, -10_000, date(2026, 10, 20), None);
    let refund = entry(&mut book, checking, 10_000, date(2026, 11, 3), None);
    let elsewhere = entry(&mut book, savings, -75_000, date(2026, 11, 3), None);
    book.match_schedule_occurrence(id, date(2026, 10, 1), partly)
        .expect("matched");
    assert_eq!(other_part(&book, partly, checking), Some(rent));
    for refused in [
        book.match_schedule_occurrence(id, date(2026, 10, 1), partly),
        book.skip_schedule_occurrence(id, date(2026, 10, 1)),
    ] {
        assert!(
            matches!(refused, Err(StoreError::OccurrenceDecided { .. })),
            "{refused:?}"
        );
    }
    assert!(matches!(
        book.match_schedule_occurrence(id, date(2026, 11, 1), partly),
        Err(StoreError::ScheduleEntryTaken(taken)) if taken == partly
    ));
    // Money coming in, or in another account, is no payment of the rent.
    for wrong in [refund, elsewhere] {
        assert!(matches!(
            book.match_schedule_occurrence(id, date(2026, 11, 1), wrong),
            Err(StoreError::ScheduleEntryNotFitting(_))
        ));
    }
    // No payment is counted on the 2nd.
    assert!(matches!(
        book.skip_schedule_occurrence(id, date(2026, 11, 2)),
        Err(StoreError::UnknownOccurrence { .. })
    ));
    book.undo().expect("undone");
    assert!(book.decided_occurrences().expect("decided").is_empty());
    assert_eq!(other_part(&book, partly, checking), None);

    // Skipped and unskipped.
    let open = contents(&book);
    book.skip_schedule_occurrence(id, date(2026, 12, 1))
        .expect("skipped");
    assert!(matches!(
        book.unskip_schedule_occurrence(id, date(2027, 1, 1)),
        Err(StoreError::OccurrenceDecided { .. })
    ));
    book.unskip_schedule_occurrence(id, date(2026, 12, 1))
        .expect("unskipped");
    assert_eq!(contents(&book), open);
    book.undo().expect("undone");
    assert_eq!(
        book.decided_occurrences()
            .expect("decided")
            .get(&(id, date(2026, 12, 1))),
        Some(&Decided::Skipped)
    );
    book.undo().expect("undone");
    assert_eq!(contents(&book), open);

    // Booked by hand: a new entry in the plan's account and category,
    // matched in the same step, which one Undo takes back.
    for wrong in [eur(0), eur(75_000), Money::from_minor_units(-75_000, USD)] {
        assert!(matches!(
            book.book_schedule_occurrence(id, date(2027, 1, 1), date(2026, 12, 30), wrong),
            Err(StoreError::Schedule(ScheduleError::BookedAmount))
        ));
    }
    let booked = book
        .book_schedule_occurrence(id, date(2027, 1, 1), date(2026, 12, 30), eur(-76_000))
        .expect("booked");
    let stored = book.transaction(booked).expect("entry");
    assert_eq!(stored.booking_date(), date(2026, 12, 30));
    assert_eq!(stored.payee(), Some("Rent"));
    assert_eq!(other_part(&book, booked, checking), Some(rent));
    assert_eq!(
        book.decided_occurrences()
            .expect("decided")
            .get(&(id, date(2027, 1, 1))),
        Some(&Decided::Matched(booked))
    );
    book.undo().expect("undone");
    assert!(matches!(
        book.transaction(booked),
        Err(StoreError::UnknownTransaction(_))
    ));
    assert_eq!(contents(&book), open);
}

#[test]
fn the_forecast_counts_the_expected_payments_of_the_chosen_accounts() {
    let household = household();
    let plan = rent_plan(&household);
    let (checking, savings, salary) = (household.checking, household.savings, household.salary);
    let mut book = household.book;
    book.use_currency(USD).expect("currency used");
    let usd_account = AccountId::generate();
    book.add_account(&Account {
        id: usd_account,
        name: "Dollar cash".into(),
        kind: AccountKind::Asset,
        currency: USD,
        account_type: Some(AccountType::Cash),
        closed: false,
        parent: None,
        number: None,
        notes: None,
        institution: None,
    })
    .expect("account added");
    let defaults = book.default_forecast_accounts().expect("defaults");
    let mut expected = vec![checking, usd_account];
    expected.sort();
    assert_eq!(defaults, expected);

    let empty = book
        .forecast(&defaults, EUR, date(2026, 11, 13), 2)
        .expect("forecast");
    assert!(!empty.has_plans);
    assert_eq!(empty.left_out, [usd_account]);
    assert_eq!(empty.forecast.left, eur(0));

    book.add_schedule(&plan).expect("planned");
    book.add_schedule(&NewSchedule {
        name: "Pay".into(),
        amount: eur(250_000),
        tolerance: eur(0),
        category: Some(salary),
        party_iban: None,
        anchor: date(2026, 10, 28),
        track_from: date(2026, 10, 28),
        ..plan.clone()
    })
    .expect("planned");
    // A plan on the savings account isn't counted for the checking account.
    book.add_schedule(&NewSchedule {
        name: "Saving".into(),
        account: savings,
        category: None,
        party_iban: None,
        ..plan.clone()
    })
    .expect("planned");
    // 3,000 EUR came in, which fits no plan.
    transaction_on(&mut book, checking, 300_000, date(2026, 11, 2), salary);
    let report = book
        .match_schedules(date(2026, 11, 13), None)
        .expect("matched");
    assert!(report.matched.is_empty(), "{report:?}");
    let found = book
        .forecast(&[checking], EUR, date(2026, 11, 13), 2)
        .expect("forecast");
    let forecast = &found.forecast;
    assert!(found.has_plans);
    assert_eq!(found.accounts, [checking]);
    assert_eq!(forecast.balance_now, eur(300_000));
    // November's rent (the 2nd) is overdue and listed, not counted; the pay
    // on Monday the 30th comes in.
    assert_eq!(forecast.out_due, eur(0));
    assert_eq!(forecast.in_due, eur(250_000));
    assert_eq!(forecast.left, eur(300_000));
    assert_eq!(forecast.expected_end, eur(550_000));
    assert_eq!(
        forecast
            .overdue
            .iter()
            .map(|open| open.due.counted)
            .collect::<Vec<_>>(),
        [date(2026, 10, 1), date(2026, 10, 28), date(2026, 11, 1)]
    );
    let months: Vec<_> = forecast
        .months
        .iter()
        .map(|month| (month.start, month.planned_in, month.planned_out, month.end))
        .collect();
    assert_eq!(
        months,
        [
            (eur(550_000), eur(250_000), eur(75_000), eur(725_000)),
            (eur(725_000), eur(250_000), eur(75_000), eur(900_000)),
        ]
    );
    // A category is no account to look ahead for.
    assert!(matches!(
        book.forecast(&[salary], EUR, date(2026, 11, 13), 1),
        Err(StoreError::NotAnAccount(_))
    ));
}

/// A transaction the user typed in: `minor_units` into `account` from
/// `category`.
fn transaction_on(
    book: &mut Book,
    account: AccountId,
    minor_units: i64,
    on: Date,
    category: AccountId,
) -> TransactionId {
    let transaction = crate::tests::transaction(
        on,
        "Example Employer",
        &[(account, eur(minor_units)), (category, eur(-minor_units))],
    );
    book.add_transaction(&transaction).expect("added");
    transaction.id()
}

#[test]
fn plans_come_back_from_a_complete_export_with_their_decisions() {
    let household = household();
    let plan = rent_plan(&household);
    let checking = household.checking;
    let mut book = household.book;
    let id = book.add_schedule(&plan).expect("planned");
    let october = entry(
        &mut book,
        checking,
        -75_000,
        date(2026, 10, 1),
        Some(LANDLORD_IBAN),
    );
    book.match_schedules(date(2026, 10, 2), None)
        .expect("matched");
    book.skip_schedule_occurrence(id, date(2026, 11, 1))
        .expect("skipped");
    let plans = book.schedules().expect("plans");
    let decided: Vec<(ScheduleId, Date, Decided)> = {
        let mut decided: Vec<_> = book
            .decided_occurrences()
            .expect("decided")
            .into_iter()
            .map(|((plan, day), decision)| (plan, day, decision))
            .collect();
        decided.sort_by_key(|(_, day, _)| *day);
        decided
    };
    assert_eq!(
        decided,
        [
            (id, date(2026, 10, 1), Decided::Matched(october)),
            (id, date(2026, 11, 1), Decided::Skipped)
        ]
    );
    // Into a book without the entry: refused.
    let mut other = household_like(&book);
    let refused = other.restore_schedules(&plans, &decided);
    assert!(
        matches!(refused, Err(StoreError::UnknownTransaction(_))),
        "{refused:?}"
    );
    // The same entry matched twice: refused.
    let twice = [
        (id, date(2026, 10, 1), Decided::Matched(october)),
        (id, date(2026, 12, 1), Decided::Matched(october)),
    ];
    book.remove_schedule(id).expect("removed");
    assert!(matches!(
        book.restore_schedules(&plans, &twice),
        Err(StoreError::ScheduleEntryTaken(_))
    ));
    book.restore_schedules(&plans, &decided).expect("restored");
    assert_eq!(book.schedules().expect("plans"), plans);
    assert_eq!(book.decided_occurrences().expect("decided").len(), 2);
}

/// A new book with the same accounts as `book`, without its entries.
fn household_like(book: &Book) -> Book {
    let mut other = Book::create_in_memory().expect("book created");
    other.use_currency(EUR).expect("currency used");
    for account in book.all_accounts().expect("accounts") {
        other.add_account(&account).expect("account added");
    }
    other
}

#[test]
fn a_plan_is_made_from_an_entry() {
    let household = household();
    let (checking, rent) = (household.checking, household.rent);
    let mut book = household.book;
    let id = entry(
        &mut book,
        checking,
        -75_000,
        date(2026, 10, 30),
        Some(LANDLORD_IBAN),
    );
    let stored = book.transaction(id).expect("entry");
    let new = NewSchedule::from_entry(&stored, checking, "Planned payment").expect("a plan");
    assert_eq!(new.name, "Example Party");
    assert_eq!(new.amount, eur(-75_000));
    assert_eq!(new.category, None);
    assert_eq!(new.party_iban.as_deref(), Some(LANDLORD_IBAN));
    assert_eq!(new.anchor, date(2026, 10, 30));
    // Not an entry of another account.
    assert!(NewSchedule::from_entry(&stored, rent, "Planned payment").is_none());
    let plan = book.add_schedule(&new).expect("planned");
    // The entry it was made from is its first payment.
    let report = book
        .match_schedules(date(2026, 10, 30), None)
        .expect("matched");
    assert_eq!(report.matched.len(), 1);
    assert_eq!(report.matched[0].transaction, id);
    assert_eq!(report.matched[0].occurrence.schedule, plan);
}
