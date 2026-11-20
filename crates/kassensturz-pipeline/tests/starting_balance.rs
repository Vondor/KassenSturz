// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The starting balance worked out from the bank's balance (D-903, B-411,
//! #1237): after the first import into an account, and as the correction
//! the user confirms. Books in memory with statements built in code; all
//! data is made up (AGENTS.md §3.1 rule 2).

use kassensturz_core::{
    Account, AccountId, AccountKind, Currency, CurrencyCode, Date, Money, Split, Transaction,
    TransactionId,
};
use kassensturz_import::{Balance, Entry, Statement, StatementAccount};
use kassensturz_pipeline::{
    Choices, ImportReport, StartError, StartSource, apply_start, derive_start_after_import,
    import_statements, parts_by_origin, plan_start, starting_balances_account,
};
use kassensturz_store::{Action, Book, StatedBalance};

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};

fn eur(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, EUR)
}

fn day(month: i8, day: i8) -> Date {
    Date::new(2026, month, day).expect("a valid date")
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
        number: None,
        notes: None,
        institution: None,
    })
    .expect("account added");
    id
}

/// A book with one account. With `stated`, the user said at setup what it
/// holds at the end of a day, as the app's "add account" writes it: a
/// starting balance dated that day and the stated balance (D-111, D-117).
fn book_with(kind: AccountKind, stated: Option<(i64, Date)>) -> (Book, AccountId) {
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    let account = add_account(&mut book, "Checking account", kind);
    if let Some((amount, on)) = stated {
        start(&mut book, account, amount, on);
        book.set_stated_balance(
            account,
            Some(&StatedBalance {
                amount: eur(amount),
                on,
            }),
        )
        .expect("stated");
    }
    (book, account)
}

/// A starting balance, as the app writes it.
fn start(book: &mut Book, account: AccountId, amount: i64, on: Date) -> TransactionId {
    let equity = starting_balances_account(book, EUR).expect("internal account");
    let transaction = Transaction::new(
        TransactionId::generate(),
        on,
        None,
        vec![
            Split::new(account, eur(amount)),
            Split::new(equity, eur(-amount)),
        ],
    )
    .expect("balanced");
    book.add_transaction(&transaction).expect("added");
    transaction.id()
}

fn entry(on: Date, minor_units: i64, other_party: &str) -> Entry {
    Entry {
        other_party_name: Some(other_party.into()),
        ..Entry::booked(on, eur(minor_units))
    }
}

fn statement(entries: Vec<Entry>, closing: Option<(i64, Date)>) -> Statement {
    Statement {
        account: StatementAccount::default(),
        currency: Currency::new(EUR, 2).expect("valid decimal places"),
        opening_balance: None,
        closing_balance: closing.map(|(amount, date)| Balance {
            amount: eur(amount),
            date,
        }),
        entries,
    }
}

/// The import as the app runs it: the pipeline, then the starting balance,
/// in one user action.
fn import(
    book: &mut Book,
    account: AccountId,
    statement: &Statement,
    fetched: bool,
) -> (ImportReport, Option<StartSource>) {
    book.as_one_action(Action::ImportStatement, |book| {
        let mut report = import_statements(
            book,
            account,
            std::slice::from_ref(statement),
            &Choices::default(),
            None,
        )?;
        let derived = derive_start_after_import(
            book,
            account,
            std::slice::from_ref(statement),
            &mut report,
            fetched,
        )?;
        Ok::<_, kassensturz_pipeline::PipelineError>((report, derived.map(|(_, source)| source)))
    })
    .expect("imported")
}

/// The account's starting balances, as (day, minor units).
fn starts(book: &Book, account: AccountId) -> Vec<(Date, i64)> {
    parts_by_origin(book, account)
        .expect("parts")
        .0
        .iter()
        .map(|part| (part.bank_date(), part.amount.minor_units()))
        .collect()
}

fn held(book: &Book, account: AccountId, on: Date) -> i64 {
    book.balance_on(account, on).expect("balance").minor_units()
}

/// Three months of history that end on 19 November with 1.000,00 in the
/// account: it started at 734,50 before the first entry on 20 August.
fn history() -> Vec<Entry> {
    vec![
        entry(day(8, 20), -45_000, "Example Landlord"),
        entry(day(9, 1), 180_000, "Example Employer"),
        entry(day(10, 12), -93_450, "Example Garage"),
        entry(day(11, 18), -15_000, "Example Grocer"),
    ]
}

