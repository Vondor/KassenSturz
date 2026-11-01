// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The bank connection behind the bank screens (M4.6, D-151): setting up a
//! bank login from an IBAN, the TANs and approvals the bank asks for while
//! it is set up, removing a login and its PIN, and linking the bank's
//! accounts to the book's.
//!
//! **One worker thread** holds the bank backend and the PINs. AqBanking's
//! calls block until the bank answers, and in the middle of one the bank
//! may ask for a TAN or an approval in its app, which the user answers in a
//! dialog; so the calls run on their own thread while the window stays
//! responsive, and the backend is opened there, since AqBanking's session
//! is bound to the thread that opened it (D-149). The worker asks through
//! [`Event`]s and waits for the [`Answer`]; `bank_connection_model.rs`
//! shows them as QML dialogs. Quick calls that only read AqBanking's own
//! files (the bank's details, the logins, removing one) wait for the
//! worker on the GUI thread, and are refused with `bank.busy` while a call
//! that may ask the user runs, so the two never wait on each other.
//!
//! Fetching the statements of the accounts linked to a bank account (M4.7,
//! D-153) is such a call: [`fetch`] runs on the worker, may ask for a PIN
//! ([`Event::Pin`]) and, through the backend's dialogs, for a TAN, and puts
//! the converted statements into `bank_fetch.rs` for the import screens.
//!
//! Without AqBanking, or with `KASSENSTURZ_FAKE_BANK=1` (for trying the
//! screens, marked in them), the worker holds the fake bank, which asks
//! for a TAN at every connection (D-055). With `KASSENSTURZ_FAKE_BANK=keychain`
//! it is the same fake bank, but a PIN the user saves goes into the real
//! system keychain, filed under [`FAKE_BANK_SERVICE`], so saving, reading
//! back and removing a PIN can be tried with the desktop's own keychain
//! before KassenSturz may talk to a real bank (D-159). With `1`, a PIN goes
//! to a KeePassXC database when the user chose one in Settings, so that
//! choice can be tried with the fake bank too (D-167).
//!
//! A build without a FinTS registration of its own refuses to contact a
//! bank (D-058). Started with `KASSENSTURZ_FINTS_ASK=1`, it asks the person
//! testing it for the product registration number instead, the first time a
//! bank is to be contacted ([`Event::Registration`]), and keeps the number
//! in memory until the app closes (D-260).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

use kassensturz_bank::{
    AqBankingBank, Availability, Bank, BankAccount, BankAccountKind, BankCode, BankDialogs,
    BankError, BankQuestion, CustomerId, FINTS_REGISTRATION_KEY, FakeBank, Iban, KeychainPinStore,
    Login, LoginId, LoginKind, LoginSetup, MemoryPinStore, NoPinStore, Pin, PinAsker, PinMode,
    PinSession, PinStore, QuestionLevel, RegistrationKey, SecondFactor, Setup, Tan, TanRequest,
    fetch_linked, find_bank_account, names_account, remove_login, setup_for,
};
use kassensturz_core::{AccountId, BankLink, Date, ErrorParam, Report};
use kassensturz_import::{EntryStatus, Limits, Statement};
use kassensturz_platform::{APP_ID, Keychain, KeychainPlace, SecretStorage};
use kassensturz_store::Book;

use crate::bank_fetch::{self, Fetch};
use crate::errors::{ErrorText, report_text};
use crate::format::Locale;

/// The environment variable that puts the fake bank behind the screens:
/// `1` with the PINs in memory, [`FAKE_BANK_WITH_KEYCHAIN`] with the PINs in
/// the system keychain.
pub const FAKE_BANK_VARIABLE: &str = "KASSENSTURZ_FAKE_BANK";

/// The value of [`FAKE_BANK_VARIABLE`] that saves the fake bank's PINs in
/// the system keychain (D-159).
pub const FAKE_BANK_WITH_KEYCHAIN: &str = "keychain";

/// The name the keychain files the fake bank's PINs under, apart from
/// KassenSturz's own [`kassensturz_platform::APP_ID`], so a made-up PIN is
/// never mistaken for a real one and the user sees in the keychain's
/// manager that it belongs to the fake bank (D-159).
pub const FAKE_BANK_SERVICE: &str = "io.github.vondor.KassenSturz.FakeBank";

/// The environment variable that lets the person testing a build without
/// a FinTS registration of its own type the product registration number
/// (D-260): with `1`, the first call that would contact a bank asks for
/// it, and it is kept in memory until the app closes. The registration
/// office's terms keep the number from being passed on, so it is never
/// part of the source, the build, a setting or a file.
pub const ASK_REGISTRATION_VARIABLE: &str = "KASSENSTURZ_FINTS_ASK";

/// How many of the fake bank's login IDs are looked up in the keychain
/// when the app starts: more than anyone sets up while trying it out.
const FAKE_LOGINS_LOOKED_UP: u32 = 9;

/// Makes the store for the PINs the user chooses to save, for each place
/// they can choose in Settings (D-163).
pub type PinStores = Box<dyn Fn(SecretStorage) -> Box<dyn PinStore> + Send>;

/// The stores in the keychain `keychain` gives for each place, or none
/// where it gives no keychain ("nowhere").
pub fn keychain_stores(
    keychain: impl Fn(SecretStorage) -> Option<Box<dyn Keychain>> + Send + 'static,
) -> PinStores {
    Box::new(move |storage| match keychain(storage) {
        Some(keychain) => Box::new(KeychainPinStore::new(keychain)) as Box<dyn PinStore>,
        None => Box::new(NoPinStore),
    })
}

/// The fake bank's PINs saved in memory only (`KASSENSTURZ_FAKE_BANK=1`),
/// or none for "nowhere".
fn memory_stores() -> PinStores {
    Box::new(|storage| match storage {
        SecretStorage::Nowhere => Box::new(NoPinStore) as Box<dyn PinStore>,
        _ => Box::new(MemoryPinStore::new()),
    })
}

/// The fake bank's PINs with `KASSENSTURZ_FAKE_BANK=1`: in a KeePassXC
/// database when the user chose one, filed under [`FAKE_BANK_SERVICE`]
/// (D-167), else as [`memory_stores`] keeps them; the system keychain only
/// with [`FAKE_BANK_WITH_KEYCHAIN`] (D-159). `keychain` gives the
/// KeePassXC keychain: in the app [`SecretStorage::keychain`].
fn fake_stores(
    keychain: impl Fn(SecretStorage) -> Option<Box<dyn Keychain>> + Send + 'static,
) -> PinStores {
    let keychains = keychain_stores(keychain);
    let memory = memory_stores();
    Box::new(move |storage| match storage {
        SecretStorage::KeePassXc => keychains(storage),
        _ => memory(storage),
    })
}

/// What the worker holds: the backend, or why it couldn't be opened, and
/// the PINs of this run of the app (D-057).
pub struct Connection {
    bank: Result<Box<dyn Bank>, BankError>,
    /// The PINs, remembered or through the keychain.
    pub pins: PinSession,
    /// Makes the store when the user chooses another place for the PINs.
    pin_stores: PinStores,
    /// Whether the backend may talk to a bank: this build has a FinTS
    /// registration (D-058), the backend is the fake bank, or the user
    /// typed the number in this run of the app (D-260).
    pub registered: bool,
    /// Whether the user is asked for the registration number when there
    /// is none ([`ASK_REGISTRATION_VARIABLE`], D-260).
    pub ask_registration: bool,
    /// Whether the backend is the fake bank, which the screens say.
    pub fake: bool,
    /// How to ask the screens something while a call runs; set by the
    /// worker, so a connection used without one can only fail with
    /// `bank.aborted` where the user would be asked.
    pub screens: Option<Screens>,
}

impl Connection {
    /// The real backend in `folder` (D-139), with the saved PINs in
    /// `storage`, filed under the app ID (D-163).
    pub fn real(folder: &std::path::Path, storage: SecretStorage) -> Self {
        let pin_stores = keychain_stores(|storage| storage.keychain(APP_ID));
        Self {
            bank: AqBankingBank::open(folder).map(|bank| Box::new(bank) as Box<dyn Bank>),
            pins: PinSession::new(pin_stores(storage)),
            pin_stores,
            registered: FINTS_REGISTRATION_KEY.is_some(),
            ask_registration: false,
            fake: false,
            screens: None,
        }
    }

    /// The fake bank, asking for a TAN at every connection, with `pins`.
    pub fn fake(bank: FakeBank, pins: PinSession) -> Self {
        Self {
            bank: Ok(Box::new(bank)),
            pins,
            pin_stores: memory_stores(),
            registered: true,
            ask_registration: false,
            fake: true,
            screens: None,
        }
    }

    /// The fake `bank` with its PINs saved in the store `pin_stores` makes
    /// for `storage` (D-159, D-163): in the app the keychain the user chose,
    /// under [`FAKE_BANK_SERVICE`]; a
    /// [`kassensturz_platform::MemoryKeychain`] in tests.
    ///
    /// The fake bank forgets its logins when the app closes, so a login
    /// whose PIN the keychain still holds from an earlier run is added
    /// again; the saved PIN is then read back from the keychain the first
    /// time the bank asks for it. A keychain that can't be read now adds
    /// none, and says so when a PIN is saved.
    pub fn fake_with_keychain(
        mut bank: FakeBank,
        pin_stores: PinStores,
        storage: SecretStorage,
    ) -> Self {
        let pins = PinSession::new(pin_stores(storage));
        for n in 1..=FAKE_LOGINS_LOOKED_UP {
            let login = FakeBank::login_id(n);
            if pins.is_saved(&login) == Ok(true) {
                bank = bank.with_login(login);
            }
        }
        Self {
            pin_stores,
            ..Self::fake(bank, pins)
        }
    }

    /// The connection the app uses: the fake bank when asked for, else
    /// AqBanking in KassenSturz's data folder, or `bank.folder` without
    /// one. Saved PINs go to `storage`, the user's choice (D-163).
    pub fn for_app(folder: Option<PathBuf>, storage: SecretStorage) -> Self {
        let fake = std::env::var_os(FAKE_BANK_VARIABLE);
        let wants_fake = fake.as_ref().is_some_and(|value| value == "1");
        let wants_keychain = fake
            .as_ref()
            .is_some_and(|value| value == FAKE_BANK_WITH_KEYCHAIN);
        match folder {
            _ if wants_fake => {
                let pin_stores = fake_stores(|storage| storage.keychain(FAKE_BANK_SERVICE));
                let pins = PinSession::new(pin_stores(storage));
                Self {
                    pin_stores,
                    ..Self::fake(FakeBank::new().with_second_factor(SecondFactor::Tan), pins)
                }
            }
            _ if wants_keychain => Self::fake_with_keychain(
                FakeBank::new().with_second_factor(SecondFactor::Tan),
                keychain_stores(|storage| storage.keychain(FAKE_BANK_SERVICE)),
                storage,
            ),
            Some(folder) => Self {
                ask_registration: std::env::var_os(ASK_REGISTRATION_VARIABLE)
                    .is_some_and(|value| value == "1"),
                ..Self::real(&folder.join("aqbanking"), storage)
            },
            // Without a data folder AqBanking has nowhere to keep its
            // settings: the screens say so.
            None => Self {
                bank: Err(BankError::Folder),
                pins: PinSession::new(memory_stores()(storage)),
                pin_stores: memory_stores(),
                registered: FINTS_REGISTRATION_KEY.is_some(),
                ask_registration: false,
                fake: false,
                screens: None,
            },
        }
    }

    /// The backend, or why there is none.
    pub fn bank(&mut self) -> Result<&mut dyn Bank, BankError> {
        match &mut self.bank {
            Ok(bank) => Ok(bank.as_mut()),
            Err(error) => Err(error.clone()),
        }
    }

    /// Whether bank connections can be used, as the screens say it.
    pub fn status(&mut self) -> Status {
        // Asking for the number comes when a bank is contacted.
        let registered = self.registered || self.ask_registration;
        let fake = self.fake;
        match self.bank() {
            Err(error) => Status::Unusable(error),
            Ok(bank) => match bank.availability() {
                Availability::Available { .. } if !registered => {
                    Status::Unusable(BankError::NotRegistered)
                }
                Availability::Available { .. } => Status::Ready { fake },
                other => Status::Unusable(other.require().err().unwrap_or(BankError::NotInstalled)),
            },
        }
    }
}

/// Whether bank connections can be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// Yes; `fake` when it is the fake bank.
    Ready {
        /// The fake bank stands in for a real one.
        fake: bool,
    },
    /// No, and why: not installed, too old, no registration (D-054,
    /// D-058).
    Unusable(BankError),
}

