// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The import pipeline on books in memory, with statements built in code,
//! so it is tested without any file format. All data is made up
//! (AGENTS.md §3.1 rule 2); IBANs are the examples in SWIFT's IBAN Registry
//! (D-055).

use std::collections::HashSet;
use std::path::Path;

use kassensturz_core::{
    Account, AccountId, AccountKind, Actions, BookTo, Condition, Currency, CurrencyCode, Date,
    ExchangeRate, ImportedDetails, MatchMode, Money, RateSource, Report, Rule, RuleId, Split,
    TextField, TextOperator, Transaction, TransactionId,
};
use kassensturz_import::{Balance, Entry, EntryStatus, ImportError, Statement, StatementAccount};
use kassensturz_pipeline::{
    BalanceCheck, Choices, Duplicate, EntryOutcome, EntryRef, ImportReport, PipelineError, Summary,
    import_statements, preview_import,
};
use kassensturz_store::{Action, Book, StoreError};

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};
const DEM: CurrencyCode = match CurrencyCode::new("DEM") {
    Ok(code) => code,
    Err(_) => panic!("DEM is a valid code"),
};

/// The IBAN for Germany in SWIFT's IBAN Registry.
const IBAN: &str = "DE89370400440532013000";

fn eur(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, EUR)
}

fn dem(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, DEM)
}

/// A day in October 2026: the 5th is a Monday, the 9th a Friday.
fn october(day: i8) -> Date {
    Date::new(2026, 10, day).expect("a valid date")
}

struct Sample {
    book: Book,
    checking: AccountId,
    savings: AccountId,
    groceries: AccountId,
}

fn add_account(book: &mut Book, name: &str, kind: AccountKind) -> AccountId {
    let id = AccountId::generate();
    book.add_account(&Account {
        id,
        name: name.into(),
        kind,
        currency: EUR,
        account_type: None,
        parent: None,
        closed: false,
    })
    .expect("account added");
    id
}

fn sample() -> Sample {
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    let checking = add_account(&mut book, "Checking account", AccountKind::Asset);
    let savings = add_account(&mut book, "Savings account", AccountKind::Asset);
    let groceries = add_account(&mut book, "Groceries", AccountKind::Expense);
    Sample {
        book,
        checking,
        savings,
        groceries,
    }
}

/// A booked entry from `other_party`, with a bank ID if `bank_id` isn't
/// empty.
fn entry(day: i8, minor_units: i64, bank_id: &str, other_party: &str) -> Entry {
    Entry {
        bank_id: (!bank_id.is_empty()).then(|| bank_id.to_owned()),
        value_date: Some(october(day)),
        other_party_name: Some(other_party.into()),
        other_party_iban: Some(IBAN.into()),
        remittance_information: Some(format!("{other_party} card payment")),
        ..Entry::booked(october(day), eur(minor_units))
    }
}

fn statement(entries: Vec<Entry>) -> Statement {
    Statement {
        account: StatementAccount {
            iban: Some(IBAN.into()),
            number: None,
        },
        currency: Currency::new(EUR, 2).expect("valid decimal places"),
        opening_balance: None,
        closing_balance: None,
        entries,
    }
}

fn at(statement: usize, entry: usize) -> EntryRef {
    EntryRef { statement, entry }
}

fn import(book: &mut Book, account: AccountId, statements: &[Statement]) -> ImportReport {
    import_statements(book, account, statements, &Choices::default(), None).expect("imported")
}

/// A transaction typed in by hand: `minor_units` into or out of `account`,
/// from or to `category`.
fn typed(
    book: &mut Book,
    account: AccountId,
    category: AccountId,
    day: i8,
    minor_units: i64,
) -> TransactionId {
    let transaction = Transaction::new(
        TransactionId::generate(),
        october(day),
        Some("Typed in".into()),
        vec![
            Split::new(account, eur(minor_units)),
            Split::new(category, eur(-minor_units)),
        ],
    )
    .expect("balanced");
    book.add_transaction(&transaction).expect("added");
    transaction.id()
}

fn history_length(book: &Book) -> usize {
    book.history().expect("history read").len()
}

