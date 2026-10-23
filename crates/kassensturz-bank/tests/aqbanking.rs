// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The real backend against the stub library (D-149, threat T-09): the
//! IBAN-first setup, the PIN's way through the session, the fetch into the
//! import pipeline, every broken answer as its error code, and AqBanking's
//! folder with no PIN in it and no message log left (T-01, D-139). No
//! real library and no network is involved (D-055).
//!
//! The stub keeps one state per process, so the tests run one at a time
//! under `LOCK`; each opens its own backend on its own folder. The PIN is
//! a made-up one for the made-up bank.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use kassensturz_aqbanking_sys::stub;
use kassensturz_bank::{
    AqBankingBank, Availability, Bank, BankAccount, BankAccountId, BankAccountKind, BankCode,
    BankError, CustomerId, Iban, KeychainPinStore, Library, Login, LoginId, LoginKind, LoginSetup,
    MemoryPinStore, Pin, PinMode, PinSession, Setup, Version, remove_login, setup_for, statement,
};
use kassensturz_core::{Account, AccountId, AccountKind, BankLink, CurrencyCode, Date};
use kassensturz_import::{EntryStatus, Limits};
use kassensturz_pipeline::{Choices, import_statements};
use kassensturz_platform::{Keychain, KeychainError, MemoryKeychain, Secret, SecretKey};
use kassensturz_store::Book;

static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The made-up PIN. Distinctive, so a file that holds it is found (T-01).
const PIN: &str = "97531";

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};

fn folder(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "kassensturz-bank-aqbanking-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&folder);
    folder
}

/// Opens the backend over the stub, with a made-up registration (D-058).
fn open(name: &str) -> AqBankingBank {
    stub::set_scenario("ok");
    let library = stub::library();
    AqBankingBank::open_from(
        library,
        library,
        &folder(name),
        Some("made-up-registration"),
    )
    .expect("the backend opens over the stub")
}

fn login_setup() -> LoginSetup {
    LoginSetup {
        bank: BankCode::new("DE", "37040044"),
        url: "https://fints.testbank.example/".to_owned(),
        kind: LoginKind::PinTan {
            customer_id: CustomerId::Separate,
        },
        login_name: "made-up-login".to_owned(),
        customer_id: Some("made-up-customer".to_owned()),
    }
}

/// A PIN session with the setup's PIN kept as `mode` says.
fn pins_with(mode: PinMode) -> PinSession {
    let mut pins = PinSession::new(Box::new(MemoryPinStore::new()));
    pins.provide(&LoginId::pending(), Pin::new(PIN.to_owned()), mode)
        .expect("provided");
    pins
}

fn journal_of(bank: &AqBankingBank) -> String {
    fs::read_to_string(bank.folder().join(stub::JOURNAL_FILE)).unwrap_or_default()
}

/// Every file below `folder`, with its text.
fn files_below(folder: &Path) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(folder) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(files_below(&path));
            } else {
                files.push((path.clone(), fs::read_to_string(&path).unwrap_or_default()));
            }
        }
    }
    files
}

/// Sets up the login and returns it with its accounts.
fn connect(bank: &mut AqBankingBank, pins: &mut PinSession) -> (LoginId, Vec<BankAccount>) {
    let login = bank.add_login(&login_setup(), pins).expect("login added");
    let accounts = bank.accounts(&login).expect("accounts");
    (login, accounts)
}

#[test]
fn the_availability_comes_from_the_libraries() {
    let _guard = lock();
    let bank = open("availability");
    assert_eq!(
        bank.availability(),
        Availability::Available {
            aqbanking: Version::new(6, 9, 2),
            gwenhywfar: Version::new(5, 14, 1),
        }
    );
    drop(bank);
    stub::set_scenario("old-gwenhywfar");
    let library = stub::library();
    let bank = AqBankingBank::open_from(library, library, &folder("old"), None)
        .expect("the backend opens without usable libraries");
    assert_eq!(
        bank.availability(),
        Availability::TooOld {
            library: Library::Gwenhywfar,
            found: Version::new(5, 13, 0),
            minimum: Version::new(5, 14, 1),
        }
    );
    assert!(matches!(bank.logins(), Err(BankError::TooOld { .. })));
    assert_eq!(
        bank.directory().lookup(&BankCode::new("DE", "37040044")),
        None
    );
    stub::set_scenario("ok");
    let missing = Path::new("/nonexistent/libgwenhywfar.so");
    let bank = AqBankingBank::open_from(missing, missing, &folder("missing"), None)
        .expect("the backend opens without the libraries");
    assert_eq!(bank.availability(), Availability::NotInstalled);
    assert_eq!(bank.logins(), Err(BankError::NotInstalled));
}

