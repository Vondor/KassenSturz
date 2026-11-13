// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the screens' requests about planned payments, on books in
//! memory. All data is made up (AGENTS.md §3.1 rule 2); the IBAN is the
//! example in SWIFT's IBAN Registry (D-055). Today is Monday, 19 October
//! 2026, as [`TestLocale`] has it.

use kassensturz_core::{Account, AccountType, Split, Transaction};

use super::*;
use crate::register::tests::TestLocale;

/// SWIFT's example IBAN for Germany, the landlord's.
const LANDLORD_IBAN: &str = "DE89370400440532013000";

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};

fn eur(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, EUR)
}

fn day(year: i16, month: i8, day: i8) -> Date {
    Date::new(year, month, day).expect("a valid date")
}

struct Household {
    book: Book,
    checking: AccountId,
    rent: AccountId,
}

fn add_account(book: &mut Book, name: &str, kind: AccountKind) -> AccountId {
    let id = AccountId::generate();
    book.add_account(&Account {
        id,
        name: name.into(),
        kind,
        currency: EUR,
        account_type: kind.is_account().then_some(AccountType::Checking),
        closed: false,
        parent: None,
        number: None,
        notes: None,
        institution: None,
    })
    .expect("account added");
    id
}

/// A checking account and the category Rent.
fn household() -> Household {
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    let checking = add_account(&mut book, "Checking account", AccountKind::Asset);
    let rent = add_account(&mut book, "Rent", AccountKind::Expense);
    Household {
        book,
        checking,
        rent,
    }
}

/// An entry of `minor_units` in `account` on `on`, from `payee`, with its
/// other side not sorted yet.
fn entry(book: &mut Book, account: AccountId, minor_units: i64, on: Date, payee: &str) -> String {
    let transaction = Transaction::new(
        TransactionId::generate(),
        on,
        Some(payee.into()),
        vec![
            Split::new(account, eur(minor_units)),
            Split::uncategorized(eur(-minor_units)),
        ],
    )
    .expect("balanced");
    book.add_transaction(&transaction).expect("added");
    transaction.id().to_string()
}

/// The rent: 750,00 € out on the 1st of every month from September.
fn rent_form(household: &Household) -> PlanForm {
    PlanForm {
        name: "Rent".into(),
        amount: "750,00".into(),
        category_id: household.rent.to_string(),
        first_due: "2026-09-01".into(),
        party: LANDLORD_IBAN.into(),
        ..new_form(&household.checking.to_string(), &TestLocale)
    }
}

fn changes(book: &Book) -> usize {
    book.history().expect("history read").len()
}

#[test]
fn a_new_plan_is_listed_with_exact_amounts_and_one_undo_takes_it_back() {
    let mut household = household();
    let form = rent_form(&household);
    let id = save(&mut household.book, "", &form, &TestLocale).expect("saved");

    let plans = list(&household.book, &TestLocale).expect("listed");
    assert_eq!(plans.len(), 1);
    let rent = &plans[0];
    assert_eq!(rent.id, id);
    assert_eq!(rent.name, "Rent");
    assert_eq!(rent.account, "Checking account");
    assert_eq!(rent.account_id, household.checking.to_string());
    assert_eq!(rent.amount.exact, "750,00 €");
    assert!(rent.outgoing);
    assert_eq!((rent.unit, rent.every), ("month", 1));
    assert_eq!(rent.first_due, "2026-09-01");
    assert_eq!(rent.ends, "");
    // 1 November 2026 is a Sunday: the payment is due on Monday.
    assert_eq!(rent.next_due, "2026-11-02");
    assert!(!rent.paused && !rent.account_closed);
    assert_eq!(rent.category, "Rent");
    assert_eq!(rent.party, LANDLORD_IBAN);

    // Stored exactly, the money going out.
    let stored = household.book.schedules().expect("read");
    assert_eq!(stored[0].amount, eur(-75_000));
    assert_eq!(stored[0].tolerance, eur(0));

    household.book.undo().expect("undone");
    assert!(
        list(&household.book, &TestLocale)
            .expect("listed")
            .is_empty()
    );
    household.book.redo().expect("redone");
    assert_eq!(list(&household.book, &TestLocale).expect("listed").len(), 1);
}