#[test]
fn a_statement_becomes_transactions_in_one_user_action() {
    let Sample {
        mut book, checking, ..
    } = sample();
    let before = history_length(&book);
    let mut pending = entry(7, -999, "P1", "Card reservation");
    pending.status = EntryStatus::Pending;
    let mut sparse = Entry::booked(october(6), eur(250_000));
    sparse.bank_id = Some("B2".into());
    let entries = vec![
        entry(5, -5320, "B1", "REWE Markt"),
        sparse.clone(),
        pending,
        entry(7, -1250, "B3", "Bakery"),
    ];
    let report = import_statements(
        &mut book,
        checking,
        &[statement(entries.clone())],
        &Choices::default(),
        Some(Path::new("/home/someone/october.sta")),
    )
    .expect("imported");
    assert_eq!(
        report.summary(),
        Summary {
            imported: 3,
            to_decide: 0,
            duplicates: 0,
            pending: 1,
        }
    );
    assert_eq!(report.entries[2].entry, at(0, 2));
    assert_eq!(report.entries[2].outcome, EntryOutcome::Pending);

    // Each entry is the account's part, with everything the bank delivered,
    // and one part without a category (D-083).
    let new = report.imported();
    assert_eq!(new.len(), 3);
    for (id, entry) in new.iter().zip([&entries[0], &entries[1], &entries[3]]) {
        let transaction = book.transaction(*id).expect("read");
        assert_eq!(transaction.booking_date(), entry.booking_date);
        assert_eq!(transaction.payee(), entry.other_party_name.as_deref());
        let [own, open] = transaction.splits() else {
            panic!("two parts");
        };
        assert_eq!(own.account, Some(checking));
        assert_eq!(own.amount, entry.amount);
        assert_eq!(
            own.imported,
            Some(ImportedDetails {
                bank_id: entry.bank_id.clone(),
                booking_date: Some(entry.booking_date),
                value_date: entry.value_date,
                other_party_name: entry.other_party_name.clone(),
                other_party_iban: entry.other_party_iban.clone(),
                remittance_information: entry.remittance_information.clone(),
            })
        );
        assert_eq!(open.account, None);
        assert_eq!(open.amount, entry.amount.checked_neg().expect("small"));
    }
    assert_eq!(
        book.balance(checking).expect("balance"),
        eur(-5320 + 250_000 - 1250)
    );

    // One user action, which undo takes back as a whole, with the file's
    // name but not its folder.
    assert_eq!(history_length(&book), before + 1);
    assert_eq!(
        book.change_to_undo()
            .expect("read")
            .map(|c| (c.action, c.file_name)),
        Some((Action::ImportStatement, Some("october.sta".into())))
    );
    book.undo().expect("undone");
    assert!(book.register(checking).expect("register").is_empty());
    book.redo().expect("redone");
    assert_eq!(book.register(checking).expect("register").len(), 3);
}

#[test]
fn an_entry_whose_bank_id_the_account_holds_is_skipped_and_counted() {
    let Sample {
        mut book,
        checking,
        savings,
        ..
    } = sample();
    let first = import(
        &mut book,
        checking,
        &[statement(vec![
            entry(5, -5320, "B1", "REWE Markt"),
            entry(6, -1250, "B2", "Bakery"),
        ])],
    );
    let [b1, b2] = first.imported()[..] else {
        panic!("two imported");
    };
    // The same file again, with one more entry, and the bank's ID twice.
    let again = import(
        &mut book,
        checking,
        &[
            statement(vec![
                entry(5, -5320, "B1", "REWE Markt"),
                entry(6, -1250, "B2", "Bakery"),
            ]),
            statement(vec![
                entry(8, -799, "B3", "Kiosk"),
                entry(8, -799, "B3", "Kiosk"),
            ]),
        ],
    );
    let outcomes: Vec<&EntryOutcome> = again.entries.iter().map(|r| &r.outcome).collect();
    assert_eq!(
        outcomes[..3],
        [
            &EntryOutcome::Duplicate(Duplicate::InBook(b1)),
            &EntryOutcome::Duplicate(Duplicate::InBook(b2)),
            &EntryOutcome::Imported {
                transaction: again.imported()[0],
                similar: Vec::new(),
            },
        ]
    );
    assert_eq!(
        outcomes[3],
        &EntryOutcome::Duplicate(Duplicate::InStatements(at(1, 0)))
    );
    assert_eq!(again.summary().duplicates, 3);
    assert_eq!(book.register(checking).expect("register").len(), 3);

    // Another account may hold the same ID: it is a different statement.
    let other = import(
        &mut book,
        savings,
        &[statement(vec![entry(5, -5320, "B1", "REWE Markt")])],
    );
    assert_eq!(other.summary().imported, 1);
}

