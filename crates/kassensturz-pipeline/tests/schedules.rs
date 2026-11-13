// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the importers meet the planned payments (D-769, AGENTS.md §9.4):
//! the real CAMT.053 and MT940 importers read synthetic statements of one
//! checking account over two months, with the rent, the gym and the salary
//! planned and a Rule that books money coming in as salary, then a late
//! file. All data is made up (AGENTS.md §3.1 rule 2); the IBANs are the
//! examples in SWIFT's IBAN Registry (D-055).

use kassensturz_core::Currency;
use kassensturz_core::{
    Account, AccountId, AccountKind, Actions, AmountTest, BookTo, Condition, CurrencyCode, Date,
    Decided, MatchMode, Money, Rule, RuleId, ScheduleId, ScheduleUnit, TransactionId,
};
use kassensturz_import::{
    Camt053, Csv, CsvSettings, Imported, Importer, Kmy, Limits, Mt940, Ofx, Statement, import,
};
use kassensturz_pipeline::{
    Choices, ImportReport, fill_book_from, import_statements, join_recurring_moves,
    match_schedules, preview_import,
};
use kassensturz_store::{Book, NewSchedule};

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};

const CHECKING_IBAN: &str = "AT611904300234573201";
const LANDLORD_IBAN: &str = "DE89370400440532013000";
const EMPLOYER_IBAN: &str = "CH9300762011623852957";
const GYM_IBAN: &str = "GB29NWBK60161331926819";

fn eur(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, EUR)
}

fn date(year: i16, month: i8, day: i8) -> Date {
    Date::new(year, month, day).expect("a valid date")
}

/// One CAMT.053 entry: `amount` in euros, `CRDT` or `DBIT`, on `day`, with
/// the other party's name and IBAN.
fn camt_entry(
    reference: &str,
    amount: &str,
    direction: &str,
    day: &str,
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
         <Sts>BOOK</Sts><BookgDt><Dt>{day}</Dt></BookgDt>\
         <ValDt><Dt>{day}</Dt></ValDt><AcctSvcrRef>{reference}</AcctSvcrRef>\
         <NtryDtls><TxDtls><RltdPties><{party}><Nm>{name}</Nm></{party}>\
         <{account}><Id><IBAN>{iban}</IBAN></Id></{account}></RltdPties>\
         <RmtInf><Ustrd>Payment</Ustrd></RmtInf></TxDtls></NtryDtls></Ntry>"
    )
}

/// A CAMT.053 statement of the checking account, without balances the
/// book has to agree with.
fn camt(id: &str, entries: &[String]) -> Vec<Statement> {
    let file = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
         <Document xmlns=\"urn:iso:std:iso:20022:tech:xsd:camt.053.001.02\"><BkToCstmrStmt>\
         <GrpHdr><MsgId>KS-SYNTHETIC-795</MsgId><CreDtTm>2026-12-10T08:00:00</CreDtTm></GrpHdr>\
         <Stmt><Id>{id}</Id><Acct><Id><IBAN>{CHECKING_IBAN}</IBAN></Id><Ccy>EUR</Ccy></Acct>\
         {}</Stmt></BkToCstmrStmt></Document>",
        entries.concat()
    );
    statements(&Camt053, file.as_bytes())
}

fn statements(importer: &dyn Importer, file: &[u8]) -> Vec<Statement> {
    match import(importer, file, &Limits::default()).expect("read") {
        Imported::Statements(statements) => statements,
        _ => panic!("statements expected"),
    }
}