/// What the worker asks the screens while a call runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The bank asks for a TAN.
    Tan(TanRequest),
    /// The bank asks a question, e.g. to approve in its app.
    Question(BankQuestion),
    /// The bank needs the PIN of a login, and none is remembered or saved
    /// (or the bank called the last one wrong).
    Pin(PinRequest),
    /// A bank is to be contacted, and the user is asked for the FinTS
    /// product registration number, which this build doesn't have
    /// (D-260).
    Registration(RegistrationRequest),
    /// A call that ran in the background ended.
    SetupDone(Result<SetupDone, ErrorText>),
    /// The fetch has reached the account with this ID (M4.7).
    FetchStarted(String),
    /// A fetch ended: what came of each account, or why none was tried.
    FetchDone(Result<Vec<FetchOutcome>, ErrorText>),
    /// *Update all accounts* ended its fetch (M6d.7): each account's
    /// statement or why it has none, for the GUI thread to book.
    UpdateAllDone(Result<Vec<UpdateFetch>, ErrorText>),
    /// Choosing the place `storage` for saved PINs, or getting it ready
    /// again, ended (D-167): what it did, or why nothing changed.
    PinStorageDone {
        storage: SecretStorage,
        result: Result<PinStorageChange, ErrorText>,
    },
}

/// What the PIN dialog says: which bank asks, and whether the bank has just
/// called the PIN typed before wrong, so the user checks it before the
/// bank blocks the login (threat T-05).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PinRequest {
    /// The bank's name, or its code when no source knows it.
    pub bank: String,
    /// The bank called the last PIN wrong.
    pub rejected: bool,
}

/// What the registration dialog says: whether the number typed before
/// wasn't 25 letters and digits, so it is asked again (D-260).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistrationRequest {
    /// The number typed before wasn't 25 letters and digits.
    pub malformed: bool,
}

/// The user's answer to an [`Event`].
pub enum Answer {
    /// The TAN typed, or `None` for "Cancel".
    Tan(Option<Tan>),
    /// The button chosen, or `None` when the dialog was closed.
    Button(Option<usize>),
    /// The PIN typed and how long to keep it, or `None` for "Cancel".
    Pin(Option<(Pin, PinMode)>),
    /// The registration number typed, or `None` for "Cancel" (D-260).
    /// Read only by [`RegistrationKey::parse`], which wipes it.
    Registration(Option<String>),
}

impl std::fmt::Debug for Answer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tan(tan) => f.debug_tuple("Tan").field(tan).finish(),
            Self::Button(button) => f.debug_tuple("Button").field(button).finish(),
            Self::Pin(pin) => f.debug_tuple("Pin").field(pin).finish(),
            Self::Registration(typed) => f
                .debug_tuple("Registration")
                .field(&typed.as_ref().map(|_| "…"))
                .finish(),
        }
    }
}

/// The screens' ear: how the worker tells them something, from its thread.
pub type Notify = Arc<dyn Fn(Event) + Send + Sync>;

/// How the worker asks the screens something while a call runs: it tells
/// them through `notify` and waits for the answer.
#[derive(Clone)]
pub struct Screens {
    notify: Notify,
    answers: Arc<Mutex<Receiver<Answer>>>,
}

impl Screens {
    /// Tells the screens `event` and waits for the answer `pick` takes; an
    /// answer meant for another question, which came late, is dropped.
    /// `None` when the screens are gone.
    fn ask<T>(&self, event: Event, pick: impl Fn(Answer) -> Result<T, Answer>) -> Option<T> {
        (self.notify)(event);
        let answers = self.answers.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            match answers.recv() {
                Ok(answer) => {
                    if let Ok(picked) = pick(answer) {
                        return Some(picked);
                    }
                }
                Err(_) => return None,
            }
        }
    }

    /// Tells the screens `event`; nothing is waited for.
    fn tell(&self, event: Event) {
        (self.notify)(event);
    }
}

/// The dialogs the worker gives the backend: each request becomes an
/// [`Event`], and the worker waits for the [`Answer`].
struct AskTheScreens {
    screens: Screens,
}

impl BankDialogs for AskTheScreens {
    fn tan(&mut self, request: &TanRequest) -> Option<Tan> {
        self.screens
            .ask(Event::Tan(request.clone()), |answer| match answer {
                Answer::Tan(tan) => Ok(tan),
                other => Err(other),
            })
            .flatten()
    }

    fn question(&mut self, question: &BankQuestion) -> Option<usize> {
        self.screens
            .ask(Event::Question(question.clone()), |answer| match answer {
                Answer::Button(button) => Ok(button),
                other => Err(other),
            })
            .flatten()
    }
}

type Job = Box<dyn FnOnce(&mut Connection) + Send>;

/// The worker thread and the ways to it.
pub struct Worker {
    jobs: Sender<Job>,
    answers: Sender<Answer>,
    busy: Arc<AtomicBool>,
    notify: Notify,
}

impl Worker {
    /// Starts the worker, which opens its connection with `open` on its
    /// own thread and gives the backend dialogs that ask through `notify`.
    pub fn start(open: impl FnOnce() -> Connection + Send + 'static, notify: Notify) -> Self {
        let (jobs, job_queue) = mpsc::channel::<Job>();
        let (answers, answer_queue) = mpsc::channel();
        let screens = Screens {
            notify: Arc::clone(&notify),
            answers: Arc::new(Mutex::new(answer_queue)),
        };
        let spawned = thread::Builder::new()
            .name("kassensturz-bank".to_owned())
            .spawn(move || {
                let mut connection = open();
                if let Ok(bank) = connection.bank() {
                    bank.set_dialogs(Box::new(AskTheScreens {
                        screens: screens.clone(),
                    }));
                }
                connection.screens = Some(screens);
                while let Ok(job) = job_queue.recv() {
                    job(&mut connection);
                }
            });
        // Without a thread every call answers "busy"; the app still runs.
        let busy = Arc::new(AtomicBool::new(spawned.is_err()));
        Self {
            jobs,
            answers,
            busy,
            notify,
        }
    }

    /// Whether a call that may ask the user is running.
    pub fn busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }

    /// Runs `job` on the worker and waits for its result: for calls that
    /// only read or change AqBanking's own files. Refused while a call
    /// that may ask the user runs.
    pub fn run<T: Send + 'static>(
        &self,
        job: impl FnOnce(&mut Connection) -> Result<T, BankError> + Send + 'static,
    ) -> Result<T, BankError> {
        if self.busy() {
            return Err(BankError::Busy);
        }
        let (reply, result) = mpsc::channel();
        self.jobs
            .send(Box::new(move |connection| {
                let _ = reply.send(job(connection));
            }))
            .map_err(|_| BankError::Busy)?;
        result.recv().unwrap_or(Err(BankError::Busy))
    }

    /// Starts `job` on the worker without waiting: for calls that talk to
    /// the bank and may ask the user. [`Worker::busy`] is true until it
    /// ends; it reports through the notify it is given, the worker's own.
    /// PINs kept for one bank session are dropped after it (D-057).
    pub fn spawn(
        &self,
        job: impl FnOnce(&mut Connection, &Notify) + Send + 'static,
    ) -> Result<(), BankError> {
        if self.busy.swap(true, Ordering::SeqCst) {
            return Err(BankError::Busy);
        }
        let busy = Arc::clone(&self.busy);
        let notify = Arc::clone(&self.notify);
        self.jobs
            .send(Box::new(move |connection| {
                // Not busy any more by the time the screens hear the end.
                let (sender, ended) = mpsc::channel();
                let report: Notify = Arc::new(move |event| {
                    let _ = sender.send(event);
                });
                job(connection, &report);
                connection.pins.end_session();
                busy.store(false, Ordering::SeqCst);
                for event in ended.try_iter() {
                    notify(event);
                }
            }))
            .map_err(|_| {
                self.busy.store(false, Ordering::SeqCst);
                BankError::Busy
            })
    }

    /// Passes the user's answer to the call waiting for it.
    pub fn answer(&self, answer: Answer) {
        let _ = self.answers.send(answer);
    }
}

/// A bank error as the screens get it: the code and its parameters as
/// text (D-034). Bank errors carry no amounts or accounts of the book.
pub fn error_text(error: &BankError) -> ErrorText {
    let report = error.report();
    let mut params: Vec<(&'static str, String)> = report
        .params
        .iter()
        .map(|(name, param)| {
            let text = match param {
                ErrorParam::Text(text) => text.clone(),
                ErrorParam::Keyword(keyword) => (*keyword).to_owned(),
                ErrorParam::Currency(currency) => currency.to_string(),
                other => format!("{other:?}"),
            };
            (*name, text)
        })
        .collect();
    // A fetched statement that failed the checks says which one, so a
    // report of it can name it.
    if let BankError::Statement(cause) = error {
        params.push(("cause", cause.report().code.to_owned()));
    }
    ErrorText {
        code: report.code,
        params,
    }
}

/// A login as Settings lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginRow {
    /// Its ID in the backend.
    pub id: String,
    /// The bank's name, or its code when no source knows it.
    pub bank: String,
    /// The bank's code.
    pub bank_code: String,
    /// `pin-tan` or `chip-card`.
    pub kind: &'static str,
    /// The IBAN of the first of its accounts that has one, grouped by
    /// four; empty if none has. Changing the login starts from it.
    pub iban: String,
}

/// The logins set up on this computer.
pub fn logins(connection: &mut Connection) -> Result<Vec<LoginRow>, BankError> {
    let bank = connection.bank()?;
    let logins = bank.logins()?;
    Ok(logins
        .into_iter()
        .map(|login: Login| {
            let name = bank
                .directory()
                .lookup(&login.bank)
                .map(|info| info.name)
                .filter(|name| !name.trim().is_empty());
            let iban = bank
                .accounts(&login.id)
                .ok()
                .and_then(|accounts| {
                    accounts.iter().find_map(|account| {
                        account.iban.as_deref().and_then(|i| Iban::parse(i).ok())
                    })
                })
                .map(|iban| iban.grouped())
                .unwrap_or_default();
            LoginRow {
                id: login.id.to_string(),
                bank: name.unwrap_or_else(|| login.bank.code().to_owned()),
                bank_code: login.bank.code().to_owned(),
                kind: kind_key(login.kind),
                iban,
            }
        })
        .collect())
}

fn kind_key(kind: LoginKind) -> &'static str {
    match kind {
        LoginKind::PinTan { .. } => "pin-tan",
        LoginKind::ChipCard => "chip-card",
    }
}

/// What the setup screen shows after the IBAN: the bank, what to ask, and
/// what to fill in by hand where the sources have a gap (D-145, item 94).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lookup {
    /// `ready`, `no-fints`, `unknown-bank`, `no-server`,
    /// `guessed-server`, `unknown-login-kind` or `chip-card-only`.
    pub state: &'static str,
    /// The IBAN, grouped by four as banks print it.
    pub iban: String,
    /// The IBAN's country.
    pub country: String,
    /// The bank's code, if the IBAN has one.
    pub bank_code: String,
    /// The bank's name, if known.
    pub bank_name: String,
    /// The FinTS server's address, if known; the user types it otherwise.
    /// For `guessed-server` it is the guess, which the user can change.
    pub url: String,
    /// Whether the address field is shown: the address must be typed by
    /// hand, or is a guess to check (D-157).
    pub ask_url: bool,
    /// `none`, `required` or `optional`: whether the customer ID is asked.
    pub customer_id: &'static str,
}

/// Looks the bank of `iban` up (D-145).
pub fn look_up(connection: &mut Connection, iban: &str) -> Result<Lookup, BankError> {
    let iban = Iban::parse(iban)?;
    let bank = connection.bank()?;
    let setup = setup_for(&iban, bank.directory());
    let mut lookup = Lookup {
        state: "ready",
        iban: iban.grouped(),
        country: iban.country().to_owned(),
        bank_code: iban
            .bank_code()
            .map(|code| code.code().to_owned())
            .unwrap_or_default(),
        bank_name: String::new(),
        url: String::new(),
        ask_url: false,
        customer_id: "optional",
    };
    match setup {
        Setup::Ready {
            bank,
            url,
            login_kinds,
            ..
        } => {
            lookup.bank_name = bank.name;
            lookup.url = url;
            match login_kinds.iter().find_map(|kind| match kind {
                LoginKind::PinTan { customer_id } => Some(*customer_id),
                LoginKind::ChipCard => None,
            }) {
                Some(CustomerId::SameAsLoginName) => lookup.customer_id = "none",
                Some(CustomerId::Separate) => lookup.customer_id = "required",
                Some(CustomerId::Unknown) => {}
                None => lookup.state = "chip-card-only",
            }
        }
        Setup::NoFints { .. } => lookup.state = "no-fints",
        Setup::UnknownBank { .. } => {
            lookup.state = "unknown-bank";
            lookup.ask_url = true;
        }
        Setup::GuessedServer { bank, url, .. } => {
            lookup.state = "guessed-server";
            lookup.bank_name = bank.name;
            lookup.url = url;
            lookup.ask_url = true;
        }
        Setup::NoServer { bank } => {
            lookup.state = "no-server";
            lookup.bank_name = bank.name;
            lookup.ask_url = true;
        }
        Setup::UnknownLoginKind { bank, url } => {
            lookup.state = "unknown-login-kind";
            lookup.bank_name = bank.name;
            lookup.url = url;
        }
    }
    Ok(lookup)
}

