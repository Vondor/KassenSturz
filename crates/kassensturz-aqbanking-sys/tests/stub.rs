// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The safe session against the stub library (threat T-09): every answer
//! the stub plays, good and broken, comes back as the records the session
//! promises, and every refusal as the error it promises. No real library
//! and no network is involved (D-055).
//!
//! The stub keeps one state per process, so the tests run one at a time
//! under `LOCK`; each gets its own data folder, sets up the stub's login
//! in the plain scenario and switches to the scenario it tests after.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use kassensturz_aqbanking_sys::gwenhywfar::{
    GWEN_ERROR_GENERIC, GWEN_ERROR_NET_UNREACHABLE, GWEN_ERROR_USER_ABORTED,
};
use kassensturz_aqbanking_sys::session::{
    AccountKind, BalanceKind, DateRecord, LogLevel, PasswordAnswer, PasswordKind, PasswordMethod,
    PasswordRequest, PasswordStatus, QuestionLevel, QuestionRequest, Session, SessionError,
    SessionHandler, SessionOptions, TransactionKind,
};
use kassensturz_aqbanking_sys::{Library, LoadError, MAX_TEXT_BYTES, Version, probe_from, stub};
use zeroize::Zeroizing;

static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A handler that answers with one PIN, a TAN and a button if it has
/// them, and records what it was asked.
#[derive(Default)]
struct Recorder {
    pin: Option<String>,
    tan: Option<String>,
    button: Option<usize>,
    requests: Vec<PasswordKind>,
    tan_requests: Vec<PasswordRequest>,
    questions: Vec<QuestionRequest>,
    statuses: Vec<PasswordStatus>,
    lines: Vec<(LogLevel, String)>,
}

impl SessionHandler for Recorder {
    fn password(&mut self, request: &PasswordRequest) -> PasswordAnswer {
        self.requests.push(request.kind);
        if request.kind == PasswordKind::Tan {
            self.tan_requests.push(request.clone());
        }
        let answer = match request.kind {
            PasswordKind::Pin => self.pin.clone(),
            PasswordKind::Tan => self.tan.clone(),
            PasswordKind::Other => None,
        };
        match answer {
            Some(text) => PasswordAnswer::Text(Zeroizing::new(text)),
            None => PasswordAnswer::Decline,
        }
    }

    fn question(&mut self, question: &QuestionRequest) -> Option<usize> {
        self.questions.push(question.clone());
        self.button
    }

    fn password_status(&mut self, status: PasswordStatus) {
        self.statuses.push(status);
    }

    fn log(&mut self, level: LogLevel, message: &str) {
        self.lines.push((level, message.to_owned()));
    }
}

fn folder(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "kassensturz-sys-stub-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).expect("the data folder is created");
    folder
}

fn open(scenario: &str, name: &str, pin: Option<&str>) -> (Session<Recorder>, PathBuf) {
    stub::set_scenario(scenario);
    let library = stub::library();
    let data_dir = folder(name);
    let session = Session::open_from(
        library,
        library,
        SessionOptions {
            app_name: "KassenSturz tests".to_owned(),
            data_dir: data_dir.clone(),
            registration_key: Some("made-up-key".to_owned()),
        },
        Recorder {
            pin: pin.map(str::to_owned),
            ..Recorder::default()
        },
    )
    .expect("the session opens over the stub");
    (session, data_dir)
}

fn journal(data_dir: &Path) -> String {
    fs::read_to_string(data_dir.join(stub::JOURNAL_FILE)).unwrap_or_default()
}

/// Sets up the stub's login and its accounts; returns the first account's
/// unique ID.
fn set_up_login(session: &Session<Recorder>) -> u32 {
    session
        .control(
            "adduser",
            &[
                "-N",
                "KassenSturz",
                "-b",
                "37040044",
                "-u",
                "made-up-login",
                "-t",
                "pintan",
                "-s",
                "https://fints.testbank.example/",
                "--hbciversion=300",
            ],
        )
        .expect("adduser");
    session
        .control("getaccounts", &["-u", "1"])
        .expect("getaccounts");
    session.accounts().expect("accounts")[0].unique_id
}