/// October from the CAMT.053 file: the rent on Thursday the 1st, the gym on
/// the 15th under its name in capitals, a shop, and the salary on the 28th.
fn october() -> Vec<Statement> {
    camt(
        "STMT-2026-10",
        &[
            camt_entry(
                "O-1",
                "750.00",
                "DBIT",
                "2026-10-01",
                "Example Landlord",
                LANDLORD_IBAN,
            ),
            camt_entry(
                "O-2",
                "29.90",
                "DBIT",
                "2026-10-15",
                "EXAMPLE  GYM",
                GYM_IBAN,
            ),
            camt_entry(
                "O-3",
                "53.20",
                "DBIT",
                "2026-10-16",
                "Example Market",
                GYM_IBAN,
            ),
            camt_entry(
                "O-4",
                "2500.00",
                "CRDT",
                "2026-10-28",
                "Example Employer",
                EMPLOYER_IBAN,
            ),
        ],
    )
}

/// November from the MT940 file: the rent due on Sunday the 1st, paid on
/// Monday the 2nd and 4 cents dearer, and the salary due on Saturday the
/// 28th, paid on Monday the 30th. The gym is not in it.
fn november() -> Vec<Statement> {
    let file = format!(
        ":20:KS795\r\n:25:{CHECKING_IBAN}\r\n:60F:C261031EUR2167,00\r\n\
         :61:2611021102D750,04NTRFNONREF\r\n\
         :86:116?00Dauerauftrag?20SVWZ+Rent?31{LANDLORD_IBAN}?32Example Landlord\r\n\
         :61:2611301130C2500,00NTRFNONREF\r\n\
         :86:153?00Lohn?20SVWZ+Salary?31{EMPLOYER_IBAN}?32Example Employer\r\n\
         :62F:C261130EUR3916,96\r\n-\r\n"
    );
    statements(&Mt940, file.as_bytes())
}

/// The gym's November entry, on Monday the 16th, in a file that comes late.
fn late_gym() -> Vec<Statement> {
    camt(
        "STMT-2026-11-LATE",
        &[camt_entry(
            "N-9",
            "29.90",
            "DBIT",
            "2026-11-16",
            "Example Gym",
            GYM_IBAN,
        )],
    )
}

