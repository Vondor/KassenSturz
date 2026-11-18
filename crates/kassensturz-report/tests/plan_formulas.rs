// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! D-826: formulas name the planned payments (`Plans.OUT_DUE`,
//! `Plans.IN_DUE`, `Plans.OUT_OVERDUE`, `Plans.IN_OVERDUE`), so a figure
//! chart can show what is probably left at the month's end. The plans count
//! as *Left this month* counts them (`docs/SCHEDULES.md` §5), from the
//! forecast's own function, so the chart and the tile never disagree.
//!
//! The book is synthetic: invented accounts, plans and amounts.

use kassensturz_core::{
    Account, AccountId, AccountKind, AccountType, CurrencyCode, Date, Money, ScheduleId,
    ScheduleUnit, Split, Transaction, TransactionId,
};
use kassensturz_report::{Filter, Formula, FormulaValue, Period, ReportError, Source, Term};
use kassensturz_store::{Book, NewSchedule};

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR"),
};
const USD: CurrencyCode = match CurrencyCode::new("USD") {
    Ok(code) => code,
    Err(_) => panic!("USD"),
};

fn day(y: i16, m: i8, d: i8) -> Date {
    Date::new(y, m, d).unwrap()
}

fn eur(minor: i64) -> Money {
    Money::from_minor_units(minor, EUR)
}

/// Tuesday 17 November 2026.
fn today() -> Date {
    day(2026, 11, 17)
}

fn november() -> Period {
    Period::new(day(2026, 11, 1), day(2026, 11, 30)).unwrap()
}

fn account(book: &mut Book, name: &str, kind: AccountKind, currency: CurrencyCode) -> AccountId {
    let id = AccountId::generate();
    book.add_account(&Account {
        id,
        name: name.into(),
        kind,
        currency,
        account_type: (kind == AccountKind::Asset).then_some(AccountType::Checking),
        parent: None,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    })
    .unwrap();
    id
}

fn pay(book: &mut Book, on: Date, from: AccountId, to: AccountId, amount: Money) -> TransactionId {
    let id = TransactionId::generate();
    book.add_transaction(
        &Transaction::new(
            id,
            on,
            None,
            vec![
                Split::new(
                    from,
                    Money::from_minor_units(-amount.minor_units(), amount.currency()),
                ),
                Split::new(to, amount),
            ],
        )
        .unwrap(),
    )
    .unwrap();
    id
}

/// A monthly plan of `amount` (negative: money out) on `account`, first
/// due on `first`.
fn plan(book: &mut Book, name: &str, account: AccountId, amount: Money, first: Date) -> ScheduleId {
    plan_until(book, name, account, amount, first, None)
}

fn plan_until(
    book: &mut Book,
    name: &str,
    account: AccountId,
    amount: Money,
    first: Date,
    ends_on: Option<Date>,
) -> ScheduleId {
    book.add_schedule(&NewSchedule {
        name: name.into(),
        account,
        amount,
        tolerance: Money::zero(amount.currency()),
        category: None,
        party_iban: None,
        party_name: None,
        unit: ScheduleUnit::Month,
        every: 1,
        anchor: first,
        track_from: first,
        ends_on,
    })
    .unwrap()
}

struct Sample {
    book: Book,
    everyday: AccountId,
    spare: AccountId,
    travel: AccountId,
    groceries: AccountId,
    phone_plan: ScheduleId,
}