/// B-411: the user enters today's balance at setup, then the first bank
/// fetch delivers the months before. The history must not move the entered
/// balance a second time: the starting balance is worked out from the
/// bank's balance and stands before the first entry.
#[test]
fn the_first_fetch_does_not_subtract_the_history_from_the_entered_balance() {
    let today = day(11, 19);
    let (mut book, account) = book_with(AccountKind::Asset, Some((100_000, today)));
    let fetched = statement(history(), Some((100_000, today)));

    let (report, derived) = import(&mut book, account, &fetched, true);

    assert_eq!(derived, Some(StartSource::Bank));
    assert_eq!(held(&book, account, today), 100_000, "what the bank says");
    assert_eq!(
        book.balance(account).expect("balance"),
        eur(100_000),
        "and what the account shows"
    );
    assert_eq!(starts(&book, account), [(day(8, 19), 73_450)]);
    // Every day in between is right too, not only the last one.
    assert_eq!(held(&book, account, day(8, 18)), 0);
    assert_eq!(held(&book, account, day(8, 19)), 73_450);
    assert_eq!(held(&book, account, day(8, 20)), 28_450);
    assert_eq!(held(&book, account, day(9, 1)), 208_450);
    assert_eq!(held(&book, account, day(10, 12)), 115_000);
    // The import's balance check sees the figures as they are afterwards.
    assert_eq!(report.balances.len(), 1);
    assert!(report.balances[0].matches(), "{:?}", report.balances[0]);
    assert_eq!(
        book.stated_balance(account).expect("stated"),
        Some(StatedBalance {
            amount: eur(100_000),
            on: today,
        })
    );

    // One Undo takes back the import and the starting balance together.
    assert_eq!(
        book.change_to_undo()
            .expect("history")
            .map(|change| change.action),
        Some(Action::ImportStatement)
    );
    book.undo().expect("undone");
    assert_eq!(starts(&book, account), [(today, 100_000)]);
    assert_eq!(book.balance(account).expect("balance"), eur(100_000));
    book.redo().expect("redone");
    assert_eq!(starts(&book, account), [(day(8, 19), 73_450)]);
    assert_eq!(book.balance(account).expect("balance"), eur(100_000));
}

/// The bank's balance wins over what the user typed: a balance typed a day
/// later than the bank's, and a little off, is replaced (D-903).
#[test]
fn the_banks_balance_replaces_a_balance_typed_for_a_later_day() {
    let (mut book, account) = book_with(AccountKind::Asset, Some((99_900, day(11, 19))));
    let fetched = statement(history(), Some((100_000, day(11, 18))));

    let (report, derived) = import(&mut book, account, &fetched, true);

    assert_eq!(derived, Some(StartSource::Bank));
    assert_eq!(starts(&book, account), [(day(8, 19), 73_450)]);
    assert_eq!(held(&book, account, day(11, 18)), 100_000);
    assert_eq!(book.balance(account).expect("balance"), eur(100_000));
    assert!(report.balances[0].matches());
    assert_eq!(
        book.stated_balance(account).expect("stated"),
        Some(StatedBalance {
            amount: eur(100_000),
            on: day(11, 18),
        }),
        "the account keeps what the bank said, not what was typed"
    );
}

/// An account added without a balance (as the first-run wizard will do for
/// a connected bank, D-895) gets its starting balance from the first fetch.
#[test]
fn an_account_without_a_starting_balance_gets_one_from_the_first_fetch() {
    let (mut book, account) = book_with(AccountKind::Asset, None);
    let fetched = statement(history(), Some((100_000, day(11, 19))));

    let (_, derived) = import(&mut book, account, &fetched, true);

    assert_eq!(derived, Some(StartSource::Bank));
    assert_eq!(starts(&book, account), [(day(8, 19), 73_450)]);
    assert_eq!(book.balance(account).expect("balance"), eur(100_000));
    book.undo().expect("undone");
    assert!(starts(&book, account).is_empty());
    assert_eq!(book.balance(account).expect("balance"), eur(0));
}

/// A file without any balance (a CSV file): the balance the user stated is
/// what is known, and the history from before it must not move it.
#[test]
fn a_file_without_a_balance_keeps_the_stated_balance() {
    let today = day(11, 19);
    let (mut book, account) = book_with(AccountKind::Asset, Some((100_000, today)));

    let (report, derived) = import(&mut book, account, &statement(history(), None), false);

    assert_eq!(derived, Some(StartSource::Stated));
    assert!(report.balances.is_empty());
    assert_eq!(starts(&book, account), [(day(8, 19), 73_450)]);
    assert_eq!(held(&book, account, today), 100_000);
    assert_eq!(
        book.stated_balance(account).expect("stated"),
        Some(StatedBalance {
            amount: eur(100_000),
            on: today,
        })
    );
}