#[test]
fn an_iban_typed_with_spaces_is_kept_without_them_and_a_name_as_typed() {
    let mut household = household();
    let form = PlanForm {
        party: "de89 3704 0044 0532 0130 00".into(),
        ..rent_form(&household)
    };
    save(&mut household.book, "", &form, &TestLocale).expect("saved");
    let form = PlanForm {
        name: "Gym".into(),
        party: "Example Gym".into(),
        ..rent_form(&household)
    };
    save(&mut household.book, "", &form, &TestLocale).expect("saved");
    let plans = household.book.schedules().expect("read");
    let rent = plans.iter().find(|plan| plan.name == "Rent").expect("rent");
    assert_eq!(rent.party_iban.as_deref(), Some(LANDLORD_IBAN));
    assert_eq!(rent.party_name, None);
    let gym = plans.iter().find(|plan| plan.name == "Gym").expect("gym");
    assert_eq!(gym.party_iban, None);
    assert_eq!(gym.party_name.as_deref(), Some("Example Gym"));
}

#[test]
fn an_edit_keeps_the_pause_and_the_edit_form_shows_the_plan_as_saved() {
    let mut household = household();
    let rent = rent_form(&household);
    let id = save(&mut household.book, "", &rent, &TestLocale).expect("saved");
    set_paused(&mut household.book, &id, true).expect("paused");
    assert_eq!(
        list(&household.book, &TestLocale).expect("listed")[0].next_due,
        "",
        "a paused plan waits for nothing"
    );

    let form = edit_form(&household.book, &id, &TestLocale).expect("form");
    assert_eq!(form.amount, "750,00");
    assert!(form.outgoing);
    assert_eq!(form.party, LANDLORD_IBAN);
    assert_eq!(form.first_due, "2026-09-01");
    assert_eq!(form.from_entry, "");
    let changed = PlanForm {
        amount: "-800,00".into(),
        first_due: "2026-09-03".into(),
        ..form
    };
    assert_eq!(
        save(&mut household.book, &id, &changed, &TestLocale).expect("saved"),
        id
    );

    let stored = household.book.schedules().expect("read");
    assert_eq!(stored.len(), 1);
    assert!(stored[0].paused, "an edit keeps the plan paused");
    // A sign typed in the amount counts for nothing: the form says "out".
    assert_eq!(stored[0].amount, eur(-80_000));
    assert_eq!(stored[0].anchor, day(2026, 9, 3));
    assert_eq!(stored[0].track_from, day(2026, 9, 3));

    set_paused(&mut household.book, &id, false).expect("going on");
    assert_eq!(
        list(&household.book, &TestLocale).expect("listed")[0].next_due,
        "2026-11-03"
    );
    household.book.undo().expect("undone");
    household.book.undo().expect("undone");
    assert_eq!(
        household.book.schedules().expect("read")[0].amount,
        eur(-75_000)
    );
}

