// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A safe session over AqBanking: the only place that calls the C
//! functions (D-060, threat T-09).
//!
//! [`Session::open`] runs the start-up the libraries need (`GWEN_Init`, a
//! Gwenhywfar GUI with KassenSturz's own callbacks, `AB_Banking_new` with
//! KassenSturz's own folder, `AB_Banking_Init`) and dropping the session
//! runs the shutdown. Between the two, the session reads bank information,
//! lists the accounts AqBanking knows, fetches balances and transactions,
//! and runs the FinTS backend's setup commands (D-144). Everything comes
//! back as plain Rust values with owned text, read under the rules in
//! [`crate::text`]: every pointer is checked for null, every string copied
//! at once with a limit, and a malformed answer is an error, never a crash.
//!
//! # Callbacks
//!
//! AqBanking asks for the PIN, reports a wrong one, logs and shows
//! certificates through Gwenhywfar's GUI (D-140), and asks for TANs and
//! questions such as "approve this in your banking app" (D-151). The
//! session installs its own functions on a plain `GWEN_Gui` and routes them
//! to the [`SessionHandler`] it was given; the password request carries no
//! token (the token holds the login name, threat T-19) and the handler
//! answers from what the app knows, asking the user through the app's own
//! dialogs where it must. Certificates are checked here, with a fixed
//! rule: only one whose every check passed is accepted (D-140, T-12).
//!
//! Text from the libraries is often written twice, as plain text followed
//! by an HTML copy (`…<html>…</html>`); the session passes on the plain
//! part only ([`plain_text`]), so no screen is tempted to show the HTML
//! (D-143).
//!
//! # One session per thread
//!
//! Gwenhywfar keeps the current GUI per thread, so the session is tied to
//! the thread that opened it, and a thread has at most one session open.

#![allow(non_upper_case_globals)] // bindgen names the C enumerators as the headers do

use std::cell::{Cell, RefCell};
use std::ffi::CString;
use std::fmt;
use std::os::raw::{c_char, c_int};
use std::path::{Path, PathBuf};
use std::ptr;

use zeroize::Zeroizing;

use crate::aqbanking::{
    AB_ACCOUNT_SPEC_LIST, AB_AccountType_Bank, AB_AccountType_Cash, AB_AccountType_Checking,
    AB_AccountType_Credit, AB_AccountType_CreditCard, AB_AccountType_Investment,
    AB_AccountType_MoneyMarket, AB_AccountType_Savings, AB_BALANCE, AB_BANKINFO, AB_BANKING,
    AB_Balance_TypeBooked, AB_Balance_TypeNoted, AB_IMEXPORTER_ACCOUNTINFO, AB_TRANSACTION,
    AB_Transaction_CommandGetBalance, AB_Transaction_CommandGetTransactions,
    AB_Transaction_TypeNotedStatement, AB_Transaction_TypeStatement, AB_VALUE, AqBanking,
};
use crate::gwenhywfar::{
    GWEN_DATE, GWEN_ERROR_USER_ABORTED, GWEN_GUI, GWEN_GUI_INPUT_FLAGS_CONFIRM,
    GWEN_GUI_INPUT_FLAGS_TAN, GWEN_GUI_MSG_FLAGS_TYPE_ERROR, GWEN_GUI_MSG_FLAGS_TYPE_MASK,
    GWEN_GUI_MSG_FLAGS_TYPE_WARN, GWEN_GUI_PASSWORD_METHOD, GWEN_GUI_PASSWORD_STATUS,
    GWEN_Gui_PasswordMethod_Mask, GWEN_Gui_PasswordMethod_Text, GWEN_Gui_PasswordMethod_Unknown,
    GWEN_Gui_PasswordStatus_Bad, GWEN_Gui_PasswordStatus_Ok, GWEN_Gui_PasswordStatus_Remove,
    GWEN_Gui_PasswordStatus_Unused, GWEN_Gui_PasswordStatus_Used, GWEN_LOGGER_LEVEL,
    GWEN_LoggerLevel_Alert, GWEN_LoggerLevel_Critical, GWEN_LoggerLevel_Emergency,
    GWEN_LoggerLevel_Error, GWEN_LoggerLevel_Info, GWEN_LoggerLevel_Notice,
    GWEN_LoggerLevel_Warning, GWEN_SSL_CERT_FLAGS_OK, GWEN_SSLCERTDESCR, GWEN_SYNCIO,
};
use crate::load::{Libraries, LoadError};
use crate::text::{copy_text, text_from_buffer};

/// The FinTS backend's name for `AB_Banking_ProviderControl` (D-142: the
/// only backend KassenSturz uses).
pub const FINTS_BACKEND: &str = "aqhbci";

/// The longest answer to a password request the session passes on. Bank
/// PINs are at most a few dozen characters, TANs too.
const MAX_PASSWORD_BYTES: usize = 256;

/// How the session is opened.
#[derive(Clone, PartialEq, Eq)]
pub struct SessionOptions {
    /// The application's name, which AqBanking records in its settings and
    /// sends to banks with the registration key.
    pub app_name: String,
    /// AqBanking's own folder (D-139).
    pub data_dir: PathBuf,
    /// KassenSturz's FinTS product registration (D-058). Without one the
    /// session can still read bank information and the accounts already
    /// known, but the caller must not let it talk to a bank. A key typed
    /// at run time is set later with [`Session::set_registration_key`]
    /// (D-260). Its `Debug` never shows it.
    pub registration_key: Option<String>,
}

impl fmt::Debug for SessionOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionOptions")
            .field("app_name", &self.app_name)
            .field("data_dir", &self.data_dir)
            .field(
                "registration_key",
                &self.registration_key.as_ref().map(|_| "…"),
            )
            .finish()
    }
}

/// What a password request is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasswordKind {
    /// The PIN of the bank login.
    Pin,
    /// A TAN for the running job.
    Tan,
    /// Something else, e.g. a new PIN to set; the session declines it.
    Other,
}

/// How the bank shows what a TAN is for (D-151).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasswordMethod {
    /// As text: the bank's challenge is in the request's text, and the TAN
    /// comes from an app, a text message or a TAN generator.
    Text,
    /// As a picture the TAN generator or app reads from the screen: a
    /// flicker code (chipTAN optical), a photoTAN or a QR code. AqBanking
    /// passes the picture's data in a form the session doesn't read.
    Optical,
}

