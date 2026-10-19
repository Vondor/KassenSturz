// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use kassensturz_core::{Currency, CurrencyCode, Date};

use super::*;
use crate::statement::{Balance, Entry, StatementAccount};

fn code(text: &str) -> CurrencyCode {
    CurrencyCode::new(text).expect("a valid code")
}

fn eur(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, code("EUR"))
}

fn date() -> Date {
    Date::new(2026, 10, 5).expect("a valid date")
}

fn statement(entries: Vec<Entry>) -> Statement {
    Statement {
        account: StatementAccount {
            iban: Some("DE89370400440532013000".to_owned()),
            number: None,
        },
        currency: Currency::new(code("EUR"), 2).expect("valid decimal places"),
        opening_balance: Some(Balance {
            amount: eur(10_000),
            date: date(),
        }),
        closing_balance: Some(Balance {
            amount: eur(7_500),
            date: date(),
        }),
        entries,
    }
}

fn checked(statements: Vec<Statement>, limits: &Limits) -> Result<(), ImportError> {
    check(&Imported::Statements(statements), limits)
}

#[test]
fn a_consistent_statement_passes() {
    let entry = Entry::booked(date(), eur(-2_500));
    assert_eq!(
        checked(vec![statement(vec![entry])], &Limits::default()),
        Ok(())
    );
    assert_eq!(checked(Vec::new(), &Limits::default()), Ok(()));
}

#[test]
fn every_amount_is_in_the_statements_currency() {
    let mismatch = Err(ImportError::CurrencyMismatch {
        expected: code("EUR"),
        found: code("USD"),
    });
    let usd = Money::from_minor_units(100, code("USD"));
    let entry = Entry::booked(date(), usd);
    assert_eq!(
        checked(vec![statement(vec![entry])], &Limits::default()),
        mismatch
    );

    let mut opening = statement(Vec::new());
    opening.opening_balance = Some(Balance {
        amount: usd,
        date: date(),
    });
    assert_eq!(checked(vec![opening], &Limits::default()), mismatch);

    let mut closing = statement(Vec::new());
    closing.closing_balance = Some(Balance {
        amount: usd,
        date: date(),
    });
    assert_eq!(checked(vec![closing], &Limits::default()), mismatch);
}

#[test]
fn entries_are_counted_across_statements() {
    let limits = Limits {
        max_entries: 2,
        ..Limits::default()
    };
    let one = || statement(vec![Entry::booked(date(), eur(1))]);
    assert_eq!(checked(vec![one(), one()], &limits), Ok(()));
    assert_eq!(
        checked(vec![one(), one(), one()], &limits),
        Err(ImportError::TooManyEntries { max: 2 })
    );
}

#[test]
fn every_text_keeps_the_limit() {
    let limits = Limits {
        max_text_bytes: 22,
        ..Limits::default()
    };
    let too_long = Err(ImportError::TextTooLong {
        max_bytes: 22,
        line: None,
    });
    let long = || Some("x".repeat(23));
    // The example IBAN has exactly 22 characters.
    assert_eq!(checked(vec![statement(Vec::new())], &limits), Ok(()));

    let mut account = statement(Vec::new());
    account.account.number = long();
    assert_eq!(checked(vec![account], &limits), too_long);

    let fields: [fn(&mut Entry) -> &mut Option<String>; 4] = [
        |entry| &mut entry.bank_id,
        |entry| &mut entry.other_party_name,
        |entry| &mut entry.other_party_iban,
        |entry| &mut entry.remittance_information,
    ];
    for field in fields {
        let mut entry = Entry::booked(date(), eur(1));
        *field(&mut entry) = long();
        assert_eq!(checked(vec![statement(vec![entry])], &limits), too_long);
    }
}