struct Household {
    book: Book,
    checking: AccountId,
    rent: AccountId,
    sports: AccountId,
    salary: AccountId,
    plans: [ScheduleId; 3],
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

/// The book: the checking account, three categories, a Rule that books
/// money coming in as salary, and three plans: the rent by the landlord's
/// IBAN, give or take 10 cents; the gym by its name; the salary, with the
/// same category as the Rule's.
fn household() -> Household {
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    let checking = add_account(
        &mut book,
        "Checking account",
        AccountKind::Asset,
        Some(CHECKING_IBAN),
    );
    let rent = add_account(&mut book, "Rent", AccountKind::Expense, None);
    let sports = add_account(&mut book, "Sports", AccountKind::Expense, None);
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
    let plan =
        |name: &str, amount: i64, day: i8, category, iban: Option<&str>, party| NewSchedule {
            name: name.into(),
            account: checking,
            amount: eur(amount),
            tolerance: eur(10),
            category: Some(category),
            party_iban: iban.map(str::to_owned),
            party_name: party,
            unit: ScheduleUnit::Month,
            every: 1,
            anchor: date(2026, 10, day),
            track_from: date(2026, 10, day),
            ends_on: None,
        };
    let plans = [
        plan("Rent", -75_000, 1, rent, Some(LANDLORD_IBAN), None),
        plan("Gym", -2_990, 15, sports, None, Some("Example Gym".into())),
        plan("Pay", 250_000, 28, salary, Some(EMPLOYER_IBAN), None),
    ]
    .map(|new| book.add_schedule(&new).expect("planned"));
    Household {
        book,
        checking,
        rent,
        sports,
        salary,
        plans,
    }
}

/// Imports as every front end does: the import's action, the joins, then
/// the matches as their own.
fn import_one(
    book: &mut Book,
    account: AccountId,
    statements: &[Statement],
    today: Date,
) -> ImportReport {
    let mut report =
        import_statements(book, account, statements, &Choices::default(), None).expect("imported");
    join_recurring_moves(book, &mut report).expect("joined");
    match_schedules(book, &mut report, today).expect("matched");
    report
}

/// The matches of `report`: due date, transaction and whether the plan's
/// category was given.
fn matched(report: &ImportReport) -> Vec<(Date, TransactionId, bool)> {
    report
        .schedules
        .matched
        .iter()
        .map(|done| {
            (
                done.occurrence.due.counted,
                done.transaction,
                done.category_set,
            )
        })
        .collect()
}

/// The category of `transaction`'s other part.
fn category(book: &Book, transaction: TransactionId, account: AccountId) -> Option<AccountId> {
    book.transaction(transaction)
        .expect("transaction")
        .splits()
        .iter()
        .find(|split| split.account != Some(account))
        .and_then(|split| split.account)
}

#[test]
fn imported_entries_are_matched_to_their_plans_month_by_month() {
    let Household {
        mut book,
        checking,
        rent,
        sports,
        salary,
        plans: [rent_plan, gym_plan, pay_plan],
    } = household();

    // The preview shows the matches and changes nothing.
    let history = book.history().expect("history").len();
    let preview = preview_import(
        &mut book,
        checking,
        &october(),
        &Choices::default(),
        date(2026, 10, 31),
    )
    .expect("previewed");
    assert_eq!(preview.schedules.matched.len(), 3);
    assert_eq!(book.history().expect("history").len(), history);
    assert!(book.decided_occurrences().expect("decided").is_empty());

    // October: three of four entries are planned ones. The salary keeps
    // the Rule's category; the others get their plan's.
    let report = import_one(&mut book, checking, &october(), date(2026, 10, 31));
    let imported = report.imported();
    assert_eq!(imported.len(), 4);
    let found = matched(&report);
    assert_eq!(
        found
            .iter()
            .map(|(due, _, set)| (*due, *set))
            .collect::<Vec<_>>(),
        [
            (date(2026, 10, 1), true),
            (date(2026, 10, 15), true),
            (date(2026, 10, 28), false)
        ]
    );
    assert!(report.schedules.needs_you.is_empty());
    assert_eq!(category(&book, found[0].1, checking), Some(rent));
    assert_eq!(category(&book, found[1].1, checking), Some(sports));
    assert_eq!(category(&book, found[2].1, checking), Some(salary));
    let shop = imported
        .iter()
        .find(|id| !found.iter().any(|(_, matched, _)| matched == *id))
        .expect("the shop");
    assert_eq!(category(&book, *shop, checking), None);

    // One Undo takes the matches and the categories they gave back and
    // keeps the entries; the next takes the import back.
    let matched_state = book.decided_occurrences().expect("decided");
    book.undo().expect("undone");
    assert!(book.decided_occurrences().expect("decided").is_empty());
    assert_eq!(category(&book, found[0].1, checking), None);
    assert_eq!(category(&book, found[2].1, checking), Some(salary));
    assert_eq!(book.transactions_of(checking).expect("entries").len(), 4);
    book.undo().expect("undone");
    assert!(book.transactions_of(checking).expect("entries").is_empty());
    book.redo().expect("redone");
    book.redo().expect("redone");
    assert_eq!(book.decided_occurrences().expect("decided"), matched_state);
    assert_eq!(category(&book, found[0].1, checking), Some(rent));

    // November, from MT940: the rent and salary come on the Monday after
    // their weekend day and are matched to them; the gym is overdue.
    let report = import_one(&mut book, checking, &november(), date(2026, 11, 30));
    let found = matched(&report);
    assert_eq!(
        found
            .iter()
            .map(|(due, _, set)| (*due, *set))
            .collect::<Vec<_>>(),
        [(date(2026, 11, 1), true), (date(2026, 11, 28), false)]
    );
    let rent_paid = book.transaction(found[0].1).expect("rent");
    assert_eq!(rent_paid.booking_date(), date(2026, 11, 2));
    // The match never changes the amount the bank booked.
    assert!(
        rent_paid
            .splits()
            .iter()
            .any(|split| split.account == Some(checking) && split.amount == eur(-75_004))
    );
    let overdue = book
        .forecast(&[checking], EUR, date(2026, 11, 30), 0)
        .expect("forecast")
        .forecast
        .overdue;
    assert_eq!(overdue.len(), 1);
    assert_eq!(overdue[0].schedule, gym_plan);

    // The late file: the gym's November entry is matched to the 15th.
    let report = import_one(&mut book, checking, &late_gym(), date(2026, 12, 10));
    let found = matched(&report);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, date(2026, 11, 15));
    let decided = book.decided_occurrences().expect("decided");
    assert_eq!(decided.len(), 6);
    for plan in [rent_plan, gym_plan, pay_plan] {
        assert_eq!(
            decided.keys().filter(|(id, _)| *id == plan).count(),
            2,
            "{plan:?}"
        );
    }
    assert!(
        decided
            .values()
            .all(|decision| matches!(decision, Decided::Matched(_)))
    );
    // Importing the same file again adds nothing and matches nothing.
    let again = import_one(&mut book, checking, &late_gym(), date(2026, 12, 10));
    assert!(again.imported().is_empty());
    assert!(again.schedules.matched.is_empty());
}