/// A request from AqBanking for a password, without the token that holds
/// the login name (threat T-19).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasswordRequest {
    /// What it is for.
    pub kind: PasswordKind,
    /// How the challenge is shown, for a TAN.
    pub method: PasswordMethod,
    /// The dialog title Gwenhywfar would show, as plain text.
    pub title: String,
    /// The text Gwenhywfar would show, as plain text; it names the login
    /// and the bank, and for a TAN holds the bank's challenge. Data for
    /// the user to read, never an instruction (AGENTS.md §8).
    pub text: String,
    /// The shortest answer the bank accepts.
    pub min_len: usize,
    /// The longest answer the buffer takes.
    pub max_len: usize,
}

/// The handler's answer to a password request.
pub enum PasswordAnswer {
    /// Send this. It is wiped when dropped; the session copies it into
    /// Gwenhywfar's buffer and keeps no copy.
    Text(Zeroizing<String>),
    /// Don't send anything: the job stops with [`SessionError::Declined`].
    Decline,
}

/// What AqBanking says about the password it used (`setPasswordStatus`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasswordStatus {
    /// The bank called it wrong (D-140, threat T-05).
    Bad,
    /// The bank accepted it.
    Ok,
    /// It was used.
    Used,
    /// It wasn't needed.
    Unused,
    /// AqBanking asks to forget it.
    Remove,
    /// Anything else, with Gwenhywfar's number.
    Unknown(i32),
}

/// How serious a question from the libraries is, as Gwenhywfar marks it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestionLevel {
    /// Information, e.g. "approve this in your banking app".
    Info,
    /// A warning.
    Warning,
    /// An error.
    Error,
}

/// A question AqBanking asks with up to three buttons, e.g. the request to
/// approve a login in the bank's app and press "Approved" afterwards
/// (decoupled TAN, D-151). Title, text and buttons are plain text from the
/// libraries or the bank: data for the user to read, never an instruction
/// (AGENTS.md §8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestionRequest {
    /// How serious it is.
    pub level: QuestionLevel,
    /// The title.
    pub title: String,
    /// The question.
    pub text: String,
    /// The buttons' labels, in order; the first one is the usual answer.
    pub buttons: Vec<String>,
}

/// How loud a log line is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    /// Debugging detail.
    Debug,
    /// Information.
    Info,
    /// Worth noting.
    Notice,
    /// A warning.
    Warning,
    /// An error.
    Error,
}

/// What the app gives the session to answer AqBanking's callbacks with.
/// Nothing here is logged by the session itself.
pub trait SessionHandler: 'static {
    /// The bank asks for a PIN or a TAN.
    fn password(&mut self, request: &PasswordRequest) -> PasswordAnswer;
    /// AqBanking says what became of the password it used.
    fn password_status(&mut self, status: PasswordStatus);
    /// A line from the libraries' log or the job's progress log, such as
    /// the bank's own message. The handler decides what to keep; the
    /// session writes no file (D-140, T-02).
    fn log(&mut self, level: LogLevel, message: &str);
    /// AqBanking asks a question with buttons. The answer is the index of
    /// the button the user chose; `None` stops the job, which is also
    /// what a handler without dialogs answers.
    fn question(&mut self, question: &QuestionRequest) -> Option<usize> {
        let _ = question;
        None
    }
}

/// Why the session couldn't do what was asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionError {
    /// The libraries aren't usable (D-054, D-138).
    Load(String),
    /// Another session is open on this thread.
    AlreadyOpen,
    /// AqBanking couldn't start with the folder given.
    Init(i32),
    /// A library call failed with Gwenhywfar's error number (negative), or
    /// a setup command with its own result (positive).
    Library(i32),
    /// The handler declined a password request, so the job stopped.
    Declined(PasswordKind),
    /// The bank's server certificate didn't pass every check, so nothing
    /// was sent (D-140, threat T-12).
    UntrustedServer,
    /// The library answered with something that can't be read.
    BadData(&'static str),
    /// A value to pass to the library held a zero byte.
    Argument,
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load(reason) => write!(f, "the libraries can't be used: {reason}"),
            Self::AlreadyOpen => f.write_str("another AqBanking session is open on this thread"),
            Self::Init(code) => write!(f, "AqBanking couldn't start ({code})"),
            Self::Library(code) => write!(f, "AqBanking answered with error {code}"),
            Self::Declined(kind) => write!(f, "the {kind:?} request was declined"),
            Self::UntrustedServer => f.write_str("the bank's server certificate was refused"),
            Self::BadData(what) => write!(f, "AqBanking's answer can't be read: {what}"),
            Self::Argument => f.write_str("a value for AqBanking held a zero byte"),
        }
    }
}

impl std::error::Error for SessionError {}

impl From<LoadError> for SessionError {
    fn from(error: LoadError) -> Self {
        Self::Load(error.to_string())
    }
}

/// A bank as AqBanking's bank information knows it (item 93 of the
/// Proposals list: the external source).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct BankRecord {
    /// The bank's code, as the record gives it.
    pub code: Option<String>,
    /// The bank's name.
    pub name: Option<String>,
    /// Its BIC.
    pub bic: Option<String>,
    /// Its online services.
    pub services: Vec<ServiceRecord>,
}

/// One service of a bank, as the bank information lists it: for FinTS the
/// type is `HBCI`, the mode `PINTAN` or a key-file mode such as `RDH10`,
/// and the version `3.0`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ServiceRecord {
    /// The service's type.
    pub kind: Option<String>,
    /// The server's address.
    pub address: Option<String>,
    /// How customers log in.
    pub mode: Option<String>,
    /// The protocol version.
    pub version: Option<String>,
}

/// What kind of account AqBanking says it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountKind {
    /// A current account.
    Checking,
    /// A savings account.
    Savings,
    /// A credit card.
    CreditCard,
    /// A bank account of another kind: investment, cash, money market,
    /// credit, or "bank".
    OtherBank,
    /// Not said, with AqBanking's number.
    Unknown(i32),
}

/// An account AqBanking knows, as its account specification lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountRecord {
    /// AqBanking's unique ID of the account.
    pub unique_id: u32,
    /// What kind of account.
    pub kind: AccountKind,
    /// The bank's country, lower case as AqBanking keeps it.
    pub country: Option<String>,
    /// The bank code.
    pub bank_code: Option<String>,
    /// The account number.
    pub account_number: Option<String>,
    /// The sub-account number, if any.
    pub sub_account_number: Option<String>,
    /// The IBAN.
    pub iban: Option<String>,
    /// The BIC.
    pub bic: Option<String>,
    /// The currency.
    pub currency: Option<String>,
    /// The account's name at the bank.
    pub name: Option<String>,
}

/// A calendar date as the library gave it, not yet checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateRecord {
    /// The year.
    pub year: i32,
    /// The month, 1 to 12.
    pub month: i32,
    /// The day, 1 to 31.
    pub day: i32,
}