#[test]
fn a_login_is_set_up_with_the_backends_commands_and_removed_again() {
    let _guard = lock();
    let mut bank = open("setup");
    let mut pins = pins_with(PinMode::RememberUntilClose);
    assert_eq!(bank.logins().expect("logins"), vec![]);
    let (login, accounts) = connect(&mut bank, &mut pins);
    assert_eq!(login, LoginId::new("1"));
    let journal = journal_of(&bank);
    let expected = [
        "control: adduser -N KassenSturz -b 37040044 -u made-up-login -t pintan -s https://fints.testbank.example/ --hbciversion=300 -c made-up-customer",
        "control: getbankinfo -u 1",
        "control: getsysid -u 1",
        "password: pin with 5 characters",
        "control: getaccounts -u 1",
        "password: pin with 5 characters",
        "control: getaccsepa -u 1",
    ];
    let mut rest = journal.as_str();
    for line in expected {
        let found = rest
            .find(line)
            .unwrap_or_else(|| panic!("{line} after the previous lines in:\n{journal}"));
        rest = &rest[found + line.len()..];
    }
    assert_eq!(
        bank.logins().expect("logins"),
        vec![Login {
            id: login.clone(),
            bank: BankCode::new("DE", "37040044"),
            kind: LoginKind::PinTan {
                customer_id: CustomerId::Unknown,
            },
        }]
    );
    assert_eq!(accounts.len(), 2);
    assert_eq!(accounts[0].id, BankAccountId::new("1"));
    assert_eq!(accounts[0].login, login);
    assert_eq!(accounts[0].kind, BankAccountKind::Current);
    assert_eq!(accounts[0].iban.as_deref(), Some("DE89370400440532013000"));
    assert_eq!(accounts[0].number.as_deref(), Some("0532013000"));
    assert_eq!(accounts[0].name.as_deref(), Some("Girokonto"));
    assert_eq!(accounts[1].kind, BankAccountKind::CreditCard);
    assert_eq!(accounts[1].iban, None);
    assert_eq!(accounts[1].number.as_deref(), Some("1234567890"));
    // The PIN moved from the pending login to the new one (D-057).
    assert!(!pins.is_remembered(&LoginId::pending()));
    assert!(pins.is_remembered(&login));
    // Nothing in AqBanking's folder holds the PIN (T-01), and the message
    // log the stub wrote is gone (D-139).
    let files = files_below(bank.folder());
    assert!(!files.is_empty());
    for (path, text) in &files {
        assert!(!text.contains(PIN), "{} holds the PIN", path.display());
    }
    assert!(
        files
            .iter()
            .all(|(path, _)| !path.components().any(|part| part.as_os_str() == "logs")),
        "{files:?}"
    );
    // The login name stays in AqBanking's file and nowhere in what the app
    // sees (T-19).
    assert!(
        files
            .iter()
            .any(|(path, text)| path.ends_with("00000001.conf") && text.contains("made-up-login"))
    );
    assert!(!format!("{:?}", bank.logins()).contains("made-up"));
    assert_eq!(
        bank.accounts(&LoginId::new("2")),
        Err(BankError::UnknownLogin(LoginId::new("2")))
    );
    remove_login(&mut bank, &login, &mut pins).expect("removed");
    assert_eq!(bank.logins().expect("logins"), vec![]);
    assert!(!pins.is_remembered(&login));
    assert_eq!(
        bank.accounts(&login),
        Err(BankError::UnknownLogin(login.clone()))
    );
    assert!(journal_of(&bank).contains("control: deluser -u 1\n"));
    assert_eq!(
        bank.remove_login(&login),
        Err(BankError::UnknownLogin(login))
    );
}