/// The golden files of the importers (`kassensturz-import/tests/golden`).
fn golden(format: &str) -> Vec<std::path::PathBuf> {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../kassensturz-import/tests/golden")
        .join(format);
    let mut files: Vec<_> = std::fs::read_dir(&folder)
        .unwrap_or_else(|error| panic!("{}: {error}", folder.display()))
        .map(|entry| entry.expect("an entry").path())
        .filter(|path| {
            !path
                .extension()
                .is_some_and(|extension| extension == "expected" || extension == "settings")
        })
        .collect();
    files.sort();
    files
}

/// Checks "Plan this again…" on every entry `book` holds in `account`:
/// where a plan is offered, the book takes it, and the entry it was made
/// from is its first payment, or waits for the user with another entry
/// that fits as well. Returns how many plans were made.
fn plan_each_entry(book: &mut Book, account: AccountId, what: &str) -> usize {
    let mut made = 0;
    // The first 40 entries of each account keep the test quick; KMyMoney's
    // sample has thousands.
    for entry in book
        .transactions_of(account)
        .expect("entries")
        .into_iter()
        .take(40)
    {
        let Some(new) = book
            .plan_from_entry(entry.id(), account, "Planned payment")
            .expect("read")
        else {
            continue;
        };
        let plan = book
            .add_schedule(&new)
            .unwrap_or_else(|error| panic!("{what}: {error:?} for {new:?}"));
        // Its first payment is matched: to this entry, or to another of the
        // same amount that is closer to the due date; or it waits for the
        // user, as more than one fits as well, or as the entry looks like
        // a side of a Transfer the user hasn't decided on (3.4 point 4).
        let found = book.match_schedules(new.anchor, None).expect("matched");
        let first = found
            .matched
            .iter()
            .find(|done| done.occurrence.schedule == plan)
            .map(|done| done.occurrence.due.counted);
        let waits = found
            .needs_you
            .iter()
            .any(|needs| needs.transactions.contains(&entry.id()));
        let a_transfer_side = book
            .transfer_suggestions()
            .expect("read")
            .iter()
            .any(|pair| {
                [pair.outgoing.transaction, pair.incoming.transaction].contains(&entry.id())
            });
        assert!(
            first == Some(new.anchor) || waits || (a_transfer_side && first.is_none()),
            "{what}: {found:?} for {new:?}"
        );
        book.remove_schedule(plan).expect("removed");
        made += 1;
    }
    made
}