/// An amount as the fraction's text (D-141) and its currency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AmountRecord {
    /// `numerator/denominator`, or a whole number.
    pub fraction: String,
    /// The currency code, if the value has one.
    pub currency: Option<String>,
}

/// What kind of balance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BalanceKind {
    /// Booked.
    Booked,
    /// Noted, i.e. with pending transactions.
    Noted,
    /// Another kind, with AqBanking's number.
    Other(i32),
}

/// A balance the bank sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BalanceRecord {
    /// Which kind.
    pub kind: BalanceKind,
    /// Its date.
    pub date: Option<DateRecord>,
    /// The amount.
    pub amount: AmountRecord,
}

/// What kind of transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionKind {
    /// Booked.
    Statement,
    /// Not booked yet.
    NotedStatement,
    /// Another kind, with AqBanking's number.
    Other(i32),
}

/// A transaction the bank sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransactionRecord {
    /// Which kind.
    pub kind: TransactionKind,
    /// The bank's own ID of it, if it sent one.
    pub bank_id: Option<String>,
    /// The booking date.
    pub booking_date: Option<DateRecord>,
    /// The value date.
    pub value_date: Option<DateRecord>,
    /// The amount.
    pub amount: AmountRecord,
    /// The other party's name.
    pub remote_name: Option<String>,
    /// The other party's IBAN.
    pub remote_iban: Option<String>,
    /// The purpose, lines separated by newlines.
    pub purpose: Option<String>,
}

/// What a fetch brought for one account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchedAccountRecord {
    /// AqBanking's unique ID of the account.
    pub unique_id: u32,
    /// The IBAN the bank named.
    pub iban: Option<String>,
    /// The account number the bank named.
    pub account_number: Option<String>,
    /// The bank code the bank named.
    pub bank_code: Option<String>,
    /// The currency the bank named.
    pub currency: Option<String>,
    /// The balances.
    pub balances: Vec<BalanceRecord>,
    /// The transactions.
    pub transactions: Vec<TransactionRecord>,
}

/// What a fetch brought.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct FetchRecord {
    /// Per account the bank answered for.
    pub accounts: Vec<FetchedAccountRecord>,
    /// The messages the bank or the backend added.
    pub messages: Vec<String>,
}

/// The callbacks reach the handler through this, so the session's type
/// doesn't have to be known there.
trait Dispatch {
    fn password(&self, request: &PasswordRequest) -> PasswordAnswer;
    fn password_status(&self, status: PasswordStatus);
    fn log(&self, level: LogLevel, message: &str);
    fn question(&self, question: &QuestionRequest) -> Option<usize>;
}

impl<H: SessionHandler> Dispatch for RefCell<H> {
    fn password(&self, request: &PasswordRequest) -> PasswordAnswer {
        match self.try_borrow_mut() {
            Ok(mut handler) => handler.password(request),
            Err(_) => PasswordAnswer::Decline,
        }
    }

    fn password_status(&self, status: PasswordStatus) {
        if let Ok(mut handler) = self.try_borrow_mut() {
            handler.password_status(status);
        }
    }

    fn log(&self, level: LogLevel, message: &str) {
        if let Ok(mut handler) = self.try_borrow_mut() {
            handler.log(level, message);
        }
    }

    fn question(&self, question: &QuestionRequest) -> Option<usize> {
        self.try_borrow_mut()
            .ok()
            .and_then(|mut handler| handler.question(question))
    }
}

/// What the callbacks of the thread's session need. Set while a session
/// is open on this thread, cleared when it is dropped.
struct Current {
    handler: *const dyn Dispatch,
    /// The session's Gwenhywfar, for the certificate callback, which
    /// needs the library's own readers for the descriptor.
    gwen: *const crate::gwenhywfar::Gwenhywfar,
    /// What happened during the running call, for the error mapping.
    declined: Cell<Option<PasswordKind>>,
    certificate_refused: Cell<bool>,
}

thread_local! {
    static CURRENT: Cell<Option<*const Current>> = const { Cell::new(None) };
}

fn with_current<T>(f: impl FnOnce(&Current) -> T) -> Option<T> {
    let pointer = CURRENT.with(Cell::get)?;
    // SAFETY: the pointer is set by `Session::open` to a `Current` the
    // session owns on the heap, and cleared in `Session::drop` before the
    // `Current` is freed; callbacks only run during the session's calls.
    Some(f(unsafe { &*pointer }))
}

/// An open session, see the module documentation.
pub struct Session<H: SessionHandler> {
    /// Boxed so that the pointer `Current` keeps stays valid when the
    /// session is moved.
    libraries: Box<Libraries>,
    ab: *mut AB_BANKING,
    gui: *mut GWEN_GUI,
    handler: Box<RefCell<H>>,
    current: Box<Current>,
    options: SessionOptions,
    /// Whether AqBanking has a registration key: from the options, or
    /// set later (D-260).
    registered: Cell<bool>,
}

impl<H: SessionHandler> fmt::Debug for Session<H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("data_dir", &self.options.data_dir)
            .finish_non_exhaustive()
    }
}

impl<H: SessionHandler> Session<H> {
    /// Opens a session over the installed libraries (D-054).
    pub fn open(options: SessionOptions, handler: H) -> Result<Self, SessionError> {
        // SAFETY: loading runs the libraries' initialisers; the session
        // owns the `Libraries` and so outlives every pointer taken from it.
        let libraries = unsafe { Libraries::load() }?;
        Self::open_with(libraries, options, handler)
    }

    /// Opens a session over the libraries at `gwenhywfar` and `aqbanking`,
    /// e.g. a stub library in a test.
    pub fn open_from(
        gwenhywfar: &Path,
        aqbanking: &Path,
        options: SessionOptions,
        handler: H,
    ) -> Result<Self, SessionError> {
        // SAFETY: as in `open`.
        let libraries = unsafe { Libraries::load_from(gwenhywfar, aqbanking) }?;
        Self::open_with(libraries, options, handler)
    }