/// What the user filled in on the setup screen.
#[derive(Clone, PartialEq, Eq)]
pub struct SetupForm {
    /// The IBAN, as typed.
    pub iban: String,
    /// The server's address, as known or typed.
    pub url: String,
    /// The login name.
    pub login_name: String,
    /// The customer ID, if any.
    pub customer_id: String,
    /// The PIN.
    pub pin: Pin,
    /// How long to keep it (D-057).
    pub pin_mode: PinMode,
    /// The ID of the login this one replaces, empty for a new login: the
    /// old login is removed, with its PIN, only after the new one is set
    /// up, so a change that fails changes nothing.
    pub replaces: String,
}

impl std::fmt::Debug for SetupForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SetupForm")
            .field("url", &self.url)
            .field("pin_mode", &self.pin_mode)
            .finish_non_exhaustive()
    }
}

/// The PIN choice by the key the screen uses.
pub fn pin_mode(key: &str) -> PinMode {
    match key {
        "remember" => PinMode::RememberUntilClose,
        "save" => PinMode::SaveInKeychain,
        _ => PinMode::AskEachTime,
    }
}

/// Checks what the user typed before anything reaches the bank: the
/// `input.` code of the first gap, if there is one.
pub fn check_form(form: &SetupForm, lookup: &Lookup) -> Result<(), ErrorText> {
    let url = form.url.trim();
    if url.is_empty() {
        return Err(ErrorText::plain("input.empty-bank-address"));
    }
    if !url.starts_with("https://")
        || url.len() == "https://".len()
        || url.contains(char::is_whitespace)
    {
        return Err(ErrorText::plain("input.bank-address-not-https"));
    }
    if form.login_name.trim().is_empty() {
        return Err(ErrorText::plain("input.empty-login-name"));
    }
    if lookup.customer_id == "required" && form.customer_id.trim().is_empty() {
        return Err(ErrorText::plain("input.empty-customer-id"));
    }
    if form.pin.as_str().is_empty() {
        return Err(ErrorText::plain("input.empty-pin"));
    }
    Ok(())
}

/// A bank account the new login reaches, as the screen lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoundAccount {
    /// Its IBAN, grouped by four, if it has one.
    pub iban: String,
    /// Its number at the bank, if it has no IBAN, e.g. a card's.
    pub number: String,
    /// The name the bank gives it.
    pub name: String,
    /// `current`, `savings`, `credit-card` or `other`.
    pub kind: &'static str,
    /// Its currency's code, if the bank says.
    pub currency: String,
    /// The bank's code, for the link.
    pub bank_code: String,
}

/// What setting up a login came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetupDone {
    /// The new login's ID.
    pub login: String,
    /// The accounts the bank lists for it.
    pub accounts: Vec<FoundAccount>,
    /// Whether a login that was to be replaced is still there, because it
    /// could not be removed. The new login works either way.
    pub old_login_left: bool,
}

/// Makes sure the backend may contact a bank (D-058): this build, or the
/// fake bank, needs nothing more, or the number was typed earlier in this
/// run of the app. Otherwise, with [`ASK_REGISTRATION_VARIABLE`], the user
/// is asked for it ([`Event::Registration`]), again as long as what they
/// type isn't 25 letters and digits, and the backend gets it for this run
/// only (D-260). "Cancel" is `bank.aborted`; without the variable it is
/// `bank.not-registered`. Called before any bank contact, on the worker.
fn ensure_registered(connection: &mut Connection) -> Result<(), BankError> {
    if connection.registered {
        return Ok(());
    }
    if !connection.ask_registration {
        return Err(BankError::NotRegistered);
    }
    let screens = connection.screens.clone().ok_or(BankError::Aborted)?;
    let mut malformed = false;
    let key = loop {
        let typed = screens
            .ask(
                Event::Registration(RegistrationRequest { malformed }),
                |answer| match answer {
                    Answer::Registration(typed) => Ok(typed),
                    other => Err(other),
                },
            )
            .flatten()
            .ok_or(BankError::Aborted)?;
        match RegistrationKey::parse(typed) {
            Ok(key) => break key,
            Err(_) => malformed = true,
        }
    };
    connection.bank()?.register(&key)?;
    connection.registered = true;
    Ok(())
}

/// Sets up a login from `form` and lists its accounts (D-145, D-149). The
/// PIN is kept as the user chose; the bank may ask for a TAN or an
/// approval through the dialogs on the way (D-151). An IBAN from a country
/// without FinTS is `input.no-fints`; the screen stops before that.
pub fn set_up(connection: &mut Connection, form: SetupForm) -> Result<SetupDone, ErrorText> {
    let iban = Iban::parse(&form.iban).map_err(|error| error_text(&error))?;
    let code: BankCode = iban
        .bank_code()
        .ok_or_else(|| ErrorText::plain("input.no-fints"))?;
    ensure_registered(connection).map_err(|error| error_text(&error))?;
    let replaces = form.replaces.clone();
    let mut done = add_login(connection, form, code).map_err(|error| error_text(&error))?;
    if !replaces.is_empty() && replaces != done.login {
        done.old_login_left = remove(connection, &replaces).is_err();
    }
    Ok(done)
}

fn add_login(
    connection: &mut Connection,
    form: SetupForm,
    code: BankCode,
) -> Result<SetupDone, BankError> {
    let pending = LoginId::pending();
    connection.pins.provide(&pending, form.pin, form.pin_mode)?;
    let customer_id = Some(form.customer_id.trim().to_owned()).filter(|id| !id.is_empty());
    let setup = LoginSetup {
        bank: code.clone(),
        url: form.url.trim().to_owned(),
        kind: LoginKind::PinTan {
            customer_id: if customer_id.is_some() {
                CustomerId::Separate
            } else {
                CustomerId::SameAsLoginName
            },
        },
        login_name: form.login_name.trim().to_owned(),
        customer_id,
    };
    let Connection { bank, pins, .. } = connection;
    let bank = match bank {
        Ok(bank) => bank.as_mut(),
        Err(error) => {
            let _ = pins.forget(&pending);
            return Err(error.clone());
        }
    };
    let login = bank.add_login(&setup, pins)?;
    let accounts = bank.accounts(&login)?;
    Ok(SetupDone {
        login: login.to_string(),
        accounts: accounts
            .into_iter()
            .map(|account| found(account, &code))
            .collect(),
        old_login_left: false,
    })
}

fn found(account: BankAccount, code: &BankCode) -> FoundAccount {
    FoundAccount {
        iban: account
            .iban
            .as_deref()
            .and_then(|iban| Iban::parse(iban).ok())
            .map(|iban| iban.grouped())
            .unwrap_or_default(),
        number: account.number.unwrap_or_default(),
        name: account.name.unwrap_or_default(),
        kind: match account.kind {
            BankAccountKind::Current => "current",
            BankAccountKind::Savings => "savings",
            BankAccountKind::CreditCard => "credit-card",
            BankAccountKind::Other => "other",
        },
        currency: account
            .currency
            .map(|currency| currency.to_string())
            .unwrap_or_default(),
        bank_code: code.code().to_owned(),
    }
}

/// Removes the login `id` and forgets its PIN, remembered or saved
/// (D-057). The links in the book stay: they name bank accounts, not
/// logins, and hold again once a login for them is set up (D-148).
pub fn remove(connection: &mut Connection, id: &str) -> Result<(), BankError> {
    let login = LoginId::new(id);
    let Connection { bank, pins, .. } = connection;
    match bank {
        Ok(bank) => remove_login(bank.as_mut(), &login, pins),
        Err(error) => {
            pins.forget(&login)?;
            Err(error.clone())
        }
    }
}

/// Forgets the PIN of the login `id`, remembered and saved (D-057).
pub fn forget_pin(connection: &mut Connection, id: &str) -> Result<(), BankError> {
    connection.pins.forget(&LoginId::new(id))
}

/// Saves PINs in `storage` from now on (D-163), after getting it ready
/// (D-167): for a KeePassXC database, KeePassXC asks the user to unlock it
/// and to allow KassenSturz, and the answer names the database and the
/// group. When it can't be got ready, the error says why and nothing
/// changes. The PINs of the logins set up here are removed from the old
/// place, never copied: the user types each again at the next connection.
/// The new place is used even when the old one can't be reached;
/// `removal` then says so. May wait for the user: runs in the background.
pub fn choose_pin_storage(
    connection: &mut Connection,
    storage: SecretStorage,
) -> Result<PinStorageChange, BankError> {
    let logins: Vec<LoginId> = match &connection.bank {
        Ok(bank) => bank
            .logins()
            .map(|logins| logins.into_iter().map(|login| login.id).collect())
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let store = (connection.pin_stores)(storage);
    // The new place first: when it can't be got ready, e.g. KeePassXC
    // isn't running or the user said no there, nothing changes (D-167).
    let place = store.open()?;
    let removal = connection.pins.replace_store(store, &logins);
    Ok(PinStorageChange { place, removal })
}

/// Gets the place PINs are saved in ready again and says where they go,
/// e.g. after the user opened another database in KeePassXC (D-167).
pub fn open_pin_storage(connection: &mut Connection) -> Result<PinStorageChange, BankError> {
    Ok(PinStorageChange {
        place: connection.pins.open_store()?,
        removal: Ok(()),
    })
}

/// What choosing a place for saved PINs did (D-163, D-167).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PinStorageChange {
    /// Where in the new place PINs go, when the user can tell it apart
    /// from others: a KeePassXC database's name and group.
    pub place: Option<KeychainPlace>,
    /// Removing the PINs saved in the old place: an error when it
    /// couldn't be reached; the new place is used anyway.
    pub removal: Result<(), BankError>,
}

/// An account in the book that a bank account can be linked to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookAccount {
    /// Its ID.
    pub id: String,
    /// Its name.
    pub name: String,
}

/// The book's open accounts, to choose from, and for each found bank
/// account the one it is linked to already, or whose number is its IBAN or
/// number: the suggestion, or empty.
pub fn link_choices(
    book: &Book,
    found: &[FoundAccount],
) -> Result<(Vec<BookAccount>, Vec<String>), kassensturz_store::StoreError> {
    let accounts = book.accounts(false)?;
    let links = book.bank_links()?;
    let suggestions = found
        .iter()
        .map(|bank_account| {
            let iban = plain(&bank_account.iban);
            let number = plain(&bank_account.number);
            let linked = links.iter().find(|link| {
                (!iban.is_empty() && link.iban.as_deref() == Some(iban.as_str()))
                    || (iban.is_empty()
                        && link.bank_code.as_deref() == Some(bank_account.bank_code.as_str())
                        && link.number.as_deref().map(plain).as_deref() == Some(number.as_str()))
            });
            if let Some(link) = linked {
                return link.account.to_string();
            }
            accounts
                .iter()
                .find(|account| {
                    account
                        .number
                        .as_deref()
                        .map(plain)
                        .is_some_and(|own| !own.is_empty() && (own == iban || own == number))
                })
                .map(|account| account.id.to_string())
                .unwrap_or_default()
        })
        .collect();
    Ok((
        accounts
            .into_iter()
            .map(|account| BookAccount {
                id: account.id.to_string(),
                name: account.name,
            })
            .collect(),
        suggestions,
    ))
}

/// Links each found bank account to the book account chosen for it (an
/// empty choice leaves it unlinked), each as its own change the user can
/// undo (D-148). Stops at the first that fails.
pub fn link(
    book: &mut Book,
    found: &[FoundAccount],
    choices: &[String],
) -> Result<usize, kassensturz_store::StoreError> {
    let accounts = book.accounts(false)?;
    let mut linked = 0;
    for (bank_account, choice) in found.iter().zip(choices) {
        let Some(account) = accounts
            .iter()
            .find(|account| account.id.to_string() == *choice)
        else {
            continue;
        };
        link_one(book, bank_account, account.id)?;
        linked += 1;
    }
    Ok(linked)
}

/// Links the bank account `found` to the book account `account`, as one
/// change the user can undo (D-148).
pub fn link_one(
    book: &mut Book,
    found: &FoundAccount,
    account: AccountId,
) -> Result<(), kassensturz_store::StoreError> {
    let iban: String = found
        .iban
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    let mut link = BankLink {
        account,
        iban: Some(iban).filter(|iban| !iban.is_empty()),
        bank_code: Some(found.bank_code.clone()),
        number: Some(found.number.trim().to_owned()).filter(|n| !n.is_empty()),
        last_fetched_on: None,
    };
    // Linking the account to the bank account it is linked to already, as
    // setting up a login again does, keeps the day of its last fetch.
    if let Some(old) = book.bank_link(account)? {
        let same = match (&old.iban, &link.iban) {
            (Some(old), Some(new)) => plain(old) == plain(new),
            (None, None) => old.bank_code == link.bank_code && old.number == link.number,
            _ => false,
        };
        if same {
            link.last_fetched_on = old.last_fetched_on;
        }
    }
    book.link_bank_account(&link)
}