#[test]
fn a_saved_pin_moves_to_the_new_logins_key_in_the_keychain() {
    let _guard = lock();
    /// The fake keychain, shared so the test can look inside it.
    #[derive(Clone)]
    struct Shared(Arc<MemoryKeychain>);
    impl Keychain for Shared {
        fn store(&self, key: &SecretKey, secret: &Secret) -> Result<(), KeychainError> {
            self.0.store(key, secret)
        }
        fn read(&self, key: &SecretKey) -> Result<Option<Secret>, KeychainError> {
            self.0.read(key)
        }
        fn delete(&self, key: &SecretKey) -> Result<(), KeychainError> {
            self.0.delete(key)
        }
    }
    let keychain = Shared(Arc::new(MemoryKeychain::new()));
    let mut pins = PinSession::new(Box::new(KeychainPinStore::new(Box::new(keychain.clone()))));
    pins.provide(
        &LoginId::pending(),
        Pin::new(PIN.to_owned()),
        PinMode::SaveInKeychain,
    )
    .expect("provided");
    let mut bank = open("keychain");
    let (login, _) = connect(&mut bank, &mut pins);
    let key = |id: &str| SecretKey::new("bank-pin", id).expect("a key");
    assert!(keychain.0.contains(&key(login.as_str())));
    assert!(!keychain.0.contains(&key("pending")));
    assert_eq!(keychain.0.len(), 1);
    // A new run reads it back and fetches without asking.
    let mut next_run = PinSession::new(Box::new(KeychainPinStore::new(Box::new(keychain.clone()))));
    let account = bank.accounts(&login).expect("accounts")[0].id.clone();
    bank.fetch(&account, None, &mut next_run).expect("fetched");
    remove_login(&mut bank, &login, &mut next_run).expect("removed");
    assert!(keychain.0.is_empty());
}

#[test]
fn a_setup_that_fails_keeps_nothing_not_even_the_pin() {
    let _guard = lock();
    // No PIN: the bank asks, the app has to ask the user.
    let mut bank = open("no-pin");
    let mut pins = PinSession::new(Box::new(MemoryPinStore::new()));
    assert_eq!(
        bank.add_login(&login_setup(), &mut pins),
        Err(BankError::PinNeeded)
    );
    assert_eq!(bank.logins().expect("logins"), vec![]);
    let journal = journal_of(&bank);
    assert!(journal.contains("password: pin declined"), "{journal}");
    assert!(journal.contains("control: deluser -u 1\n"), "{journal}");
    assert!(
        !bank
            .folder()
            .join("settings6")
            .join("users")
            .join("00000001.conf")
            .exists()
    );
    // A wrong PIN: forgotten, also the saved one (T-05).
    let mut pins = pins_with(PinMode::SaveInKeychain);
    stub::set_scenario("bad-pin");
    assert_eq!(
        bank.add_login(&login_setup(), &mut pins),
        Err(BankError::PinRejected)
    );
    assert!(!pins.is_remembered(&LoginId::pending()));
    assert_eq!(pins.is_saved(&LoginId::pending()), Ok(false));
    assert_eq!(bank.logins().expect("logins"), vec![]);
    // The bank can't be reached, or its certificate isn't right.
    stub::set_scenario("unreachable");
    let mut pins = pins_with(PinMode::RememberUntilClose);
    assert_eq!(
        bank.add_login(&login_setup(), &mut pins),
        Err(BankError::Unreachable)
    );
    assert!(!pins.is_remembered(&LoginId::pending()));
    stub::set_scenario("bad-cert");
    let mut pins = pins_with(PinMode::RememberUntilClose);
    assert_eq!(
        bank.add_login(&login_setup(), &mut pins),
        Err(BankError::UntrustedServer)
    );
    stub::set_scenario("ok");
    assert_eq!(bank.logins().expect("logins"), vec![]);
    // A login name AqBanking can't take.
    let mut pins = pins_with(PinMode::RememberUntilClose);
    let mut setup = login_setup();
    setup.login_name = "no\0zero".to_owned();
    assert_eq!(
        bank.add_login(&setup, &mut pins),
        Err(BankError::SetupFailed { text: None })
    );
    // A chip card is set up in the dialog, not here: nothing runs.
    let mut setup = login_setup();
    setup.kind = LoginKind::ChipCard;
    let before = journal_of(&bank);
    assert_eq!(
        bank.add_login(&setup, &mut pins),
        Err(BankError::NeedsSetupDialog)
    );
    assert_eq!(journal_of(&bank), before);
    assert!(pins.is_remembered(&LoginId::pending()));
}

