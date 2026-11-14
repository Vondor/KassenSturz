// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! KMyMoney's scheduled transactions as planned payments (#919), from the
//! importer's real output for the project's test book with one schedule of
//! each kind (`kassensturz-import/tests/golden/`, invented data only), and
//! from the same output changed to reach the limits of what a plan can hold.

use std::collections::HashMap;

use kassensturz_core::{
    Account, CurrencyCode, Date, MAX_SCHEDULE_NAME_CHARS, MAX_SCHEDULES, Money, Schedule,
    ScheduleUnit, due_dates,
};
use kassensturz_import::{
    BookWeekend, Imported, ImportedBook, Kmy, KmySqlite, Limits, SkippedData, import,
};
use kassensturz_pipeline::{BookImportReport, fill_book_from};
use kassensturz_store::Book;

const XML: &[u8] = include_bytes!("../../kassensturz-import/tests/golden/kmy/schedules.kmy");
const SQLITE: &[u8] =
    include_bytes!("../../kassensturz-import/tests/golden/kmy-sqlite/schedules.sqlite");

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};

fn imported(xml: bool) -> ImportedBook {
    let result = if xml {
        import(&Kmy, XML, &Limits::default())
    } else {
        import(&KmySqlite, SQLITE, &Limits::default())
    };
    match result.expect("the sample imports") {
        Imported::Book(book) => *book,
        _ => panic!("the sample is a book"),
    }
}

fn filled(imported: &ImportedBook) -> (Book, BookImportReport) {
    let mut book = Book::create_in_memory().expect("book created");
    let report = fill_book_from(&mut book, imported, &HashMap::new()).expect("filled");
    (book, report)
}

fn date(year: i16, month: i8, day: i8) -> Date {
    Date::new(year, month, day).expect("a valid date")
}

fn eur(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, EUR)
}

fn skipped(report: &BookImportReport, what: SkippedData) -> usize {
    report
        .skipped
        .iter()
        .find(|skipped| skipped.what == what)
        .map_or(0, |skipped| skipped.count)
}

fn account_named(book: &Book, name: &str) -> Account {
    let categories = book
        .categories()
        .expect("categories")
        .into_iter()
        .map(|entry| entry.category);
    book.accounts(true)
        .expect("accounts")
        .into_iter()
        .chain(categories)
        .find(|account| account.name == name)
        .unwrap_or_else(|| panic!("an account named {name}"))
}

fn plan_named(book: &Book, name: &str) -> Schedule {
    book.schedules()
        .expect("schedules")
        .into_iter()
        .find(|schedule| schedule.name == name)
        .unwrap_or_else(|| panic!("a planned payment named {name}"))
}