    fn open_with(
        libraries: Libraries,
        options: SessionOptions,
        handler: H,
    ) -> Result<Self, SessionError> {
        if CURRENT.with(Cell::get).is_some() {
            return Err(SessionError::AlreadyOpen);
        }
        let app_name = c_string(&options.app_name)?;
        let data_dir = c_string(&options.data_dir.to_string_lossy())?;
        let key = options
            .registration_key
            .as_deref()
            .map(c_string)
            .transpose()?;
        let libraries = Box::new(libraries);
        let handler = Box::new(RefCell::new(handler));
        let current = Box::new(Current {
            handler: ptr::from_ref::<RefCell<H>>(&handler) as *const dyn Dispatch,
            gwen: ptr::from_ref(libraries.gwenhywfar()),
            declined: Cell::new(None),
            certificate_refused: Cell::new(false),
        });
        let gwen = libraries.gwenhywfar();
        let aq = libraries.aqbanking();

        // SAFETY: the start-up sequence the libraries document, with the
        // thread-local `Current` set before any callback can run; every
        // failure path undoes what was done, in reverse order.
        unsafe {
            let rv = gwen.GWEN_Init();
            if rv < 0 {
                return Err(SessionError::Init(rv));
            }
            CURRENT.with(|cell| cell.set(Some(ptr::from_ref(&*current))));
            quiet_logs(gwen);
            let gui = gwen.GWEN_Gui_new();
            if gui.is_null() {
                CURRENT.with(|cell| cell.set(None));
                gwen.GWEN_Fini();
                return Err(SessionError::Init(0));
            }
            gwen.GWEN_Gui_SetGetPasswordFn(gui, Some(get_password));
            gwen.GWEN_Gui_SetSetPasswordStatusFn(gui, Some(set_password_status));
            gwen.GWEN_Gui_SetLogHookFn(gui, Some(log_hook));
            gwen.GWEN_Gui_SetCheckCertFn(gui, Some(check_certificate));
            gwen.GWEN_Gui_SetProgressLogFn(gui, Some(progress_log));
            gwen.GWEN_Gui_SetMessageBoxFn(gui, Some(message_box));
            gwen.GWEN_Gui_SetGui(gui);

            let ab = aq.AB_Banking_new(app_name.as_ptr(), data_dir.as_ptr(), 0);
            if ab.is_null() {
                gwen.GWEN_Gui_SetGui(ptr::null_mut());
                gwen.GWEN_Gui_free(gui);
                CURRENT.with(|cell| cell.set(None));
                gwen.GWEN_Fini();
                return Err(SessionError::Init(0));
            }
            if let Some(key) = &key {
                aq.AB_Banking_RuntimeConfig_SetCharValue(
                    ab,
                    c"fintsRegistrationKey".as_ptr(),
                    key.as_ptr(),
                );
            }
            let rv = aq.AB_Banking_Init(ab);
            if rv < 0 {
                aq.AB_Banking_free(ab);
                gwen.GWEN_Gui_SetGui(ptr::null_mut());
                gwen.GWEN_Gui_free(gui);
                CURRENT.with(|cell| cell.set(None));
                gwen.GWEN_Fini();
                return Err(SessionError::Init(rv));
            }
            Ok(Session {
                libraries,
                ab,
                gui,
                handler,
                current,
                registered: Cell::new(key.is_some()),
                options,
            })
        }
    }

    /// The options the session was opened with.
    pub fn options(&self) -> &SessionOptions {
        &self.options
    }

    /// Whether AqBanking has a registration key (D-058): one the session
    /// was opened with, or one set since.
    pub fn is_registered(&self) -> bool {
        self.registered.get()
    }

    /// Gives the open session the registration key `key`, typed by the
    /// user after it was opened (D-260). AqBanking copies it and reads it
    /// each time it writes a message to a bank (`AH_HBCI_GetProductName`
    /// in AqBanking 6.9.2), so it counts from the next call on. The caller
    /// checks the key first; the copy made here is wiped when it is
    /// dropped.
    pub fn set_registration_key(&self, key: &str) -> Result<(), SessionError> {
        if key.contains('\0') {
            return Err(SessionError::Argument);
        }
        let mut terminated = Zeroizing::new(Vec::with_capacity(key.len() + 1));
        terminated.extend_from_slice(key.as_bytes());
        terminated.push(0);
        // SAFETY: `ab` is this session's banking object, alive until the
        // session drops; both strings are terminated and only read during
        // the call, which copies the value.
        unsafe {
            self.libraries
                .aqbanking()
                .AB_Banking_RuntimeConfig_SetCharValue(
                    self.ab,
                    c"fintsRegistrationKey".as_ptr(),
                    terminated.as_ptr().cast::<c_char>(),
                );
        }
        self.registered.set(true);
        Ok(())
    }