#[test]
fn entries_that_look_like_a_transaction_the_account_has_wait_for_the_user() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    // Typed in on Friday, before the statement came.
    let bakery = typed(&mut book, checking, groceries, 9, -2500);
    let delivered = import(
        &mut book,
        checking,
        &[statement(vec![entry(9, -4000, "X1", "Shoes")])],
    );
    let shoes = delivered.imported()[0];

    let statements = [statement(vec![
        // Monday: one business day after the typed-in one.
        entry(12, -2500, "", "Bakery"),
        // With a bank ID, against one typed in without: still similar.
        entry(13, -2500, "B9", "Bakery"),
        // Thursday is four business days later: not similar.
        entry(15, -2500, "", "Bakery"),
        // Another amount on the same day.
        entry(12, -2600, "", "Bakery"),
        // Against an entry with another bank ID: two different entries.
        entry(9, -4000, "X2", "Shoes"),
        // Without an ID, against one with: similar.
        entry(9, -4000, "", "Shoes"),
        // The same twice in one statement, without IDs: two coffees.
        entry(14, -350, "", "Cafe"),
        entry(14, -350, "", "Cafe"),
    ])];
    let report =
        preview_import(&mut book, checking, &statements, &Choices::default()).expect("previewed");
    let similar: Vec<(EntryRef, Vec<TransactionId>)> = report
        .entries
        .iter()
        .filter_map(|result| match &result.outcome {
            EntryOutcome::Similar(ids) => Some((result.entry, ids.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        similar,
        [
            (at(0, 0), vec![bakery]),
            (at(0, 1), vec![bakery]),
            (at(0, 5), vec![shoes]),
        ]
    );
    assert_eq!(
        report.summary(),
        Summary {
            imported: 5,
            to_decide: 3,
            duplicates: 0,
            pending: 0,
        }
    );

    // The user decides to import one of them anyway.
    let choices = Choices {
        import_similar: HashSet::from([at(0, 1)]),
    };
    let report =
        import_statements(&mut book, checking, &statements, &choices, None).expect("imported");
    let EntryOutcome::Imported {
        transaction,
        similar,
    } = &report.entries[1].outcome
    else {
        panic!("imported as chosen: {:?}", report.entries[1]);
    };
    assert_eq!(similar, &[bakery]);
    assert!(book.transaction(*transaction).is_ok());
    assert_eq!(report.summary().to_decide, 2);
    assert_eq!(report.summary().imported, 6);
}

#[test]
fn transfers_are_suggested_and_the_rules_run_on_the_new_transactions_only() {
    let Sample {
        mut book,
        checking,
        savings,
        groceries,
    } = sample();
    let rule = |name: &str, other_party: &str, book_to| Rule {
        id: RuleId::generate(),
        name: name.into(),
        match_mode: MatchMode::All,
        conditions: vec![Condition::Text {
            field: TextField::OtherPartyName,
            operator: TextOperator::Contains,
            value: other_party.into(),
            case_sensitive: false,
        }],
        actions: Actions {
            book_to,
            payee: None,
        },
    };
    // The savings side of a Transfer is in the book already, and so is a
    // purchase from before the Rules were made.
    let arrived = import(
        &mut book,
        savings,
        &[statement(vec![entry(6, 10_000, "S1", "Own transfer")])],
    );
    let earlier = import(
        &mut book,
        checking,
        &[statement(vec![entry(1, -1999, "B0", "REWE City")])],
    );
    let earlier = earlier.imported()[0];
    let shops = rule("Shops", "REWE", Some(BookTo::Category(groceries)));
    book.add_rule(&shops, None).expect("added");
    let own = rule("Own", "Own transfer", Some(BookTo::Category(groceries)));
    book.add_rule(&own, None).expect("added");

    let report = import(
        &mut book,
        checking,
        &[statement(vec![
            entry(5, -10_000, "B1", "Own transfer"),
            entry(5, -5320, "B2", "REWE Markt"),
        ])],
    );
    let [sent, purchase] = report.imported()[..] else {
        panic!("two imported");
    };
    let [found] = &report.transfers[..] else {
        panic!("one Transfer: {:?}", report.transfers);
    };
    assert_eq!(found.outgoing.transaction, sent);
    assert_eq!(found.incoming.transaction, arrived.imported()[0]);

    // The Rules booked the new purchase, left the Transfer's side to the
    // user and didn't touch the purchase from before.
    let changed: Vec<TransactionId> = report.rules.changed.iter().map(|c| c.transaction).collect();
    assert_eq!(changed, [purchase]);
    let [skipped] = &report.rules.skipped[..] else {
        panic!("one skipped: {:?}", report.rules.skipped);
    };
    assert_eq!(skipped.transaction, sent);
    assert!(matches!(skipped.reason, StoreError::TransferSuggested(_)));
    let category = |book: &Book, id| book.transaction(id).expect("read").splits()[1].account;
    assert_eq!(category(&book, purchase), Some(groceries));
    assert_eq!(category(&book, earlier), None);
    assert_eq!(category(&book, sent), None);

    // All in the one import action.
    assert_eq!(
        book.change_to_undo().expect("read").map(|c| c.action),
        Some(Action::ImportStatement)
    );
    book.undo().expect("undone");
    assert!(book.transaction(purchase).is_err());
    assert_eq!(category(&book, earlier), None);
}

#[test]
fn the_balance_is_checked_against_the_statements_closing_balance() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    // Typed in: what was in the account before, and a purchase after the
    // statement's last day.
    typed(&mut book, checking, groceries, 1, 100_000);
    typed(&mut book, checking, groceries, 20, -700);
    let closing = |day, minor_units| {
        Some(Balance {
            amount: eur(minor_units),
            date: october(day),
        })
    };
    let mut first = statement(vec![
        entry(5, -2500, "B1", "Bakery"),
        entry(6, 50_000, "B2", "Employer"),
    ]);
    first.opening_balance = closing(2, 100_000);
    first.closing_balance = closing(6, 147_500);
    let mut second = statement(vec![entry(9, -1000, "B3", "Kiosk")]);
    // The bank says a different balance: KassenSturz says what it has.
    second.closing_balance = closing(9, 146_000);
    let without = statement(vec![entry(12, -100, "B4", "Kiosk")]);

    let report = import(&mut book, checking, &[first, without, second]);
    assert_eq!(
        report.balances,
        [
            BalanceCheck {
                statement: 0,
                date: october(6),
                bank: eur(147_500),
                bank_original: None,
                book: eur(147_500),
            },
            BalanceCheck {
                statement: 2,
                date: october(9),
                bank: eur(146_000),
                bank_original: None,
                book: eur(146_500),
            },
        ]
    );
    assert!(report.balances[0].matches());
    assert!(!report.balances[1].matches());
    // The balance the app holds against the bank's own, "Check against my
    // bank" (D-029), is the one the import reports for the same day.
    for check in &report.balances {
        assert_eq!(
            book.balance_on(checking, check.date).expect("balance"),
            check.book
        );
    }
}

#[test]
fn a_preview_shows_what_the_import_would_do_and_changes_nothing() {
    let Sample {
        mut book,
        checking,
        savings,
        ..
    } = sample();
    import(
        &mut book,
        savings,
        &[statement(vec![entry(6, 10_000, "S1", "Own transfer")])],
    );
    let history = book.history().expect("history read");
    let balance = book.balance(checking).expect("balance");
    let mut statements = [statement(vec![
        entry(5, -10_000, "B1", "Own transfer"),
        entry(5, -5320, "B2", "REWE Markt"),
    ])];
    statements[0].closing_balance = Some(Balance {
        amount: eur(-15_320),
        date: october(5),
    });

    let preview =
        preview_import(&mut book, checking, &statements, &Choices::default()).expect("previewed");
    assert_eq!(book.history().expect("history read"), history);
    assert_eq!(book.balance(checking).expect("balance"), balance);
    assert!(book.transaction(preview.imported()[0]).is_err());
    assert!(book.transfer_suggestions().expect("read").is_empty());

    let report = import(&mut book, checking, &statements);
    assert_eq!(preview.summary(), report.summary());
    assert_eq!(preview.balances, report.balances);
    assert_eq!(preview.transfers.len(), 1);
    assert_eq!(report.transfers.len(), 1);
}

#[test]
fn statements_that_do_not_fit_the_account_are_refused() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let history = history_length(&book);
    let refused = |book: &mut Book, account, statement: Statement| {
        let Err(error) = import_statements(book, account, &[statement], &Choices::default(), None)
        else {
            panic!("imported");
        };
        error
    };

    let usd = CurrencyCode::new("USD").expect("a valid code");
    let mut dollars = statement(Vec::new());
    dollars.currency = Currency::new(usd, 2).expect("valid decimal places");
    let error = refused(&mut book, checking, dollars);
    assert!(matches!(
        error,
        PipelineError::Import(ImportError::AccountCurrencyDiffers {
            statement_currency,
            ..
        }) if statement_currency == usd
    ));
    assert_eq!(error.report().code, "import.account-currency-differs");

    // The mark comes into an account in euros (D-105), but not into one in
    // dollars, and only with the decimal places it had.
    book.use_currency(usd).expect("currency used");
    let dollar_account = AccountId::generate();
    book.add_account(&Account {
        id: dollar_account,
        name: "Dollar account".into(),
        kind: AccountKind::Asset,
        currency: usd,
        account_type: None,
        parent: None,
        closed: false,
    })
    .expect("account added");
    let marks = |digits| Statement {
        currency: Currency::new(DEM, digits).expect("valid decimal places"),
        ..statement(Vec::new())
    };
    let error = refused(&mut book, dollar_account, marks(2));
    assert!(matches!(
        error,
        PipelineError::Import(ImportError::ReplacedCurrencyDiffers {
            statement_currency: DEM,
            successor: EUR,
            ..
        })
    ));
    assert_eq!(error.report().code, "import.replaced-currency-differs");
    assert!(matches!(
        refused(&mut book, checking, marks(3)),
        PipelineError::Import(ImportError::DecimalPlacesDiffer {
            currency: DEM,
            book: 2,
            file: 3,
        })
    ));

    let mut three = statement(vec![entry(5, -5320, "B1", "REWE Markt")]);
    three.currency = Currency::new(EUR, 3).expect("valid decimal places");
    assert!(matches!(
        refused(&mut book, checking, three),
        PipelineError::Import(ImportError::DecimalPlacesDiffer {
            book: 2,
            file: 3,
            ..
        })
    ));

    let purchase = || statement(vec![entry(5, -5320, "B1", "REWE Markt")]);
    let error = refused(&mut book, groceries, purchase());
    assert!(matches!(
        error,
        PipelineError::Store(StoreError::NotAnAccount(_))
    ));
    assert_eq!(error.report().code, "store.not-an-account");
    let closed = add_account(&mut book, "Old account", AccountKind::Asset);
    book.close_account(closed).expect("closed");
    // The dollars, the dollar account, the old account and closing it.
    let history = history + 4;
    assert!(matches!(
        refused(&mut book, closed, purchase()),
        PipelineError::Store(StoreError::AccountClosed(_))
    ));
    assert!(matches!(
        refused(&mut book, AccountId::generate(), purchase()),
        PipelineError::Store(StoreError::UnknownAccount(_))
    ));
    assert_eq!(history_length(&book), history);
}