#[test]
fn each_kind_of_schedule_becomes_a_planned_payment() {
    let (book, report) = filled(&imported(true));
    assert_eq!(report.schedules, 6);
    assert_eq!(skipped(&report, SkippedData::Schedules), 0);
    let one = account_named(&book, "Girokonto Eins");
    let two = account_named(&book, "Girokonto Zwei");
    let id = |name: &str| account_named(&book, name).id;

    // A monthly bill, paid on the day after a weekend date.
    let rent = plan_named(&book, "Miete");
    assert_eq!(rent.account, one.id);
    assert_eq!(rent.amount, eur(-85_000));
    assert_eq!(rent.tolerance, eur(0));
    assert_eq!(rent.category, Some(id("Rent")));
    // The payee is the user's own label, not the bank's text: not kept.
    assert_eq!(
        (rent.party_name.as_deref(), rent.party_iban.as_deref()),
        (None, None)
    );
    assert_eq!((rent.unit, rent.every), (ScheduleUnit::Month, 1));
    assert_eq!(
        (rent.anchor, rent.track_from),
        (date(2026, 12, 1), date(2026, 12, 1))
    );
    assert_eq!(rent.ends_on, None);
    assert!(!rent.paused);

    // A yearly one.
    let insurance = plan_named(&book, "Versicherung jährlich");
    assert_eq!((insurance.unit, insurance.every), (ScheduleUnit::Year, 1));
    assert_eq!(insurance.amount, eur(-12_000));
    assert_eq!(insurance.anchor, date(2027, 1, 15));

    // Every two weeks, ending on a day.
    let gym = plan_named(&book, "Fitnessstudio");
    assert_eq!((gym.unit, gym.every), (ScheduleUnit::Week, 2));
    assert_eq!(gym.ends_on, Some(date(2027, 3, 31)));
    assert_eq!(gym.amount, eur(-1_500));

    // Money that comes in.
    let salary = plan_named(&book, "Gehalt");
    assert_eq!(salary.amount, eur(200_000));
    assert_eq!(salary.category, Some(id("Salary")));

    // A move between two of the user's accounts: planned on the account
    // the money leaves, with no category.
    let savings = plan_named(&book, "Sparen (automatisch)");
    assert_eq!(savings.account, one.id);
    assert_eq!(savings.amount, eur(-10_000));
    assert_eq!(savings.category, None);

    // One that was entered once: it is expected from its next due date on,
    // so the entry the file already holds is not expected a second time.
    let home = plan_named(&book, "Hausratversicherung");
    assert_eq!(home.account, two.id);
    assert_eq!(home.anchor, date(2026, 11, 10));
    assert_eq!(home.track_from, date(2026, 12, 10));
    let due = due_dates(&home, date(2026, 11, 1), date(2027, 1, 31));
    let dates: Vec<Date> = due.iter().map(|due| due.counted).collect();
    assert_eq!(dates, [date(2026, 12, 10), date(2027, 1, 10)]);
}

#[test]
fn the_xml_and_the_sqlite_file_make_the_same_plans() {
    let plans = |xml: bool| {
        let (book, _) = filled(&imported(xml));
        let mut plans: Vec<_> = book
            .schedules()
            .expect("schedules")
            .into_iter()
            .map(|plan| Schedule {
                id: kassensturz_core::ScheduleId::generate(),
                // Account IDs differ between the two imports; names don't.
                account: account_named(&book, "Girokonto Eins").id,
                category: None,
                ..plan
            })
            .map(|plan| {
                (
                    plan.name.clone(),
                    plan.amount,
                    plan.unit,
                    plan.every,
                    plan.anchor,
                    plan.track_from,
                    plan.ends_on,
                )
            })
            .collect();
        plans.sort_by(|a, b| a.0.cmp(&b.0));
        plans
    };
    assert_eq!(plans(true), plans(false));
}

#[test]
fn what_the_other_program_does_beyond_a_plan_is_read_but_not_kept() {
    let book = imported(true);
    let by = |name: &str| {
        book.schedules
            .iter()
            .find(|s| s.name == name)
            .expect("a schedule")
    };
    // It enters this one by itself; a planned payment never books by itself.
    assert!(by("Sparen (automatisch)").auto_enter);
    assert_eq!(by("Fitnessstudio").weekend, BookWeekend::Before);
    assert_eq!(by("Miete").weekend, BookWeekend::After);
    assert_eq!(by("Gehalt").weekend, BookWeekend::Unchanged);
}

#[test]
fn a_matching_entry_the_plan_is_expected_to_meet_is_matched() {
    // The steps meet: the importer's plan and the book's own matching.
    let (mut book, _) = filled(&imported(true));
    let two = account_named(&book, "Girokonto Zwei");
    let insurance = account_named(&book, "Insurance");
    let plan = plan_named(&book, "Hausratversicherung");
    let entry = kassensturz_core::Transaction::new(
        kassensturz_core::TransactionId::generate(),
        date(2026, 12, 10),
        Some("Beispiel Versicherung".to_owned()),
        vec![
            kassensturz_core::Split {
                account: Some(two.id),
                ..kassensturz_core::Split::uncategorized(eur(-990))
            },
            kassensturz_core::Split {
                account: Some(insurance.id),
                ..kassensturz_core::Split::uncategorized(eur(990))
            },
        ],
    )
    .expect("a balanced transaction");
    book.add_transaction(&entry).expect("added");
    let report = book
        .match_schedules(date(2026, 12, 12), None)
        .expect("matching runs");
    assert_eq!(report.matched.len(), 1);
    assert_eq!(report.matched[0].occurrence.schedule, plan.id);
}