#[test]
fn did_this_happen_offers_match_skip_and_book_each_taken_back_by_undo() {
    let mut household = household();
    let checking = household.checking;
    let rent = rent_form(&household);
    let id = save(&mut household.book, "", &rent, &TestLocale).expect("saved");
    let paid = entry(
        &mut household.book,
        checking,
        -75_000,
        day(2026, 10, 2),
        "Landlord",
    );
    // Money coming in is never offered for money going out.
    entry(
        &mut household.book,
        checking,
        75_000,
        day(2026, 10, 2),
        "Refund",
    );

    let open = did_this_happen(&household.book, &TestLocale).expect("read");
    let days: Vec<(&str, &str, &str)> = open
        .iter()
        .map(|payment| (payment.due_on.as_str(), payment.due.as_str(), payment.state))
        .collect();
    assert_eq!(
        days,
        [
            ("2026-09-01", "2026-09-01", "overdue"),
            ("2026-10-01", "2026-10-01", "overdue")
        ]
    );
    assert_eq!(open[0].plan_id, id);
    assert_eq!(open[0].amount.exact, "750,00 €");

    // Match an entry…
    let choices = entry_choices(&household.book, &id, "2026-10-01", &TestLocale).expect("read");
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].id, paid);
    assert_eq!(choices[0].date, "2026-10-02");
    assert_eq!(choices[0].payee, "Landlord");
    assert_eq!(choices[0].amount.exact, "-750,00 €");
    match_entry(&mut household.book, &id, "2026-10-01", &paid).expect("matched");
    assert_eq!(
        did_this_happen(&household.book, &TestLocale)
            .expect("read")
            .len(),
        1
    );
    assert!(
        entry_choices(&household.book, &id, "2026-09-01", &TestLocale)
            .expect("read")
            .is_empty(),
        "a matched entry is offered no more"
    );

    // Skip this time.
    let before_skip = changes(&household.book);
    skip(&mut household.book, &id, "2026-09-01").expect("skipped");
    assert!(
        did_this_happen(&household.book, &TestLocale)
            .expect("read")
            .is_empty()
    );
    assert_eq!(changes(&household.book), before_skip + 1);
    household.book.undo().expect("undone");
    assert_eq!(
        did_this_happen(&household.book, &TestLocale)
            .expect("read")
            .len(),
        1
    );

    // Book it.
    assert_eq!(
        book_form(&household.book, &id, &TestLocale).expect("form"),
        ("750,00".to_owned(), "2026-10-19".to_owned())
    );
    let parts_before = household.book.account_parts(checking).expect("read").len();
    let booked = book_it(
        &mut household.book,
        &id,
        "2026-09-01",
        "2026-09-03",
        "750,01",
        &TestLocale,
    )
    .expect("booked");
    assert!(
        did_this_happen(&household.book, &TestLocale)
            .expect("read")
            .is_empty()
    );
    let parts = household.book.account_parts(checking).expect("read");
    assert_eq!(parts.len(), parts_before + 1);
    let new = parts
        .iter()
        .find(|part| part.transaction.to_string() == booked)
        .expect("the booked entry");
    assert_eq!(
        new.amount,
        eur(-75_001),
        "the typed amount, exactly, going out"
    );
    assert_eq!(new.booking_date, day(2026, 9, 3));

    // One Undo takes back the entry and opens the payment again.
    household.book.undo().expect("undone");
    assert_eq!(
        household.book.account_parts(checking).expect("read").len(),
        parts_before
    );
    assert_eq!(
        did_this_happen(&household.book, &TestLocale)
            .expect("read")
            .len(),
        1
    );
    // And the match before it.
    household.book.undo().expect("undone");
    assert_eq!(
        did_this_happen(&household.book, &TestLocale)
            .expect("read")
            .len(),
        2
    );
}

#[test]
fn coming_up_lists_the_open_payments_of_the_next_days() {
    let mut household = household();
    let rent = rent_form(&household);
    save(&mut household.book, "", &rent, &TestLocale).expect("saved");
    let soon = coming_up(&household.book, COMING_UP_DAYS, &TestLocale).expect("read");
    let days: Vec<(&str, &str)> = soon
        .iter()
        .map(|payment| (payment.due.as_str(), payment.state))
        .collect();
    assert_eq!(days, [("2026-11-02", "upcoming")]);
    assert_eq!(soon[0].due_on, "2026-11-01", "counted on the 1st");
    assert_eq!(soon[0].account, "Checking account");
}

#[test]
fn removing_a_plan_keeps_its_entries_and_undo_brings_back_its_matches() {
    let mut household = household();
    let checking = household.checking;
    let rent = rent_form(&household);
    let id = save(&mut household.book, "", &rent, &TestLocale).expect("saved");
    let paid = entry(
        &mut household.book,
        checking,
        -75_000,
        day(2026, 10, 1),
        "Landlord",
    );
    match_entry(&mut household.book, &id, "2026-10-01", &paid).expect("matched");
    let parts = household.book.account_parts(checking).expect("read").len();

    remove(&mut household.book, &id).expect("removed");
    assert!(
        list(&household.book, &TestLocale)
            .expect("listed")
            .is_empty()
    );
    assert_eq!(
        household.book.account_parts(checking).expect("read").len(),
        parts
    );
    assert!(matches!(
        remove(&mut household.book, &id),
        Err(PlansError::NoSuchEntry)
    ));

    household.book.undo().expect("undone");
    let decided = household.book.decided_occurrences().expect("read");
    assert_eq!(decided.len(), 1);
    assert!(decided.values().all(|decided| *decided
        == Decided::Matched(TransactionId::from_bytes(id_bytes(&paid).expect("an ID")))));
}