#[test]
fn an_import_that_fails_part_way_changes_nothing() {
    let Sample {
        mut book, checking, ..
    } = sample();
    let history = history_length(&book);
    let year_0 = Date::new(0, 1, 1).expect("a valid date");
    let statements = [statement(vec![
        entry(5, -5320, "B1", "REWE Markt"),
        Entry::booked(year_0, eur(-100)),
    ])];
    let failed = import_statements(&mut book, checking, &statements, &Choices::default(), None);
    assert!(matches!(
        failed,
        Err(PipelineError::Store(StoreError::DateOutOfRange(_)))
    ));
    assert!(book.register(checking).expect("register").is_empty());
    assert_eq!(history_length(&book), history);
}

/// A booked entry of a statement in German marks, in October 2001.
fn in_marks(day: i8, minor_units: i64, bank_id: &str) -> Entry {
    let date = Date::new(2001, 10, day).expect("a valid date");
    Entry {
        bank_id: (!bank_id.is_empty()).then(|| bank_id.to_owned()),
        other_party_name: Some("Kiosk".into()),
        ..Entry::booked(date, dem(minor_units))
    }
}

/// A statement in German marks, from 1,000.00 DEM to `closing` minor units.
fn statement_in_marks(entries: Vec<Entry>, closing: i64) -> Statement {
    let date = |day| Date::new(2001, 10, day).expect("a valid date");
    Statement {
        currency: Currency::new(DEM, 2).expect("valid decimal places"),
        opening_balance: Some(Balance {
            amount: dem(100_000),
            date: date(1),
        }),
        closing_balance: Some(Balance {
            amount: dem(closing),
            date: date(10),
        }),
        ..statement(entries)
    }
}

