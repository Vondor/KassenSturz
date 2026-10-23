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
//! Without AqBanking, or with `KASSENSTURZ_FAKE_BANK=1` (for trying the
//! screens, marked in them), the worker holds the fake bank, which asks
//! for a TAN at every connection (D-055).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use kassensturz_bank::{
    AqBankingBank, Availability, Bank, BankAccount, BankAccountKind, BankCode, BankDialogs,
    BankError, BankQuestion, CustomerId, FINTS_REGISTRATION_KEY, FakeBank, Iban, KeychainPinStore,
    Login, LoginId, LoginKind, LoginSetup, Pin, PinMode, PinSession, QuestionLevel, SecondFactor,
    Setup, Tan, TanRequest, remove_login, setup_for,
};
use kassensturz_core::{BankLink, ErrorParam, Report};
use kassensturz_platform::SystemKeychain;
use kassensturz_store::Book;

use crate::errors::ErrorText;

/// The environment variable that puts the fake bank behind the screens.
pub const FAKE_BANK_VARIABLE: &str = "KASSENSTURZ_FAKE_BANK";

/// What the worker holds: the backend, or why it couldn't be opened, and
/// the PINs of this run of the app (D-057).
pub struct Connection {
    bank: Result<Box<dyn Bank>, BankError>,
    /// The PINs, remembered or through the keychain.
    pub pins: PinSession,
    /// Whether the backend may talk to a bank: this build has a FinTS
    /// registration (D-058), or the backend is the fake bank.
    pub registered: bool,
    /// Whether the backend is the fake bank, which the screens say.
    pub fake: bool,
}

impl Connection {
    /// The real backend in `folder` (D-139), with the PINs in the system
    /// keychain.
    pub fn real(folder: &std::path::Path) -> Self {
        Self {
            bank: AqBankingBank::open(folder).map(|bank| Box::new(bank) as Box<dyn Bank>),
            pins: PinSession::new(Box::new(KeychainPinStore::new(Box::new(
                SystemKeychain::new(),
            )))),
            registered: FINTS_REGISTRATION_KEY.is_some(),
            fake: false,
        }
    }

    /// The fake bank, asking for a TAN at every connection, with `pins`.
    pub fn fake(bank: FakeBank, pins: PinSession) -> Self {
        Self {
            bank: Ok(Box::new(bank)),
            pins,
            registered: true,
            fake: true,
        }
    }

    /// The connection the app uses: the fake bank when asked for, else
    /// AqBanking in KassenSturz's data folder, or `bank.folder` without
    /// one.
    pub fn for_app(folder: Option<PathBuf>) -> Self {
        let wants_fake = std::env::var_os(FAKE_BANK_VARIABLE).is_some_and(|value| value == "1");
        match folder {
            _ if wants_fake => Self::fake(
                FakeBank::new().with_second_factor(SecondFactor::Tan),
                PinSession::new(Box::new(kassensturz_bank::MemoryPinStore::new())),
            ),
            Some(folder) => Self::real(&folder.join("aqbanking")),
            // Without a data folder AqBanking has nowhere to keep its
            // settings: the screens say so.
            None => Self {
                bank: Err(BankError::Folder),
                pins: PinSession::new(Box::new(kassensturz_bank::MemoryPinStore::new())),
                registered: FINTS_REGISTRATION_KEY.is_some(),
                fake: false,
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
        let registered = self.registered;
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
    /// A call that ran in the background ended.
    SetupDone(Result<SetupDone, ErrorText>),
}

/// The user's answer to an [`Event`].
#[derive(Debug)]
pub enum Answer {
    /// The TAN typed, or `None` for "Cancel".
    Tan(Option<Tan>),
    /// The button chosen, or `None` when the dialog was closed.
    Button(Option<usize>),
}

/// The screens' ear: how the worker tells them something, from its thread.
pub type Notify = Arc<dyn Fn(Event) + Send + Sync>;

/// The dialogs the worker gives the backend: each request becomes an
/// [`Event`], and the worker waits for the [`Answer`].
struct AskTheScreens {
    notify: Notify,
    answers: Receiver<Answer>,
}

impl BankDialogs for AskTheScreens {
    fn tan(&mut self, request: &TanRequest) -> Option<Tan> {
        (self.notify)(Event::Tan(request.clone()));
        loop {
            match self.answers.recv() {
                Ok(Answer::Tan(tan)) => return tan,
                // A late answer to an earlier question: not this one's.
                Ok(Answer::Button(_)) => {}
                Err(_) => return None,
            }
        }
    }

    fn question(&mut self, question: &BankQuestion) -> Option<usize> {
        (self.notify)(Event::Question(question.clone()));
        loop {
            match self.answers.recv() {
                Ok(Answer::Button(button)) => return button,
                Ok(Answer::Tan(_)) => {}
                Err(_) => return None,
            }
        }
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
        let dialogs_notify = Arc::clone(&notify);
        let spawned = thread::Builder::new()
            .name("kassensturz-bank".to_owned())
            .spawn(move || {
                let mut connection = open();
                if let Ok(bank) = connection.bank() {
                    bank.set_dialogs(Box::new(AskTheScreens {
                        notify: dialogs_notify,
                        answers: answer_queue,
                    }));
                }
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
    ErrorText {
        code: report.code,
        params: report
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
            .collect(),
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
            LoginRow {
                id: login.id.to_string(),
                bank: name.unwrap_or_else(|| login.bank.code().to_owned()),
                bank_code: login.bank.code().to_owned(),
                kind: kind_key(login.kind),
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
    /// `unknown-login-kind` or `chip-card-only`.
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
    pub url: String,
    /// Whether the address must be typed by hand.
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
    if !connection.registered {
        return Err(error_text(&BankError::NotRegistered));
    }
    add_login(connection, form, code).map_err(|error| error_text(&error))
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
    let plain = |text: &str| -> String {
        text.chars()
            .filter(|character| !character.is_whitespace())
            .map(|character| character.to_ascii_uppercase())
            .collect()
    };
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
        let iban: String = bank_account
            .iban
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect();
        book.link_bank_account(&BankLink {
            account: account.id,
            iban: Some(iban).filter(|iban| !iban.is_empty()),
            bank_code: Some(bank_account.bank_code.clone()),
            number: Some(bank_account.number.trim().to_owned()).filter(|n| !n.is_empty()),
            last_fetched_on: None,
        })?;
        linked += 1;
    }
    Ok(linked)
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
        ] {
            let iban = german_iban(code, "0000000001");
            let lookup = look_up(&mut fake(None), &iban).expect("looked up");
            assert_eq!((lookup.state, lookup.ask_url), (state, ask_url), "{code}");
            assert_eq!(lookup.customer_id, "optional", "{code}");
        }
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
}