/// Removes the link of `account` to its bank account, as one change the
/// user can undo (D-148). The account keeps its transactions; nothing is
/// asked of the bank, so no login is needed.
pub fn unlink_one(
    book: &mut Book,
    account: AccountId,
) -> Result<(), kassensturz_store::StoreError> {
    book.unlink_bank_account(account)
}

/// An account of the book as the fetch screen lists it (M4.7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchRow {
    /// Its ID.
    pub id: String,
    /// Its name.
    pub name: String,
    /// Whether it is linked to a bank account (D-148), so it can be fetched.
    pub linked: bool,
    /// The bank account it is linked to: the IBAN grouped by four, or the
    /// number the bank gives; empty when not linked.
    pub bank_account: String,
    /// The day of the last fetch in the user's format; empty if none.
    pub last_fetched: String,
}

/// The book's open accounts with their links, for the fetch screen.
pub fn fetch_rows(
    book: &Book,
    locale: &impl Locale,
) -> Result<Vec<FetchRow>, kassensturz_store::StoreError> {
    let links = book.bank_links()?;
    Ok(book
        .accounts(false)?
        .into_iter()
        .map(|account| {
            let link = links.iter().find(|link| link.account == account.id);
            FetchRow {
                id: account.id.to_string(),
                name: account.name,
                linked: link.is_some(),
                bank_account: link.map(shown_bank_account).unwrap_or_default(),
                last_fetched: link
                    .and_then(|link| link.last_fetched_on)
                    .map(|day| locale.short_date(day))
                    .unwrap_or_default(),
            }
        })
        .collect())
}

fn shown_bank_account(link: &BankLink) -> String {
    link.iban
        .as_deref()
        .and_then(|iban| Iban::parse(iban).ok())
        .map(|iban| iban.grouped())
        .or_else(|| link.number.clone())
        .unwrap_or_default()
}

/// One account to fetch, as the GUI thread read it from the book: the
/// worker has no access to the book.
#[derive(Clone, Debug)]
pub struct FetchRequest {
    /// The account in the book.
    pub account: AccountId,
    /// Its name, for the report.
    pub name: String,
    /// The bank account it is linked to, and the day of the last fetch.
    pub link: BankLink,
}

/// The requests for the accounts with these IDs. An account that isn't one
/// of the book's open accounts is `input.no-such-entry`, one that isn't
/// linked `store.not-linked`.
pub fn fetch_requests(
    book: &Book,
    ids: &[String],
    locale: &impl Locale,
) -> Result<Vec<FetchRequest>, ErrorText> {
    let accounts = book
        .accounts(false)
        .map_err(|error| report_text(book, &error.report(), locale))?;
    let mut requests = Vec::with_capacity(ids.len());
    for id in ids {
        let account = accounts
            .iter()
            .find(|account| account.id.to_string() == *id)
            .ok_or_else(|| ErrorText::plain("input.no-such-entry"))?;
        let link = book
            .bank_link(account.id)
            .map_err(|error| report_text(book, &error.report(), locale))?
            .ok_or_else(|| {
                report_text(
                    book,
                    &kassensturz_store::StoreError::NotLinked(account.id).report(),
                    locale,
                )
            })?;
        requests.push(FetchRequest {
            account: account.id,
            name: account.name.clone(),
            link,
        });
    }
    Ok(requests)
}

/// What came of fetching one account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchOutcome {
    /// The account's ID in the book.
    pub account: String,
    /// Its name.
    pub name: String,
    /// `ready` (a statement waits for the user to look at it), `failed`
    /// (see `error`) or `skipped` (an earlier failure stopped the fetch).
    pub state: &'static str,
    /// How many booked transactions the statement holds.
    pub transactions: usize,
    /// Why it failed.
    pub error: Option<ErrorText>,
}

/// Whether an error ends the whole fetch, since fetching the next account
/// would fail the same way or, for a PIN the bank called wrong, risk a
/// blocked login (threat T-05).
fn ends_the_fetch(error: &BankError) -> bool {
    matches!(
        error,
        BankError::Aborted
            | BankError::Unreachable
            | BankError::NotInstalled
            | BankError::TooOld { .. }
            | BankError::NotRegistered
            | BankError::Folder
            | BankError::Busy
            | BankError::Keychain
            | BankError::UntrustedServer
            | BankError::PinRejected
            | BankError::TanNeeded
            | BankError::TanMethodUnsupported
    )
}

/// Asks the user for a PIN through the screens.
struct AskForPins<'a> {
    screens: Option<&'a Screens>,
    logins: &'a [LoginRow],
}

impl PinAsker for AskForPins<'_> {
    fn pin(&mut self, login: &Login, rejected: bool) -> Option<(Pin, PinMode)> {
        let bank = self
            .logins
            .iter()
            .find(|row| row.id == login.id.as_str())
            .map_or_else(|| login.bank.code().to_owned(), |row| row.bank.clone());
        self.screens?
            .ask(
                Event::Pin(PinRequest { bank, rejected }),
                |answer| match answer {
                    Answer::Pin(pin) => Ok(pin),
                    other => Err(other),
                },
            )
            .flatten()
    }
}

/// Fetches the statements of the accounts in `requests`, one after the
/// other, and keeps each for the import screens (`bank_fetch.rs`). Nothing
/// in the book changes: that happens when the user imports a statement.
/// `today` is the day the import will note on the account's link.
///
/// The bank may ask for a PIN ([`Event::Pin`]) and, through the backend's
/// dialogs, for a TAN, so this is called with [`Worker::spawn`]. An account
/// that fails doesn't stop the others, unless the same would happen to
/// them (`ends_the_fetch`); those are `skipped`. The statements of an
/// earlier fetch of the same account are dropped first, so a failed fetch
/// leaves nothing stale to import.
pub fn fetch(
    connection: &mut Connection,
    requests: Vec<FetchRequest>,
    today: Date,
) -> Result<Vec<FetchOutcome>, ErrorText> {
    if let Status::Unusable(error) = connection.status() {
        return Err(error_text(&error));
    }
    ensure_registered(connection).map_err(|error| error_text(&error))?;
    let rows = logins(connection).map_err(|error| error_text(&error))?;
    let Connection {
        bank,
        pins,
        screens,
        ..
    } = connection;
    let bank = match bank {
        Ok(bank) => bank.as_mut(),
        Err(error) => return Err(error_text(error)),
    };
    let mut asker = AskForPins {
        screens: screens.as_ref(),
        logins: &rows,
    };
    let mut outcomes = Vec::with_capacity(requests.len());
    let mut ended = false;
    for request in requests {
        let mut outcome = FetchOutcome {
            account: request.account.to_string(),
            name: request.name,
            state: "skipped",
            transactions: 0,
            error: None,
        };
        if !ended {
            let _ = bank_fetch::take(request.account);
            if let Some(screens) = screens.as_ref() {
                screens.tell(Event::FetchStarted(outcome.account.clone()));
            }
            match fetch_linked(bank, pins, &mut asker, &request.link, &Limits::default()) {
                Ok(statement) => {
                    outcome.state = "ready";
                    outcome.transactions = statement
                        .entries
                        .iter()
                        .filter(|entry| entry.status == EntryStatus::Booked)
                        .count();
                    bank_fetch::put(Fetch {
                        account: request.account,
                        statement,
                        day: today,
                    });
                }
                Err(error) => {
                    ended = ends_the_fetch(&error);
                    outcome.state = "failed";
                    outcome.error = Some(error_text(&error));
                }
            }
        }
        outcomes.push(outcome);
    }
    Ok(outcomes)
}

/// What came of one account in *Update all accounts* (M6d.7, D-446).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Updated {
    /// The bank's statement, converted, for the GUI thread to book.
    Fetched(Statement),
    /// Why fetching it failed.
    Failed(ErrorText),
    /// Why it was not tried: `cancelled` (the user cancelled the PIN or the
    /// TAN of its bank), `bank-failed` (an earlier account of its bank
    /// failed in a way this one would too), `earlier-failure` (an earlier
    /// account failed in a way every bank would) or `stopped` (the user
    /// pressed *Stop*).
    Skipped(&'static str),
}

/// One account's part of *Update all accounts*.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateFetch {
    /// The account in the book.
    pub account: AccountId,
    /// Its name, for the summary.
    pub name: String,
    /// The day it was fetched, which the import notes on its link.
    pub day: Date,
    /// What came of it.
    pub result: Updated,
}

/// Whether an error ends the fetch of one bank login only, so *Update all
/// accounts* goes on with the next bank (D-470): the user cancelled its PIN
/// or TAN, the bank called the PIN wrong (trying again risks a blocked
/// login, T-05), or the bank's second factor or server cannot be used.
/// Every other error of [`ends_the_fetch`] would happen with every bank.
fn ends_the_login(error: &BankError) -> bool {
    matches!(
        error,
        BankError::Aborted
            | BankError::PinRejected
            | BankError::TanNeeded
            | BankError::TanMethodUnsupported
            | BankError::UntrustedServer
    )
}

/// `requests` in the order *Update all accounts* fetches them (D-446): one
/// bank login after another, in the order of each login's first account in
/// `requests`, and within a login in the order of `requests`, so each bank
/// asks for its PIN once. `login_of` gives an account's login; one whose
/// login is not known stays a group of its own, where it fails.
pub fn by_login<K: PartialEq>(
    requests: Vec<FetchRequest>,
    login_of: impl Fn(&FetchRequest) -> Option<K>,
) -> Vec<(Option<K>, Vec<FetchRequest>)> {
    let mut groups: Vec<(Option<K>, Vec<FetchRequest>)> = Vec::new();
    for request in requests {
        let login = login_of(&request);
        match groups
            .iter_mut()
            .find(|(key, _)| login.is_some() && *key == login)
        {
            Some((_, group)) => group.push(request),
            None => groups.push((login, vec![request])),
        }
    }
    groups
}

/// Fetches every account of `requests` for *Update all accounts* (M6d.7,
/// D-446), one bank login after another ([`by_login`]), and hands the
/// statements back instead of keeping them for the import screens: the GUI
/// thread books them at once (D-449). Nothing in the book changes here.
///
/// Like [`fetch`] it may ask for PINs and TANs, so it runs with
/// [`Worker::spawn`], and an account that fails doesn't stop the others.
/// An error that would end the whole fetch ends only its login when it
/// belongs to that bank ([`ends_the_login`]); then the next bank is asked.
/// `stop` is read before each account: once set, the accounts not begun
/// are `stopped`, and the one being fetched is finished first.
pub fn update_all(
    connection: &mut Connection,
    requests: Vec<FetchRequest>,
    today: Date,
    stop: &AtomicBool,
) -> Result<Vec<UpdateFetch>, ErrorText> {
    if let Status::Unusable(error) = connection.status() {
        return Err(error_text(&error));
    }
    ensure_registered(connection).map_err(|error| error_text(&error))?;
    let rows = logins(connection).map_err(|error| error_text(&error))?;
    let Connection {
        bank,
        pins,
        screens,
        ..
    } = connection;
    let bank = match bank {
        Ok(bank) => bank.as_mut(),
        Err(error) => return Err(error_text(error)),
    };
    let groups = by_login(requests, |request| {
        find_bank_account(&*bank, &request.link)
            .ok()
            .map(|(login, _)| login.id)
    });
    let mut asker = AskForPins {
        screens: screens.as_ref(),
        logins: &rows,
    };
    let mut done = Vec::new();
    let mut ended: Option<&'static str> = None;
    for (_, group) in groups {
        let mut login_ended: Option<&'static str> = None;
        for request in group {
            let skipped = if stop.load(Ordering::SeqCst) {
                Some("stopped")
            } else {
                ended.or(login_ended)
            };
            let result = match skipped {
                Some(reason) => Updated::Skipped(reason),
                None => {
                    // A statement the *Get new transactions* page fetched
                    // earlier is stale once this one is booked.
                    let _ = bank_fetch::take(request.account);
                    if let Some(screens) = screens.as_ref() {
                        screens.tell(Event::FetchStarted(request.account.to_string()));
                    }
                    match fetch_linked(bank, pins, &mut asker, &request.link, &Limits::default()) {
                        Ok(statement) => Updated::Fetched(statement),
                        // Cancelling a PIN or TAN skips the bank's
                        // accounts, this one too (D-446).
                        Err(BankError::Aborted) => {
                            login_ended = Some("cancelled");
                            Updated::Skipped("cancelled")
                        }
                        Err(error) => {
                            if ends_the_login(&error) {
                                login_ended = Some("bank-failed");
                            } else if ends_the_fetch(&error) {
                                ended = Some("earlier-failure");
                            }
                            Updated::Failed(error_text(&error))
                        }
                    }
                }
            };
            done.push(UpdateFetch {
                account: request.account,
                name: request.name,
                day: today,
                result,
            });
        }
    }
    Ok(done)
}

