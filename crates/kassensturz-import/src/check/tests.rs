// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use kassensturz_core::{
    AccountKind, AccountType, Currency, CurrencyCode, Date, MatchMode, TextField, TextOperator,
};

use super::*;
use crate::book::{BookCondition, BookPart, BookRate, BookRule, Replacement, UnlistedCurrency};
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

fn usd(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, code("USD"))
}

fn book_account(key: &str, kind: AccountKind) -> BookAccount {
    BookAccount {
        key: key.to_owned(),
        name: format!("Name of {key}"),
        kind,
        account_type: None,
        currency: Currency::new(code("EUR"), 2).expect("valid decimal places"),
        parent: None,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    }
}

fn part(account: Option<&str>, amount: Money) -> BookPart {
    BookPart {
        account: account.map(str::to_owned),
        amount,
        account_amount: None,
        details: None,
        note: None,
        number: None,
        status: None,
        tags: Vec::new(),
    }
}

/// A checking account, a category inside another and one in dollars, and
/// a purchase booked to them.
fn book() -> ImportedBook {
    let mut groceries = book_account("E2", AccountKind::Expense);
    groceries.parent = Some("E1".to_owned());
    let mut travel = book_account("E3", AccountKind::Expense);
    travel.currency = Currency::new(code("USD"), 2).expect("valid decimal places");
    ImportedBook {
        accounts: vec![
            book_account("A1", AccountKind::Asset),
            book_account("E1", AccountKind::Expense),
            groceries,
            travel,
        ],
        transactions: vec![BookTransaction {
            date: date(),
            payee: Some("Corner Grocery".to_owned()),
            currency: Currency::new(code("EUR"), 2).expect("valid decimal places"),
            parts: vec![
                part(Some("A1"), eur(-3_000)),
                part(Some("E2"), eur(2_000)),
                BookPart {
                    account_amount: Some(usd(1_100)),
                    ..part(Some("E3"), eur(1_000))
                },
            ],
            note: None,
            entered_on: None,
        }],
        rates: vec![BookRate {
            date: date(),
            from: code("EUR"),
            to: code("USD"),
            rate: "1.1".parse().expect("a rate"),
        }],
        unlisted: Vec::new(),
        reports: Vec::new(),
        skipped: Vec::new(),
        institutions: Vec::new(),
        payees: Vec::new(),
        rules: Vec::new(),
        tags: Vec::new(),
    }
}

fn book_checked(book: ImportedBook, limits: &Limits) -> Result<(), ImportError> {
    check(&Imported::Book(Box::new(book)), limits)
}

const MALFORMED: Result<(), ImportError> = Err(ImportError::Malformed { line: None });

#[test]
fn a_consistent_book_passes() {
    assert_eq!(book_checked(book(), &Limits::default()), Ok(()));
    let empty = ImportedBook {
        accounts: Vec::new(),
        transactions: Vec::new(),
        rates: Vec::new(),
        unlisted: Vec::new(),
        reports: Vec::new(),
        skipped: Vec::new(),
        institutions: Vec::new(),
        payees: Vec::new(),
        rules: Vec::new(),
        tags: Vec::new(),
    };
    assert_eq!(book_checked(empty, &Limits::default()), Ok(()));
}

#[test]
fn every_account_is_known_once_and_fits_its_tree() {
    let mut unknown = book();
    unknown.transactions[0].parts[1].account = Some("E9".to_owned());
    assert_eq!(book_checked(unknown, &Limits::default()), MALFORMED);

    let mut twice = book();
    twice.accounts[1].key = "A1".to_owned();
    assert_eq!(book_checked(twice, &Limits::default()), MALFORMED);

    let mut no_parent = book();
    no_parent.accounts[2].parent = Some("E9".to_owned());
    assert_eq!(book_checked(no_parent, &Limits::default()), MALFORMED);

    // Only categories sit inside others, and only in their own kind.
    let mut account_inside = book();
    account_inside.accounts[0].parent = Some("E1".to_owned());
    assert_eq!(book_checked(account_inside, &Limits::default()), MALFORMED);
    let mut other_kind = book();
    other_kind.accounts[1].kind = AccountKind::Income;
    assert_eq!(book_checked(other_kind, &Limits::default()), MALFORMED);

    // A credit card is something the household owes.
    let mut card = book();
    card.accounts[0].account_type = Some(AccountType::CreditCard);
    assert_eq!(book_checked(card.clone(), &Limits::default()), MALFORMED);
    card.accounts[0].kind = AccountKind::Liability;
    assert_eq!(book_checked(card, &Limits::default()), Ok(()));
}