#[test]
fn without_a_registration_no_bank_is_contacted() {
    let _guard = lock();
    stub::set_scenario("ok");
    let library = stub::library();
    let mut bank =
        AqBankingBank::open_from(library, library, &folder("unregistered"), None).expect("opens");
    let mut pins = pins_with(PinMode::RememberUntilClose);
    assert_eq!(
        bank.add_login(&login_setup(), &mut pins),
        Err(BankError::NotRegistered)
    );
    assert!(
        !journal_of(&bank).contains("control:"),
        "{}",
        journal_of(&bank)
    );
    // Looking a bank up needs no bank contact.
    assert!(
        bank.directory()
            .lookup(&BankCode::new("DE", "37040044"))
            .is_some()
    );
}

/// A book with an account for each of the stub's bank accounts, as the
/// link screen makes them (M4.4a).
fn book_for(bank: &AqBankingBank, accounts: &[BankAccount]) -> (Book, AccountId, AccountId) {
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("currency used");
    let mut add = |name: &str, kind: AccountKind, account: &BankAccount| {
        let id = AccountId::generate();
        book.add_account(&Account {
            id,
            name: name.into(),
            kind,
            currency: EUR,
            account_type: None,
            parent: None,
            closed: false,
            number: account.iban.clone(),
            notes: None,
            institution: None,
        })
        .expect("account added");
        let login = bank
            .logins()
            .expect("logins")
            .into_iter()
            .find(|login| login.id == account.login)
            .expect("the account's login");
        book.link_bank_account(&BankLink {
            account: id,
            iban: account.iban.clone(),
            bank_code: Some(login.bank.code().to_owned()),
            number: account.number.clone(),
            last_fetched_on: None,
        })
        .expect("linked");
        id
    };
    let checking = add("Checking account", AccountKind::Asset, &accounts[0]);
    let card = add("Credit card", AccountKind::Liability, &accounts[1]);
    (book, checking, card)
}

fn october(day: i8) -> Date {
    Date::new(2026, 10, day).expect("a valid date")
}

#[test]
fn a_fetch_is_imported_like_a_file() {
    let _guard = lock();
    let mut bank = open("fetch");
    let mut pins = pins_with(PinMode::RememberUntilClose);
    let (_, accounts) = connect(&mut bank, &mut pins);
    let (mut book, checking, _) = book_for(&bank, &accounts);
    let fetched = bank
        .fetch(&accounts[0].id, None, &mut pins)
        .expect("fetched");
    assert_eq!(fetched.account, accounts[0]);
    let balance = fetched.balance.as_ref().expect("the booked balance");
    assert_eq!(balance.amount.fraction, "123457/100");
    assert_eq!(balance.amount.currency.as_deref(), Some("EUR"));
    assert_eq!(balance.date, Some(october(22)));
    assert_eq!(fetched.transactions.len(), 3);
    let first = &fetched.transactions[0];
    assert!(first.booked);
    assert_eq!(first.bank_id.as_deref(), Some("T-1"));
    assert_eq!(first.booking_date, Some(october(15)));
    assert_eq!(first.amount.fraction, "-4999/100");
    assert_eq!(first.other_party_name.as_deref(), Some("Supermarkt Nord"));
    assert_eq!(first.purpose, vec!["Einkauf", "Karte 1234"]);
    assert!(!fetched.transactions[2].booked);
    let journal = journal_of(&bank);
    assert!(journal.contains("fetch: account 1 since -\n"), "{journal}");
    // Into the pipeline, as the fetch screen will (M4.7).
    let statement = statement(&fetched, &Limits::default()).expect("a statement");
    assert_eq!(statement.entries.len(), 3);
    assert_eq!(statement.entries[2].status, EntryStatus::Pending);
    let report = import_statements(
        &mut book,
        checking,
        std::slice::from_ref(&statement),
        &Choices::default(),
        None,
    )
    .expect("imported");
    let summary = report.summary();
    assert_eq!((summary.imported, summary.pending), (2, 1));
    assert_eq!(report.balances.len(), 1);
    assert_eq!(report.balances[0].date, october(22));
    assert_eq!(
        report.balances[0].bank,
        kassensturz_core::Money::from_minor_units(123_457, EUR)
    );
    let rent = book
        .transaction(report.imported()[1])
        .expect("the rent transaction");
    assert_eq!(rent.booking_date(), october(1));
    assert_eq!(rent.payee(), Some("Vermieter"));
    // From a date on: the bank is asked for that date and sends less.
    let again = bank
        .fetch(&accounts[0].id, Some(october(10)), &mut pins)
        .expect("fetched");
    assert_eq!(again.transactions.len(), 2);
    assert!(journal_of(&bank).contains("fetch: account 1 since 2026-10-10\n"));
    // The credit card is fetched the same way (D-029).
    let card = bank
        .fetch(&accounts[1].id, None, &mut pins)
        .expect("fetched");
    assert_eq!(card.account.kind, BankAccountKind::CreditCard);
    assert!(journal_of(&bank).contains("fetch: account 2 since -\n"));
    // Accounts the backend doesn't know.
    for id in ["999", "abc", ""] {
        assert_eq!(
            bank.fetch(&BankAccountId::new(id), None, &mut pins),
            Err(BankError::UnknownAccount(BankAccountId::new(id)))
        );
    }
    // With the PIN kept only for one session, the next fetch asks again.
    let mut once = pins_once(&mut bank);
    assert_eq!(
        bank.fetch(&accounts[0].id, None, &mut once),
        Err(BankError::PinNeeded)
    );
}