/// A bank account some login lists, to link an account of the book to.
#[derive(Clone, Debug)]
pub struct ListedBankAccount {
    /// Names the login and the account to [`bank_account_choices`].
    pub key: String,
    /// The login's bank: its name, or its code.
    pub bank: String,
    /// The login it is reached through.
    pub login: Login,
    /// The account as the backend lists it.
    pub account: BankAccount,
    /// The account as the setup screen lists it.
    pub found: FoundAccount,
}

/// Every bank account the logins on this computer list.
pub fn bank_account_choices(
    connection: &mut Connection,
) -> Result<Vec<ListedBankAccount>, BankError> {
    let rows = logins(connection)?;
    let bank = connection.bank()?;
    let mut listed = Vec::new();
    for login in bank.logins()? {
        let name = rows
            .iter()
            .find(|row| row.id == login.id.as_str())
            .map_or_else(|| login.bank.code().to_owned(), |row| row.bank.clone());
        for account in bank.accounts(&login.id)? {
            listed.push(ListedBankAccount {
                key: format!("{}\n{}", login.id, account.id),
                bank: name.clone(),
                found: found(account.clone(), &login.bank),
                login: login.clone(),
                account,
            });
        }
    }
    Ok(listed)
}

/// The bank accounts `account` of the book could be linked to: the listed
/// ones that no account is linked to yet, and the key of the one whose IBAN
/// or number is the account's own number, if there is one.
pub fn linkable<'a>(
    book: &Book,
    account: AccountId,
    listed: &'a [ListedBankAccount],
) -> Result<(Vec<&'a ListedBankAccount>, String), kassensturz_store::StoreError> {
    let links = book.bank_links()?;
    let own = book.account(account)?.number.as_deref().map(plain);
    let free: Vec<&ListedBankAccount> = listed
        .iter()
        .filter(|entry| {
            !links
                .iter()
                .any(|link| names_account(link, &entry.login, &entry.account))
        })
        .collect();
    let suggested = free
        .iter()
        .find(|entry| {
            own.as_deref().is_some_and(|own| {
                !own.is_empty()
                    && (own == plain(&entry.found.iban) || own == plain(&entry.found.number))
            })
        })
        .map(|entry| entry.key.clone())
        .unwrap_or_default();
    Ok((free, suggested))
}

fn plain(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .map(|character| character.to_ascii_uppercase())
        .collect()
}