#[test]
fn every_amount_is_in_its_transactions_and_its_accounts_currency() {
    let mut foreign = book();
    foreign.transactions[0].parts[1].amount = usd(2_000);
    assert_eq!(
        book_checked(foreign, &Limits::default()),
        Err(ImportError::CurrencyMismatch {
            expected: code("EUR"),
            found: code("USD"),
        })
    );

    // The dollar category needs its amount in dollars, the euro account
    // none.
    let mut missing = book();
    missing.transactions[0].parts[2].account_amount = None;
    assert_eq!(
        book_checked(missing, &Limits::default()),
        Err(ImportError::CurrencyMismatch {
            expected: code("USD"),
            found: code("EUR"),
        })
    );
    let mut extra = book();
    extra.transactions[0].parts[0].account_amount = Some(usd(-3_300));
    assert_eq!(
        book_checked(extra, &Limits::default()),
        Err(ImportError::CurrencyMismatch {
            expected: code("EUR"),
            found: code("USD"),
        })
    );

    // A rate is above zero, so both amounts point the same way.
    let mut opposite = book();
    opposite.transactions[0].parts[2].account_amount = Some(usd(-1_100));
    assert_eq!(book_checked(opposite, &Limits::default()), MALFORMED);

    // A part without a category has no account to count in, nor a bank.
    let mut uncategorized = book();
    uncategorized.transactions[0].parts[2].account = None;
    assert_eq!(book_checked(uncategorized, &Limits::default()), MALFORMED);
    let mut delivered = book();
    delivered.transactions[0].parts[1] = BookPart {
        details: Some(ImportedDetails::default()),
        ..part(None, eur(2_000))
    };
    assert_eq!(book_checked(delivered, &Limits::default()), MALFORMED);
}

#[test]
fn a_currency_keeps_its_decimal_places_throughout() {
    let mut differs = book();
    differs.transactions[0].currency = Currency::new(code("EUR"), 3).expect("valid");
    assert_eq!(
        book_checked(differs, &Limits::default()),
        Err(ImportError::DecimalPlacesDiffer {
            currency: code("EUR"),
            book: 2,
            file: 3,
        })
    );
}

#[test]
fn the_currencies_no_table_lists_are_listed_once_each() {
    let leu = Currency::new(code("ROL"), 2).expect("valid decimal places");
    let in_lei = || {
        let mut in_lei = book();
        in_lei.accounts[3].currency = leu;
        in_lei.transactions[0].parts[2].account_amount =
            Some(Money::from_minor_units(110_000, code("ROL")));
        in_lei
    };
    let unlisted = UnlistedCurrency {
        currency: leu,
        suggestion: Some(Replacement {
            successor: code("RON"),
            rate: "10000".parse().expect("a rate"),
            date: date(),
        }),
    };
    let mut listed = in_lei();
    listed.unlisted = vec![unlisted];
    assert_eq!(book_checked(listed.clone(), &Limits::default()), Ok(()));
    // Not listed, listed twice, or listed but unused.
    assert_eq!(book_checked(in_lei(), &Limits::default()), MALFORMED);
    let mut twice = listed.clone();
    twice.unlisted.push(unlisted);
    assert_eq!(book_checked(twice, &Limits::default()), MALFORMED);
    let mut unused = book();
    unused.unlisted = vec![unlisted];
    assert_eq!(book_checked(unused, &Limits::default()), MALFORMED);
    // In other decimal places than the amounts, or replaced by a currency
    // that isn't current.
    let mut places = listed.clone();
    places.unlisted[0].currency = Currency::new(code("ROL"), 0).expect("valid");
    assert_eq!(book_checked(places, &Limits::default()), MALFORMED);
    let mut withdrawn = listed;
    withdrawn.unlisted[0].suggestion = Some(Replacement {
        successor: code("DEM"),
        ..unlisted.suggestion.expect("given")
    });
    assert_eq!(book_checked(withdrawn, &Limits::default()), MALFORMED);
}