#[test]
fn a_statement_in_marks_comes_into_an_account_in_euros() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    // 1,000.00 DEM, as the account held them before, are 511.29 EUR.
    let before = Transaction::new(
        TransactionId::generate(),
        Date::new(2001, 9, 30).expect("a valid date"),
        None,
        vec![
            Split::new(checking, eur(51_129)),
            Split::new(groceries, eur(-51_129)),
        ],
    )
    .expect("balanced");
    book.add_transaction(&before).expect("added");
    let mut pending = in_marks(9, -500, "");
    pending.status = EntryStatus::Pending;
    let statements = [statement_in_marks(
        vec![
            in_marks(2, -1, "M1"),
            in_marks(3, -1, "M2"),
            in_marks(4, -1, "M3"),
            in_marks(5, 10_000, "M4"),
            pending,
        ],
        109_997,
    )];

    let preview =
        preview_import(&mut book, checking, &statements, &Choices::default()).expect("previewed");
    let report = import(&mut book, checking, &statements);
    assert_eq!(preview.balances, report.balances);
    let mark = DEM.withdrawn().expect("the table lists the mark");
    assert_eq!(report.converted_from, vec![mark]);
    assert_eq!(report.summary().imported, 4);
    // On their own, 0.01 DEM would be 0.01 EUR three times; the balance in
    // marks, converted, goes from 511.29 EUR to 511.29, 511.28 and 511.28
    // EUR, so the bank's converted balance is the book's.
    let transactions: Vec<Transaction> = report
        .imported()
        .into_iter()
        .map(|id| book.transaction(id).expect("imported"))
        .collect();
    let amounts: Vec<Money> = transactions
        .iter()
        .map(|transaction| transaction.splits()[0].amount)
        .collect();
    assert_eq!(amounts, [eur(0), eur(-1), eur(0), eur(5_113)]);
    for (transaction, original) in transactions.iter().zip([-1, -1, -1, 10_000]) {
        let [own, open] = transaction.splits() else {
            panic!("two parts");
        };
        let kept = own.original.expect("an original amount");
        assert_eq!(kept.amount, dem(original));
        assert_eq!(kept.rate.from, EUR);
        assert_eq!(kept.rate.to, DEM);
        assert_eq!(
            kept.rate.rate,
            "1.95583".parse::<ExchangeRate>().expect("a rate")
        );
        assert_eq!(kept.rate.source, RateSource::Changeover);
        // What it was for can be broken down freely.
        assert_eq!(open.original, None);
        assert_eq!(open.amount, own.amount.checked_neg().expect("fits"));
    }
    assert_eq!(
        report.balances,
        [BalanceCheck {
            statement: 0,
            date: Date::new(2001, 10, 10).expect("a valid date"),
            bank: eur(56_241),
            bank_original: Some(dem(109_997)),
            book: eur(56_241),
        }]
    );
    assert!(report.balances[0].matches());
    assert!(
        book.currencies()
            .expect("currencies")
            .contains(&Currency::new(DEM, 2).expect("2"))
    );

    // Again: each entry is in the book already. Without its bank ID, an
    // entry of the same amount in marks looks like one.
    let again = import(&mut book, checking, &statements);
    assert_eq!(again.summary().imported, 0);
    let kiosk = report.imported()[0];
    let later = [statement_in_marks(vec![in_marks(2, -1, "")], 99_999)];
    let preview =
        preview_import(&mut book, checking, &later, &Choices::default()).expect("previewed");
    assert!(matches!(
        &preview.entries[0].outcome,
        EntryOutcome::Similar(similar) if similar.contains(&kiosk)
    ));
}