    /// The handler, for the app to set what the next call may answer and
    /// to read what happened.
    pub fn handler(&self) -> std::cell::RefMut<'_, H> {
        self.handler.borrow_mut()
    }

    /// The versions the libraries report.
    pub fn versions(&self) -> (crate::Version, crate::Version) {
        (
            self.libraries.gwenhywfar_version(),
            self.libraries.aqbanking_version(),
        )
    }

    fn begin_call(&self) {
        self.current.declined.set(None);
        self.current.certificate_refused.set(false);
    }

    fn map_result(&self, rv: i32) -> Result<(), SessionError> {
        if let Some(kind) = self.current.declined.get() {
            return Err(SessionError::Declined(kind));
        }
        if self.current.certificate_refused.get() {
            return Err(SessionError::UntrustedServer);
        }
        if rv == 0 {
            Ok(())
        } else {
            Err(SessionError::Library(rv))
        }
    }

    /// The bank information AqBanking has for the bank `code` in `country`
    /// (two lower-case letters), if any. AqBanking answers with the first
    /// record of the code; [`Session::bank_infos`] reads all of them.
    pub fn bank_info(&self, country: &str, code: &str) -> Result<Option<BankRecord>, SessionError> {
        let country = c_string(country)?;
        let code = c_string(code)?;
        let aq = self.libraries.aqbanking();
        // SAFETY: `ab` is open; the result is owned by the caller and freed
        // below after its text was copied; every pointer is null-checked.
        unsafe {
            let info: *mut AB_BANKINFO =
                aq.AB_Banking_GetBankInfo(self.ab, country.as_ptr(), ptr::null(), code.as_ptr());
            if info.is_null() {
                return Ok(None);
            }
            let record = bank_record(aq, info);
            aq.AB_BankInfo_free(info);
            Ok(Some(record))
        }
    }

    /// Every record of AqBanking's bank information whose code matches
    /// `pattern` in `country` (two lower-case letters), in the order of the
    /// data. A code is eight digits; a `*` at the end stands for any
    /// further digits, so `5519*` lists the banks of one region. A code can
    /// have several records (branches, old names); the first of them is
    /// the one [`Session::bank_info`] returns, and a service can sit in a
    /// later one.
    pub fn bank_infos(
        &self,
        country: &str,
        pattern: &str,
    ) -> Result<Vec<BankRecord>, SessionError> {
        let country = c_string(country)?;
        let pattern = c_string(pattern)?;
        let aq = self.libraries.aqbanking();
        // SAFETY: `ab` is open; the template and the list are owned here and
        // freed on every path (the list's records with it); every pointer is
        // null-checked.
        unsafe {
            let template = aq.AB_BankInfo_new();
            let list = aq.AB_BankInfo_List2_new();
            if template.is_null() || list.is_null() {
                if !template.is_null() {
                    aq.AB_BankInfo_free(template);
                }
                if !list.is_null() {
                    aq.AB_BankInfo_List2_freeAll(list);
                }
                return Err(SessionError::BadData("no bank information list"));
            }
            aq.AB_BankInfo_SetBankId(template, pattern.as_ptr());
            // "Not found" is an answer, not an error: no record matches.
            let _ = aq.AB_Banking_GetBankInfoByTemplate(self.ab, country.as_ptr(), template, list);
            let mut records = Vec::new();
            let iterator = aq.AB_BankInfo_List2_First(list);
            if !iterator.is_null() {
                let mut info = aq.AB_BankInfo_List2Iterator_Data(iterator);
                while !info.is_null() {
                    records.push(bank_record(aq, info));
                    info = aq.AB_BankInfo_List2Iterator_Next(iterator);
                }
                aq.AB_BankInfo_List2Iterator_free(iterator);
            }
            aq.AB_BankInfo_List2_freeAll(list);
            aq.AB_BankInfo_free(template);
            Ok(records)
        }
    }

    /// The accounts AqBanking knows, from every login.
    pub fn accounts(&self) -> Result<Vec<AccountRecord>, SessionError> {
        let aq = self.libraries.aqbanking();
        let mut list: *mut AB_ACCOUNT_SPEC_LIST = ptr::null_mut();
        // SAFETY: `ab` is open; the list is owned by the caller and freed
        // below; every pointer is null-checked.
        unsafe {
            let rv = aq.AB_Banking_GetAccountSpecList(self.ab, &mut list);
            if rv < 0 || list.is_null() {
                // AqBanking answers "not found" when no account is set up.
                return Ok(Vec::new());
            }
            let mut records = Vec::new();
            let mut spec = aq.AB_AccountSpec_List_First(list);
            while !spec.is_null() {
                records.push(AccountRecord {
                    unique_id: aq.AB_AccountSpec_GetUniqueId(spec),
                    kind: account_kind(aq.AB_AccountSpec_GetType(spec)),
                    country: copy_text(aq.AB_AccountSpec_GetCountry(spec)),
                    bank_code: copy_text(aq.AB_AccountSpec_GetBankCode(spec)),
                    account_number: copy_text(aq.AB_AccountSpec_GetAccountNumber(spec)),
                    sub_account_number: copy_text(aq.AB_AccountSpec_GetSubAccountNumber(spec)),
                    iban: copy_text(aq.AB_AccountSpec_GetIban(spec)),
                    bic: copy_text(aq.AB_AccountSpec_GetBic(spec)),
                    currency: copy_text(aq.AB_AccountSpec_GetCurrency(spec)),
                    name: copy_text(aq.AB_AccountSpec_GetAccountName(spec)),
                });
                spec = aq.AB_AccountSpec_List_Next(spec);
            }
            aq.AB_AccountSpec_List_free(list);
            Ok(records)
        }
    }

    /// Fetches the balance and the transactions of the account with
    /// AqBanking's `unique_id`, from `since` on if given. This talks to the
    /// bank: the caller checks the registration first (D-058). The
    /// messages the bank sent come back even when the fetch failed, as
    /// [`SessionError`] carries none; the handler got them as log lines.
    pub fn fetch(
        &self,
        unique_id: u32,
        since: Option<DateRecord>,
    ) -> Result<FetchRecord, SessionError> {
        let aq = self.libraries.aqbanking();
        let gwen = self.libraries.gwenhywfar();
        self.begin_call();
        // SAFETY: `ab` is open; the command list and the context are owned
        // here and freed on every path; every pointer is null-checked.
        unsafe {
            let context = aq.AB_ImExporterContext_new();
            let commands = aq.AB_Transaction_List2_new();
            if context.is_null() || commands.is_null() {
                return Err(SessionError::BadData("no command list"));
            }
            for command in [
                AB_Transaction_CommandGetBalance,
                AB_Transaction_CommandGetTransactions,
            ] {
                let transaction = aq.AB_Transaction_new();
                if transaction.is_null() {
                    aq.AB_Transaction_List2_freeAll(commands);
                    aq.AB_ImExporterContext_free(context);
                    return Err(SessionError::BadData("no command"));
                }
                aq.AB_Transaction_SetCommand(transaction, command);
                aq.AB_Transaction_SetUniqueAccountId(transaction, unique_id);
                if command == AB_Transaction_CommandGetTransactions
                    && let Some(since) = since
                {
                    let date: *mut GWEN_DATE =
                        gwen.GWEN_Date_fromGregorian(since.year, since.month, since.day);
                    if !date.is_null() {
                        aq.AB_Transaction_SetFirstDate(transaction, date);
                        gwen.GWEN_Date_free(date);
                    }
                }
                aq.AB_Transaction_List2_PushBack(commands, transaction);
            }
            let rv = aq.AB_Banking_SendCommands(self.ab, commands, context);
            let mut record = FetchRecord::default();
            let messages = aq.AB_ImExporterContext_GetMessageList(context);
            if !messages.is_null() {
                let mut message = aq.AB_Message_List_First(messages);
                while !message.is_null() {
                    if let Some(text) = copy_text(aq.AB_Message_GetText(message)) {
                        self.current_log(LogLevel::Notice, &text);
                        record.messages.push(text);
                    }
                    message = aq.AB_Message_List_Next(message);
                }
            }
            let result = self.map_result(rv);
            if result.is_ok() {
                let infos = aq.AB_ImExporterContext_GetAccountInfoList(context);
                if !infos.is_null() {
                    let mut info = aq.AB_ImExporterAccountInfo_List_First(infos);
                    while !info.is_null() {
                        record.accounts.push(self.read_account_info(info));
                        info = aq.AB_ImExporterAccountInfo_List_Next(info);
                    }
                }
            }
            aq.AB_Transaction_List2_freeAll(commands);
            aq.AB_ImExporterContext_free(context);
            result.map(|()| record)
        }
    }

    fn current_log(&self, level: LogLevel, message: &str) {
        if let Ok(mut handler) = self.handler.try_borrow_mut() {
            handler.log(level, message);
        }
    }

    /// # Safety
    ///
    /// `info` points to a live account info of an open context.
    unsafe fn read_account_info(
        &self,
        info: *mut AB_IMEXPORTER_ACCOUNTINFO,
    ) -> FetchedAccountRecord {
        let aq = self.libraries.aqbanking();
        // SAFETY: as promised by the caller; lists and elements are
        // null-checked; the values are copied.
        unsafe {
            let mut record = FetchedAccountRecord {
                unique_id: aq.AB_ImExporterAccountInfo_GetAccountId(info),
                iban: copy_text(aq.AB_ImExporterAccountInfo_GetIban(info)),
                account_number: copy_text(aq.AB_ImExporterAccountInfo_GetAccountNumber(info)),
                bank_code: copy_text(aq.AB_ImExporterAccountInfo_GetBankCode(info)),
                currency: copy_text(aq.AB_ImExporterAccountInfo_GetCurrency(info)),
                balances: Vec::new(),
                transactions: Vec::new(),
            };
            let balances = aq.AB_ImExporterAccountInfo_GetBalanceList(info);
            if !balances.is_null() {
                let mut balance: *mut AB_BALANCE = aq.AB_Balance_List_First(balances);
                while !balance.is_null() {
                    record.balances.push(BalanceRecord {
                        kind: match aq.AB_Balance_GetType(balance) {
                            AB_Balance_TypeBooked => BalanceKind::Booked,
                            AB_Balance_TypeNoted => BalanceKind::Noted,
                            other => BalanceKind::Other(other),
                        },
                        date: self.read_date(aq.AB_Balance_GetDate(balance)),
                        amount: self.read_value(aq.AB_Balance_GetValue(balance)),
                    });
                    balance = aq.AB_Balance_List_Next(balance);
                }
            }
            let transactions = aq.AB_ImExporterAccountInfo_GetTransactionList(info);
            if !transactions.is_null() {
                let mut transaction: *mut AB_TRANSACTION =
                    aq.AB_Transaction_List_First(transactions);
                while !transaction.is_null() {
                    record.transactions.push(TransactionRecord {
                        kind: match aq.AB_Transaction_GetType(transaction) {
                            AB_Transaction_TypeStatement => TransactionKind::Statement,
                            AB_Transaction_TypeNotedStatement => TransactionKind::NotedStatement,
                            other => TransactionKind::Other(other),
                        },
                        bank_id: copy_text(aq.AB_Transaction_GetFiId(transaction)),
                        booking_date: self.read_date(aq.AB_Transaction_GetDate(transaction)),
                        value_date: self.read_date(aq.AB_Transaction_GetValutaDate(transaction)),
                        amount: self.read_value(aq.AB_Transaction_GetValue(transaction)),
                        remote_name: copy_text(aq.AB_Transaction_GetRemoteName(transaction)),
                        remote_iban: copy_text(aq.AB_Transaction_GetRemoteIban(transaction)),
                        purpose: copy_text(aq.AB_Transaction_GetPurpose(transaction)),
                    });
                    transaction = aq.AB_Transaction_List_Next(transaction);
                }
            }
            record
        }
    }

    /// # Safety
    ///
    /// `date` is null or points to a live date.
    unsafe fn read_date(&self, date: *const GWEN_DATE) -> Option<DateRecord> {
        if date.is_null() {
            return None;
        }
        let gwen = self.libraries.gwenhywfar();
        // SAFETY: as promised by the caller.
        unsafe {
            Some(DateRecord {
                year: gwen.GWEN_Date_GetYear(date),
                month: gwen.GWEN_Date_GetMonth(date),
                day: gwen.GWEN_Date_GetDay(date),
            })
        }
    }

    /// # Safety
    ///
    /// `value` is null or points to a live value.
    unsafe fn read_value(&self, value: *const AB_VALUE) -> AmountRecord {
        if value.is_null() {
            return AmountRecord {
                fraction: String::new(),
                currency: None,
            };
        }
        let aq = self.libraries.aqbanking();
        let mut buffer = [0u8; 128];
        // SAFETY: as promised by the caller; the buffer's length is passed
        // with it, and the library zero-terminates within it or fails.
        unsafe {
            let rv = aq.AB_Value_GetNumDenomString(
                value,
                buffer.as_mut_ptr().cast::<c_char>(),
                u32::try_from(buffer.len()).unwrap_or(u32::MAX),
            );
            AmountRecord {
                fraction: if rv < 0 {
                    String::new()
                } else {
                    text_from_buffer(&buffer)
                },
                currency: copy_text(aq.AB_Value_GetCurrency(value)),
            }
        }
    }

    /// Runs one of the FinTS backend's setup commands (D-144), e.g.
    /// `adduser` with its options, the same ones `aqhbci-tool4` takes.
    /// `0` is success; a positive result is the command's own error, a
    /// negative one Gwenhywfar's. Commands that talk to the bank need the
    /// registration (D-058); the caller checks.
    pub fn control(&self, command: &str, arguments: &[&str]) -> Result<(), SessionError> {
        let backend = c_string(FINTS_BACKEND)?;
        let mut owned = Vec::with_capacity(arguments.len() + 1);
        owned.push(c_string(command)?);
        for argument in arguments {
            owned.push(c_string(argument)?);
        }
        let mut argv: Vec<*mut c_char> = owned
            .iter()
            .map(|argument| argument.as_ptr().cast_mut())
            .collect();
        let argc = c_int::try_from(argv.len()).map_err(|_| SessionError::Argument)?;
        self.begin_call();
        // SAFETY: `ab` is open; `argv` holds `argc` pointers to strings that
        // live until the call returns; the backend reads them only.
        let rv = unsafe {
            self.libraries.aqbanking().AB_Banking_ProviderControl(
                self.ab,
                backend.as_ptr(),
                argc,
                argv.as_mut_ptr(),
            )
        };
        self.map_result(rv)
    }
}