/// A session whose PIN was given for one bank session, which the setup
/// used up.
fn pins_once(bank: &mut AqBankingBank) -> PinSession {
    let mut pins = pins_with(PinMode::AskEachTime);
    let _ = bank.add_login(&login_setup(), &mut pins);
    pins
}

#[test]
fn fetch_errors_are_error_codes_and_a_wrong_pin_is_forgotten() {
    let _guard = lock();
    let mut bank = open("fetch-errors");
    let mut pins = pins_with(PinMode::SaveInKeychain);
    let (login, accounts) = connect(&mut bank, &mut pins);
    let account = accounts[0].id.clone();
    for (scenario, error) in [
        ("unreachable", BankError::Unreachable),
        (
            "refused",
            BankError::Refused {
                text: Some("Auftrag abgelehnt: Konto gesperrt (9010)".to_owned()),
            },
        ),
        ("tan", BankError::TanNeeded),
        ("bad-cert", BankError::UntrustedServer),
    ] {
        stub::set_scenario(scenario);
        assert_eq!(
            bank.fetch(&account, None, &mut pins),
            Err(error),
            "{scenario}"
        );
        assert!(pins.is_remembered(&login), "{scenario} keeps the PIN");
    }
    stub::set_scenario("bad-pin");
    assert_eq!(
        bank.fetch(&account, None, &mut pins),
        Err(BankError::PinRejected)
    );
    assert!(!pins.is_remembered(&login));
    assert_eq!(pins.is_saved(&login), Ok(false));
    stub::set_scenario("ok");
    assert_eq!(
        bank.fetch(&account, None, &mut pins),
        Err(BankError::PinNeeded)
    );
    // No message log is left after a failed call either (D-139).
    assert!(
        !bank
            .folder()
            .join("backends")
            .join("aqhbci")
            .join("data")
            .join("banks")
            .join("de")
            .join("37040044")
            .join("logs")
            .exists()
    );
}

