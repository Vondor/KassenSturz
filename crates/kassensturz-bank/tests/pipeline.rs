// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where this crate meets the import pipeline (AGENTS.md §9.4, threat T-11):
//! the fake bank's statements, exactly as [`kassensturz_bank::statement`]
//! builds them, go through `import_statements` into a book in memory. No
//! value here is built by hand; the expectations were read from the
//! pipeline's real output first (M3 retro R3).

use kassensturz_bank::{
    Bank, BankAccount, BankAccountId, Failure, FakeBank, LoginId, MemoryPinStore, Pin, PinAsker,
    PinMode, PinPrompt, PinReply, PinSession, fetch_linked, find_bank_account, statement,
};
use kassensturz_core::{Account, AccountId, AccountKind, BankLink, CurrencyCode, Date, Money};
use kassensturz_import::{EntryStatus, Limits, Statement};
use kassensturz_pipeline::{Choices, Duplicate, EntryOutcome, import_statements, preview_import};
use kassensturz_store::Book;

/// The day a preview matches planned payments for; these books have none.
const PREVIEW_DAY: kassensturz_core::Date = kassensturz_core::Date::constant(2026, 11, 13);

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};

fn eur(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, EUR)
}

fn october(day: i8) -> Date {
    Date::new(2026, 10, day).expect("a valid date")
}