#[test]
fn the_stub_reports_its_versions_and_an_old_one_is_too_old() {
    let _guard = lock();
    stub::set_scenario("ok");
    let library = stub::library();
    let (gwenhywfar, aqbanking) = probe_from(library, library).expect("the stub loads");
    assert_eq!(gwenhywfar, Version::new(5, 14, 1));
    assert_eq!(aqbanking, Version::new(6, 9, 2));
    stub::set_scenario("old-gwenhywfar");
    match probe_from(library, library) {
        Err(LoadError::TooOld { library, found, .. }) => {
            assert_eq!(library, Library::Gwenhywfar);
            assert_eq!(found, Version::new(5, 13, 0));
        }
        other => panic!("an old Gwenhywfar is refused: {other:?}"),
    }
    stub::set_scenario("ok");
}

#[test]
fn bank_information_comes_back_with_every_gap_kept() {
    let _guard = lock();
    let (session, _) = open("ok", "bank-info", None);
    assert_eq!(session.versions().1, Version::new(6, 9, 2));
    let bank = session
        .bank_info("de", "37040044")
        .expect("looked up")
        .expect("known");
    assert_eq!(bank.name.as_deref(), Some("Testbank"));
    assert_eq!(bank.services.len(), 2);
    let pintan = &bank.services[0];
    assert_eq!(pintan.kind.as_deref(), Some("HBCI"));
    assert_eq!(
        pintan.address.as_deref(),
        Some("https://fints.testbank.example/")
    );
    assert_eq!(pintan.mode.as_deref(), Some("PINTAN"));
    assert_eq!(bank.services[1].mode.as_deref(), Some("RDH10"));
    let none = session
        .bank_info("de", "10000000")
        .expect("looked up")
        .expect("known");
    assert!(none.services.is_empty());
    let bare = session
        .bank_info("de", "20000000")
        .expect("looked up")
        .expect("known");
    assert_eq!(bare.services.len(), 1);
    assert_eq!(bare.services[0].address, None);
    assert_eq!(bare.services[0].mode, None);
    let odd = session
        .bank_info("de", "30000000")
        .expect("looked up")
        .expect("known");
    let name = odd.name.expect("a name");
    assert!(name.contains('\u{FFFD}'), "{name}");
    assert_eq!(
        session.bank_info("de", "99999999").expect("looked up"),
        None
    );
    assert_eq!(
        session.bank_info("de", "bad\0code"),
        Err(SessionError::Argument)
    );
}

#[test]
fn setup_commands_reach_the_backend_with_their_options() {
    let _guard = lock();
    let (session, data_dir) = open("ok", "control", Some("1234"));
    let account = set_up_login(&session);
    let journal = journal(&data_dir);
    assert!(
        journal.contains(
            "control: adduser -N KassenSturz -b 37040044 -u made-up-login -t pintan -s https://fints.testbank.example/ --hbciversion=300\n"
        ),
        "{journal}"
    );
    assert!(journal.contains("control: getaccounts -u 1\n"), "{journal}");
    assert!(
        journal.contains("password: pin with 4 characters\n"),
        "{journal}"
    );
    assert!(!journal.contains("1234"), "the journal never holds the PIN");
    let accounts = session.accounts().expect("accounts");
    assert_eq!(accounts.len(), 2);
    assert_eq!(accounts[0].unique_id, account);
    assert_eq!(accounts[0].kind, AccountKind::Checking);
    assert_eq!(accounts[0].iban.as_deref(), Some("DE89370400440532013000"));
    assert_eq!(accounts[1].kind, AccountKind::CreditCard);
    assert_eq!(accounts[1].iban, None);
    assert_eq!(accounts[1].account_number.as_deref(), Some("1234567890"));
    // A command the backend doesn't know is its own error.
    assert_eq!(
        session.control("sendmoney", &[]),
        Err(SessionError::Library(1))
    );
    assert!(
        data_dir
            .join("settings6")
            .join("users")
            .join("00000001.conf")
            .is_file()
    );
    session.control("deluser", &["-u", "1"]).expect("deluser");
    assert!(
        !data_dir
            .join("settings6")
            .join("users")
            .join("00000001.conf")
            .exists()
    );
}