#[test]
fn broken_answers_are_refused_never_rounded_or_guessed() {
    let _guard = lock();
    let mut bank = open("broken");
    let mut pins = pins_with(PinMode::RememberUntilClose);
    let (_, accounts) = connect(&mut bank, &mut pins);
    let account = accounts[0].id.clone();
    let limits = Limits::default();
    for (scenario, error) in [
        (
            "broken-amount",
            BankError::MalformedAmount {
                amount: "abc".to_owned(),
            },
        ),
        (
            "broken-inexact",
            BankError::InexactAmount {
                amount: "1/3".to_owned(),
            },
        ),
        ("broken-date", BankError::MissingDate),
        (
            "broken-currency",
            BankError::CurrencyMismatch {
                expected: EUR,
                found: CurrencyCode::new("USD").expect("a code"),
            },
        ),
        (
            "broken-balance",
            BankError::InexactAmount {
                amount: "12345/1000".to_owned(),
            },
        ),
    ] {
        stub::set_scenario(scenario);
        let fetched = bank
            .fetch(&account, None, &mut pins)
            .expect("the backend hands on what the bank sent");
        assert_eq!(statement(&fetched, &limits), Err(error), "{scenario}");
    }
    // Over-long and non-UTF-8 text arrives bounded and readable, and
    // amounts without a currency take the account's, which the backend
    // knows from AqBanking's account record (D-141).
    stub::set_scenario("broken-text");
    let fetched = bank.fetch(&account, None, &mut pins).expect("fetched");
    assert_eq!(fetched.transactions[0].amount.currency, None);
    assert_eq!(fetched.account.currency, Some(EUR));
    let name = fetched.transactions[0]
        .other_party_name
        .as_deref()
        .expect("a name");
    assert!(name.len() <= kassensturz_aqbanking_sys::MAX_TEXT_BYTES);
    assert_eq!(
        fetched.transactions[0].purpose,
        vec!["Caf\u{FFFD} am Markt", "Tisch 4"]
    );
    let readable = statement(&fetched, &limits).expect("a statement");
    assert_eq!(readable.currency.code(), EUR);
    assert_eq!(
        readable.entries[0].remittance_information.as_deref(),
        Some("Caf\u{FFFD} am Markt\nTisch 4")
    );
    // An answer without anything: a statement with nothing in it.
    stub::set_scenario("empty");
    let fetched = bank.fetch(&account, None, &mut pins).expect("fetched");
    assert_eq!(fetched.balance, None);
    assert!(fetched.transactions.is_empty());
    let statement = statement(&fetched, &limits).expect("a statement");
    assert!(statement.entries.is_empty());
    stub::set_scenario("ok");
}

#[test]
fn the_directory_is_aqbankings_bank_information_behind_the_hand_checked_list() {
    let _guard = lock();
    let bank = open("directory");
    let directory = bank.directory();
    let testbank = directory
        .lookup(&BankCode::new("DE", "37040044"))
        .expect("known to AqBanking");
    assert_eq!(testbank.name, "Testbank");
    let server = testbank.server.expect("a server");
    assert_eq!(server.url, "https://fints.testbank.example/");
    assert_eq!(
        server.login_kinds,
        Some(vec![LoginKind::PinTan {
            customer_id: CustomerId::Unknown
        }])
    );
    // The IBAN-first flow (Q-035) from an IBAN at that bank.
    let iban = Iban::parse("DE89 3704 0044 0532 0130 00").expect("an IBAN");
    assert!(matches!(
        setup_for(&iban, directory),
        Setup::Ready { account_number: Some(number), .. } if number == "0532013000"
    ));
    // Each gap in AqBanking's data is its own state.
    let no_services = directory
        .lookup(&BankCode::new("DE", "10000000"))
        .expect("known");
    assert_eq!(no_services.server, None);
    let no_address = directory
        .lookup(&BankCode::new("DE", "20000000"))
        .expect("known");
    assert_eq!(no_address.server, None);
    let odd_name = directory
        .lookup(&BankCode::new("DE", "30000000"))
        .expect("known");
    assert!(odd_name.name.contains('\u{FFFD}'), "{}", odd_name.name);
    assert_eq!(directory.lookup(&BankCode::new("DE", "99999999")), None);
    // The hand-checked list wins, for a bank AqBanking's data lacks too.
    let ing = directory
        .lookup(&BankCode::new("DE", "50010517"))
        .expect("in the hand-checked list");
    assert_eq!(ing.name, "ING");
    assert_eq!(
        ing.server.expect("a server").url,
        "https://fints.ing.de/fints"
    );
}

#[test]
fn only_one_backend_runs_per_thread() {
    let _guard = lock();
    let bank = open("first");
    stub::set_scenario("ok");
    let library = stub::library();
    assert!(matches!(
        AqBankingBank::open_from(library, library, &folder("second"), None),
        Err(BankError::Busy)
    ));
    drop(bank);
    assert!(AqBankingBank::open_from(library, library, &folder("third"), None).is_ok());
}