#[test]
fn what_a_plan_cant_hold_stays_counted_as_left_out() {
    let mut base = imported(true);
    let template = base.schedules[0].clone();
    let mut more = |change: &dyn Fn(&mut kassensturz_import::BookSchedule)| {
        let mut one = template.clone();
        one.key = format!("X{}", base.schedules.len());
        change(&mut one);
        base.schedules.push(one);
    };
    // A payment of three parts.
    more(&|s| s.payment.parts.push(s.payment.parts[0].clone()));
    // A payment with no part in the user's accounts.
    more(&|s| {
        for part in &mut s.payment.parts {
            part.account = None;
        }
    });
    // A plan that ended before its next due date.
    more(&|s| s.ends_on = Some(date(2020, 1, 1)));
    // A payment that doesn't add up.
    more(&|s| s.payment.parts[1].amount = eur(1));
    // A name of nothing but spaces, with no payee or note to use instead.
    more(&|s| {
        s.name = "  \n ".to_owned();
        s.payment.payee = None;
        s.payment.note = None;
    });
    let (_, report) = filled(&base);
    assert_eq!(report.schedules, 6);
    assert_eq!(skipped(&report, SkippedData::Schedules), 5);
}

#[test]
fn texts_beyond_what_a_plan_keeps_are_cut_never_refused() {
    let mut base = imported(true);
    base.schedules[0].name = format!("  {}  ", "Zeile\n".repeat(30));
    let (book, report) = filled(&base);
    assert_eq!(report.schedules, 6);
    let plan = book
        .schedules()
        .expect("schedules")
        .into_iter()
        .find(|plan| plan.name.starts_with("Zeile"))
        .expect("the long one");
    // Cut at the limit, then trimmed: the cut fell after a space.
    assert!(
        (MAX_SCHEDULE_NAME_CHARS - 1..=MAX_SCHEDULE_NAME_CHARS)
            .contains(&plan.name.chars().count())
    );
    assert_eq!(plan.name, plan.name.trim());
    assert!(!plan.name.contains('\n'));
}

#[test]
fn a_book_holds_only_so_many_plans_and_the_rest_is_counted() {
    let mut base = imported(true);
    let template = base.schedules[0].clone();
    for number in 0..MAX_SCHEDULES {
        let mut one = template.clone();
        one.key = format!("N{number}");
        base.schedules.push(one);
    }
    let all = base.schedules.len();
    let (_, report) = filled(&base);
    assert_eq!(report.schedules, MAX_SCHEDULES);
    assert_eq!(
        skipped(&report, SkippedData::Schedules),
        all - MAX_SCHEDULES
    );
}

#[test]
fn a_plan_on_a_closed_account_is_left_out() {
    // Girokonto Eins holds no money in the sample, so it stays closed;
    // five of the six plans are on it. Girokonto Zwei's plan stays.
    let mut base = imported(true);
    base.accounts
        .iter_mut()
        .filter(|account| account.name == "Girokonto Eins")
        .for_each(|account| account.closed = true);
    let (book, report) = filled(&base);
    assert_eq!(report.schedules, 1);
    assert_eq!(skipped(&report, SkippedData::Schedules), 5);
    assert_eq!(
        plan_named(&book, "Hausratversicherung").account,
        account_named(&book, "Girokonto Zwei").id
    );
}