#[test]
fn a_fetch_delivers_balances_and_transactions_from_the_date_on() {
    let _guard = lock();
    let (session, data_dir) = open("ok", "fetch", Some("1234"));
    let account = set_up_login(&session);
    let record = session.fetch(account, None).expect("fetched");
    assert_eq!(record.accounts.len(), 1);
    let fetched = &record.accounts[0];
    assert_eq!(fetched.unique_id, account);
    assert_eq!(fetched.currency.as_deref(), Some("EUR"));
    assert_eq!(fetched.balances.len(), 2);
    assert_eq!(fetched.balances[0].kind, BalanceKind::Noted);
    assert_eq!(fetched.balances[0].amount.fraction, "373457/100");
    assert_eq!(fetched.balances[1].kind, BalanceKind::Booked);
    assert_eq!(fetched.balances[1].amount.fraction, "123457/100");
    assert_eq!(
        fetched.balances[1].date,
        Some(DateRecord {
            year: 2026,
            month: 10,
            day: 22
        })
    );
    assert_eq!(fetched.transactions.len(), 3);
    let first = &fetched.transactions[0];
    assert_eq!(first.kind, TransactionKind::Statement);
    assert_eq!(first.bank_id.as_deref(), Some("T-1"));
    assert_eq!(first.amount.fraction, "-4999/100");
    assert_eq!(first.amount.currency.as_deref(), Some("EUR"));
    assert_eq!(first.remote_name.as_deref(), Some("Supermarkt Nord"));
    assert_eq!(first.purpose.as_deref(), Some("Einkauf\nKarte 1234"));
    assert_eq!(
        fetched.transactions[2].kind,
        TransactionKind::NotedStatement
    );
    assert_eq!(record.messages, vec!["Hinweis: Auftrag ausgeführt"]);
    let handler = session.handler();
    assert_eq!(handler.requests, vec![PasswordKind::Pin, PasswordKind::Pin]);
    assert!(
        handler
            .lines
            .iter()
            .any(|(level, line)| *level == LogLevel::Warning && line.contains("made-up bank")),
        "{:?}",
        handler.lines
    );
    drop(handler);
    // From a date on: only what is on or after it.
    let record = session
        .fetch(
            account,
            Some(DateRecord {
                year: 2026,
                month: 10,
                day: 10,
            }),
        )
        .expect("fetched");
    let ids: Vec<_> = record.accounts[0]
        .transactions
        .iter()
        .map(|transaction| transaction.bank_id.clone())
        .collect();
    assert_eq!(ids, vec![Some("T-1".to_owned()), None]);
    let journal = journal(&data_dir);
    assert!(
        journal.contains(&format!("fetch: account {account} since -\n")),
        "{journal}"
    );
    assert!(
        journal.contains(&format!("fetch: account {account} since 2026-10-10\n")),
        "{journal}"
    );
    // An account the bank doesn't know: a message, no account.
    let record = session.fetch(999, None).expect("answered");
    assert!(record.accounts.is_empty());
    assert_eq!(record.messages, vec!["No such account"]);
}

