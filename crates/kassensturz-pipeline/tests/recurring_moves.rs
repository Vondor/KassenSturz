// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the importers meet the remembered monthly moves (D-749, AGENTS.md
//! §9.4): the real CAMT.053 and MT940 importers read synthetic statements of
//! four accounts with three standing orders and a Rule that books money
//! coming in as salary, and the pipeline imports them in either order. All
//! data is made up (AGENTS.md §3.1 rule 2); the IBANs are the examples in
//! SWIFT's IBAN Registry (D-055).

use kassensturz_core::{
    Account, AccountId, AccountKind, Actions, AmountTest, BookTo, Condition, CurrencyCode,
    MatchMode, Money, Rule, RuleId,
};
use kassensturz_import::{Camt053, Imported, Importer, Limits, Mt940, Statement, import};
use kassensturz_pipeline::{
    Choices, ImportReport, import_statements, join_recurring_moves, preview_import,
};
use kassensturz_store::{Action, Book, NewRecurringMove, StoreError};

/// The day a preview matches planned payments for; these books have none.
const PREVIEW_DAY: kassensturz_core::Date = kassensturz_core::Date::constant(2026, 11, 13);

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};

const CHECKING_IBAN: &str = "AT611904300234573201";
const SAVINGS_IBAN: &str = "DE89370400440532013000";
const HOLIDAY_IBAN: &str = "GB29NWBK60161331926819";
const CHILD_IBAN: &str = "NL91ABNA0417164300";
const EMPLOYER_IBAN: &str = "CH9300762011623852957";

fn eur(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, EUR)
}

/// One CAMT.053 entry: `amount` in euros, `CRDT` or `DBIT`, on `day` of
/// October 2026, with the other party's name and IBAN.
fn camt_entry(
    reference: &str,
    amount: &str,
    direction: &str,
    day: u8,
    name: &str,
    iban: &str,
) -> String {
    let (party, account) = if direction == "DBIT" {
        ("Cdtr", "CdtrAcct")
    } else {
        ("Dbtr", "DbtrAcct")
    };
    format!(
        "<Ntry><Amt Ccy=\"EUR\">{amount}</Amt><CdtDbtInd>{direction}</CdtDbtInd>\
         <Sts>BOOK</Sts><BookgDt><Dt>2026-10-{day:02}</Dt></BookgDt>\
         <ValDt><Dt>2026-10-{day:02}</Dt></ValDt><AcctSvcrRef>{reference}</AcctSvcrRef>\
         <NtryDtls><TxDtls><RltdPties><{party}><Nm>{name}</Nm></{party}>\
         <{account}><Id><IBAN>{iban}</IBAN></Id></{account}></RltdPties>\
         <RmtInf><Ustrd>Standing order</Ustrd></RmtInf></TxDtls></NtryDtls></Ntry>"
    )
}

/// A CAMT.053 statement of the account `iban` for October 2026, from zero
/// to `closing` euros.
fn camt(iban: &str, closing: &str, entries: &[String]) -> Vec<Statement> {
    let file = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
         <Document xmlns=\"urn:iso:std:iso:20022:tech:xsd:camt.053.001.02\"><BkToCstmrStmt>\
         <GrpHdr><MsgId>KS-SYNTHETIC-784</MsgId><CreDtTm>2026-10-31T23:30:00</CreDtTm></GrpHdr>\
         <Stmt><Id>STMT-{iban}</Id><Acct><Id><IBAN>{iban}</IBAN></Id><Ccy>EUR</Ccy></Acct>\
         <Bal><Tp><CdOrPrtry><Cd>OPBD</Cd></CdOrPrtry></Tp><Amt Ccy=\"EUR\">0.00</Amt>\
         <CdtDbtInd>CRDT</CdtDbtInd><Dt><Dt>2026-09-30</Dt></Dt></Bal>\
         <Bal><Tp><CdOrPrtry><Cd>CLBD</Cd></CdOrPrtry></Tp><Amt Ccy=\"EUR\">{closing}</Amt>\
         <CdtDbtInd>CRDT</CdtDbtInd><Dt><Dt>2026-10-31</Dt></Dt></Bal>\
         {}</Stmt></BkToCstmrStmt></Document>",
        entries.concat()
    );
    statements(&Camt053, file.as_bytes())
}