#[test]
fn plan_this_again_saves_the_plan_with_its_entry_matched_in_one_action() {
    let mut household = household();
    let checking = household.checking;
    let gym = entry(
        &mut household.book,
        checking,
        -2_990,
        day(2026, 10, 5),
        "Example Gym",
    );

    let form = form_from_entry(
        &household.book,
        &gym,
        &checking.to_string(),
        "Planned payment",
        &TestLocale,
    )
    .expect("read")
    .expect("a plan can match this entry");
    assert_eq!(form.name, "Example Gym");
    assert_eq!(form.amount, "29,90");
    assert!(form.outgoing);
    assert_eq!((form.unit.as_str(), form.every.as_str()), ("month", "1"));
    assert_eq!(form.first_due, "2026-10-05");
    assert_eq!(form.category_id, "", "the other side isn't sorted");
    assert_eq!(form.from_entry, gym);

    let before = changes(&household.book);
    let id = save(&mut household.book, "", &form, &TestLocale).expect("saved");
    assert_eq!(changes(&household.book), before + 1, "one action");
    let decided = household.book.decided_occurrences().expect("read");
    let plan = plan_id(&id).expect("an ID");
    assert_eq!(
        decided.get(&(plan, day(2026, 10, 5))),
        Some(&Decided::Matched(TransactionId::from_bytes(
            id_bytes(&gym).expect("an ID")
        )))
    );
    assert!(
        did_this_happen(&household.book, &TestLocale)
            .expect("read")
            .is_empty()
    );

    household.book.undo().expect("undone");
    assert!(household.book.schedules().expect("read").is_empty());
    assert!(
        household
            .book
            .decided_occurrences()
            .expect("read")
            .is_empty()
    );
}

#[test]
fn plan_this_again_refuses_entries_that_are_not_there() {
    let household = household();
    let unknown = TransactionId::generate().to_string();
    for (transaction, account) in [
        (unknown.as_str(), household.checking.to_string()),
        ("not an ID", household.checking.to_string()),
        (unknown.as_str(), AccountId::generate().to_string()),
    ] {
        assert!(
            matches!(
                form_from_entry(&household.book, transaction, &account, "Plan", &TestLocale),
                Err(PlansError::NoSuchEntry)
            ),
            "{transaction} in {account}"
        );
    }
}

#[test]
fn mistakes_in_the_form_are_refused_without_changing_the_book() {
    let mut household = household();
    let good = rent_form(&household);
    let before = changes(&household.book);
    let cases: Vec<(PlanForm, &str)> = vec![
        (
            PlanForm {
                every: "0".into(),
                ..good.clone()
            },
            "input.plan-every",
        ),
        (
            PlanForm {
                every: "53".into(),
                ..good.clone()
            },
            "input.plan-every",
        ),
        (
            PlanForm {
                unit: "fortnight".into(),
                ..good.clone()
            },
            "input.plan-every",
        ),
        (
            PlanForm {
                first_due: "the first".into(),
                ..good.clone()
            },
            "input.malformed-date",
        ),
        (
            PlanForm {
                ends: "2026-13-01".into(),
                ..good.clone()
            },
            "input.malformed-date",
        ),
        (
            PlanForm {
                account_id: AccountId::generate().to_string(),
                ..good.clone()
            },
            "input.no-such-entry",
        ),
        (
            PlanForm {
                name: "   ".into(),
                ..good.clone()
            },
            "store.schedule-invalid",
        ),
        (
            PlanForm {
                ends: "2026-08-01".into(),
                ..good.clone()
            },
            "store.schedule-invalid",
        ),
    ];
    for (form, code) in cases {
        let error = save(&mut household.book, "", &form, &TestLocale).expect_err("refused");
        assert_eq!(
            error_text(&household.book, &error, &TestLocale).code,
            code,
            "{form:?}"
        );
        assert!(
            next_dates(&household.book, &form, &TestLocale).is_empty(),
            "{form:?}"
        );
    }
    let error = save(
        &mut household.book,
        "",
        &PlanForm {
            amount: "seven".into(),
            ..good.clone()
        },
        &TestLocale,
    )
    .expect_err("refused");
    assert!(matches!(error, PlansError::Amount(_, currency) if currency == EUR));
    assert!(household.book.schedules().expect("read").is_empty());
    assert_eq!(changes(&household.book), before);

    // A plan the screen names that is gone is no entry, whatever it asks.
    let gone = ScheduleId::generate().to_string();
    assert!(matches!(
        set_paused(&mut household.book, &gone, true),
        Err(PlansError::NoSuchEntry)
    ));
    assert!(matches!(
        skip(&mut household.book, &gone, "2026-10-01"),
        Err(PlansError::NoSuchEntry)
    ));
    assert!(matches!(
        edit_form(&household.book, "x", &TestLocale),
        Err(PlansError::NoSuchEntry)
    ));
    assert!(matches!(
        save(&mut household.book, &gone, &good, &TestLocale),
        Err(PlansError::NoSuchEntry)
    ));
    assert_eq!(changes(&household.book), before);
}