/// The text of a [`QuestionLevel`] for the screens.
pub fn level_key(level: QuestionLevel) -> &'static str {
    match level {
        QuestionLevel::Info => "info",
        QuestionLevel::Warning => "warning",
        QuestionLevel::Error => "error",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, PoisonError};
    use std::time::Duration;

    use kassensturz_bank::MemoryPinStore;
    use kassensturz_core::{Account, AccountId, AccountKind, CurrencyCode};

    use super::*;

    const IBAN: &str = "DE89 3704 0044 0532 0130 00";

    fn fake(factor: Option<SecondFactor>) -> Connection {
        let bank = match factor {
            Some(factor) => FakeBank::new().with_second_factor(factor),
            None => FakeBank::new(),
        };
        Connection::fake(bank, PinSession::new(Box::new(MemoryPinStore::new())))
    }

    fn form(pin: &str) -> SetupForm {
        SetupForm {
            iban: IBAN.to_owned(),
            url: kassensturz_bank::FakeDirectory::URL.to_owned(),
            login_name: "made-up-login".to_owned(),
            customer_id: String::new(),
            pin: Pin::new(pin.to_owned()),
            pin_mode: PinMode::RememberUntilClose,
            replaces: String::new(),
        }
    }

    /// A worker over the connection `open` makes, whose events arrive in
    /// the returned queue.
    fn worker(open: impl FnOnce() -> Connection + Send + 'static) -> (Worker, Receiver<Event>) {
        let (events, queue) = mpsc::channel();
        let events = Mutex::new(events);
        let worker = Worker::start(
            open,
            Arc::new(move |event| {
                let _ = events
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .send(event);
            }),
        );
        (worker, queue)
    }

    fn next(queue: &Receiver<Event>) -> Event {
        queue
            .recv_timeout(Duration::from_secs(10))
            .expect("the worker reports")
    }

    #[test]
    fn the_iban_gives_the_bank_and_what_to_ask() {
        let mut connection = fake(None);
        let lookup = look_up(&mut connection, IBAN).expect("looked up");
        assert_eq!(
            lookup,
            Lookup {
                state: "ready",
                iban: IBAN.to_owned(),
                country: "DE".to_owned(),
                bank_code: "37040044".to_owned(),
                bank_name: lookup.bank_name.clone(),
                url: kassensturz_bank::FakeDirectory::URL.to_owned(),
                ask_url: false,
                customer_id: "none",
            }
        );
        assert!(!lookup.bank_name.is_empty());
        // Every gap is its own state (item 94).
        let state = |iban: &str| {
            look_up(&mut fake(None), iban).map(|lookup| (lookup.state, lookup.ask_url))
        };
        assert_eq!(state("NL91ABNA0417164300"), Ok(("no-fints", false)));
        assert_eq!(
            state("de89370400440532013001"),
            Err(BankError::MalformedIban)
        );
    }

    #[test]
    fn the_gaps_of_the_directory_ask_for_the_address() {
        // The fake directory knows 10000000 without a server and 20000000
        // without login kinds; a code it doesn't know is unknown.
        for (code, state, ask_url) in [
            ("10000000", "no-server", true),
            ("20000000", "unknown-login-kind", false),
            ("30000000", "unknown-bank", true),
            // Offered with the neighbour's Atruvia server, to be checked.
            ("40000010", "guessed-server", true),
        ] {
            let iban = german_iban(code, "0000000001");
            let lookup = look_up(&mut fake(None), &iban).expect("looked up");
            assert_eq!((lookup.state, lookup.ask_url), (state, ask_url), "{code}");
            assert_eq!(lookup.customer_id, "optional", "{code}");
        }
    }

    #[test]
    fn a_guessed_server_is_filled_in_and_can_be_changed() {
        let iban = german_iban("40000010", "0000000001");
        let lookup = look_up(&mut fake(None), &iban).expect("looked up");
        assert_eq!(lookup.state, "guessed-server");
        assert_eq!(lookup.url, kassensturz_bank::FakeDirectory::ATRUVIA_URL);
        assert_eq!(lookup.bank_name, "Cooperative bank without FinTS");
        // A guess still has to pass the same checks as a typed address.
        let form = |url: &str| SetupForm {
            iban: lookup.iban.clone(),
            url: url.to_owned(),
            login_name: "login".to_owned(),
            customer_id: String::new(),
            pin: Pin::new("12345".to_owned()),
            pin_mode: PinMode::AskEachTime,
            replaces: String::new(),
        };
        assert_eq!(
            check_form(&form(&lookup.url), &lookup).map_err(|error| error.code),
            Ok(())
        );
        assert_eq!(
            check_form(&form("http://fints.example/"), &lookup).map_err(|error| error.code),
            Err("input.bank-address-not-https")
        );
    }

    /// A German IBAN with valid check digits.
    fn german_iban(code: &str, account: &str) -> String {
        let digits: String = format!("{code}{account}131400");
        let remainder = digits.bytes().fold(0_u32, |rest, digit| {
            (rest * 10 + u32::from(digit - b'0')) % 97
        });
        format!("DE{:02}{code}{account}", 98 - remainder)
    }

    #[test]
    fn the_form_is_checked_before_the_bank_sees_it() {
        let lookup = look_up(&mut fake(None), IBAN).expect("looked up");
        let checked = |change: fn(&mut SetupForm)| {
            let mut form = form("12345");
            change(&mut form);
            check_form(&form, &lookup).map_err(|error| error.code)
        };
        assert_eq!(checked(|_| {}), Ok(()));
        assert_eq!(
            checked(|form| form.url = " ".to_owned()),
            Err("input.empty-bank-address")
        );
        assert_eq!(
            checked(|form| form.url = "http://fints.example/".to_owned()),
            Err("input.bank-address-not-https")
        );
        assert_eq!(
            checked(|form| form.url = "https://".to_owned()),
            Err("input.bank-address-not-https")
        );
        assert_eq!(
            checked(|form| form.login_name = String::new()),
            Err("input.empty-login-name")
        );
        assert_eq!(
            checked(|form| form.pin = Pin::new(String::new())),
            Err("input.empty-pin")
        );
        let required = Lookup {
            customer_id: "required",
            ..lookup.clone()
        };
        assert_eq!(
            check_form(&form("12345"), &required).map_err(|error| error.code),
            Err("input.empty-customer-id")
        );
        // The form never shows the PIN or the login name.
        let debug = format!("{:?}", form("12345"));
        assert!(
            !debug.contains("12345") && !debug.contains("made-up-login"),
            "{debug}"
        );
    }

    #[test]
    fn a_setup_asks_for_the_tan_through_the_worker_and_lists_the_accounts() {
        let (worker, events) = worker(|| fake(Some(SecondFactor::Tan)));
        worker
            .spawn(move |connection, notify| {
                notify(Event::SetupDone(set_up(connection, form(FakeBank::PIN))));
            })
            .expect("started");
        assert_eq!(
            worker.spawn(|_, _| {}),
            Err(BankError::Busy),
            "one call that may ask the user at a time"
        );
        // A quick call is refused while the setup may ask the user.
        assert_eq!(worker.run(logins), Err(BankError::Busy));
        match next(&events) {
            Event::Tan(request) => assert!(!request.challenge.is_empty()),
            other => panic!("a TAN is asked for, not {other:?}"),
        }
        worker.answer(Answer::Tan(Some(Tan::new(FakeBank::TAN.to_owned()))));
        let Event::SetupDone(done) = next(&events) else {
            panic!("the setup ends");
        };
        // The worker is free by the time the screens hear of the end.
        assert!(!worker.busy());
        let done = done.expect("set up");
        assert_eq!(done.accounts.len(), 2);
        assert_eq!(done.accounts[0].iban, IBAN);
        assert_eq!(done.accounts[0].kind, "current");
        assert_eq!(done.accounts[1].kind, "credit-card");
        assert_eq!(done.accounts[1].iban, "");
        assert_eq!(done.accounts[1].number, "4000 0000 0000 0001");
        let listed = worker.run(logins).expect("listed");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, done.login);
        assert_eq!(listed[0].bank_code, "37040044");
        assert_eq!(listed[0].kind, "pin-tan");
        // Removing it removes it, and its PIN.
        let id = done.login.clone();
        worker
            .run(move |connection| {
                remove(connection, &id)?;
                assert!(!connection.pins.is_remembered(&LoginId::new(&id)));
                Ok(())
            })
            .expect("removed");
        assert_eq!(worker.run(logins).map(|l| l.len()), Ok(0));
    }

    #[test]
    fn a_cancelled_tan_ends_the_setup_and_keeps_nothing() {
        let (worker, events) = worker(|| fake(Some(SecondFactor::Approval)));
        let (done, done_queue) = mpsc::channel();
        worker
            .spawn(move |connection, _| {
                let result = set_up(connection, form(FakeBank::PIN));
                let pending = connection.pins.is_remembered(&LoginId::pending());
                let _ = done.send((result, pending));
            })
            .expect("started");
        match next(&events) {
            Event::Question(question) => {
                assert_eq!(question.buttons, vec!["Approved", "Abort"]);
                assert_eq!(level_key(question.level), "info");
            }
            other => panic!("an approval is asked for, not {other:?}"),
        }
        worker.answer(Answer::Button(None));
        let (result, pending) = done_queue
            .recv_timeout(Duration::from_secs(10))
            .expect("the setup ends");
        assert_eq!(result.map_err(|error| error.code), Err("bank.aborted"));
        assert!(!pending, "the PIN typed for the setup is forgotten");
        while worker.busy() {
            thread::yield_now();
        }
        assert_eq!(worker.run(logins).map(|l| l.len()), Ok(0));
    }

    #[test]
    fn without_a_registration_or_aqbanking_the_status_says_why() {
        let mut connection = fake(None);
        assert_eq!(connection.status(), Status::Ready { fake: true });
        connection.registered = false;
        assert_eq!(
            connection.status(),
            Status::Unusable(BankError::NotRegistered)
        );
        assert_eq!(
            set_up(&mut connection, form("12345")).map_err(|error| error.code),
            Err("bank.not-registered")
        );
        let foreign = SetupForm {
            iban: "NL91ABNA0417164300".to_owned(),
            ..form("12345")
        };
        assert_eq!(
            set_up(&mut fake(None), foreign).map_err(|error| error.code),
            Err("input.no-fints")
        );
        let mut missing = Connection::fake(
            FakeBank::not_installed(),
            PinSession::new(Box::new(MemoryPinStore::new())),
        );
        assert_eq!(missing.status(), Status::Unusable(BankError::NotInstalled));
        let mut broken = fake(None);
        broken.bank = Err(BankError::Folder);
        assert_eq!(broken.status(), Status::Unusable(BankError::Folder));
        assert_eq!(remove(&mut broken, "1"), Err(BankError::Folder));
    }

    #[test]
    fn bank_errors_keep_their_parameters_as_text() {
        let text = error_text(&BankError::Refused {
            text: Some("9010 made-up".to_owned()),
        });
        assert_eq!(text.code, "bank.refused");
        assert_eq!(text.params, vec![("text", "9010 made-up".to_owned())]);
        assert_eq!(error_text(&BankError::Busy).code, "bank.busy");
        let statement = error_text(&BankError::Statement(
            kassensturz_import::ImportError::TooManyEntries { max: 1 },
        ));
        assert_eq!(statement.code, "bank.statement");
        assert_eq!(
            statement.params,
            vec![("cause", "import.too-many-entries".to_owned())]
        );
    }

    fn account(book: &mut Book, name: &str, number: Option<&str>) -> AccountId {
        let id = AccountId::generate();
        let eur = CurrencyCode::new("EUR").expect("EUR");
        book.use_currency(eur).expect("currency");
        book.add_account(&Account {
            id,
            name: name.to_owned(),
            kind: AccountKind::Asset,
            currency: eur,
            account_type: None,
            parent: None,
            closed: false,
            number: number.map(str::to_owned),
            notes: None,
            institution: None,
        })
        .expect("added");
        id
    }

    #[test]
    fn found_accounts_are_suggested_and_linked_in_the_book() {
        let mut connection = fake(None);
        let done = set_up(&mut connection, form(FakeBank::PIN)).expect("set up");
        let mut book = Book::create_in_memory().expect("book");
        let current = account(
            &mut book,
            "Current account",
            Some("de89 3704 0044 0532 0130 00"),
        );
        let card = account(&mut book, "Card", None);
        let (choices, suggested) = link_choices(&book, &done.accounts).expect("choices");
        assert_eq!(choices.len(), 2);
        assert_eq!(suggested, vec![current.to_string(), String::new()]);
        let linked = link(
            &mut book,
            &done.accounts,
            &[current.to_string(), card.to_string()],
        )
        .expect("linked");
        assert_eq!(linked, 2);
        let links = book.bank_links().expect("links");
        let of = |id: AccountId| {
            links
                .iter()
                .find(|link| link.account == id)
                .expect("linked")
        };
        assert_eq!(of(current).iban.as_deref(), Some("DE89370400440532013000"));
        assert_eq!(of(card).iban, None);
        assert_eq!(of(card).bank_code.as_deref(), Some("37040044"));
        assert_eq!(of(card).number.as_deref(), Some("4000 0000 0000 0001"));
        // Once linked, the links are what is suggested, also for the card.
        let (_, suggested) = link_choices(&book, &done.accounts).expect("choices");
        assert_eq!(suggested, vec![current.to_string(), card.to_string()]);
        // An empty choice links nothing.
        let mut other = Book::create_in_memory().expect("book");
        account(&mut other, "Unrelated", None);
        assert_eq!(
            link(&mut other, &done.accounts, &[String::new(), String::new()]).expect("link"),
            0
        );
    }

    // --- Fetching (M4.7, D-153) ---

    use kassensturz_bank::Failure;

    use crate::register::tests::TestLocale;

    const BANK_CODE: &str = "37040044";
    const CARD_NUMBER: &str = "4000 0000 0000 0001";

    fn today() -> Date {
        Date::new(2026, 10, 9).expect("a date")
    }

    /// A fake bank with one login whose PIN is remembered, as the setup
    /// leaves it, and a connection over it.
    fn fetch_connection(configure: impl FnOnce(&mut FakeBank)) -> Connection {
        let mut bank = FakeBank::new();
        let mut pins = PinSession::new(Box::new(MemoryPinStore::new()));
        pins.provide(
            &LoginId::pending(),
            Pin::new(FakeBank::PIN.to_owned()),
            PinMode::RememberUntilClose,
        )
        .expect("provided");
        bank.add_login(&FakeBank::setup(), &mut pins)
            .expect("login added");
        configure(&mut bank);
        Connection::fake(bank, pins)
    }

    fn link_to(book: &mut Book, account: AccountId, iban: Option<&str>, number: Option<&str>) {
        book.link_bank_account(&BankLink {
            account,
            iban: iban.map(str::to_owned),
            bank_code: Some(BANK_CODE.to_owned()),
            number: number.map(str::to_owned),
            last_fetched_on: None,
        })
        .expect("linked");
    }

    fn requests(book: &Book, accounts: &[AccountId]) -> Vec<FetchRequest> {
        let ids: Vec<String> = accounts.iter().map(ToString::to_string).collect();
        fetch_requests(book, &ids, &TestLocale).expect("requests")
    }

    #[test]
    fn the_fetch_screen_lists_the_accounts_with_their_links() {
        let mut book = Book::create_in_memory().expect("book");
        let current = account(&mut book, "Current account", None);
        let card = account(&mut book, "Card", None);
        let cash = account(&mut book, "Wallet", None);
        link_to(&mut book, current, Some("DE89370400440532013000"), None);
        book.link_bank_account(&BankLink {
            account: card,
            iban: None,
            bank_code: Some(BANK_CODE.to_owned()),
            number: Some(CARD_NUMBER.to_owned()),
            last_fetched_on: Some(today()),
        })
        .expect("linked");
        let rows = fetch_rows(&book, &TestLocale).expect("rows");
        let row = |id: AccountId| {
            rows.iter()
                .find(|row| row.id == id.to_string())
                .expect("row")
        };
        assert_eq!(rows.len(), 3);
        assert_eq!(row(current).bank_account, "DE89 3704 0044 0532 0130 00");
        assert!(row(current).linked && row(current).last_fetched.is_empty());
        assert_eq!(row(card).bank_account, CARD_NUMBER);
        assert_eq!(row(card).last_fetched, "2026-10-09");
        assert!(!row(cash).linked && row(cash).bank_account.is_empty());
    }

    #[test]
    fn only_linked_open_accounts_can_be_fetched() {
        let mut book = Book::create_in_memory().expect("book");
        let current = account(&mut book, "Current account", None);
        let cash = account(&mut book, "Wallet", None);
        link_to(&mut book, current, Some("DE89370400440532013000"), None);
        let asked = |ids: &[String]| fetch_requests(&book, ids, &TestLocale).map_err(|e| e.code);
        let found = asked(&[current.to_string()]).expect("requests");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Current account");
        assert_eq!(
            found[0].link.iban.as_deref(),
            Some("DE89370400440532013000")
        );
        assert_eq!(
            asked(&[cash.to_string()]).map(|_| ()),
            Err("store.not-linked")
        );
        assert_eq!(
            asked(&["not-an-account".to_owned()]).map(|_| ()),
            Err("input.no-such-entry")
        );
    }

    /// A connection without a registration, started with the switch that
    /// asks for one (D-260).
    fn asking_for_the_registration(mut connection: Connection) -> Connection {
        connection.registered = false;
        connection.ask_registration = true;
        connection
    }

    /// A made-up number of the right shape; no real one is ever written
    /// down (D-260).
    const MADE_UP_REGISTRATION: &str = "0123456789ABCDEFGHIJKLMNO";

    #[test]
    fn with_the_switch_the_registration_is_asked_for_once_per_run() {
        // The screens can be used; the number is asked for when a bank is
        // contacted.
        assert_eq!(
            asking_for_the_registration(fake(None)).status(),
            Status::Ready { fake: true }
        );
        let (worker, events) = worker(|| asking_for_the_registration(fake(None)));
        let set_up_once = || {
            worker
                .spawn(|connection, notify| {
                    notify(Event::SetupDone(set_up(connection, form(FakeBank::PIN))));
                })
                .expect("started");
        };
        set_up_once();
        assert_eq!(
            next(&events),
            Event::Registration(RegistrationRequest { malformed: false })
        );
        // A number of the wrong shape is asked for again, and the dialog
        // says why.
        worker.answer(Answer::Registration(Some("made-up".to_owned())));
        assert_eq!(
            next(&events),
            Event::Registration(RegistrationRequest { malformed: true })
        );
        worker.answer(Answer::Registration(Some(format!(
            " {MADE_UP_REGISTRATION} "
        ))));
        let Event::SetupDone(done) = next(&events) else {
            panic!("the setup ends");
        };
        assert!(done.is_ok(), "{done:?}");
        // The number is kept for this run: the next call doesn't ask.
        set_up_once();
        let Event::SetupDone(again) = next(&events) else {
            panic!("the second setup ends without asking");
        };
        assert!(again.is_ok(), "{again:?}");
        assert_eq!(worker.run(|connection| Ok(connection.registered)), Ok(true));
    }

    #[test]
    fn cancelling_the_registration_contacts_no_bank_and_asks_again_next_time() {
        let (worker, events) = worker(|| asking_for_the_registration(fake(None)));
        for _ in 0..2 {
            worker
                .spawn(|connection, notify| {
                    notify(Event::SetupDone(set_up(connection, form(FakeBank::PIN))));
                })
                .expect("started");
            assert_eq!(
                next(&events),
                Event::Registration(RegistrationRequest { malformed: false })
            );
            worker.answer(Answer::Registration(None));
            let Event::SetupDone(done) = next(&events) else {
                panic!("the setup ends");
            };
            assert_eq!(
                done.map(|_| ()).map_err(|error| error.code),
                Err("bank.aborted")
            );
        }
        assert_eq!(worker.run(logins).map(|l| l.len()), Ok(0));
        assert_eq!(
            worker.run(|connection| Ok(connection.registered)),
            Ok(false)
        );
    }

    #[test]
    fn a_fetch_asks_for_the_registration_before_the_bank() {
        let mut book = Book::create_in_memory().expect("book");
        let current = account(&mut book, "Current account", None);
        link_to(&mut book, current, Some("DE89370400440532013000"), None);
        let requests = requests(&book, &[current]);
        let (worker, events) = worker(|| asking_for_the_registration(fetch_connection(|_| {})));
        worker
            .spawn(move |connection, notify| {
                notify(Event::FetchDone(fetch(connection, requests, today())));
            })
            .expect("started");
        // Asked before any account is fetched.
        assert_eq!(
            next(&events),
            Event::Registration(RegistrationRequest { malformed: false })
        );
        worker.answer(Answer::Registration(None));
        let Event::FetchDone(done) = next(&events) else {
            panic!("the fetch ends");
        };
        assert_eq!(
            done.map(|_| ()).map_err(|error| error.code),
            Err("bank.aborted")
        );
        assert!(bank_fetch::take(current).is_none());
    }

    #[test]
    fn a_typed_registration_never_shows_itself() {
        let answer = Answer::Registration(Some(MADE_UP_REGISTRATION.to_owned()));
        let shown = format!("{answer:?}");
        assert!(!shown.contains(MADE_UP_REGISTRATION), "{shown}");
    }

    #[test]
    fn a_fetch_asks_for_the_pin_through_the_worker_and_keeps_the_statements_for_the_import() {
        let (worker, events) = worker(|| fake(None));
        worker
            .spawn(|connection, notify| {
                notify(Event::SetupDone(set_up(connection, form(FakeBank::PIN))));
            })
            .expect("started");
        let Event::SetupDone(done) = next(&events) else {
            panic!("the setup ends");
        };
        let done = done.expect("set up");
        let mut book = Book::create_in_memory().expect("book");
        let current = account(&mut book, "Current account", Some(IBAN));
        link(
            &mut book,
            &done.accounts,
            &[current.to_string(), String::new()],
        )
        .expect("linked");
        // The PIN was only remembered for this run of the app; now it is
        // gone, so the bank's request has to be answered by the user.
        let login = done.login.clone();
        worker
            .run(move |connection| forget_pin(connection, &login))
            .expect("forgotten");
        let requests = requests(&book, &[current]);
        worker
            .spawn(move |connection, notify| {
                notify(Event::FetchDone(fetch(connection, requests, today())));
            })
            .expect("started");

        assert_eq!(next(&events), Event::FetchStarted(current.to_string()));
        let Event::Pin(asked) = next(&events) else {
            panic!("the PIN is asked for");
        };
        assert!(!asked.rejected && !asked.bank.is_empty());
        // A wrong PIN: the bank refuses it, the dialog says so and asks
        // once more, and nothing is sent a third time.
        worker.answer(Answer::Pin(Some((
            Pin::new("00000".to_owned()),
            PinMode::AskEachTime,
        ))));
        let Event::Pin(again) = next(&events) else {
            panic!("the PIN is asked for again");
        };
        assert!(again.rejected);
        worker.answer(Answer::Pin(Some((
            Pin::new(FakeBank::PIN.to_owned()),
            PinMode::RememberUntilClose,
        ))));
        let Event::FetchDone(done) = next(&events) else {
            panic!("the fetch ends");
        };
        assert!(!worker.busy());
        let outcomes = done.expect("fetched");
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].state, "ready");
        assert_eq!(
            outcomes[0].transactions, 3,
            "the unbooked payment isn't counted"
        );
        assert_eq!(outcomes[0].error, None);
        // The statement waits for the import screens, with the day.
        let waiting = bank_fetch::take(current).expect("a statement waits");
        assert_eq!(waiting.day, today());
        assert_eq!(waiting.statement.entries.len(), 4);
        // The right PIN was kept as the user chose.
        let remembered = worker
            .run(|connection| {
                let id = logins(connection)?.remove(0).id;
                Ok(connection.pins.is_remembered(&LoginId::new(id)))
            })
            .expect("asked");
        assert!(remembered);
    }

    #[test]
    fn a_failing_account_does_not_stop_the_others_but_an_unreachable_bank_does() {
        let mut book = Book::create_in_memory().expect("book");
        let unknown = account(&mut book, "Closed at the bank", None);
        let card = account(&mut book, "Card", None);
        let current = account(&mut book, "Current account", None);
        // The first account's bank account isn't listed by any login.
        link_to(&mut book, unknown, Some("BE68539007547034"), None);
        link_to(&mut book, card, None, Some(CARD_NUMBER));
        link_to(&mut book, current, Some("DE89370400440532013000"), None);
        // An earlier fetch of the first account is stale now.
        bank_fetch::put(bank_fetch::Fetch {
            account: unknown,
            statement: bank_statement(&mut fetch_connection(|_| {})),
            day: today(),
        });
        let mut connection = fetch_connection(|_| {});
        let outcomes =
            fetch(&mut connection, requests(&book, &[unknown, card]), today()).expect("fetched");
        let states: Vec<_> = outcomes.iter().map(|outcome| outcome.state).collect();
        assert_eq!(states, vec!["failed", "ready"]);
        assert_eq!(
            outcomes[0].error.as_ref().map(|error| error.code),
            Some("bank.no-bank-account")
        );
        assert_eq!(outcomes[1].transactions, 2);
        assert!(bank_fetch::take(unknown).is_none(), "nothing stale is left");
        assert!(bank_fetch::take(card).is_some());

        // The bank can't be reached: the next account would fail the same
        // way, so it isn't tried.
        let mut connection = fetch_connection(|bank| bank.fail_next(Failure::Unreachable));
        let outcomes =
            fetch(&mut connection, requests(&book, &[current, card]), today()).expect("fetched");
        let states: Vec<_> = outcomes.iter().map(|outcome| outcome.state).collect();
        assert_eq!(states, vec!["failed", "skipped"]);
        assert_eq!(
            outcomes[0].error.as_ref().map(|error| error.code),
            Some("bank.unreachable")
        );
        assert!(bank_fetch::take(card).is_none());
    }

    /// The statement of the fake bank's current account, as fetched.
    fn bank_statement(connection: &mut Connection) -> kassensturz_import::Statement {
        let mut book = Book::create_in_memory().expect("book");
        let current = account(&mut book, "Current account", None);
        link_to(&mut book, current, Some("DE89370400440532013000"), None);
        fetch(connection, requests(&book, &[current]), today()).expect("fetched");
        bank_fetch::take(current).expect("a statement").statement
    }

    #[test]
    fn a_cancelled_pin_ends_the_fetch_and_nothing_is_fetched() {
        let mut book = Book::create_in_memory().expect("book");
        let current = account(&mut book, "Current account", None);
        let card = account(&mut book, "Card", None);
        link_to(&mut book, current, Some("DE89370400440532013000"), None);
        link_to(&mut book, card, None, Some(CARD_NUMBER));
        let mut connection = fetch_connection(|_| {});
        connection
            .pins
            .forget(&LoginId::new("fake-login-1"))
            .expect("forgotten");
        // A connection without screens can't ask, which is a cancelled
        // dialog.
        let outcomes =
            fetch(&mut connection, requests(&book, &[current, card]), today()).expect("fetched");
        let states: Vec<_> = outcomes.iter().map(|outcome| outcome.state).collect();
        assert_eq!(states, vec!["failed", "skipped"]);
        assert_eq!(
            outcomes[0].error.as_ref().map(|error| error.code),
            Some("bank.aborted")
        );
        assert!(bank_fetch::take(current).is_none());
    }

    #[test]
    fn without_a_usable_bank_nothing_is_fetched() {
        let mut book = Book::create_in_memory().expect("book");
        let current = account(&mut book, "Current account", None);
        link_to(&mut book, current, Some("DE89370400440532013000"), None);
        let mut connection = fetch_connection(|_| {});
        connection.registered = false;
        assert_eq!(
            fetch(&mut connection, requests(&book, &[current]), today())
                .map(|_| ())
                .map_err(|error| error.code),
            Err("bank.not-registered")
        );
    }

    #[test]
    fn an_account_is_linked_from_the_fetch_screen_to_a_bank_account_nobody_has() {
        let mut connection = fake(None);
        set_up(&mut connection, form(FakeBank::PIN)).expect("set up");
        let listed = bank_account_choices(&mut connection).expect("listed");
        assert_eq!(listed.len(), 2);
        assert!(
            listed
                .iter()
                .all(|entry| !entry.key.is_empty() && !entry.bank.is_empty())
        );
        let mut book = Book::create_in_memory().expect("book");
        let current = account(
            &mut book,
            "Current account",
            Some("de89 3704 0044 0532 0130 00"),
        );
        let wallet = account(&mut book, "Wallet", None);

        // The account whose number is a bank account's IBAN gets it
        // suggested; the others are all offered.
        let (free, suggested) = linkable(&book, current, &listed).expect("choices");
        assert_eq!(free.len(), 2);
        let current_key = listed
            .iter()
            .find(|entry| entry.found.iban == IBAN)
            .map(|entry| entry.key.clone())
            .expect("the current account");
        assert_eq!(suggested, current_key);
        let (_, suggested) = linkable(&book, wallet, &listed).expect("choices");
        assert_eq!(suggested, "");

        // Once linked, a bank account is offered to no other account, and
        // the link can't be made twice.
        let chosen = listed
            .iter()
            .find(|entry| entry.key == current_key)
            .expect("listed");
        link_one(&mut book, &chosen.found, current).expect("linked");
        let link = book.bank_link(current).expect("read").expect("linked");
        assert_eq!(link.iban.as_deref(), Some("DE89370400440532013000"));
        let (free, _) = linkable(&book, wallet, &listed).expect("choices");
        assert_eq!(free.len(), 1);
        assert_eq!(free[0].found.kind, "credit-card");
        assert!(link_one(&mut book, &chosen.found, wallet).is_err());
        // And the link is what a fetch finds.
        let outcomes =
            fetch(&mut connection, requests(&book, &[current]), today()).expect("fetched");
        assert_eq!(outcomes[0].state, "ready");
    }

    #[test]
    fn a_link_is_changed_to_another_bank_account_or_removed_again() {
        let mut connection = fake(None);
        set_up(&mut connection, form(FakeBank::PIN)).expect("set up");
        let listed = bank_account_choices(&mut connection).expect("listed");
        let mut book = Book::create_in_memory().expect("book");
        let current = account(&mut book, "Current account", None);
        let card = listed
            .iter()
            .find(|entry| entry.found.kind == "credit-card")
            .expect("the card");
        let checking = listed
            .iter()
            .find(|entry| entry.found.iban == IBAN)
            .expect("the current account");

        link_one(&mut book, &checking.found, current).expect("linked");
        // The bank account it is linked to is not offered again, but the
        // other one is: that is what "change" shows.
        let (free, _) = linkable(&book, current, &listed).expect("choices");
        assert_eq!(free.len(), 1);
        assert_eq!(free[0].key, card.key);
        // Linking again replaces the link, and the new link is what the
        // fetch screen lists.
        link_one(&mut book, &card.found, current).expect("changed");
        let rows = fetch_rows(&book, &TestLocale).expect("rows");
        assert!(rows[0].linked);
        assert!(!rows[0].bank_account.contains("DE89"));
        let (free, _) = linkable(&book, current, &listed).expect("choices");
        assert_eq!(free.len(), 1);
        assert_eq!(free[0].key, checking.key);

        // Linking it to the same bank account again keeps the last fetch.
        book.note_bank_fetch(current, today()).expect("fetched");
        link_one(&mut book, &card.found, current).expect("again");
        let kept = book.bank_link(current).expect("read").expect("linked");
        assert_eq!(kept.last_fetched_on, Some(today()));

        unlink_one(&mut book, current).expect("unlinked");
        assert!(book.bank_link(current).expect("read").is_none());
        let (free, _) = linkable(&book, current, &listed).expect("choices");
        assert_eq!(free.len(), 2);
        // An account that isn't linked is left as it is.
        unlink_one(&mut book, current).expect("nothing to do");
    }

    #[test]
    fn changing_a_login_sets_up_the_new_one_and_then_removes_the_old_one() {
        let mut connection = fake(None);
        let first = set_up(&mut connection, form(FakeBank::PIN)).expect("set up");
        let rows = logins(&mut connection).expect("logins");
        assert_eq!(rows.len(), 1);
        // The IBAN of the login's account is what a change starts from.
        assert_eq!(rows[0].iban, "DE89 3704 0044 0532 0130 00");

        // A change that fails (wrong PIN) leaves the old login as it was.
        let mut change = form("00000");
        change.replaces = first.login.clone();
        assert!(set_up(&mut connection, change).is_err());
        assert_eq!(logins(&mut connection).expect("logins").len(), 1);

        // One that works leaves only the new login.
        let mut change = form(FakeBank::PIN);
        change.replaces = first.login.clone();
        let second = set_up(&mut connection, change).expect("changed");
        assert!(!second.old_login_left);
        assert_ne!(second.login, first.login);
        let rows = logins(&mut connection).expect("logins");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, second.login);
    }

    /// A keychain the test can look inside while a connection uses it.
    #[derive(Clone)]
    struct SharedKeychain(Arc<kassensturz_platform::MemoryKeychain>);

    impl Keychain for SharedKeychain {
        fn store(
            &self,
            key: &kassensturz_platform::SecretKey,
            secret: &kassensturz_platform::Secret,
        ) -> Result<(), kassensturz_platform::KeychainError> {
            self.0.store(key, secret)
        }

        fn read(
            &self,
            key: &kassensturz_platform::SecretKey,
        ) -> Result<Option<kassensturz_platform::Secret>, kassensturz_platform::KeychainError>
        {
            self.0.read(key)
        }

        fn delete(
            &self,
            key: &kassensturz_platform::SecretKey,
        ) -> Result<(), kassensturz_platform::KeychainError> {
            self.0.delete(key)
        }

        fn open(&self) -> Result<Option<KeychainPlace>, kassensturz_platform::KeychainError> {
            self.0.open()
        }
    }

    fn shared() -> SharedKeychain {
        SharedKeychain(Arc::new(kassensturz_platform::MemoryKeychain::new()))
    }

    /// The stores for each place: the shared fake keychain for the system
    /// keychain, none for "nowhere".
    fn stores(keychain: &SharedKeychain) -> PinStores {
        stores_with(keychain, &shared())
    }

    /// The same with `keepassxc` as the KeePassXC database.
    fn stores_with(system: &SharedKeychain, keepassxc: &SharedKeychain) -> PinStores {
        let (system, keepassxc) = (system.clone(), keepassxc.clone());
        keychain_stores(move |storage| match storage {
            SecretStorage::System => Some(Box::new(system.clone()) as Box<dyn Keychain>),
            SecretStorage::KeePassXc => Some(Box::new(keepassxc.clone()) as Box<dyn Keychain>),
            SecretStorage::Nowhere => None,
        })
    }

    fn household() -> KeychainPlace {
        KeychainPlace {
            name: "Household".to_owned(),
            group: "KassenSturz".to_owned(),
        }
    }

    #[test]
    fn keepassxc_is_got_ready_first_and_nothing_changes_when_it_cant_be() {
        let (system, keepassxc) = (shared(), shared());
        let mut connection = Connection::fake_with_keychain(
            FakeBank::new(),
            stores_with(&system, &keepassxc),
            SecretStorage::System,
        );
        let mut setup = form(FakeBank::PIN);
        setup.pin_mode = PinMode::SaveInKeychain;
        set_up(&mut connection, setup).expect("set up");
        assert_eq!(system.0.len(), 1);

        // KeePassXC isn't running: the PIN stays where it is.
        keepassxc.0.fail_with(Some(
            kassensturz_platform::KeychainError::KeePassXcUnavailable,
        ));
        let refused = choose_pin_storage(&mut connection, SecretStorage::KeePassXc);
        assert_eq!(refused, Err(BankError::KeePassXc));
        assert_eq!(error_text(&BankError::KeePassXc).code, "bank.keepassxc");
        assert_eq!(system.0.len(), 1);
        assert_eq!(
            open_pin_storage(&mut connection),
            Ok(PinStorageChange {
                place: None,
                removal: Ok(()),
            })
        );

        // Now it answers and names the database and the group.
        keepassxc.0.fail_with(None);
        keepassxc.0.place(Some(household()));
        let chosen = choose_pin_storage(&mut connection, SecretStorage::KeePassXc);
        assert_eq!(
            chosen,
            Ok(PinStorageChange {
                place: Some(household()),
                removal: Ok(()),
            })
        );
        assert!(system.0.is_empty());
        let mut setup = form(FakeBank::PIN);
        setup.pin_mode = PinMode::SaveInKeychain;
        set_up(&mut connection, setup).expect("set up");
        assert_eq!(keepassxc.0.len(), 1);
        assert!(system.0.is_empty());

        // Connecting again names it again, and says when it can't.
        assert_eq!(
            open_pin_storage(&mut connection).map(|change| change.place),
            Ok(Some(household()))
        );
        keepassxc.0.fail_with(Some(
            kassensturz_platform::KeychainError::KeePassXcUnavailable,
        ));
        assert_eq!(open_pin_storage(&mut connection), Err(BankError::KeePassXc));
    }

    #[test]
    fn the_fake_bank_in_memory_saves_in_keepassxc_when_chosen() {
        let keepassxc = shared();
        keepassxc.0.place(Some(household()));
        let shared_keepassxc = keepassxc.clone();
        let pin_stores = fake_stores(move |storage| {
            (storage == SecretStorage::KeePassXc)
                .then(|| Box::new(shared_keepassxc.clone()) as Box<dyn Keychain>)
        });
        let pins = PinSession::new(pin_stores(SecretStorage::System));
        let mut connection = Connection {
            pin_stores,
            ..Connection::fake(FakeBank::new(), pins)
        };
        // The system keychain stays out of it: in memory (D-159).
        let mut setup = form(FakeBank::PIN);
        setup.pin_mode = PinMode::SaveInKeychain;
        set_up(&mut connection, setup).expect("set up");
        assert!(keepassxc.0.is_empty());

        let chosen = choose_pin_storage(&mut connection, SecretStorage::KeePassXc);
        assert_eq!(chosen.map(|change| change.place), Ok(Some(household())));
        let mut setup = form(FakeBank::PIN);
        setup.pin_mode = PinMode::SaveInKeychain;
        set_up(&mut connection, setup).expect("set up");
        assert_eq!(keepassxc.0.len(), 1);
    }

    #[test]
    fn choosing_nowhere_removes_the_saved_pins_and_offers_no_saving() {
        let keychain = SharedKeychain(Arc::new(kassensturz_platform::MemoryKeychain::new()));
        let mut connection = Connection::fake_with_keychain(
            FakeBank::new(),
            stores(&keychain),
            SecretStorage::System,
        );
        let mut setup = form(FakeBank::PIN);
        setup.pin_mode = PinMode::SaveInKeychain;
        set_up(&mut connection, setup).expect("set up");
        assert_eq!(keychain.0.len(), 1);
        assert!(connection.pins.saves());

        choose_pin_storage(&mut connection, SecretStorage::Nowhere).expect("chosen");

        // Removed from the keychain; the fetch still has it until the app
        // closes, since it was typed in this run.
        assert!(keychain.0.is_empty());
        assert!(!connection.pins.saves());
        assert!(!bank_statement(&mut connection).entries.is_empty());
        // A second login "saved" now stays out of the keychain.
        let mut setup = form(FakeBank::PIN);
        setup.pin_mode = PinMode::SaveInKeychain;
        let done = set_up(&mut connection, setup).expect("set up");
        assert!(keychain.0.is_empty());
        assert!(connection.pins.is_remembered(&LoginId::new(done.login)));
        drop(connection);

        // The next run with "nowhere" brings no fake login back, and
        // choosing the keychain again saves the next PIN there.
        let mut next = Connection::fake_with_keychain(
            FakeBank::new(),
            stores(&keychain),
            SecretStorage::Nowhere,
        );
        assert!(logins(&mut next).expect("logins").is_empty());
        choose_pin_storage(&mut next, SecretStorage::System).expect("chosen");
        let mut setup = form(FakeBank::PIN);
        setup.pin_mode = PinMode::SaveInKeychain;
        set_up(&mut next, setup).expect("set up");
        assert_eq!(keychain.0.len(), 1);
    }

    #[test]
    fn a_keychain_out_of_reach_is_still_left_and_says_so() {
        let keychain = SharedKeychain(Arc::new(kassensturz_platform::MemoryKeychain::new()));
        let mut connection = Connection::fake_with_keychain(
            FakeBank::new(),
            stores(&keychain),
            SecretStorage::System,
        );
        let mut setup = form(FakeBank::PIN);
        setup.pin_mode = PinMode::SaveInKeychain;
        set_up(&mut connection, setup).expect("set up");
        keychain
            .0
            .fail_with(Some(kassensturz_platform::KeychainError::Unavailable));

        let result = choose_pin_storage(&mut connection, SecretStorage::Nowhere);

        assert_eq!(
            result.map(|change| change.removal),
            Ok(Err(BankError::Keychain))
        );
        assert_eq!(
            error_text(&BankError::Keychain).code,
            "bank.keychain",
            "the screens explain it"
        );
        assert!(!connection.pins.saves());
    }

    #[test]
    fn the_fake_banks_pin_is_saved_read_back_and_removed_through_the_keychain() {
        let keychain = SharedKeychain(Arc::new(kassensturz_platform::MemoryKeychain::new()));
        let key = kassensturz_platform::SecretKey::new("bank-pin", "fake-login-1").expect("key");
        // First run: the setup saves the PIN in the keychain.
        let mut first = Connection::fake_with_keychain(
            FakeBank::new(),
            stores(&keychain),
            SecretStorage::System,
        );
        assert!(first.fake);
        assert_eq!(first.status(), Status::Ready { fake: true });
        assert!(logins(&mut first).expect("logins").is_empty());
        let mut setup = form(FakeBank::PIN);
        setup.pin_mode = PinMode::SaveInKeychain;
        let done = set_up(&mut first, setup).expect("set up");
        assert_eq!(done.login, "fake-login-1");
        assert!(keychain.0.contains(&key));
        assert_eq!(keychain.0.len(), 1);
        drop(first);

        // Next run: the login is back, and the fetch reads the PIN from the
        // keychain; without screens, asking the user would end it.
        let mut next = Connection::fake_with_keychain(
            FakeBank::new(),
            stores(&keychain),
            SecretStorage::System,
        );
        let rows = logins(&mut next).expect("logins");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "fake-login-1");
        assert!(!next.pins.is_remembered(&LoginId::new("fake-login-1")));
        let statement = bank_statement(&mut next);
        assert!(!statement.entries.is_empty());
        // A second setup gets a new ID instead of taking the old login's.
        let done = set_up(&mut next, form(FakeBank::PIN)).expect("set up");
        assert_eq!(done.login, "fake-login-2");

        // Removing the login removes its saved PIN from the keychain.
        remove(&mut next, "fake-login-1").expect("removed");
        assert!(keychain.0.is_empty());
        drop(next);
        let mut after = Connection::fake_with_keychain(
            FakeBank::new(),
            stores(&keychain),
            SecretStorage::System,
        );
        assert!(logins(&mut after).expect("logins").is_empty());
    }

    #[test]
    fn a_keychain_that_cant_be_read_brings_back_no_fake_login() {
        let keychain = SharedKeychain(Arc::new(kassensturz_platform::MemoryKeychain::new()));
        keychain
            .0
            .fail_with(Some(kassensturz_platform::KeychainError::Unavailable));
        let mut connection = Connection::fake_with_keychain(
            FakeBank::new(),
            stores(&keychain),
            SecretStorage::System,
        );
        assert!(logins(&mut connection).expect("logins").is_empty());
        let mut setup = form(FakeBank::PIN);
        setup.pin_mode = PinMode::SaveInKeychain;
        assert_eq!(
            set_up(&mut connection, setup)
                .map(|_| ())
                .map_err(|e| e.code),
            Err("bank.keychain")
        );
    }

    // --- Update all accounts (M6d.7, D-446, D-470) ---

    /// What came of each account, as words.
    fn results(done: &[UpdateFetch]) -> Vec<String> {
        done.iter()
            .map(|fetch| match &fetch.result {
                Updated::Fetched(statement) => format!("fetched {}", statement.entries.len()),
                Updated::Failed(error) => format!("failed {}", error.code),
                Updated::Skipped(reason) => format!("skipped {reason}"),
            })
            .collect()
    }

    #[test]
    fn update_all_goes_bank_by_bank_in_the_order_of_each_banks_first_account() {
        let mut book = Book::create_in_memory().expect("book");
        let names = [
            "A at bank 1",
            "B at bank 2",
            "C unknown",
            "D at bank 1",
            "E at bank 2",
        ];
        let ids: Vec<_> = names
            .iter()
            .map(|name| account(&mut book, name, None))
            .collect();
        for (n, &id) in ids.iter().enumerate() {
            link_to(&mut book, id, None, Some(&format!("4000 0000 0000 000{n}")));
        }
        let login = |request: &FetchRequest| match request.name.as_bytes()[0] {
            b'A' | b'D' => Some(1),
            b'B' | b'E' => Some(2),
            _ => None,
        };
        let groups = by_login(requests(&book, &ids), login);
        let order: Vec<Vec<&str>> = groups
            .iter()
            .map(|(_, group)| group.iter().map(|request| &request.name[..1]).collect())
            .collect();
        assert_eq!(order, vec![vec!["A", "D"], vec!["B", "E"], vec!["C"]]);
    }

    #[test]
    fn update_all_hands_the_statements_back_and_a_failed_account_does_not_stop_the_rest() {
        let mut book = Book::create_in_memory().expect("book");
        let unknown = account(&mut book, "Closed at the bank", None);
        let card = account(&mut book, "Card", None);
        let current = account(&mut book, "Current account", None);
        link_to(&mut book, unknown, Some("BE68539007547034"), None);
        link_to(&mut book, card, None, Some(CARD_NUMBER));
        link_to(&mut book, current, Some("DE89370400440532013000"), None);
        // A statement the fetch page fetched earlier is stale once updated.
        bank_fetch::put(bank_fetch::Fetch {
            account: current,
            statement: bank_statement(&mut fetch_connection(|_| {})),
            day: today(),
        });
        let all = [unknown, card, current];
        let stop = AtomicBool::new(false);
        let mut connection = fetch_connection(|_| {});
        let done =
            update_all(&mut connection, requests(&book, &all), today(), &stop).expect("fetched");
        assert_eq!(
            results(&done),
            vec!["failed bank.no-bank-account", "fetched 2", "fetched 4"]
        );
        assert!(done.iter().all(|fetch| fetch.day == today()));
        assert_eq!(done[1].name, "Card");
        assert!(
            bank_fetch::take(current).is_none(),
            "nothing kept for the page"
        );

        // The bank can't be reached: every bank would fail the same way.
        let mut connection = fetch_connection(|bank| bank.fail_next(Failure::Unreachable));
        let done = update_all(
            &mut connection,
            requests(&book, &[card, current]),
            today(),
            &stop,
        )
        .expect("fetched");
        assert_eq!(
            results(&done),
            vec!["failed bank.unreachable", "skipped earlier-failure"]
        );
    }

    #[test]
    fn a_cancelled_pin_skips_its_banks_accounts_and_stop_skips_the_rest() {
        let mut book = Book::create_in_memory().expect("book");
        let current = account(&mut book, "Current account", None);
        let card = account(&mut book, "Card", None);
        link_to(&mut book, current, Some("DE89370400440532013000"), None);
        link_to(&mut book, card, None, Some(CARD_NUMBER));
        // No screens to answer: the PIN dialog counts as cancelled.
        let mut connection = fetch_connection(|_| {});
        connection
            .pins
            .forget(&LoginId::new("fake-login-1"))
            .expect("forgotten");
        let stop = AtomicBool::new(false);
        let done = update_all(
            &mut connection,
            requests(&book, &[current, card]),
            today(),
            &stop,
        )
        .expect("ran");
        assert_eq!(
            results(&done),
            vec!["skipped cancelled", "skipped cancelled"]
        );

        let stop = AtomicBool::new(true);
        let mut connection = fetch_connection(|_| {});
        let done = update_all(
            &mut connection,
            requests(&book, &[current, card]),
            today(),
            &stop,
        )
        .expect("ran");
        assert_eq!(results(&done), vec!["skipped stopped", "skipped stopped"]);
    }

    #[test]
    fn only_errors_of_one_bank_end_only_that_bank() {
        for error in [
            BankError::Aborted,
            BankError::PinRejected,
            BankError::TanNeeded,
            BankError::TanMethodUnsupported,
            BankError::UntrustedServer,
        ] {
            assert!(
                ends_the_login(&error) && ends_the_fetch(&error),
                "{error:?}"
            );
        }
        for error in [
            BankError::Unreachable,
            BankError::NotRegistered,
            BankError::Keychain,
        ] {
            assert!(
                !ends_the_login(&error) && ends_the_fetch(&error),
                "{error:?}"
            );
        }
    }
}