/// An MT940 statement of the account `iban`: one credit of 100,00 on `day`
/// of October 2026 from the checking account, from zero.
fn mt940(iban: &str, day: u8) -> Vec<Statement> {
    let file = format!(
        ":20:KS784\r\n:25:{iban}\r\n:60F:C260930EUR0,00\r\n\
         :61:2610{day:02}10{day:02}C100,00NTRFNONREF\r\n\
         :86:166?00Dauerauftrag?20SVWZ+Standing order?31{CHECKING_IBAN}?32Ada Example\r\n\
         :62F:C2610{day:02}EUR100,00\r\n-\r\n"
    );
    statements(&Mt940, file.as_bytes())
}

fn statements(importer: &dyn Importer, file: &[u8]) -> Vec<Statement> {
    match import(importer, file, &Limits::default()).expect("read") {
        Imported::Statements(statements) => statements,
        _ => panic!("statements expected"),
    }
}

struct Household {
    book: Book,
    checking: AccountId,
    savings: AccountId,
    holiday: AccountId,
    child: AccountId,
    salary: AccountId,
}

fn add_account(book: &mut Book, name: &str, kind: AccountKind, number: Option<&str>) -> AccountId {
    let id = AccountId::generate();
    book.add_account(&Account {
        id,
        name: name.into(),
        kind,
        currency: EUR,
        account_type: None,
        parent: None,
        closed: false,
        number: number.map(str::to_owned),
        notes: None,
        institution: None,
    })
    .expect("account added");
    id
}

/// The book: four accounts, a "Money in" Rule that books everything coming
/// in as salary, and the three standing orders remembered as the user
/// ticked the box for each.
fn household() -> Household {
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    let checking = add_account(
        &mut book,
        "Checking account",
        AccountKind::Asset,
        Some(CHECKING_IBAN),
    );
    let savings = add_account(&mut book, "Savings", AccountKind::Asset, Some(SAVINGS_IBAN));
    let holiday = add_account(
        &mut book,
        "Holiday fund",
        AccountKind::Asset,
        Some(HOLIDAY_IBAN),
    );
    let child = add_account(
        &mut book,
        "Child's account",
        AccountKind::Asset,
        Some(CHILD_IBAN),
    );
    let salary = add_account(&mut book, "Salary", AccountKind::Income, None);
    book.add_rule(
        &Rule {
            id: RuleId::generate(),
            name: "Money in".into(),
            match_mode: MatchMode::All,
            conditions: vec![Condition::Amount(AmountTest::MoneyIn)],
            actions: Actions {
                book_to: Some(BookTo::Category(salary)),
                payee: None,
            },
        },
        None,
    )
    .expect("Rule added");
    for (to, iban) in [
        (savings, SAVINGS_IBAN),
        (holiday, HOLIDAY_IBAN),
        (child, CHILD_IBAN),
    ] {
        book.add_recurring_move(&NewRecurringMove {
            from_account: checking,
            to_account: to,
            amount: eur(10_000),
            day_of_month: 1,
            from_party_iban: Some(iban.into()),
            from_party_name: Some("Ada Example".into()),
            created_on: kassensturz_core::Date::constant(2026, 9, 1),
        })
        .expect("remembered");
    }
    Household {
        book,
        checking,
        savings,
        holiday,
        child,
        salary,
    }
}

/// The four accounts' statements, each with the account it goes into.
fn files(household: &Household) -> Vec<(AccountId, Vec<Statement>)> {
    let checking = camt(
        CHECKING_IBAN,
        "2200.00",
        &[
            camt_entry(
                "C-1",
                "2500.00",
                "CRDT",
                1,
                "Example Employer",
                EMPLOYER_IBAN,
            ),
            camt_entry("C-2", "100.00", "DBIT", 1, "Ada Example", SAVINGS_IBAN),
            camt_entry("C-3", "100.00", "DBIT", 1, "Ada Example", HOLIDAY_IBAN),
            camt_entry("C-4", "100.00", "DBIT", 1, "Ada Example", CHILD_IBAN),
        ],
    );
    let child = camt(
        CHILD_IBAN,
        "100.00",
        &[camt_entry(
            "K-1",
            "100.00",
            "CRDT",
            2,
            "Ada Example",
            CHECKING_IBAN,
        )],
    );
    vec![
        (household.checking, checking),
        (household.savings, mt940(SAVINGS_IBAN, 1)),
        (household.holiday, mt940(HOLIDAY_IBAN, 2)),
        (household.child, child),
    ]
}