#[test]
fn the_form_shows_the_next_three_payments_moved_to_a_banking_day() {
    let household = household();
    let form = PlanForm {
        first_due: "2026-11-01".into(),
        ..rent_form(&household)
    };
    assert_eq!(
        next_dates(&household.book, &form, &TestLocale),
        ["2026-11-02", "2026-12-01", "2027-01-01"]
    );
    // Counted from today when the first payment is in the past.
    let weekly = PlanForm {
        unit: "week".into(),
        every: "2".into(),
        first_due: "2026-10-05".into(),
        ..rent_form(&household)
    };
    assert_eq!(
        next_dates(&household.book, &weekly, &TestLocale),
        ["2026-10-19", "2026-11-02", "2026-11-16"]
    );
}

#[test]
fn only_the_users_own_accounts_take_plans() {
    assert!(takes_plans(AccountKind::Asset));
    assert!(takes_plans(AccountKind::Liability));
    assert!(!takes_plans(AccountKind::Expense));
    assert!(!takes_plans(AccountKind::Income));
}

/// A plan of `amount` on `account`, every `every` `unit` from `first_due`,
/// going out or coming in.
fn plan_on(
    book: &mut Book,
    account: AccountId,
    name: &str,
    amount: &str,
    outgoing: bool,
    unit: &str,
    first_due: &str,
) {
    let form = PlanForm {
        name: name.into(),
        amount: amount.into(),
        outgoing,
        unit: unit.into(),
        first_due: first_due.into(),
        ..new_form(&account.to_string(), &TestLocale)
    };
    save(book, "", &form, &TestLocale).expect("saved");
}