/// The statements of every golden file `importer` reads, each imported
/// into an account of its own in a new book, and planned from. Returns how
/// many plans were made.
fn plan_from_statements(importer: &dyn Importer, files: &[std::path::PathBuf]) -> usize {
    let mut made = 0;
    for file in files {
        let bytes = std::fs::read(file).expect("read");
        let Ok(Imported::Statements(found)) = import(importer, &bytes, &Limits::default()) else {
            continue;
        };
        for statement in found {
            let mut book = Book::create_in_memory().expect("book created");
            let currency = statement.currency.code();
            if book.use_currency(currency).is_err() {
                continue;
            }
            // A withdrawn currency's statement goes into an account in its
            // successor, converted; that is the importers' business, not
            // the plans'.
            let account = AccountId::generate();
            let added = book.add_account(&Account {
                id: account,
                name: "Account".into(),
                kind: AccountKind::Asset,
                currency,
                account_type: None,
                parent: None,
                closed: false,
                number: None,
                notes: None,
                institution: None,
            });
            if added.is_err() {
                continue;
            }
            if import_statements(&mut book, account, &[statement], &Choices::default(), None)
                .is_err()
            {
                continue;
            }
            made += plan_each_entry(&mut book, account, &file.display().to_string());
        }
    }
    made
}

#[test]
fn a_plan_is_made_from_the_entries_of_every_importer() {
    // AGENTS.md §9.4: the importers' own output, from their golden files.
    assert!(plan_from_statements(&Camt053, &golden("camt053")) > 0);
    assert!(plan_from_statements(&Mt940, &golden("mt940")) > 0);
    assert!(plan_from_statements(&Ofx, &golden("ofx")) > 0);
    let euros = Currency::new(EUR, 2).expect("a currency");
    let mut from_csv = 0;
    for file in golden("csv") {
        let mut settings_path = file.clone().into_os_string();
        settings_path.push(".settings");
        let Ok(text) = std::fs::read_to_string(&settings_path) else {
            continue;
        };
        let Ok(settings) = CsvSettings::from_text(&text) else {
            continue;
        };
        let Ok(importer) = Csv::new(settings, euros) else {
            continue;
        };
        from_csv += plan_from_statements(&importer, std::slice::from_ref(&file));
    }
    assert!(from_csv > 0);
    // KMyMoney: the whole book comes in; each entry of each open account.
    let mut made = 0;
    for file in golden("kmy") {
        let bytes = std::fs::read(&file).expect("read");
        let Ok(Imported::Book(imported)) = import(&Kmy, &bytes, &Limits::default()) else {
            continue;
        };
        let mut book = Book::create_in_memory().expect("book created");
        if fill_book_from(&mut book, &imported, &std::collections::HashMap::new()).is_err() {
            continue;
        }
        for account in book.accounts(false).expect("accounts") {
            made += plan_each_entry(&mut book, account.id, &file.display().to_string());
        }
    }
    assert!(made > 0, "KMyMoney: no entry was planned from");
}

#[test]
fn a_plan_made_from_any_importers_entry_matches_that_entry() {
    // "Plan this again…" on each importer's own entries: the plan made from
    // an entry is valid and that entry is its first payment.
    let files: [(&str, Vec<Statement>); 2] = [("CAMT.053", october()), ("MT940", november())];
    for (format, statements) in files {
        let Household {
            mut book, checking, ..
        } = household();
        for plan in book.schedules().expect("plans") {
            book.remove_schedule(plan.id).expect("removed");
        }
        let report = import_one(&mut book, checking, &statements, date(2026, 12, 10));
        for id in report.imported() {
            let entry = book.transaction(id).expect("entry");
            let new = NewSchedule::from_entry(&entry, checking, "Planned payment")
                .unwrap_or_else(|| panic!("{format}: no plan from {entry:?}"));
            let plan = book.add_schedule(&new).expect("planned");
            let found = book
                .match_schedules(date(2026, 12, 10), None)
                .expect("matched");
            assert_eq!(found.matched.len(), 1, "{format}: {found:?}");
            assert_eq!(found.matched[0].transaction, id, "{format}");
            assert_eq!(found.matched[0].occurrence.schedule, plan, "{format}");
            book.remove_schedule(plan).expect("removed");
        }
    }
}