/// The same situation as the request's example, with invented figures:
/// 3,000.00 salary in October, 400.00 spent so far in November, a phone
/// plan of 15.50 still due on 20 November, three plans whose day passed
/// with no entry (40.00 and 9.99 on 9 November, 5.00 on 10 November), a
/// refund of 25.00 expected on 25 November, a paused plan and a plan that
/// ended. A USD account has a plan of its own.
fn sample() -> Sample {
    let mut book = Book::create_in_memory().unwrap();
    book.use_currency(EUR).unwrap();
    book.use_currency(USD).unwrap();
    let everyday = account(&mut book, "Everyday account", AccountKind::Asset, EUR);
    let spare = account(&mut book, "Spare account", AccountKind::Asset, EUR);
    let travel = account(&mut book, "Travel account", AccountKind::Asset, USD);
    let salary = account(&mut book, "Salary", AccountKind::Income, EUR);
    let groceries = account(&mut book, "Groceries", AccountKind::Expense, EUR);
    pay(&mut book, day(2026, 10, 30), salary, everyday, eur(300_000));
    pay(
        &mut book,
        day(2026, 11, 5),
        everyday,
        groceries,
        eur(40_000),
    );

    let phone_plan = plan(&mut book, "Phone", everyday, eur(-1_550), day(2026, 11, 20));
    plan(
        &mut book,
        "Savings plan",
        everyday,
        eur(-4_000),
        day(2026, 11, 9),
    );
    plan(&mut book, "Music", everyday, eur(-999), day(2026, 11, 9));
    plan(&mut book, "Club", everyday, eur(-500), day(2026, 11, 10));
    plan(&mut book, "Refund", everyday, eur(2_500), day(2026, 11, 25));
    // Paused: counts for nothing.
    let gym = plan(&mut book, "Gym", everyday, eur(-3_000), day(2026, 11, 24));
    book.set_schedule_paused(gym, true).unwrap();
    // Ended on 1 November: its 26 November never comes. Its October day
    // was skipped, so it is not overdue either.
    let magazine = plan_until(
        &mut book,
        "Magazine",
        everyday,
        eur(-700),
        day(2026, 10, 26),
        Some(day(2026, 11, 1)),
    );
    book.skip_schedule_occurrence(magazine, day(2026, 10, 26))
        .unwrap();
    // A plan in another currency, on its own account.
    plan(
        &mut book,
        "Hostel",
        travel,
        Money::from_minor_units(-1_200, USD),
        day(2026, 11, 23),
    );
    Sample {
        book,
        everyday,
        spare,
        travel,
        groceries,
        phone_plan,
    }
}

fn accounts(chosen: &[AccountId]) -> Filter {
    Filter {
        accounts: Some(chosen.to_vec()),
        ..Filter::default()
    }
}

fn worked(
    book: &Book,
    text: &str,
    period: Period,
    filter: &Filter,
) -> Result<FormulaValue, ReportError> {
    let formula = Formula::parse(text)?;
    let source = Source::read_with_plans(book, formula.span(period)?, today())?;
    Ok(formula.evaluate(&source, period, filter, EUR)?.value)
}

fn amount(book: &Book, text: &str, period: Period, filter: &Filter) -> Money {
    match worked(book, text, period, filter) {
        Ok(FormulaValue::Amount(money)) => money,
        other => panic!("{text}: {other:?}"),
    }
}

const LEFT: &str = "=Income.Salary@\"last month\" - Spending.ALL";

#[test]
fn what_is_probably_left_takes_off_what_the_plans_still_expect() {
    let sample = sample();
    let book = &sample.book;
    let mine = accounts(&[sample.everyday]);
    assert_eq!(amount(book, LEFT, november(), &mine), eur(260_000));
    assert_eq!(
        amount(book, &format!("{LEFT} - Plans.OUT_DUE"), november(), &mine),
        eur(258_450)
    );
    // Option (a) of the request: the overdue ones are a name of their own.
    assert_eq!(
        amount(
            book,
            &format!("{LEFT} - Plans.OUT_DUE - Plans.OUT_OVERDUE"),
            november(),
            &mine
        ),
        eur(252_951)
    );
    assert_eq!(
        amount(book, "=Plans.OUT_OVERDUE", november(), &mine),
        eur(5_499)
    );
    assert_eq!(amount(book, "=Plans.IN_DUE", november(), &mine), eur(2_500));
    assert_eq!(amount(book, "=Plans.IN_OVERDUE", november(), &mine), eur(0));
    // Capitals don't matter, as for every word of a formula.
    assert_eq!(
        amount(book, "=plans.out_due", november(), &mine),
        eur(1_550)
    );
}

#[test]
fn the_chart_and_left_this_month_count_the_same() {
    let sample = sample();
    let book = &sample.book;
    let tile = book
        .forecast(&[sample.everyday], EUR, today(), 0)
        .unwrap()
        .forecast;
    let mine = accounts(&[sample.everyday]);
    assert_eq!(
        amount(book, "=Plans.OUT_DUE", november(), &mine),
        tile.out_due
    );
    assert_eq!(
        amount(book, "=Plans.IN_DUE", november(), &mine),
        tile.in_due
    );
    // The tile lists the overdue ones as "Did this happen?": 9 and 10
    // November, three of them.
    assert_eq!(tile.overdue.len(), 3, "{tile:#?}");
    assert_eq!(tile.out_due, eur(1_550));
}