#[test]
fn left_this_month_takes_off_the_payments_still_due_and_counts_no_overdue_one() {
    let mut household = household();
    let checking = household.checking;
    let book = &mut household.book;
    entry(book, checking, 234_000, day(2026, 10, 2), "Employer");
    // A second checking account in dollars, left out and said so (D-043).
    let usd = CurrencyCode::new("USD").expect("valid");
    book.use_currency(usd).expect("currency used");
    let dollars = AccountId::generate();
    book.add_account(&Account {
        id: dollars,
        name: "Dollar account".into(),
        kind: AccountKind::Asset,
        currency: usd,
        account_type: Some(AccountType::Checking),
        closed: false,
        parent: None,
        number: None,
        notes: None,
        institution: None,
    })
    .expect("account added");
    // Rent starts next month; the gym is due Sunday the 25th, so Monday
    // the 26th; the salary comes on the 27th; the insurance of the 5th
    // never came, so it waits as "Did this happen?" and counts nowhere.
    plan_on(
        book,
        checking,
        "Rent",
        "750,00",
        true,
        "month",
        "2026-11-01",
    );
    plan_on(book, checking, "Gym", "29,90", true, "month", "2026-10-25");
    plan_on(
        book,
        checking,
        "Salary",
        "2200,00",
        false,
        "month",
        "2026-10-27",
    );
    plan_on(
        book,
        checking,
        "Insurance",
        "96,00",
        true,
        "year",
        "2026-10-05",
    );
    plan_on(
        book,
        dollars,
        "Dollar plan",
        "10,00",
        true,
        "month",
        "2026-10-30",
    );

    let month = this_month(book, TABLE_MONTHS, &TestLocale).expect("read");
    assert_eq!(month.accounts, ["Checking account"]);
    assert_eq!(month.left_out, 1, "the dollar account is left out");
    assert!(month.has_plans);
    let exact = |shown: &Shown| shown.exact.clone();
    assert_eq!(exact(&month.balance_now), "2340,00 €");
    assert_eq!(exact(&month.out_due), "29,90 €");
    assert_eq!(
        month.in_due.as_ref().map(exact).as_deref(),
        Some("2200,00 €")
    );
    // 2340,00 − 29,90; the salary not yet here is not added.
    assert_eq!(exact(&month.left), "2310,10 €");
    assert!(!month.below_zero);
    assert_eq!(
        month.expected_end.as_ref().map(exact).as_deref(),
        Some("4510,10 €")
    );
    assert_eq!(month.overdue, 1);
    assert_eq!(
        month.status,
        status::forecast(Some(ForecastFigures {
            accounts: true,
            plans: true,
            overdue: 1,
        }))
    );
    assert_eq!(month.status.status.id(), "to-do");
    // November: rent 750,00 and gym 29,90 out, salary in; each end is the
    // next month's start.
    let rows: Vec<(&str, String, String, String, String)> = month
        .months
        .iter()
        .map(|row| {
            (
                row.month.as_str(),
                exact(&row.start),
                exact(&row.planned_in),
                exact(&row.planned_out),
                exact(&row.end),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            (
                "November 2026",
                "4510,10 €".to_owned(),
                "2200,00 €".to_owned(),
                "779,90 €".to_owned(),
                "5930,20 €".to_owned()
            ),
            (
                "December 2026",
                "5930,20 €".to_owned(),
                "2200,00 €".to_owned(),
                "779,90 €".to_owned(),
                "7350,30 €".to_owned()
            ),
            (
                "January 2027",
                "7350,30 €".to_owned(),
                "2200,00 €".to_owned(),
                "779,90 €".to_owned(),
                "8770,40 €".to_owned()
            ),
        ]
    );

    // Matching the insurance by hand takes it out of the overdue count
    // without changing a figure: its entry is in the balance from then on.
    let insurance = list(book, &TestLocale)
        .expect("listed")
        .into_iter()
        .find(|plan| plan.name == "Insurance")
        .expect("insurance listed");
    skip(book, &insurance.id, "2026-10-05").expect("skipped");
    let after = this_month(book, 0, &TestLocale).expect("read");
    assert_eq!(after.overdue, 0);
    assert_eq!(exact(&after.left), "2310,10 €");
    assert_eq!(after.status.status.id(), "fine");
    assert!(after.months.is_empty());
    // One Undo brings the payment back as overdue.
    book.undo().expect("undone");
    assert_eq!(this_month(book, 0, &TestLocale).expect("read").overdue, 1);
}

#[test]
fn left_this_month_shows_no_figure_without_a_plan_or_an_account() {
    // No account at all: nothing to look ahead for.
    let empty = Book::create_in_memory().expect("book created");
    let month = this_month(&empty, TABLE_MONTHS, &TestLocale).expect("read");
    assert!(month.accounts.is_empty());
    assert_eq!(month.left.exact, "");
    assert_eq!(month.status.status.id(), "unknown");
    // An account but no plan: the balance alone is not a forecast.
    let mut household = household();
    let checking = household.checking;
    entry(
        &mut household.book,
        checking,
        -5_000,
        day(2026, 10, 2),
        "Shop",
    );
    let month = this_month(&household.book, TABLE_MONTHS, &TestLocale).expect("read");
    assert!(!month.has_plans);
    assert_eq!(month.status.status.id(), "unknown");
    assert!(month.below_zero, "the balance is below zero");
    assert_eq!(month.left.exact, "-50,00 €");
    assert_eq!(month.in_due, None);
    assert_eq!(month.expected_end, None);
    // A paused plan counts for nothing either.
    plan_on(
        &mut household.book,
        checking,
        "Gym",
        "29,90",
        true,
        "month",
        "2026-10-25",
    );
    let gym = list(&household.book, &TestLocale).expect("listed")[0]
        .id
        .clone();
    set_paused(&mut household.book, &gym, true).expect("paused");
    let month = this_month(&household.book, TABLE_MONTHS, &TestLocale).expect("read");
    assert_eq!(month.status.status.id(), "unknown");
    assert_eq!(month.out_due.exact, "0,00 €");
}