/// An old file's closing balance is older knowledge than what the user
/// stated for a later day: the stated balance holds, and the file's balance
/// is checked against it as before.
#[test]
fn an_old_files_balance_does_not_replace_a_later_stated_balance() {
    let today = day(11, 19);
    let (mut book, account) = book_with(AccountKind::Asset, Some((100_000, today)));
    // Only August and September, with the bank's balance at the end of
    // September; October and November are not in the file.
    let file = statement(history()[..2].to_vec(), Some((208_450, day(9, 30))));

    let (report, derived) = import(&mut book, account, &file, false);

    assert_eq!(derived, Some(StartSource::Stated));
    assert_eq!(held(&book, account, today), 100_000);
    assert_eq!(starts(&book, account), [(day(8, 19), -35_000)]);
    // The months that are missing show as a difference, never as a match.
    assert_eq!(report.balances[0].book, eur(100_000));
    assert!(!report.balances[0].matches());
    assert_eq!(
        book.stated_balance(account).expect("stated").map(|s| s.on),
        Some(today)
    );
}

/// A file's closing balance on or after the stated day is the newer
/// knowledge and counts.
#[test]
fn a_files_balance_on_a_later_day_than_the_stated_one_counts() {
    let (mut book, account) = book_with(AccountKind::Asset, Some((50_000, day(8, 1))));
    let file = statement(history(), Some((100_000, day(11, 19))));

    let (report, derived) = import(&mut book, account, &file, false);

    assert_eq!(derived, Some(StartSource::Bank));
    // The starting balance stays on its day: no entry is dated before it.
    assert_eq!(starts(&book, account), [(day(8, 1), 73_450)]);
    assert!(report.balances[0].matches());
    assert_eq!(book.balance(account).expect("balance"), eur(100_000));
}

/// A file that gives its opening balance (CAMT.053, MT940) dates it before
/// its first entry, often by several days. The starting balance is the same
/// figure and takes that day, so the import's check of the opening balance
/// agrees too.
#[test]
fn the_starting_balance_is_dated_no_later_than_the_files_opening_balance() {
    let today = day(11, 19);
    let (mut book, account) = book_with(AccountKind::Asset, Some((100_000, today)));
    let mut file = statement(history(), Some((100_000, today)));
    file.opening_balance = Some(Balance {
        amount: eur(73_450),
        date: day(7, 31),
    });

    let (report, derived) = import(&mut book, account, &file, false);

    assert_eq!(derived, Some(StartSource::Bank));
    assert_eq!(starts(&book, account), [(day(7, 31), 73_450)]);
    assert_eq!(held(&book, account, day(7, 31)), 73_450);
    assert!(report.balances[0].matches());
}

/// What a credit card owes is money that went out: the same rule with the
/// signs of a debt.
#[test]
fn a_credit_cards_starting_balance_is_worked_out_the_same_way() {
    let today = day(11, 19);
    let (mut book, card) = book_with(AccountKind::Liability, Some((-34_565, today)));
    let fetched = statement(
        vec![
            entry(day(10, 12), -3_525, "Example Bookshop"),
            entry(day(11, 2), -1_040, "Example Cafe"),
        ],
        Some((-34_565, today)),
    );

    let (report, derived) = import(&mut book, card, &fetched, true);

    assert_eq!(derived, Some(StartSource::Bank));
    assert_eq!(starts(&book, card), [(day(10, 11), -30_000)]);
    assert_eq!(book.balance(card).expect("balance"), eur(-34_565));
    assert!(report.balances[0].matches());
}

/// Neighbouring path: only the *first* import works the starting balance
/// out. A later fetch with a difference leaves it alone, since then an
/// entry is missing or there twice, which the user looks into (D-111).
#[test]
fn a_later_import_never_changes_the_starting_balance() {
    let today = day(11, 19);
    let (mut book, account) = book_with(AccountKind::Asset, Some((100_000, today)));
    import(
        &mut book,
        account,
        &statement(history(), Some((100_000, today))),
        true,
    );
    let before = starts(&book, account);

    // The bank's balance is 50,00 lower than the one new entry explains.
    let next = statement(
        vec![entry(day(11, 20), -2_000, "Example Bakery")],
        Some((93_000, day(11, 20))),
    );
    let (report, derived) = import(&mut book, account, &next, true);

    assert_eq!(derived, None);
    assert_eq!(starts(&book, account), before);
    assert_eq!(report.balances[0].book, eur(98_000));
    assert!(!report.balances[0].matches(), "the difference is shown");
    assert_eq!(
        book.stated_balance(account).expect("stated").map(|s| s.on),
        Some(today),
        "a balance that differs is never kept"
    );
}