#[test]
fn a_paused_plan_and_an_ended_plan_count_for_nothing() {
    let sample = sample();
    let book = &sample.book;
    let mine = accounts(&[sample.everyday]);
    // Gym (30.00, paused) and Magazine (7.00, ended) are in neither figure.
    assert_eq!(
        amount(book, "=Plans.OUT_DUE", november(), &mine),
        eur(1_550)
    );
    assert_eq!(
        amount(book, "=Plans.OUT_OVERDUE", november(), &mine),
        eur(5_499)
    );
}

#[test]
fn a_matched_due_day_counts_for_nothing_and_the_figure_stays_the_same() {
    let mut sample = sample();
    let phone = account(&mut sample.book, "Phone", AccountKind::Expense, EUR);
    let paid = pay(
        &mut sample.book,
        today(),
        sample.everyday,
        phone,
        eur(1_550),
    );
    sample
        .book
        .match_schedule_occurrence(sample.phone_plan, day(2026, 11, 20), paid)
        .unwrap();
    let book = &sample.book;
    let mine = accounts(&[sample.everyday]);
    assert_eq!(amount(book, "=Plans.OUT_DUE", november(), &mine), eur(0));
    assert_eq!(
        amount(book, "=Spending.ALL", november(), &mine),
        eur(41_550)
    );
    assert_eq!(
        amount(book, &format!("{LEFT} - Plans.OUT_DUE"), november(), &mine),
        eur(258_450)
    );
}

#[test]
fn an_entry_booked_ahead_is_counted_once() {
    // "Book it" on the due day, three days ahead of today: the entry is in
    // November's spending, so the plan no longer counts. (The tile adds it
    // because its balance stops at today, B-331.)
    let mut sample = sample();
    sample
        .book
        .book_schedule_occurrence(
            sample.phone_plan,
            day(2026, 11, 20),
            day(2026, 11, 20),
            eur(-1_550),
        )
        .unwrap();
    let book = &sample.book;
    let mine = accounts(&[sample.everyday]);
    assert_eq!(amount(book, "=Plans.OUT_DUE", november(), &mine), eur(0));
    assert_eq!(
        amount(book, &format!("{LEFT} - Plans.OUT_DUE"), november(), &mine),
        eur(258_450)
    );
    let tile = book
        .forecast(&[sample.everyday], EUR, today(), 0)
        .unwrap()
        .forecast;
    assert_eq!(tile.out_due, eur(1_550));
}

#[test]
fn only_the_plans_of_the_charts_accounts_count() {
    let sample = sample();
    let book = &sample.book;
    // An account with no plans.
    assert_eq!(
        amount(
            book,
            "=Plans.OUT_DUE",
            november(),
            &accounts(&[sample.spare])
        ),
        eur(0)
    );
    // The USD account alone: its own plan, in its currency.
    assert_eq!(
        amount(
            book,
            "=Plans.OUT_DUE",
            november(),
            &accounts(&[sample.travel])
        ),
        Money::from_minor_units(1_200, USD)
    );
    // A category filter doesn't apply: plans have no categories to filter.
    let filter = Filter {
        accounts: Some(vec![sample.everyday]),
        categories: Some(vec![sample.groceries]),
        ..Filter::default()
    };
    assert_eq!(
        amount(book, "=Plans.OUT_DUE", november(), &filter),
        eur(1_550)
    );
}

#[test]
fn two_currencies_are_one_result_each() {
    let sample = sample();
    let book = &sample.book;
    // All accounts: plans in EUR and USD can't be one figure, as for
    // Spending.ALL with spending in two currencies.
    match worked(book, "=Plans.OUT_DUE", november(), &Filter::default()) {
        Err(ReportError::FormulaMixedCurrencies {
            first,
            first_currency,
            second_currency,
            ..
        }) => {
            assert_eq!(first, "Plans.OUT_DUE");
            assert_eq!((first_currency, second_currency), (EUR, USD));
        }
        other => panic!("{other:?}"),
    }
    // Each currency on its own accounts.
    assert_eq!(
        amount(
            book,
            "=Plans.OUT_DUE",
            november(),
            &accounts(&[sample.everyday])
        ),
        eur(1_550)
    );
    assert_eq!(
        amount(
            book,
            "=Plans.OUT_DUE",
            november(),
            &accounts(&[sample.travel])
        ),
        Money::from_minor_units(1_200, USD)
    );
    // A currency with nothing expected doesn't mix: the USD plan is
    // upcoming, so nothing of it is overdue.
    assert_eq!(
        amount(book, "=Plans.OUT_OVERDUE", november(), &Filter::default()),
        eur(5_499)
    );
}