#[test]
fn a_rate_is_between_two_currencies() {
    let mut same = book();
    same.rates[0].to = code("EUR");
    assert_eq!(book_checked(same, &Limits::default()), MALFORMED);
}

#[test]
fn a_book_keeps_the_limits() {
    let one = Limits {
        max_entries: 1,
        ..Limits::default()
    };
    assert_eq!(book_checked(book(), &one), Ok(()));
    let mut two = book();
    two.transactions.push(two.transactions[0].clone());
    assert_eq!(
        book_checked(two, &one),
        Err(ImportError::TooManyEntries { max: 1 })
    );

    let limits = Limits {
        max_text_bytes: 22,
        ..Limits::default()
    };
    let too_long = Err(ImportError::TextTooLong {
        max_bytes: 22,
        line: None,
    });
    let long = || "x".repeat(23);
    assert_eq!(book_checked(book(), &limits), Ok(()));
    let mut key = book();
    key.accounts[3].key = long();
    assert_eq!(book_checked(key, &limits), too_long);
    let mut name = book();
    name.accounts[0].name = long();
    assert_eq!(book_checked(name, &limits), too_long);
    let mut payee = book();
    payee.transactions[0].payee = Some(long());
    assert_eq!(book_checked(payee, &limits), too_long);
    let mut details = book();
    details.transactions[0].parts[0].details = Some(ImportedDetails {
        remittance_information: Some(long()),
        ..ImportedDetails::default()
    });
    assert_eq!(book_checked(details, &limits), too_long);
}

fn book_rule() -> BookRule {
    BookRule {
        name: "Corner Grocery".to_owned(),
        match_mode: MatchMode::Any,
        conditions: vec![BookCondition {
            field: TextField::OtherPartyName,
            operator: TextOperator::Matches,
            value: "^Corner$".to_owned(),
        }],
        name_conditions: vec![BookCondition {
            field: TextField::Payee,
            operator: TextOperator::Equals,
            value: "Corner Grocery".to_owned(),
        }],
        case_sensitive: false,
        book_to: "E2".to_owned(),
        payee: "Corner Grocery".to_owned(),
    }
}

#[test]
fn a_rule_has_a_name_a_condition_and_an_account_the_book_has() {
    let with = |change: &dyn Fn(&mut BookRule)| {
        let mut rule = book_rule();
        change(&mut rule);
        let mut book = book();
        book.rules = vec![rule];
        book_checked(book, &Limits::default())
    };
    assert_eq!(with(&|_| {}), Ok(()));
    // A payee without a matching text still has its name to look at.
    assert_eq!(with(&|rule| rule.conditions.clear()), Ok(()));
    assert_eq!(with(&|rule| rule.name = " ".to_owned()), MALFORMED);
    assert_eq!(with(&|rule| rule.payee = String::new()), MALFORMED);
    assert_eq!(with(&|rule| rule.name_conditions.clear()), MALFORMED);
    assert_eq!(
        with(&|rule| rule.conditions[0].value = "  ".to_owned()),
        MALFORMED
    );
    assert_eq!(
        with(&|rule| rule.name_conditions[0].value = "  ".to_owned()),
        MALFORMED
    );
    assert_eq!(with(&|rule| rule.book_to = "E9".to_owned()), MALFORMED);
    let limits = Limits {
        max_text_bytes: 4,
        ..Limits::default()
    };
    let mut long = book();
    long.rules = vec![book_rule()];
    assert!(matches!(
        book_checked(long, &limits),
        Err(ImportError::TextTooLong { .. })
    ));
}