impl<H: SessionHandler> Drop for Session<H> {
    fn drop(&mut self) {
        let aq = self.libraries.aqbanking();
        let gwen = self.libraries.gwenhywfar();
        // SAFETY: the shutdown in reverse order of `open_with`; the
        // thread-local is cleared before `current` is freed with `self`.
        unsafe {
            aq.AB_Banking_Fini(self.ab);
            aq.AB_Banking_free(self.ab);
            gwen.GWEN_Gui_SetGui(ptr::null_mut());
            gwen.GWEN_Gui_free(self.gui);
            CURRENT.with(|cell| cell.set(None));
            gwen.GWEN_Fini();
        }
    }
}

/// Copies the text of a bank record and of its services.
///
/// # Safety
///
/// `info` is a valid `AB_BANKINFO`.
unsafe fn bank_record(aq: &AqBanking, info: *mut AB_BANKINFO) -> BankRecord {
    // SAFETY: the caller passes a valid record; the services' list and
    // every text are null-checked before use.
    unsafe {
        let mut record = BankRecord {
            code: copy_text(aq.AB_BankInfo_GetBankId(info)),
            name: copy_text(aq.AB_BankInfo_GetBankName(info)),
            bic: copy_text(aq.AB_BankInfo_GetBic(info)),
            services: Vec::new(),
        };
        let services = aq.AB_BankInfo_GetServices(info);
        if !services.is_null() {
            let mut service = aq.AB_BankInfoService_List_First(services);
            while !service.is_null() {
                record.services.push(ServiceRecord {
                    kind: copy_text(aq.AB_BankInfoService_GetType(service)),
                    address: copy_text(aq.AB_BankInfoService_GetAddress(service)),
                    mode: copy_text(aq.AB_BankInfoService_GetMode(service)),
                    version: copy_text(aq.AB_BankInfoService_GetPversion(service)),
                });
                service = aq.AB_BankInfoService_List_Next(service);
            }
        }
        record
    }
}