/// Neighbouring path: an account the user already typed entries into is not
/// a first import, so nothing is worked out by itself.
#[test]
fn an_account_with_typed_entries_is_left_alone() {
    let today = day(11, 19);
    let (mut book, account) = book_with(AccountKind::Asset, Some((100_000, today)));
    let groceries = add_account(&mut book, "Groceries", AccountKind::Expense);
    let typed = Transaction::new(
        TransactionId::generate(),
        day(11, 10),
        Some("Typed in".into()),
        vec![
            Split::new(account, eur(-1_000)),
            Split::new(groceries, eur(1_000)),
        ],
    )
    .expect("balanced");
    book.add_transaction(&typed).expect("added");

    let (report, derived) = import(
        &mut book,
        account,
        &statement(history(), Some((100_000, today))),
        true,
    );

    assert_eq!(derived, None);
    assert_eq!(starts(&book, account), [(today, 100_000)]);
    assert!(!report.balances[0].matches());
}

/// Neighbouring paths where there is nothing to work out: no balance known
/// at all, nothing imported, several starting balances, and a first import
/// that agrees already.
#[test]
fn nothing_is_worked_out_without_need_or_knowledge() {
    let today = day(11, 19);

    // No balance from the bank and none stated.
    let (mut book, account) = book_with(AccountKind::Asset, None);
    let (_, derived) = import(&mut book, account, &statement(history(), None), false);
    assert_eq!(derived, None);
    assert!(starts(&book, account).is_empty());
    assert_eq!(book.balance(account).expect("balance"), eur(26_550));

    // Nothing imported: the statement is empty.
    let (mut book, account) = book_with(AccountKind::Asset, Some((100_000, today)));
    let empty = statement(Vec::new(), Some((90_000, today)));
    let (_, derived) = import(&mut book, account, &empty, true);
    assert_eq!(derived, None);
    assert_eq!(starts(&book, account), [(today, 100_000)]);

    // Two starting balances: which one is meant is the user's to say.
    let (mut book, account) = book_with(AccountKind::Asset, Some((100_000, today)));
    start(&mut book, account, 5_000, day(8, 1));
    let (_, derived) = import(
        &mut book,
        account,
        &statement(history(), Some((100_000, today))),
        true,
    );
    assert_eq!(derived, None);
    assert_eq!(
        starts(&book, account),
        [(day(8, 1), 5_000), (today, 100_000)]
    );

    // The history starts after the starting balance and adds up to the
    // bank's balance: nothing changes, and no change is reported.
    let (mut book, account) = book_with(AccountKind::Asset, Some((73_450, day(8, 19))));
    let (report, derived) = import(
        &mut book,
        account,
        &statement(history(), Some((100_000, today))),
        true,
    );
    assert_eq!(derived, None);
    assert_eq!(starts(&book, account), [(day(8, 19), 73_450)]);
    assert!(report.balances[0].matches());
}

/// A dry run shows what the first import would do to the starting balance
/// and leaves the book as it was (the app's preview).
#[test]
fn a_dry_run_shows_the_change_and_keeps_the_book() {
    let today = day(11, 19);
    let (mut book, account) = book_with(AccountKind::Asset, Some((100_000, today)));
    let fetched = statement(history(), Some((100_000, today)));

    let change = book
        .dry_run(|book| {
            let mut report = import_statements(
                book,
                account,
                std::slice::from_ref(&fetched),
                &Choices::default(),
                None,
            )?;
            derive_start_after_import(
                book,
                account,
                std::slice::from_ref(&fetched),
                &mut report,
                true,
            )
        })
        .expect("previewed")
        .expect("a change")
        .0;

    assert_eq!(change.was, Some((eur(100_000), today)));
    assert_eq!(change.becomes, (eur(73_450), day(8, 19)));
    assert!(change.moves() && !change.adds());
    assert_eq!(starts(&book, account), [(today, 100_000)]);
    assert_eq!(book.balance(account).expect("balance"), eur(100_000));
    assert!(
        book.change_to_undo()
            .expect("history")
            .is_some_and(|c| { c.action != Action::ImportStatement })
    );
}