struct Setup {
    bank: FakeBank,
    pins: PinSession,
    current: BankAccountId,
    card: BankAccountId,
    book: Book,
    checking: AccountId,
    credit_card: AccountId,
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

/// The bank link (M4.4a, D-148) for a bank account as the backend lists it:
/// by IBAN when it has one, otherwise by the login's bank code and the
/// number the bank gives, so that it finds the bank account again after the
/// login is set up on another computer.
fn link_for(bank: &dyn Bank, account: &BankAccount, to: AccountId) -> BankLink {
    let login = bank
        .logins()
        .expect("logins")
        .into_iter()
        .find(|login| login.id == account.login)
        .expect("the account's login");
    BankLink {
        account: to,
        iban: account.iban.clone(),
        bank_code: Some(login.bank.code().to_owned()),
        number: account.number.clone(),
        last_fetched_on: None,
    }
}

/// A fake bank with one login, and a book with an account for each of its
/// bank accounts.
fn setup() -> Setup {
    let mut bank = FakeBank::new();
    let mut pins = PinSession::new(Box::new(MemoryPinStore::new()));
    pins.provide(
        &LoginId::new("fake-login-1"),
        Pin::new(FakeBank::PIN.to_owned()),
        PinMode::RememberUntilClose,
    )
    .expect("provided");
    let login = bank
        .add_login(&FakeBank::setup(), &mut pins)
        .expect("login added");
    let accounts = bank.accounts(&login).expect("accounts");
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    let checking = add_account(
        &mut book,
        "Checking account",
        AccountKind::Asset,
        accounts[0].iban.as_deref(),
    );
    let credit_card = add_account(&mut book, "Credit card", AccountKind::Liability, None);
    book.link_bank_account(&link_for(&bank, &accounts[0], checking))
        .expect("linked");
    book.link_bank_account(&link_for(&bank, &accounts[1], credit_card))
        .expect("linked");
    Setup {
        current: accounts[0].id.clone(),
        card: accounts[1].id.clone(),
        bank,
        pins,
        book,
        checking,
        credit_card,
    }
}

impl Setup {
    /// Fetches `account` and converts it, as the fetch screen will (M4.7).
    fn fetch(&mut self, account: &BankAccountId, since: Option<Date>) -> Statement {
        let fetched = self
            .bank
            .fetch(account, since, &mut self.pins)
            .expect("fetched");
        statement(&fetched, &Limits::default()).expect("a statement")
    }
}

#[test]
fn the_fake_banks_statement_is_imported_like_a_file() {
    let mut setup = setup();
    let statement = setup.fetch(&setup.current.clone(), None);
    assert_eq!(statement.entries.len(), 4);
    let report = import_statements(
        &mut setup.book,
        setup.checking,
        std::slice::from_ref(&statement),
        &Choices::default(),
        None,
    )
    .expect("imported");
    let summary = report.summary();
    assert_eq!(
        (
            summary.imported,
            summary.to_decide,
            summary.duplicates,
            summary.pending
        ),
        (3, 0, 0, 1)
    );
    // The pending card payment is the one left out (D-030).
    let pending: Vec<usize> = report
        .entries
        .iter()
        .filter(|result| result.outcome == EntryOutcome::Pending)
        .map(|result| result.entry.entry)
        .collect();
    assert_eq!(pending, vec![3]);
    assert_eq!(statement.entries[3].status, EntryStatus::Pending);
    // The bank's balance is checked against the book's (D-029), and they
    // agree once the starting balance is there: here the account started
    // empty, so the book is short by what was in it before October.
    assert_eq!(report.balances.len(), 1);
    let balance = report.balances[0];
    assert_eq!(balance.date, october(9));
    assert_eq!(balance.bank, eur(325_477));
    assert_eq!(balance.book, eur(225_477));
    assert!(!balance.matches());
    // The bank's texts arrived as the bank sent them.
    let imported = report.imported();
    assert_eq!(imported.len(), 3);
    let salary = setup
        .book
        .transaction(imported[0])
        .expect("the salary transaction");
    assert_eq!(salary.booking_date(), october(1));
    assert_eq!(salary.payee(), Some("Example Employer"));
    assert_eq!(
        setup
            .book
            .balance_on(setup.checking, october(9))
            .expect("a balance"),
        eur(225_477)
    );
}

#[test]
fn fetching_again_imports_nothing_twice() {
    let mut setup = setup();
    let first = setup.fetch(&setup.current.clone(), None);
    import_statements(
        &mut setup.book,
        setup.checking,
        &[first],
        &Choices::default(),
        None,
    )
    .expect("imported");
    // The fetch day is noted on the bank link within the import's change
    // (M4.4a); the next fetch starts from it.
    setup
        .book
        .note_bank_fetch(setup.checking, october(5))
        .expect("noted");
    let since = setup
        .book
        .bank_link(setup.checking)
        .expect("read")
        .expect("linked")
        .last_fetched_on;
    assert_eq!(since, Some(october(5)));
    let again = setup.fetch(&setup.current.clone(), since);
    assert_eq!(again.entries.len(), 2);
    let report = preview_import(
        &mut setup.book,
        setup.checking,
        &[again],
        &Choices::default(),
        PREVIEW_DAY,
    )
    .expect("previewed");
    let outcomes: Vec<&EntryOutcome> = report
        .entries
        .iter()
        .map(|result| &result.outcome)
        .collect();
    assert!(
        matches!(outcomes[0], EntryOutcome::Duplicate(Duplicate::InBook(_))),
        "{outcomes:?}"
    );
    assert_eq!(outcomes[1], &EntryOutcome::Pending);
    assert_eq!(report.summary().imported, 0);
}

#[test]
fn the_links_find_the_banks_accounts_again() {
    // As the fetch screen does (M4.7): the book's links, matched against
    // what the bank lists, by IBAN or by bank code and number. The card has
    // no IBAN, and the number is kept as the bank shows it.
    let setup = setup();
    let links = setup.book.bank_links().expect("links");
    assert_eq!(links.len(), 2);
    for link in &links {
        let (login, found) = find_bank_account(&setup.bank, link).expect("found");
        assert_eq!(login.id, LoginId::new("fake-login-1"));
        let expected = if link.account == setup.checking {
            &setup.current
        } else {
            &setup.card
        };
        assert_eq!(&found.id, expected);
    }
    let card = links
        .iter()
        .find(|link| link.account == setup.credit_card)
        .expect("the card's link");
    assert_eq!(card.iban, None);
    assert_eq!(card.bank_code.as_deref(), Some("37040044"));
    assert_eq!(card.number.as_deref(), Some("4000 0000 0000 0001"));
}

#[test]
fn a_credit_cards_statement_goes_into_a_liability_account() {
    let mut setup = setup();
    let statement = setup.fetch(&setup.card.clone(), None);
    assert_eq!(statement.account.iban, None);
    assert_eq!(
        statement.account.number.as_deref(),
        Some("4000 0000 0000 0001")
    );
    let report = import_statements(
        &mut setup.book,
        setup.credit_card,
        &[statement],
        &Choices::default(),
        None,
    )
    .expect("imported");
    assert_eq!(report.summary().imported, 2);
    // Both charges are debt, so the book owes 123.45 less than the bank
    // says: the card's debt from before October isn't in the book.
    let balance = report.balances[0];
    assert_eq!(balance.bank, eur(-12_345));
    assert_eq!(balance.book, eur(-12_345));
    assert!(balance.matches());
}

#[test]
fn broken_data_from_the_bank_imports_nothing() {
    let mut setup = setup();
    setup.bank.fail_next(Failure::InexactAmount);
    let fetched = setup
        .bank
        .fetch(&setup.current.clone(), None, &mut setup.pins)
        .expect("fetched");
    let error = statement(&fetched, &Limits::default()).expect_err("refused");
    assert_eq!(
        error,
        kassensturz_bank::BankError::InexactAmount {
            amount: "1/3".to_owned()
        }
    );
    assert_eq!(
        setup
            .book
            .balance_on(setup.checking, october(9))
            .expect("a balance"),
        eur(0)
    );
}

/// Never asked for a PIN: the session already has it.
struct NoQuestions;

impl PinAsker for NoQuestions {
    fn pin(&mut self, _login: &kassensturz_bank::Login, _prompt: PinPrompt) -> PinReply {
        panic!("the PIN was remembered, so nothing should be asked");
    }
}

#[test]
fn a_linked_account_is_fetched_and_imported_with_its_day_as_one_change() {
    // What "Get new transactions" does for a linked account (M4.7): the
    // link finds the bank account, the fetch starts on the link's day, and
    // the import and the day are one user action that undo takes back
    // together.
    let mut setup = setup();
    let link = setup
        .book
        .bank_link(setup.checking)
        .expect("read")
        .expect("linked");
    assert_eq!(link.last_fetched_on, None);
    let first = fetch_linked(
        &mut setup.bank,
        &mut setup.pins,
        &mut NoQuestions,
        &link,
        &Limits::default(),
    )
    .expect("a statement");
    assert_eq!(first.entries.len(), 4);
    let checking = setup.checking;
    setup
        .book
        .as_one_action(
            kassensturz_store::Action::ImportStatement,
            |book| -> Result<(), kassensturz_pipeline::PipelineError> {
                import_statements(book, checking, &[first], &Choices::default(), None)?;
                book.note_bank_fetch(checking, october(9))?;
                Ok(())
            },
        )
        .expect("imported");
    let link = setup
        .book
        .bank_link(setup.checking)
        .expect("read")
        .expect("linked");
    assert_eq!(link.last_fetched_on, Some(october(9)));

    // The next fetch starts five days before that day (Q-090): the 4th
    // on, so it brings the entries of the 5th again and the 9th's pending
    // payment, and the duplicate check keeps them from being added twice.
    let second = fetch_linked(
        &mut setup.bank,
        &mut setup.pins,
        &mut NoQuestions,
        &link,
        &Limits::default(),
    )
    .expect("a statement");
    let before = setup
        .book
        .balance_on(setup.checking, october(9))
        .expect("a balance");
    let report = import_statements(
        &mut setup.book,
        setup.checking,
        &[second],
        &Choices::default(),
        None,
    )
    .expect("imported");
    let summary = report.summary();
    assert_eq!(
        (
            summary.imported,
            summary.to_decide,
            summary.duplicates,
            summary.pending
        ),
        (0, 0, 1, 1)
    );
    assert_eq!(
        setup
            .book
            .balance_on(setup.checking, october(9))
            .expect("a balance"),
        before
    );
    // One undo of that import, then the first import with its day.
    setup.book.undo().expect("undone").expect("a change");

    // One undo takes back the transactions and the day.
    setup.book.undo().expect("undone").expect("a change");
    let link = setup
        .book
        .bank_link(setup.checking)
        .expect("read")
        .expect("linked");
    assert_eq!(link.last_fetched_on, None);
    assert_eq!(
        setup
            .book
            .balance_on(setup.checking, october(9))
            .expect("a balance"),
        eur(0)
    );
}

/// The IBAN the fake current account's standing order pays to (SWIFT's
/// example for Belgium), as the fake bank delivers it.
const FAKE_SAVINGS_IBAN: &str = "BE68539007547034";

#[test]
fn a_remembered_monthly_move_joins_the_fake_banks_standing_order() {
    // The fake current account pays 200.00 to the user's savings on
    // 5 October (D-749): with the move remembered, the savings account's
    // MT940 statement first, then a fetch, the two sides join by
    // themselves and no balance changes.
    let mut setup = setup();
    let savings = add_account(
        &mut setup.book,
        "Savings",
        AccountKind::Asset,
        Some(FAKE_SAVINGS_IBAN),
    );
    setup
        .book
        .add_recurring_move(&kassensturz_store::NewRecurringMove {
            from_account: setup.checking,
            to_account: savings,
            amount: eur(20_000),
            day_of_month: 5,
            from_party_iban: Some(FAKE_SAVINGS_IBAN.into()),
            from_party_name: Some("Own savings".into()),
            created_on: october(1),
        })
        .expect("remembered");
    let file = format!(
        ":20:KS784\r\n:25:{}\r\n:60F:C260930EUR0,00\r\n\
         :61:2610051005C200,00NTRFNONREF\r\n:86:166?00Dauerauftrag?20SVWZ+Monthly saving\r\n\
         :62F:C261005EUR200,00\r\n-\r\n",
        FAKE_SAVINGS_IBAN
    );
    let kassensturz_import::Imported::Statements(arrived) = kassensturz_import::import(
        &kassensturz_import::Mt940,
        file.as_bytes(),
        &Limits::default(),
    )
    .expect("read") else {
        panic!("statements expected");
    };
    let mut first = import_statements(
        &mut setup.book,
        savings,
        &arrived,
        &Choices::default(),
        None,
    )
    .expect("imported");
    kassensturz_pipeline::join_recurring_moves(&mut setup.book, &mut first).expect("ran");
    assert!(first.joins.joined.is_empty());

    let statement = setup.fetch(&setup.current.clone(), None);
    let before = setup
        .book
        .balance_on(setup.checking, october(9))
        .expect("balance");
    let mut report = import_statements(
        &mut setup.book,
        setup.checking,
        std::slice::from_ref(&statement),
        &Choices::default(),
        None,
    )
    .expect("imported");
    kassensturz_pipeline::join_recurring_moves(&mut setup.book, &mut report).expect("joined");
    let [join] = &report.joins.joined[..] else {
        panic!("one join: {:?}", report.joins);
    };
    assert_eq!(join.outgoing.account, setup.checking);
    assert_eq!(join.outgoing.amount, eur(-20_000));
    assert_eq!(join.incoming.account, savings);
    assert_eq!(join.incoming.transaction, first.imported()[0]);
    assert!(report.transfers.is_empty());
    // The same balances as without the join: checking as the import
    // test above reads it, savings as its statement says.
    assert_eq!(before, eur(0));
    assert_eq!(
        setup
            .book
            .balance_on(setup.checking, october(9))
            .expect("balance"),
        eur(225_477)
    );
    assert_eq!(
        setup.book.balance_on(savings, october(9)).expect("balance"),
        eur(20_000)
    );
}

#[test]
fn the_fake_banks_payments_are_matched_to_their_plans() {
    // The salary on 1 October, planned by the employer's name, and the
    // 200.00 to savings on the 5th, planned by its IBAN (D-769): a fetch
    // matches both after the import, in a change of its own.
    let mut setup = setup();
    let salary = add_account(&mut setup.book, "Salary", AccountKind::Income, None);
    let plan =
        |name: &str, amount: i64, day: i8, category, iban: Option<&str>, party: Option<&str>| {
            kassensturz_store::NewSchedule {
                name: name.into(),
                account: setup.checking,
                amount: eur(amount),
                tolerance: eur(0),
                category,
                party_iban: iban.map(str::to_owned),
                party_name: party.map(str::to_owned),
                unit: kassensturz_core::ScheduleUnit::Month,
                every: 1,
                anchor: october(day),
                track_from: october(day),
                ends_on: None,
            }
        };
    let pay = plan(
        "Pay",
        250_000,
        1,
        Some(salary),
        None,
        Some("Example Employer"),
    );
    let saving = plan("Saving", -20_000, 5, None, Some(FAKE_SAVINGS_IBAN), None);
    for new in [&pay, &saving] {
        setup.book.add_schedule(new).expect("planned");
    }
    let statement = setup.fetch(&setup.current.clone(), None);
    let mut report = import_statements(
        &mut setup.book,
        setup.checking,
        std::slice::from_ref(&statement),
        &Choices::default(),
        None,
    )
    .expect("imported");
    kassensturz_pipeline::join_recurring_moves(&mut setup.book, &mut report).expect("joined");
    kassensturz_pipeline::match_schedules(&mut setup.book, &mut report, october(9))
        .expect("matched");
    let matched: Vec<_> = report
        .schedules
        .matched
        .iter()
        .map(|done| (done.occurrence.due.counted, done.category_set))
        .collect();
    assert_eq!(matched, [(october(1), true), (october(5), false)]);
    let salary_entry = report.schedules.matched[0].transaction;
    let parts = setup.book.transaction(salary_entry).expect("entry");
    assert!(
        parts
            .splits()
            .iter()
            .any(|split| split.account == Some(salary) && split.amount == eur(-250_000))
    );
    // One Undo takes the matches back and keeps what the bank delivered.
    let balance = setup
        .book
        .balance_on(setup.checking, october(9))
        .expect("balance");
    setup.book.undo().expect("undone");
    assert!(
        setup
            .book
            .decided_occurrences()
            .expect("decided")
            .is_empty()
    );
    assert_eq!(
        setup
            .book
            .balance_on(setup.checking, october(9))
            .expect("balance"),
        balance
    );
    // Fetching again brings nothing new and matches nothing behind the
    // user's back.
    let again = setup.fetch(&setup.current.clone(), None);
    let mut report = import_statements(
        &mut setup.book,
        setup.checking,
        std::slice::from_ref(&again),
        &Choices::default(),
        None,
    )
    .expect("imported");
    kassensturz_pipeline::match_schedules(&mut setup.book, &mut report, october(9))
        .expect("matched");
    assert!(report.schedules.matched.is_empty());
    assert!(
        setup
            .book
            .decided_occurrences()
            .expect("decided")
            .is_empty()
    );
}