#[test]
fn a_declined_pin_a_bad_pin_and_a_tan_request_stop_the_fetch() {
    let _guard = lock();
    let (session, data_dir) = open("ok", "declined", Some("1234"));
    let account = set_up_login(&session);
    clear(&session, None);
    assert_eq!(
        session.fetch(account, None),
        Err(SessionError::Declined(PasswordKind::Pin))
    );
    assert!(journal(&data_dir).contains("password: pin declined"));
    clear(&session, Some("0000"));
    stub::set_scenario("bad-pin");
    assert_eq!(
        session.fetch(account, None),
        Err(SessionError::Library(GWEN_ERROR_GENERIC))
    );
    let handler = session.handler();
    assert_eq!(handler.statuses, vec![PasswordStatus::Bad]);
    assert!(
        handler
            .lines
            .iter()
            .any(|(_, line)| line.contains("PIN wrong (9942)")),
        "{:?}",
        handler.lines
    );
    drop(handler);
    clear(&session, Some("1234"));
    stub::set_scenario("tan");
    assert_eq!(
        session.fetch(account, None),
        Err(SessionError::Declined(PasswordKind::Tan))
    );
    assert_eq!(
        session.handler().requests,
        vec![PasswordKind::Pin, PasswordKind::Tan]
    );
    stub::set_scenario("ok");
}

#[test]
fn a_tan_and_an_approval_are_asked_for_with_plain_text() {
    let _guard = lock();
    let (session, data_dir) = open("ok", "second-factor", Some("1234"));
    let account = set_up_login(&session);
    // A text TAN: the challenge comes without its HTML copy.
    stub::set_scenario("tan");
    session.handler().tan = Some("123456".to_owned());
    session.fetch(account, None).expect("the TAN is accepted");
    {
        let handler = session.handler();
        let request = &handler.tan_requests[0];
        assert_eq!(request.method, PasswordMethod::Text);
        assert_eq!(request.title, "Enter TAN");
        assert_eq!(
            request.text,
            "Challenge: type the TAN your app shows for job 4711"
        );
    }
    assert!(journal(&data_dir).contains("password: tan with 6 characters"));
    // A picture TAN is marked as such, so the app can say it can't show it.
    clear(&session, Some("1234"));
    stub::set_scenario("optical-tan");
    session.handler().tan = None;
    assert_eq!(
        session.fetch(account, None),
        Err(SessionError::Declined(PasswordKind::Tan))
    );
    assert_eq!(
        session.handler().tan_requests[0].method,
        PasswordMethod::Optical
    );
    // An approval in the bank's app: the first button goes on.
    clear(&session, Some("1234"));
    stub::set_scenario("approval");
    session.handler().button = Some(0);
    session.fetch(account, None).expect("approved");
    {
        let handler = session.handler();
        let question = &handler.questions[0];
        assert_eq!(question.level, QuestionLevel::Info);
        assert_eq!(question.title, "Decoupled Mode: Waiting for Approval");
        assert_eq!(
            question.text,
            "Your approval is required.\nPlease use your smart phone app."
        );
        assert_eq!(question.buttons, vec!["Approved", "Abort"]);
    }
    assert!(journal(&data_dir).contains("question: approval answered 1"));
    // "Abort", no answer and a button that isn't there all stop the job.
    for button in [Some(1), None, Some(5)] {
        clear(&session, Some("1234"));
        session.handler().button = button;
        assert_eq!(
            session.fetch(account, None),
            Err(SessionError::Library(GWEN_ERROR_USER_ABORTED)),
            "{button:?}"
        );
    }
    // During the setup too: the bank asks at the first login.
    clear(&session, Some("1234"));
    stub::set_scenario("setup-approval");
    session.handler().button = Some(0);
    session
        .control("getsysid", &["-u", "1"])
        .expect("approved at getsysid");
    assert_eq!(session.handler().questions.len(), 1);
    stub::set_scenario("ok");
}

/// Gives the handler a new PIN and a clean record, between the calls of
/// one test.
fn clear(session: &Session<Recorder>, pin: Option<&str>) {
    let mut handler = session.handler();
    handler.pin = pin.map(str::to_owned);
    handler.requests.clear();
    handler.tan_requests.clear();
    handler.questions.clear();
    handler.statuses.clear();
    handler.lines.clear();
}