/// The one-click correction for a book that B-411 already made wrong: the
/// balance was entered for today, the history came in on top of it, and the
/// account still knows what was stated. Confirming the plan puts the
/// starting balance where it belongs, as one change that Undo takes back.
#[test]
fn a_book_that_is_already_wrong_is_corrected_in_one_change() {
    let today = day(11, 19);
    let (mut book, account) = book_with(AccountKind::Asset, Some((100_000, today)));
    // The import as it was before the fix: the pipeline alone.
    import_statements(
        &mut book,
        account,
        &[statement(history(), Some((100_000, today)))],
        &Choices::default(),
        None,
    )
    .expect("imported");
    assert_eq!(
        book.balance(account).expect("balance"),
        eur(126_550),
        "the wrong figure: the history moved the entered balance again"
    );

    let stated = book.stated_balance(account).expect("stated").expect("set");
    let change = plan_start(&book, account, stated.amount, stated.on)
        .expect("planned")
        .expect("a change");
    assert_eq!(change.was, Some((eur(100_000), today)));
    assert_eq!(change.becomes, (eur(73_450), day(8, 19)));
    assert_eq!(change.holds, stated);
    // Planning changes nothing.
    assert_eq!(book.balance(account).expect("balance"), eur(126_550));

    apply_start(&mut book, &change).expect("corrected");
    assert_eq!(book.balance(account).expect("balance"), eur(100_000));
    assert_eq!(starts(&book, account), [(day(8, 19), 73_450)]);
    assert_eq!(held(&book, account, day(9, 1)), 208_450);
    assert_eq!(
        book.change_to_undo()
            .expect("history")
            .map(|change| change.action),
        Some(Action::CorrectStartingBalance)
    );
    assert!(
        plan_start(&book, account, stated.amount, stated.on)
            .expect("planned")
            .is_none(),
        "nothing left to correct"
    );

    book.undo().expect("undone");
    assert_eq!(book.balance(account).expect("balance"), eur(126_550));
    assert_eq!(starts(&book, account), [(today, 100_000)]);
}

/// The correction's own neighbouring paths: a day before the starting
/// balance is refused, the latest of several starting balances changes
/// without moving, and a starting balance shared with another account stays
/// on its day.
#[test]
fn the_correction_moves_only_a_single_plain_starting_balance() {
    let today = day(11, 19);

    let (book, account) = book_with(AccountKind::Asset, Some((100_000, today)));
    assert!(matches!(
        plan_start(&book, account, eur(90_000), day(11, 1)),
        Err(StartError::Later)
    ));

    // Two starting balances and an entry before both.
    let (mut book, account) = book_with(AccountKind::Asset, Some((100_000, day(9, 1))));
    start(&mut book, account, 5_000, day(10, 1));
    import_statements(
        &mut book,
        account,
        &[statement(
            vec![entry(day(8, 20), -45_000, "Example Landlord")],
            None,
        )],
        &Choices::default(),
        None,
    )
    .expect("imported");
    let change = plan_start(&book, account, eur(70_000), today)
        .expect("planned")
        .expect("a change");
    assert_eq!(change.was, Some((eur(5_000), day(10, 1))));
    assert_eq!(change.becomes, (eur(15_000), day(10, 1)));
    apply_start(&mut book, &change).expect("corrected");
    assert_eq!(
        starts(&book, account),
        [(day(9, 1), 100_000), (day(10, 1), 15_000)]
    );
    assert_eq!(held(&book, account, today), 70_000);

    // One starting-balance transaction for two accounts.
    let (mut book, account) = book_with(AccountKind::Asset, None);
    let savings = add_account(&mut book, "Savings account", AccountKind::Asset);
    let equity = starting_balances_account(&mut book, EUR).expect("internal account");
    let shared = Transaction::new(
        TransactionId::generate(),
        day(9, 1),
        None,
        vec![
            Split::new(account, eur(100_000)),
            Split::new(savings, eur(20_000)),
            Split::new(equity, eur(-120_000)),
        ],
    )
    .expect("balanced");
    book.add_transaction(&shared).expect("added");
    import_statements(
        &mut book,
        account,
        &[statement(
            vec![entry(day(8, 20), -45_000, "Example Landlord")],
            None,
        )],
        &Choices::default(),
        None,
    )
    .expect("imported");
    let change = plan_start(&book, account, eur(100_000), today)
        .expect("planned")
        .expect("a change");
    assert_eq!(change.becomes, (eur(145_000), day(9, 1)));
    apply_start(&mut book, &change).expect("corrected");
    assert_eq!(held(&book, account, today), 100_000);
    assert_eq!(
        book.balance(savings).expect("balance"),
        eur(20_000),
        "the other account's starting balance is untouched"
    );
    assert_eq!(starts(&book, savings), [(day(9, 1), 20_000)]);
}