fn c_string(text: &str) -> Result<CString, SessionError> {
    CString::new(text).map_err(|_| SessionError::Argument)
}

/// The plain part of a text from the libraries. Gwenhywfar's convention is
/// plain text first, then the same in HTML from `<html>` on; only the part
/// before is kept, trimmed. A text that is HTML alone loses its tags, so no
/// markup ever reaches a screen (D-143).
pub fn plain_text(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let plain = match lower.find("<html>") {
        Some(0) => strip_tags(text),
        Some(start) => text[..start].to_owned(),
        None => text.to_owned(),
    };
    plain.trim().to_owned()
}

/// `text` without anything between `<` and `>`.
fn strip_tags(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut in_tag = false;
    for character in text.chars() {
        match character {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => plain.push(character),
            _ => {}
        }
    }
    plain
}

/// Keeps the libraries' own logging to warnings and errors, which reach
/// the handler through the log hook (D-140: no log file).
///
/// # Safety
///
/// `GWEN_Init` was called.
unsafe fn quiet_logs(gwen: &crate::gwenhywfar::Gwenhywfar) {
    for domain in [c"gwenhywfar", c"aqbanking", c"aqhbci"] {
        // SAFETY: as promised; the domain strings are static.
        unsafe {
            if gwen.GWEN_Logger_Exists(domain.as_ptr()) != 0 {
                gwen.GWEN_Logger_SetLevel(domain.as_ptr(), GWEN_LoggerLevel_Warning);
            }
        }
    }
}

fn account_kind(kind: c_int) -> AccountKind {
    match kind {
        AB_AccountType_Checking => AccountKind::Checking,
        AB_AccountType_Savings => AccountKind::Savings,
        AB_AccountType_CreditCard => AccountKind::CreditCard,
        AB_AccountType_Bank
        | AB_AccountType_Investment
        | AB_AccountType_Cash
        | AB_AccountType_MoneyMarket
        | AB_AccountType_Credit => AccountKind::OtherBank,
        other => AccountKind::Unknown(other),
    }
}

fn log_level(level: GWEN_LOGGER_LEVEL) -> LogLevel {
    match level {
        GWEN_LoggerLevel_Emergency
        | GWEN_LoggerLevel_Alert
        | GWEN_LoggerLevel_Critical
        | GWEN_LoggerLevel_Error => LogLevel::Error,
        GWEN_LoggerLevel_Warning => LogLevel::Warning,
        GWEN_LoggerLevel_Notice => LogLevel::Notice,
        GWEN_LoggerLevel_Info => LogLevel::Info,
        _ => LogLevel::Debug,
    }
}

/// Gwenhywfar's answer for "stop": the job is aborted and the session's
/// error says why.
const ABORT: c_int = GWEN_ERROR_USER_ABORTED;

unsafe extern "C" fn get_password(
    _gui: *mut GWEN_GUI,
    flags: u32,
    _token: *const c_char,
    title: *const c_char,
    text: *const c_char,
    buffer: *mut c_char,
    min_len: c_int,
    max_len: c_int,
    method: GWEN_GUI_PASSWORD_METHOD,
    _params: *mut crate::gwenhywfar::GWEN_DB_NODE,
    _gui_id: u32,
) -> c_int {
    let kind = if flags & GWEN_GUI_INPUT_FLAGS_TAN != 0 {
        PasswordKind::Tan
    } else if flags & GWEN_GUI_INPUT_FLAGS_CONFIRM != 0 {
        PasswordKind::Other
    } else {
        PasswordKind::Pin
    };
    let Ok(max_len) = usize::try_from(max_len) else {
        return ABORT;
    };
    if buffer.is_null() || max_len < 2 {
        return ABORT;
    }
    // SAFETY: the library passes its own terminated strings or null.
    let request = unsafe {
        PasswordRequest {
            kind,
            method: password_method(method),
            title: plain_text(&copy_text(title).unwrap_or_default()),
            text: plain_text(&copy_text(text).unwrap_or_default()),
            min_len: usize::try_from(min_len).unwrap_or(0),
            max_len,
        }
    };
    let answer = with_current(|current| {
        if kind == PasswordKind::Other {
            current.declined.set(Some(kind));
            return None;
        }
        // SAFETY: `handler` points to the session's own cell, see `Current`.
        match unsafe { &*current.handler }.password(&request) {
            PasswordAnswer::Text(text) => Some(text),
            PasswordAnswer::Decline => {
                current.declined.set(Some(kind));
                None
            }
        }
    });
    let Some(Some(text)) = answer else {
        return ABORT;
    };
    let bytes = text.as_bytes();
    if bytes.len() >= max_len.min(MAX_PASSWORD_BYTES) || bytes.contains(&0) {
        with_current(|current| current.declined.set(Some(kind)));
        return ABORT;
    }
    // SAFETY: `buffer` holds `max_len` bytes (Gwenhywfar's contract) and
    // the text is shorter than that, so the copy and the terminator fit.
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr().cast::<c_char>(), buffer, bytes.len());
        buffer.add(bytes.len()).write(0);
    }
    0
}

unsafe extern "C" fn set_password_status(
    _gui: *mut GWEN_GUI,
    _token: *const c_char,
    _pin: *const c_char,
    status: GWEN_GUI_PASSWORD_STATUS,
    _gui_id: u32,
) -> c_int {
    let status = match status {
        GWEN_Gui_PasswordStatus_Bad => PasswordStatus::Bad,
        GWEN_Gui_PasswordStatus_Ok => PasswordStatus::Ok,
        GWEN_Gui_PasswordStatus_Used => PasswordStatus::Used,
        GWEN_Gui_PasswordStatus_Unused => PasswordStatus::Unused,
        GWEN_Gui_PasswordStatus_Remove => PasswordStatus::Remove,
        other => PasswordStatus::Unknown(other),
    };
    with_current(|current| {
        // SAFETY: see `Current`.
        unsafe { &*current.handler }.password_status(status);
    });
    0
}