/// Imports as every front end does: the import's action, then the joins as
/// their own.
fn import_one(book: &mut Book, account: AccountId, statements: &[Statement]) -> ImportReport {
    let mut report =
        import_statements(book, account, statements, &Choices::default(), None).expect("imported");
    join_recurring_moves(book, &mut report).expect("joined");
    report
}

/// How much `account` holds, from its parts.
fn balance(book: &Book, account: AccountId) -> Money {
    book.balance_on(account, kassensturz_core::Date::constant(2026, 10, 31))
        .expect("a balance")
}

/// What the categories got: salary counted as income, per category.
fn in_salary(book: &Book, salary: AccountId) -> Money {
    balance(book, salary)
}

fn check_the_end(household: &Household, reports: &[ImportReport]) {
    let book = &household.book;
    // The three pairs joined, one per move, none twice.
    let joined: usize = reports.iter().map(|report| report.joins.joined.len()).sum();
    assert_eq!(joined, 3);
    assert!(
        reports
            .iter()
            .all(|report| report.joins.needs_you.is_empty())
    );
    // Nothing is left to suggest, and no standing order counts as income:
    // only the salary is in the salary category.
    assert!(book.transfer_suggestions().expect("suggestions").is_empty());
    assert_eq!(in_salary(book, household.salary), eur(-250_000));
    // Every balance is the statement's (D-029).
    assert_eq!(balance(book, household.checking), eur(220_000));
    for account in [household.savings, household.holiday, household.child] {
        assert_eq!(balance(book, account), eur(10_000));
    }
    for report in reports {
        assert!(
            report.balances.iter().all(|check| check.matches()),
            "{:?}",
            report.balances
        );
    }
}

#[test]
fn the_standing_orders_join_whichever_statement_comes_first() {
    for reversed in [false, true] {
        let mut household = household();
        let mut files = files(&household);
        if reversed {
            files.reverse();
        }
        let reports: Vec<ImportReport> = files
            .iter()
            .map(|(account, statements)| import_one(&mut household.book, *account, statements))
            .collect();
        check_the_end(&household, &reports);
        // An entry that waited was left alone by the Rule.
        let waited = reports
            .iter()
            .flat_map(|report| &report.rules.skipped)
            .filter(|skip| matches!(skip.reason, StoreError::FitsMonthlyMove(_)))
            .count();
        assert_eq!(waited, 3, "reversed: {reversed}");
    }
}

#[test]
fn the_preview_shows_the_joins_and_undo_takes_back_only_them() {
    let mut household = household();
    let files = files(&household);
    // The receiving accounts first.
    for (account, statements) in &files[1..] {
        import_one(&mut household.book, *account, statements);
    }
    let (checking, statements) = &files[0];
    let preview = preview_import(
        &mut household.book,
        *checking,
        statements,
        &Choices::default(),
        PREVIEW_DAY,
    )
    .expect("previewed");
    assert_eq!(preview.joins.joined.len(), 3);
    assert!(preview.transfers.is_empty());
    // The preview changed nothing.
    assert_eq!(
        household
            .book
            .transfer_suggestions()
            .expect("suggestions")
            .len(),
        0
    );
    assert_eq!(balance(&household.book, *checking), eur(0));

    let report = import_one(&mut household.book, *checking, statements);
    assert_eq!(report.joins.joined.len(), 3);
    let book = &mut household.book;
    assert_eq!(
        book.change_to_undo()
            .expect("read")
            .map(|change| change.action),
        Some(Action::JoinRecurringMoves)
    );
    book.undo().expect("joins undone");
    // The entries stay: the three pairs are suggestions again, the salary
    // is sorted, the standing orders wait.
    assert_eq!(book.transfer_suggestions().expect("suggestions").len(), 3);
    assert_eq!(balance(book, *checking), eur(220_000));
    assert_eq!(in_salary(book, household.salary), eur(-250_000));
    assert_eq!(
        book.change_to_undo()
            .expect("read")
            .map(|change| change.action),
        Some(Action::ImportStatement)
    );
}