#[test]
fn the_server_is_refused_when_its_certificate_is_not_fully_ok() {
    let _guard = lock();
    let (session, _) = open("ok", "bad-cert", Some("1234"));
    let account = set_up_login(&session);
    clear(&session, Some("1234"));
    stub::set_scenario("bad-cert");
    assert_eq!(
        session.fetch(account, None),
        Err(SessionError::UntrustedServer)
    );
    // Nothing was asked after the certificate was refused.
    assert!(session.handler().requests.is_empty());
    stub::set_scenario("ok");
}

#[test]
fn network_and_bank_errors_come_back_as_numbers_with_the_messages() {
    let _guard = lock();
    let (session, _) = open("ok", "unreachable", Some("1234"));
    let account = set_up_login(&session);
    clear(&session, Some("1234"));
    stub::set_scenario("unreachable");
    assert_eq!(
        session.fetch(account, None),
        Err(SessionError::Library(GWEN_ERROR_NET_UNREACHABLE))
    );
    assert!(
        session
            .handler()
            .lines
            .iter()
            .any(|(_, line)| line.contains("Could not connect")),
        "{:?}",
        session.handler().lines
    );
    // A setup command that needs the bank fails the same way; `adduser`
    // doesn't, since it only writes the settings.
    assert_eq!(
        session.control("getsysid", &["-u", "1"]),
        Err(SessionError::Library(GWEN_ERROR_NET_UNREACHABLE))
    );
    clear(&session, Some("1234"));
    stub::set_scenario("refused");
    assert_eq!(
        session.fetch(account, None),
        Err(SessionError::Library(GWEN_ERROR_GENERIC))
    );
    stub::set_scenario("ok");
}

#[test]
fn broken_data_is_copied_within_bounds_and_never_panics() {
    let _guard = lock();
    let (session, _) = open("ok", "broken", Some("1234"));
    let account = set_up_login(&session);
    for scenario in [
        "broken-amount",
        "broken-inexact",
        "broken-date",
        "broken-currency",
        "broken-balance",
        "empty",
    ] {
        stub::set_scenario(scenario);
        let record = session
            .fetch(account, None)
            .expect("the session copies what it gets");
        if scenario == "empty" {
            assert!(record.accounts.is_empty());
        } else {
            assert_eq!(record.accounts.len(), 1);
        }
    }
    stub::set_scenario("broken-text");
    let record = session.fetch(account, None).expect("fetched");
    let fetched = &record.accounts[0];
    assert_eq!(fetched.currency, None);
    let transaction = &fetched.transactions[0];
    let name = transaction.remote_name.as_deref().expect("a name");
    assert!(name.len() <= MAX_TEXT_BYTES, "{}", name.len());
    assert!(!name.is_empty());
    assert_eq!(
        transaction.purpose.as_deref(),
        Some("Caf\u{FFFD} am Markt\nTisch 4")
    );
    assert_eq!(transaction.amount.currency, None);
    stub::set_scenario("ok");
}

#[test]
fn only_one_session_is_open_per_thread() {
    let _guard = lock();
    let (session, _) = open("ok", "first", None);
    stub::set_scenario("ok");
    let library = stub::library();
    let second = Session::open_from(
        library,
        library,
        SessionOptions {
            app_name: "KassenSturz tests".to_owned(),
            data_dir: folder("second"),
            registration_key: None,
        },
        Recorder::default(),
    );
    assert!(matches!(second, Err(SessionError::AlreadyOpen)));
    drop(session);
    let third = Session::open_from(
        library,
        library,
        SessionOptions {
            app_name: "KassenSturz tests".to_owned(),
            data_dir: folder("third"),
            registration_key: None,
        },
        Recorder::default(),
    );
    assert!(third.is_ok());
}