unsafe extern "C" fn log_hook(
    _gui: *mut GWEN_GUI,
    _domain: *const c_char,
    level: GWEN_LOGGER_LEVEL,
    message: *const c_char,
) -> c_int {
    // SAFETY: the library passes its own terminated string or null.
    if let Some(message) = unsafe { copy_text(message) } {
        with_current(|current| {
            // SAFETY: see `Current`.
            unsafe { &*current.handler }.log(log_level(level), &message);
        });
    }
    // Handled: Gwenhywfar doesn't print it itself.
    1
}

unsafe extern "C" fn progress_log(
    _gui: *mut GWEN_GUI,
    _id: u32,
    level: GWEN_LOGGER_LEVEL,
    message: *const c_char,
) -> c_int {
    // SAFETY: the library passes its own terminated string or null.
    if let Some(message) = unsafe { copy_text(message) } {
        with_current(|current| {
            // SAFETY: see `Current`.
            unsafe { &*current.handler }.log(log_level(level), &message);
        });
    }
    0
}

/// How the libraries' password method shows a TAN's challenge.
fn password_method(method: GWEN_GUI_PASSWORD_METHOD) -> PasswordMethod {
    match method & GWEN_Gui_PasswordMethod_Mask {
        GWEN_Gui_PasswordMethod_Text | GWEN_Gui_PasswordMethod_Unknown => PasswordMethod::Text,
        // The optical methods, and any newer one the session can't show.
        _ => PasswordMethod::Optical,
    }
}

/// A question the libraries would show in a box: logged, then put to the
/// handler, whose answer is the button (1 to 3, Gwenhywfar's numbering).
/// Without an answer the job stops.
unsafe extern "C" fn message_box(
    _gui: *mut GWEN_GUI,
    flags: u32,
    title: *const c_char,
    text: *const c_char,
    b1: *const c_char,
    b2: *const c_char,
    b3: *const c_char,
    _gui_id: u32,
) -> c_int {
    // SAFETY: the library passes its own terminated strings or null.
    let (title, text, buttons) = unsafe {
        (
            plain_text(&copy_text(title).unwrap_or_default()),
            plain_text(&copy_text(text).unwrap_or_default()),
            [b1, b2, b3]
                .into_iter()
                .map_while(|button| copy_text(button))
                .map(|button| plain_text(&button))
                .collect::<Vec<_>>(),
        )
    };
    let request = QuestionRequest {
        level: match flags & GWEN_GUI_MSG_FLAGS_TYPE_MASK {
            GWEN_GUI_MSG_FLAGS_TYPE_WARN => QuestionLevel::Warning,
            GWEN_GUI_MSG_FLAGS_TYPE_ERROR => QuestionLevel::Error,
            _ => QuestionLevel::Info,
        },
        title,
        text,
        buttons,
    };
    let answer = with_current(|current| {
        // SAFETY: see `Current`.
        let handler = unsafe { &*current.handler };
        handler.log(
            LogLevel::Warning,
            &format!("{}: {}", request.title, request.text),
        );
        handler.question(&request)
    });
    match answer.flatten() {
        Some(index) if index < request.buttons.len() => c_int::try_from(index + 1).unwrap_or(ABORT),
        _ => ABORT,
    }
}

/// Only a certificate whose every check passed is accepted (D-140,
/// threat T-12); Gwenhywfar's own check would ask "connect anyway?".
unsafe extern "C" fn check_certificate(
    _gui: *mut GWEN_GUI,
    cert: *const GWEN_SSLCERTDESCR,
    _sio: *mut GWEN_SYNCIO,
    _gui_id: u32,
) -> c_int {
    let verdict = with_current(|current| {
        if cert.is_null() {
            current.certificate_refused.set(true);
            return crate::gwenhywfar::GWEN_ERROR_SSL_SECURITY;
        }
        // SAFETY: `cert` is the library's live descriptor; `gwen` points to
        // the session's boxed library, which outlives every callback.
        let (is_error, flags, status, name) = unsafe {
            let gwen = &*current.gwen;
            (
                gwen.GWEN_SslCertDescr_GetIsError(cert),
                gwen.GWEN_SslCertDescr_GetStatusFlags(cert),
                copy_text(gwen.GWEN_SslCertDescr_GetStatusText(cert)),
                copy_text(gwen.GWEN_SslCertDescr_GetCommonName(cert)),
            )
        };
        let accepted = is_error == 0 && flags == GWEN_SSL_CERT_FLAGS_OK;
        let message = format!(
            "certificate of {}: {} ({})",
            name.unwrap_or_default(),
            status.unwrap_or_default(),
            if accepted { "accepted" } else { "refused" }
        );
        // SAFETY: see `Current`.
        unsafe { &*current.handler }.log(
            if accepted {
                LogLevel::Info
            } else {
                LogLevel::Error
            },
            &message,
        );
        if accepted {
            0
        } else {
            current.certificate_refused.set(true);
            crate::gwenhywfar::GWEN_ERROR_SSL_SECURITY
        }
    });
    verdict.unwrap_or(crate::gwenhywfar::GWEN_ERROR_SSL_SECURITY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_plain_part_of_a_text_is_kept() {
        assert_eq!(plain_text("  Enter the TAN  "), "Enter the TAN");
        assert_eq!(
            plain_text("Approve it.\n<html><p>Approve <b>it</b>.</p></html>"),
            "Approve it."
        );
        assert_eq!(
            plain_text("Approve it.<HTML><b>x</b></HTML>"),
            "Approve it."
        );
        assert_eq!(
            plain_text("<html><p>Only <b>HTML</b> <img src=\"http://x\"></p></html>"),
            "Only HTML"
        );
        assert_eq!(plain_text("a < b and c > d"), "a < b and c > d");
    }

    #[test]
    fn the_password_method_says_whether_the_challenge_is_a_picture() {
        assert_eq!(
            password_method(GWEN_Gui_PasswordMethod_Text),
            PasswordMethod::Text
        );
        assert_eq!(
            password_method(GWEN_Gui_PasswordMethod_Unknown),
            PasswordMethod::Text
        );
        assert_eq!(
            password_method(crate::gwenhywfar::GWEN_Gui_PasswordMethod_OpticalHHD | 3),
            PasswordMethod::Optical
        );
        assert_eq!(password_method(0x0003_0000), PasswordMethod::Optical);
    }
}