#[test]
fn another_period_counts_from_today_to_its_last_day() {
    let sample = sample();
    let book = &sample.book;
    let mine = accounts(&[sample.everyday]);
    // November and December: the December days of the monthly plans come
    // in (15.50 twice, 40.00, 9.99 and 5.00); the overdue ones stay.
    let two = Period::new(day(2026, 11, 1), day(2026, 12, 31)).unwrap();
    assert_eq!(amount(book, "=Plans.OUT_DUE", two, &mine), eur(8_599));
    assert_eq!(amount(book, "=Plans.OUT_OVERDUE", two, &mine), eur(5_499));
    // A name's own period: the same as the chart for December.
    assert_eq!(
        amount(book, "=Plans.OUT_DUE@2026-11..2026-12", november(), &mine),
        eur(8_599)
    );
    // October is over: nothing in it is still to come, and nothing of it
    // is overdue (the plans start in November).
    let october = Period::new(day(2026, 10, 1), day(2026, 10, 31)).unwrap();
    assert_eq!(amount(book, "=Plans.OUT_DUE", october, &mine), eur(0));
    assert_eq!(amount(book, "=Plans.OUT_OVERDUE", october, &mine), eur(0));
}

#[test]
fn what_the_formula_counted_names_the_plans() {
    let sample = sample();
    let formula = Formula::parse("=Spending.ALL + Plans.OUT_DUE").unwrap();
    assert!(formula.names_plans());
    assert!(!Formula::parse("=Spending.ALL").unwrap().names_plans());
    let source = Source::read_with_plans(&sample.book, november(), today()).unwrap();
    let result = formula
        .evaluate(&source, november(), &accounts(&[sample.everyday]), EUR)
        .unwrap();
    let terms: Vec<Term> = result.counted.iter().map(|counted| counted.of).collect();
    assert_eq!(
        terms,
        [
            Term::AllSpending,
            Term::Plans(kassensturz_report::PlanFigure::OutDue)
        ]
    );
    assert!(result.counted[1].path.is_empty());
    assert_eq!(result.counted[1].period, november());
}

#[test]
fn mistakes_with_the_plans_are_codes() {
    let sample = sample();
    let book = &sample.book;
    let code = |text: &str| -> String {
        let error = worked(book, text, november(), &Filter::default()).unwrap_err();
        let report = kassensturz_core::Report::report(&error);
        let mut shown = report.code.to_string();
        for (name, value) in &report.params {
            match value {
                kassensturz_core::ErrorParam::Number(number) => {
                    shown.push_str(&format!(" {name}={number}"));
                }
                kassensturz_core::ErrorParam::Keyword(word) => {
                    shown.push_str(&format!(" {name}={word}"));
                }
                kassensturz_core::ErrorParam::Text(text) => {
                    shown.push_str(&format!(" {name}={text}"));
                }
                other => shown.push_str(&format!(" {name}={other:?}")),
            }
        }
        shown
    };
    assert_eq!(
        code("=Plans.FOO"),
        "report.formula-syntax position=8 reason=unknown-plan-figure"
    );
    assert_eq!(
        code("=Plans.'OUT_DUE'"),
        "report.formula-syntax position=8 reason=unknown-plan-figure"
    );
    assert_eq!(
        code("=COUNT(Plans.OUT_DUE)"),
        "report.formula-arguments position=8 function=COUNT reason=names-only"
    );
    // A Source read without the plans never gives a figure without them.
    let formula = Formula::parse("=Plans.OUT_DUE").unwrap();
    let source = Source::read(book, november()).unwrap();
    assert!(matches!(
        formula.evaluate(&source, november(), &Filter::default(), EUR),
        Err(ReportError::PeriodNotRead)
    ));
}
