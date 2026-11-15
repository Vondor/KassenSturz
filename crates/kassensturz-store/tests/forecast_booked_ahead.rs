// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! B-331: the forecast ("Left this month") must never be too high
//! (docs/SCHEDULES.md 5.2: `balance_now` leaves future-dated entries out,
//! and "the figure may be too low, never too high"). A planned payment that
//! is matched to an entry dated after today is in neither `balance_now` nor
//! `out_due`, so it vanished from the figure; now its entry counts as still
//! to come.

use kassensturz_core::{
    Account, AccountId, AccountKind, AccountType, CurrencyCode, Date, Money, ScheduleUnit, Split,
    Transaction, TransactionId,
};
use kassensturz_store::{Book, NewSchedule};

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR"),
};

fn day(y: i16, m: i8, d: i8) -> Date {
    Date::new(y, m, d).unwrap()
}

fn eur(minor: i64) -> Money {
    Money::from_minor_units(minor, EUR)
}

fn account(book: &mut Book, name: &str, kind: AccountKind) -> AccountId {
    let id = AccountId::generate();
    book.add_account(&Account {
        id,
        name: name.into(),
        kind,
        currency: EUR,
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

#[test]
fn a_payment_booked_ahead_of_today_still_counts_in_what_is_left() {
    let mut book = Book::create_in_memory().unwrap();
    book.use_currency(EUR).unwrap();
    let checking = account(&mut book, "Checking", AccountKind::Asset);
    let salary = account(&mut book, "Salary", AccountKind::Income);
    let rent = account(&mut book, "Rent", AccountKind::Expense);
    // 1,000.00 arrived on 2 November.
    book.add_transaction(
        &Transaction::new(
            TransactionId::generate(),
            day(2026, 11, 2),
            None,
            vec![
                Split::new(checking, eur(100_000)),
                Split::new(salary, eur(-100_000)),
            ],
        )
        .unwrap(),
    )
    .unwrap();
    // Rent of 800.00, due on Friday 20 November.
    let plan = book
        .add_schedule(&NewSchedule {
            name: "Rent".into(),
            account: checking,
            amount: eur(-80_000),
            tolerance: eur(0),
            category: Some(rent),
            party_iban: None,
            party_name: None,
            unit: ScheduleUnit::Month,
            every: 1,
            anchor: day(2026, 11, 20),
            track_from: day(2026, 11, 20),
            ends_on: None,
        })
        .unwrap();
    let today = day(2026, 11, 13);
    let before = book.forecast(&[checking], EUR, today, 0).unwrap().forecast;
    assert_eq!(before.balance_now, eur(100_000));
    assert_eq!(before.left, eur(20_000), "{before:#?}");

    // The user books the rent with "Book it" on its due date, a week ahead.
    book.book_schedule_occurrence(plan, day(2026, 11, 20), day(2026, 11, 20), eur(-80_000))
        .unwrap();
    let after = book.forecast(&[checking], EUR, today, 0).unwrap().forecast;
    // Nothing has left the account yet, and the rent still will: what is
    // left this month is still 200.00, not 1,000.00.
    assert_eq!(after.balance_now, eur(100_000), "{after:#?}");
    assert_eq!(after.left, eur(20_000), "{after:#?}");
}
