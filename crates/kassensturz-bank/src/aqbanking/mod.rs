// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The real backend: the [`Bank`] over AqBanking's FinTS backend (D-012,
//! D-060, D-142, D-149).
//!
//! Everything unsafe stays in `kassensturz-aqbanking-sys`, whose
//! [`Session`] is the only way AqBanking is called from here; this module
//! turns its records into the crate's types and its errors into error
//! codes (D-034). What it adds:
//!
//! - **The setup of a login** in the IBAN-first flow (Q-035): the setup
//!   screens hand over a [`LoginSetup`]; the backend runs the FinTS
//!   backend's own commands, `adduser`, `getbankinfo`, `getsysid` and
//!   `getaccounts` (the same ones `aqhbci-tool4` runs, D-144), and
//!   removes the login again when one of them fails, so nothing half set
//!   up is kept (D-057). A chip-card login is left to the setup dialog
//!   (`bank.needs-setup-dialog`, M4.6).
//! - **TANs and questions** (D-151): the app's [`BankDialogs`] answer what
//!   the bank asks while a call runs, through the [`Handler`]; a TAN shown
//!   as a picture ends the call as `bank.tan-method-unsupported`, a
//!   cancelled dialog as `bank.aborted`.
//! - **The PIN**: before a call that may reach the bank, the PIN the
//!   [`PinSession`] allows is put into the [`Handler`], which answers
//!   AqBanking's request with it or declines; a declined request ends as
//!   `bank.pin-needed`, a PIN the bank called wrong as `bank.pin-rejected`
//!   and the session forgets it (D-140, threat T-05). While a login is
//!   being set up, its PIN is the one under [`LoginId::pending`].
//! - **AqBanking's own folder** (D-139): the backend reads the logins and
//!   the accounts' owners from it ([`settings`]) and removes the FinTS
//!   message logs AqBanking writes there, at the start and after every
//!   call that could have written one (threat T-06).
//! - **The registration** (D-058): without a FinTS registration number of
//!   its own, KassenSturz never contacts a bank: every call that would
//!   fails with `bank.not-registered` first.
//! - **AqBanking's bank information** as a [`BankDirectory`] (item 93 of
//!   the Proposals list), behind the hand-checked list's corrections
//!   ([`Corrected`]).

mod handler;
mod settings;

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use kassensturz_aqbanking_sys::aqbanking::{AB_ERROR_NETWORK, AB_ERROR_SECURITY};
use kassensturz_aqbanking_sys::gwenhywfar::{
    GWEN_ERROR_ABORTED, GWEN_ERROR_BROKEN_PIPE, GWEN_ERROR_CONN_REFUSED,
    GWEN_ERROR_COULD_NOT_RESOLVE, GWEN_ERROR_HOST_NOT_FOUND, GWEN_ERROR_NET_UNREACHABLE,
    GWEN_ERROR_NOT_CONNECTED, GWEN_ERROR_SSL, GWEN_ERROR_SSL_PREMATURE_CLOSE,
    GWEN_ERROR_SSL_SECURITY, GWEN_ERROR_TIMEOUT, GWEN_ERROR_USER_ABORTED,
};
use kassensturz_aqbanking_sys::session::{
    AccountKind, AccountRecord, BalanceKind, BankRecord, DateRecord, FetchRecord,
    FetchedAccountRecord, PasswordKind, Session, SessionError, SessionOptions, TransactionKind,
};
use kassensturz_aqbanking_sys::{LoadError, Version as LibraryVersion};
use kassensturz_core::{CurrencyCode, Date};

use crate::availability::{Availability, FINTS_REGISTRATION_KEY, Library, Version};
use crate::backend::{
    Bank, BankAccount, BankAccountId, BankAccountKind, Login, LoginId, LoginSetup,
};
use crate::directory::{BankDirectory, BankInfo, Corrected, CustomerId, FintsServer, LoginKind};
use crate::error::BankError;
use crate::fetched::{Fetched, FetchedAmount, FetchedBalance, FetchedTransaction};
use crate::iban::BankCode;
use crate::pin::{Pin, PinAnswer, PinSession};

use self::handler::{Handler, Refusal};
use crate::dialogs::BankDialogs;

/// The name AqBanking knows KassenSturz by, e.g. in its user settings.
const APP_NAME: &str = "KassenSturz";

/// The FinTS version every login is set up with: 3.0, the one German
/// banks run today (D-142).
const FINTS_VERSION: &str = "--hbciversion=300";

/// The FinTS backend's name for a PIN/TAN login.
const PIN_TAN: &str = "pintan";

/// The FinTS backend's name for a chip-card login.
const CHIP_CARD: &str = "ddv";

/// The service kind of a FinTS server in AqBanking's bank information.
const FINTS_SERVICE: &str = "HBCI";

/// The [`Bank`] over AqBanking (D-149).
///
/// One per app, opened on the thread that uses it: AqBanking isn't
/// thread-safe, and the session is bound to its thread (`bank.busy` when
/// another is open there). Dropping it closes AqBanking.
pub struct AqBankingBank {
    /// AqBanking's own folder below KassenSturz's data folder (D-139).
    folder: PathBuf,
    availability: Availability,
    /// The session, when the libraries are there.
    session: Option<Rc<Session<Handler>>>,
    directory: Corrected<AqBankingDirectory>,
}

impl std::fmt::Debug for AqBankingBank {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AqBankingBank")
            .field("folder", &self.folder)
            .field("availability", &self.availability)
            .finish_non_exhaustive()
    }
}

impl AqBankingBank {
    /// Opens the backend with AqBanking's folder at `folder` (D-139),
    /// over the installed libraries (D-054) and the registration this
    /// build has ([`FINTS_REGISTRATION_KEY`]).
    ///
    /// When the libraries aren't there or are too old, the backend still
    /// opens: [`Bank::availability`] says so, and every other call fails
    /// with that error, so the app can show what to install.
    pub fn open(folder: &Path) -> Result<Self, BankError> {
        let probed = kassensturz_aqbanking_sys::probe();
        Self::open_with(folder, FINTS_REGISTRATION_KEY, probed, Session::open)
    }

    /// Opens the backend over the libraries at `gwenhywfar` and
    /// `aqbanking`, with `registration_key` as the registration: for
    /// tests with the stub library, which can have a made-up key.
    pub fn open_from(
        gwenhywfar: &Path,
        aqbanking: &Path,
        folder: &Path,
        registration_key: Option<&str>,
    ) -> Result<Self, BankError> {
        let probed = kassensturz_aqbanking_sys::probe_from(gwenhywfar, aqbanking);
        Self::open_with(folder, registration_key, probed, |options, handler| {
            Session::open_from(gwenhywfar, aqbanking, options, handler)
        })
    }

    fn open_with(
        folder: &Path,
        registration_key: Option<&str>,
        probed: Result<(LibraryVersion, LibraryVersion), LoadError>,
        open: impl FnOnce(SessionOptions, Handler) -> Result<Session<Handler>, SessionError>,
    ) -> Result<Self, BankError> {
        let availability = availability(probed);
        let session = if availability.is_available() {
            fs::create_dir_all(folder).map_err(|_| BankError::Folder)?;
            settings::delete_message_logs(folder).map_err(|_| BankError::Folder)?;
            let options = SessionOptions {
                app_name: APP_NAME.to_owned(),
                data_dir: folder.to_path_buf(),
                registration_key: registration_key.map(str::to_owned),
            };
            Some(Rc::new(open(options, Handler::default()).map_err(
                |error| match error {
                    SessionError::AlreadyOpen => BankError::Busy,
                    SessionError::Load(_) => BankError::NotInstalled,
                    _ => BankError::Folder,
                },
            )?))
        } else {
            None
        };
        Ok(Self {
            folder: folder.to_path_buf(),
            availability,
            directory: Corrected::new(AqBankingDirectory {
                session: session.clone(),
            }),
            session,
        })
    }

    /// AqBanking's folder.
    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// The session, when the backend can be used at all.
    fn session(&self) -> Result<&Session<Handler>, BankError> {
        self.availability.require()?;
        self.session.as_deref().ok_or(BankError::NotInstalled)
    }

    /// Fails before any bank contact when this build has no registration
    /// (D-058).
    fn require_registration(&self, session: &Session<Handler>) -> Result<(), BankError> {
        if session.options().registration_key.is_none() {
            return Err(BankError::NotRegistered);
        }
        Ok(())
    }

    /// Removes the message logs AqBanking may have written (D-139). A
    /// folder that can't be cleaned isn't an error of the call that ran:
    /// the next call tries again.
    fn clean_logs(&self) {
        let _ = settings::delete_message_logs(&self.folder);
    }

    /// The logins AqBanking has set up, by their unique IDs.
    fn user_ids(&self) -> Result<HashSet<u32>, BankError> {
        Ok(settings::users(&self.folder)
            .map_err(|_| BankError::Folder)?
            .into_iter()
            .map(|user| user.unique_id)
            .collect())
    }

    /// The unique ID of `login`, which must be set up.
    fn user_id(&self, login: &LoginId) -> Result<u32, BankError> {
        let id = login
            .as_str()
            .parse()
            .map_err(|_| BankError::UnknownLogin(login.clone()))?;
        if !self.user_ids()?.contains(&id) {
            return Err(BankError::UnknownLogin(login.clone()));
        }
        Ok(id)
    }

    /// Runs one call that may reach the bank, with `pin` as the answer to
    /// a PIN request, and reads back what the handler recorded.
    fn call<T>(
        session: &Session<Handler>,
        pin: Option<Pin>,
        action: impl FnOnce() -> Result<T, SessionError>,
    ) -> Outcome<T> {
        session.handler().begin(pin);
        let result = action();
        let mut handler = session.handler();
        handler.end();
        Outcome {
            result,
            rejected: handler.rejected,
            refusal: handler.refusal,
            message: handler.last_message(),
        }
    }

    /// Runs one setup command for the login `uid` with the PIN `pin`,
    /// turning a failure into the error code and telling `pins` about a
    /// PIN the bank called wrong.
    fn setup_command(
        &self,
        session: &Session<Handler>,
        command: &str,
        arguments: &[&str],
        pin: Option<Pin>,
        login: &LoginId,
        pins: &mut PinSession,
    ) -> Result<(), BankError> {
        let outcome = Self::call(session, pin, || session.control(command, arguments));
        self.clean_logs();
        if outcome.rejected {
            pins.rejected(login)?;
            return Err(BankError::PinRejected);
        }
        outcome.into_result(true)
    }

    /// The commands that make a new login usable, after `adduser`.
    fn connect_new(
        &self,
        session: &Session<Handler>,
        uid: &str,
        pin: Option<Pin>,
        pins: &mut PinSession,
    ) -> Result<(), BankError> {
        let login = LoginId::pending();
        let user = ["-u", uid];
        // The bank's parameters, without a login.
        self.setup_command(session, "getbankinfo", &user, None, &login, pins)?;
        // The system ID the bank gives this login, with the PIN.
        self.setup_command(session, "getsysid", &user, pin.clone(), &login, pins)?;
        // The accounts the login reaches.
        self.setup_command(session, "getaccounts", &user, pin.clone(), &login, pins)?;
        // IBANs for accounts the bank listed without one. A credit card
        // has none, so a failure here leaves the login usable.
        let uid: u32 = uid
            .parse()
            .map_err(|_| BankError::SetupFailed { text: None })?;
        let owners = settings::account_owners(&self.folder).map_err(|_| BankError::Folder)?;
        let without_iban = session
            .accounts()
            .map_err(|error| map_error(error, None, true))?
            .into_iter()
            .any(|account| owners.get(&account.unique_id) == Some(&uid) && account.iban.is_none());
        if without_iban {
            let _ = self.setup_command(session, "getaccsepa", &user, pin, &login, pins);
        }
        Ok(())
    }

    /// The accounts of the login with `uid`, as AqBanking lists them.
    fn accounts_of(
        &self,
        session: &Session<Handler>,
        uid: u32,
        login: &LoginId,
    ) -> Result<Vec<BankAccount>, BankError> {
        let owners = settings::account_owners(&self.folder).map_err(|_| BankError::Folder)?;
        Ok(session
            .accounts()
            .map_err(|error| map_error(error, None, false))?
            .into_iter()
            .filter(|account| owners.get(&account.unique_id) == Some(&uid))
            .map(|account| bank_account(account, login))
            .collect())
    }
}

/// What one call left behind.
struct Outcome<T> {
    result: Result<T, SessionError>,
    rejected: bool,
    refusal: Option<Refusal>,
    message: Option<String>,
}

impl<T> Outcome<T> {
    /// The call's result with its error code: why the handler declined a
    /// request, if it did, says more than the error the call ended with.
    fn into_result(self, setup: bool) -> Result<T, BankError> {
        self.result.map_err(|error| match self.refusal {
            Some(Refusal::PictureTan) => BankError::TanMethodUnsupported,
            Some(Refusal::Cancelled) => BankError::Aborted,
            None => map_error(error, self.message, setup),
        })
    }
}

impl Bank for AqBankingBank {
    fn availability(&self) -> Availability {
        self.availability
    }

    fn directory(&self) -> &dyn BankDirectory {
        &self.directory
    }

    fn set_dialogs(&mut self, dialogs: Box<dyn BankDialogs>) {
        if let Some(session) = &self.session {
            session.handler().dialogs = Some(dialogs);
        }
    }

    fn logins(&self) -> Result<Vec<Login>, BankError> {
        self.session()?;
        Ok(settings::users(&self.folder)
            .map_err(|_| BankError::Folder)?
            .into_iter()
            .filter_map(|user| {
                let kind = match user.crypt_mode.as_deref() {
                    Some(PIN_TAN) => LoginKind::PinTan {
                        customer_id: CustomerId::Unknown,
                    },
                    Some(CHIP_CARD) => LoginKind::ChipCard,
                    // Key files (D-059) or something newer: not ours.
                    _ => return None,
                };
                Some(Login {
                    id: LoginId::new(user.unique_id.to_string()),
                    bank: BankCode::new(&user.country, &user.bank_code),
                    kind,
                })
            })
            .collect())
    }

    fn add_login(
        &mut self,
        setup: &LoginSetup,
        pins: &mut PinSession,
    ) -> Result<LoginId, BankError> {
        let session = self.session()?;
        if setup.kind == LoginKind::ChipCard {
            return Err(BankError::NeedsSetupDialog);
        }
        self.require_registration(session)?;
        let pending = LoginId::pending();
        let pin = match pins.answer(&pending)? {
            PinAnswer::Known(pin) => Some(pin),
            PinAnswer::Ask => None,
        };
        // `adduser` only writes the settings file; it is undone below
        // when the bank doesn't accept the login.
        let before = self.user_ids()?;
        let mut arguments = vec![
            "-N",
            APP_NAME,
            "-b",
            setup.bank.code(),
            "-u",
            setup.login_name.as_str(),
            "-t",
            PIN_TAN,
            "-s",
            setup.url.as_str(),
            FINTS_VERSION,
        ];
        if let Some(customer_id) = &setup.customer_id {
            arguments.extend(["-c", customer_id.as_str()]);
        }
        self.setup_command(session, "adduser", &arguments, None, &pending, pins)?;
        let mut added: Vec<u32> = self.user_ids()?.difference(&before).copied().collect();
        let (Some(uid), true) = (added.pop(), added.is_empty()) else {
            return Err(BankError::SetupFailed { text: None });
        };
        let uid_text = uid.to_string();
        let id = LoginId::new(uid_text.clone());
        if let Err(error) = self.connect_new(session, &uid_text, pin, pins) {
            // Nothing is kept of a login that didn't work, including its
            // PIN (D-057).
            let _ = Self::call(session, None, || {
                session.control("deluser", &["-u", &uid_text])
            });
            self.clean_logs();
            pins.forget(&pending)?;
            return Err(error);
        }
        pins.rename(&pending, &id)?;
        pins.end_session();
        Ok(id)
    }

    fn remove_login(&mut self, login: &LoginId) -> Result<(), BankError> {
        let session = self.session()?;
        let uid = self.user_id(login)?.to_string();
        let outcome = Self::call(session, None, || session.control("deluser", &["-u", &uid]));
        self.clean_logs();
        outcome.into_result(true)
    }

    fn accounts(&self, login: &LoginId) -> Result<Vec<BankAccount>, BankError> {
        let session = self.session()?;
        let uid = self.user_id(login)?;
        self.accounts_of(session, uid, login)
    }

    fn fetch(
        &mut self,
        account: &BankAccountId,
        since: Option<Date>,
        pins: &mut PinSession,
    ) -> Result<Fetched, BankError> {
        let session = self.session()?;
        let unknown = || BankError::UnknownAccount(account.clone());
        let aid: u32 = account.as_str().parse().map_err(|_| unknown())?;
        let owners = settings::account_owners(&self.folder).map_err(|_| BankError::Folder)?;
        let uid = *owners.get(&aid).ok_or_else(unknown)?;
        let login = LoginId::new(uid.to_string());
        let bank_account = self
            .accounts_of(session, uid, &login)?
            .into_iter()
            .find(|candidate| &candidate.id == account)
            .ok_or_else(unknown)?;
        self.require_registration(session)?;
        let pin = match pins.answer(&login)? {
            PinAnswer::Known(pin) => Some(pin),
            PinAnswer::Ask => None,
        };
        let outcome = Self::call(session, pin, || session.fetch(aid, since.map(date_record)));
        self.clean_logs();
        if outcome.rejected {
            pins.rejected(&login)?;
            return Err(BankError::PinRejected);
        }
        let record = outcome.into_result(false)?;
        pins.end_session();
        Ok(fetched(bank_account, aid, record))
    }
}

/// What the probe of the libraries comes to (D-054, D-138).
fn availability(probed: Result<(LibraryVersion, LibraryVersion), LoadError>) -> Availability {
    match probed {
        Ok((gwenhywfar, aqbanking)) => {
            Availability::check(Some(version(aqbanking)), Some(version(gwenhywfar)))
        }
        Err(LoadError::TooOld { library, found, .. }) => {
            let library = match library {
                kassensturz_aqbanking_sys::Library::AqBanking => Library::AqBanking,
                kassensturz_aqbanking_sys::Library::Gwenhywfar => Library::Gwenhywfar,
            };
            Availability::TooOld {
                library,
                found: version(found),
                minimum: library.minimum(),
            }
        }
        Err(LoadError::NotFound { .. } | LoadError::MissingFunction { .. }) => {
            Availability::NotInstalled
        }
    }
}

/// A library's version as the crate reports it; a negative number, which
/// no release has, counts as 0.
fn version(found: LibraryVersion) -> Version {
    let part = |number: i32| u32::try_from(number).unwrap_or(0);
    Version::new(part(found.major), part(found.minor), part(found.patch))
}

/// The error code for what AqBanking answered. `message` is the last
/// line the bank or the backend logged, for the codes that carry a text;
/// `setup` says whether a setup command ran, whose own results are
/// `bank.setup-failed`.
fn map_error(error: SessionError, message: Option<String>, setup: bool) -> BankError {
    match error {
        SessionError::Declined(PasswordKind::Pin) => BankError::PinNeeded,
        SessionError::Declined(PasswordKind::Tan) => BankError::TanNeeded,
        SessionError::Declined(PasswordKind::Other) => BankError::Aborted,
        SessionError::UntrustedServer => BankError::UntrustedServer,
        SessionError::Library(code) => match code {
            GWEN_ERROR_NET_UNREACHABLE
            | GWEN_ERROR_HOST_NOT_FOUND
            | GWEN_ERROR_CONN_REFUSED
            | GWEN_ERROR_TIMEOUT
            | GWEN_ERROR_SSL_PREMATURE_CLOSE
            | GWEN_ERROR_NOT_CONNECTED
            | GWEN_ERROR_BROKEN_PIPE
            | GWEN_ERROR_COULD_NOT_RESOLVE
            | AB_ERROR_NETWORK => BankError::Unreachable,
            GWEN_ERROR_SSL | GWEN_ERROR_SSL_SECURITY | AB_ERROR_SECURITY => {
                BankError::UntrustedServer
            }
            GWEN_ERROR_USER_ABORTED | GWEN_ERROR_ABORTED => BankError::Aborted,
            _ if setup => BankError::SetupFailed { text: message },
            _ => BankError::Refused { text: message },
        },
        SessionError::Load(_) => BankError::NotInstalled,
        SessionError::AlreadyOpen => BankError::Busy,
        SessionError::Init(_) => BankError::Folder,
        SessionError::BadData(_) | SessionError::Argument if setup => {
            BankError::SetupFailed { text: None }
        }
        SessionError::BadData(_) | SessionError::Argument => BankError::Refused { text: None },
    }
}

/// An account as the app lists it, from AqBanking's record.
fn bank_account(account: AccountRecord, login: &LoginId) -> BankAccount {
    BankAccount {
        id: BankAccountId::new(account.unique_id.to_string()),
        login: login.clone(),
        kind: match account.kind {
            AccountKind::Checking => BankAccountKind::Current,
            AccountKind::Savings => BankAccountKind::Savings,
            AccountKind::CreditCard => BankAccountKind::CreditCard,
            AccountKind::OtherBank | AccountKind::Unknown(_) => BankAccountKind::Other,
        },
        iban: account.iban.filter(|iban| !iban.is_empty()),
        number: account.account_number.filter(|number| !number.is_empty()),
        currency: account
            .currency
            .as_deref()
            .and_then(|code| CurrencyCode::new(code).ok()),
        name: account.name.filter(|name| !name.is_empty()),
    }
}

/// A date for AqBanking.
fn date_record(date: Date) -> DateRecord {
    DateRecord {
        year: i32::from(date.year()),
        month: i32::from(date.month()),
        day: i32::from(date.day()),
    }
}

/// A date from AqBanking; one that isn't a calendar date is none.
fn date(record: Option<DateRecord>) -> Option<Date> {
    let record = record?;
    Date::new(
        i16::try_from(record.year).ok()?,
        i8::try_from(record.month).ok()?,
        i8::try_from(record.day).ok()?,
    )
    .ok()
}

/// What the fetch delivered for `account`, as the crate hands it on. The
/// amounts stay text: [`crate::statement`] turns them into money and
/// refuses what isn't exact (D-141).
fn fetched(account: BankAccount, unique_id: u32, record: FetchRecord) -> Fetched {
    let Some(found) = record
        .accounts
        .into_iter()
        .find(|found| found.unique_id == unique_id)
    else {
        // The bank answered without this account: nothing new.
        return Fetched {
            account,
            balance: None,
            transactions: Vec::new(),
        };
    };
    let FetchedAccountRecord {
        balances,
        transactions,
        ..
    } = found;
    let balance = balances
        .iter()
        .find(|balance| balance.kind == BalanceKind::Booked)
        .or_else(|| {
            balances
                .iter()
                .find(|balance| balance.kind == BalanceKind::Noted)
        })
        .map(|balance| FetchedBalance {
            amount: FetchedAmount {
                fraction: balance.amount.fraction.clone(),
                currency: balance.amount.currency.clone(),
            },
            date: date(balance.date),
        });
    let transactions = transactions
        .into_iter()
        .filter_map(|transaction| {
            let booked = match transaction.kind {
                TransactionKind::Statement => true,
                TransactionKind::NotedStatement => false,
                TransactionKind::Other(_) => return None,
            };
            Some(FetchedTransaction {
                booked,
                bank_id: transaction.bank_id.filter(|id| !id.is_empty()),
                booking_date: date(transaction.booking_date),
                value_date: date(transaction.value_date),
                amount: FetchedAmount {
                    fraction: transaction.amount.fraction,
                    currency: transaction.amount.currency,
                },
                other_party_name: transaction.remote_name.filter(|name| !name.is_empty()),
                other_party_iban: transaction.remote_iban.filter(|iban| !iban.is_empty()),
                purpose: transaction
                    .purpose
                    .as_deref()
                    .unwrap_or_default()
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .map(str::to_owned)
                    .collect(),
            })
        })
        .collect();
    Fetched {
        account,
        balance,
        transactions,
    }
}

/// AqBanking's bank information as a directory (item 93 of the Proposals
/// list, `docs/BANK_LIST.md`): the bank's name, BIC and FinTS server with
/// the login kinds its services list. Without a session (the libraries
/// aren't there) it knows no bank.
pub(crate) struct AqBankingDirectory {
    session: Option<Rc<Session<Handler>>>,
}

impl BankDirectory for AqBankingDirectory {
    fn lookup(&self, code: &BankCode) -> Option<BankInfo> {
        let session = self.session.as_ref()?;
        let country = code.country().to_ascii_lowercase();
        // AqBanking's own answer is the first record of a code, but a
        // server can sit in a later one, so all records are read and
        // merged; if listing them fails, its answer stands.
        if let Ok(records) = session.bank_infos(&country, code.code()) {
            let records: Vec<_> = records
                .into_iter()
                .filter(|record| record.code.as_deref() == Some(code.code()))
                .collect();
            if let Some(info) = merge_records(code, records) {
                return Some(info);
            }
        }
        let record = session.bank_info(&country, code.code()).ok()??;
        Some(bank_info(code, record))
    }

    fn banks_starting_with(&self, country: &str, prefix: &str) -> Vec<BankInfo> {
        let Some(session) = self.session.as_ref() else {
            return Vec::new();
        };
        let country_code = country.to_ascii_lowercase();
        let Ok(records) = session.bank_infos(&country_code, &format!("{prefix}*")) else {
            return Vec::new();
        };
        // One bank per code, in the order of the data.
        let mut codes: Vec<String> = Vec::new();
        for record in &records {
            if let Some(code) = record
                .code
                .as_deref()
                .filter(|code| code.starts_with(prefix))
                && !codes.iter().any(|known| known == code)
            {
                codes.push(code.to_owned());
            }
        }
        codes
            .into_iter()
            .filter_map(|code| {
                let bank = BankCode::new(country, &code);
                let own: Vec<_> = records
                    .iter()
                    .filter(|record| record.code.as_deref() == Some(code.as_str()))
                    .cloned()
                    .collect();
                merge_records(&bank, own)
            })
            .collect()
    }
}

/// One bank from all the records AqBanking has for its code: the first
/// record names it, the first BIC any record has is its BIC, and the server
/// is the first one a record has. Banks of the cooperative group have old
/// names and branches as extra records, and the server is not always in
/// the first.
fn merge_records(code: &BankCode, records: Vec<BankRecord>) -> Option<BankInfo> {
    let mut infos = records.into_iter().map(|record| bank_info(code, record));
    let mut bank = infos.next()?;
    for info in infos {
        if bank.bic.is_none() {
            bank.bic = info.bic;
        }
        if bank.server.is_none() {
            bank.server = info.server;
        }
    }
    Some(bank)
}

/// What AqBanking's record says about the bank `code`.
///
/// Only an `https://` address is a server (threat T-12): AqBanking's data
/// writes a bare host name for most key-file services, which isn't the
/// address a PIN/TAN login needs, so a bank with such services only is
/// shown as "no server" and the user gives the address by hand. The
/// address of a PIN/TAN service is preferred, then a chip card's, then
/// any; the login kinds come from every service's mode.
fn bank_info(code: &BankCode, record: BankRecord) -> BankInfo {
    let fints: Vec<_> = record
        .services
        .iter()
        .filter(|service| {
            service
                .kind
                .as_deref()
                .is_some_and(|kind| kind.eq_ignore_ascii_case(FINTS_SERVICE))
        })
        .filter_map(|service| {
            let address = service.address.as_deref()?.trim();
            if !address
                .get(..8)
                .is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://"))
            {
                return None;
            }
            let kind = match service
                .mode
                .as_deref()
                .map(str::to_ascii_uppercase)
                .as_deref()
            {
                Some("PINTAN") => Some(Some(LoginKind::PinTan {
                    customer_id: CustomerId::Unknown,
                })),
                Some("DDV") => Some(Some(LoginKind::ChipCard)),
                // Key files (RDH, RAH) aren't offered (D-059).
                Some(_) => Some(None),
                None => None,
            };
            Some((address, kind))
        })
        .collect();
    let server = fints
        .iter()
        .find(|(_, kind)| matches!(kind, Some(Some(LoginKind::PinTan { .. }))))
        .or_else(|| {
            fints
                .iter()
                .find(|(_, kind)| matches!(kind, Some(Some(LoginKind::ChipCard))))
        })
        .or_else(|| fints.first())
        .map(|(address, _)| {
            let mut login_kinds: Vec<LoginKind> = fints
                .iter()
                .filter_map(|(_, kind)| kind.flatten())
                .collect();
            // PIN/TAN first: the usual way, and what the screens show first.
            login_kinds.sort_by_key(|kind| *kind == LoginKind::ChipCard);
            login_kinds.dedup();
            let unknown = fints.iter().any(|(_, kind)| kind.is_none());
            FintsServer {
                url: (*address).to_owned(),
                login_kinds: if login_kinds.is_empty() && unknown {
                    None
                } else {
                    Some(login_kinds)
                },
            }
        });
    BankInfo {
        code: code.clone(),
        name: record
            .name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| code.code().to_owned()),
        bic: record.bic.filter(|bic| !bic.is_empty()),
        server,
    }
}

#[cfg(test)]
mod tests {
    use kassensturz_aqbanking_sys::session::{
        AmountRecord, BalanceRecord, ServiceRecord, TransactionRecord,
    };

    use super::*;

    fn code() -> BankCode {
        BankCode::new("DE", "37040044")
    }

    fn service(kind: &str, address: Option<&str>, mode: Option<&str>) -> ServiceRecord {
        ServiceRecord {
            kind: Some(kind.to_owned()),
            address: address.map(str::to_owned),
            mode: mode.map(str::to_owned),
            version: Some("3.0".to_owned()),
        }
    }

    #[test]
    fn bank_information_gives_the_server_and_the_login_kinds() {
        let info = bank_info(
            &code(),
            BankRecord {
                code: None,
                name: Some("Testbank".to_owned()),
                bic: Some("COBADEFFXXX".to_owned()),
                services: vec![
                    service("EBICS", Some("https://ebics.example/"), None),
                    service("HBCI", Some("fints.testbank.example"), Some("RDH7")),
                    service(
                        "HBCI",
                        Some("https://fints.testbank.example/ddv"),
                        Some("DDV"),
                    ),
                    service(
                        "HBCI",
                        Some("https://fints.testbank.example/"),
                        Some("PINTAN"),
                    ),
                    service(
                        "HBCI",
                        Some("https://fints.testbank.example/"),
                        Some("RDH10"),
                    ),
                ],
            },
        );
        assert_eq!(info.name, "Testbank");
        assert_eq!(info.bic.as_deref(), Some("COBADEFFXXX"));
        let server = info.server.expect("a FinTS server");
        assert_eq!(server.url, "https://fints.testbank.example/");
        assert_eq!(
            server.login_kinds,
            Some(vec![
                LoginKind::PinTan {
                    customer_id: CustomerId::Unknown
                },
                LoginKind::ChipCard
            ])
        );
    }

    #[test]
    fn missing_parts_of_the_bank_information_are_explicit() {
        // No services: no server.
        let info = bank_info(
            &code(),
            BankRecord {
                code: None,
                name: None,
                bic: Some(String::new()),
                services: vec![],
            },
        );
        assert_eq!(info.name, "37040044");
        assert_eq!(info.bic, None);
        assert_eq!(info.server, None);
        // A service without an address, or with a bare host name as
        // AqBanking's data has for key files: no server either.
        let info = bank_info(
            &code(),
            BankRecord {
                code: None,
                name: Some(" ".to_owned()),
                bic: None,
                services: vec![
                    service("HBCI", None, Some("PINTAN")),
                    service("HBCI", Some("fints2.example"), Some("RDH9")),
                ],
            },
        );
        assert_eq!(info.server, None);
        // A server whose mode isn't said: unknown login kinds.
        let info = bank_info(
            &code(),
            BankRecord {
                code: None,
                name: None,
                bic: None,
                services: vec![service("HBCI", Some("https://fints.example/"), None)],
            },
        );
        assert_eq!(info.server.expect("a server").login_kinds, None);
        // A server with key files only: no login kind KassenSturz offers.
        let info = bank_info(
            &code(),
            BankRecord {
                code: None,
                name: None,
                bic: None,
                services: vec![service(
                    "HBCI",
                    Some("https://fints.example/"),
                    Some("RDH10"),
                )],
            },
        );
        assert_eq!(info.server.expect("a server").login_kinds, Some(vec![]));
    }

    #[test]
    fn a_server_with_trailing_space_is_trimmed() {
        // AqBanking's data has one address written with a space at the end.
        let info = bank_info(
            &code(),
            BankRecord {
                services: vec![service(
                    "HBCI",
                    Some("https://fints.example/hbci "),
                    Some("PINTAN"),
                )],
                ..BankRecord::default()
            },
        );
        assert_eq!(
            info.server.expect("a server").url,
            "https://fints.example/hbci"
        );
    }

    #[test]
    fn the_records_of_one_code_are_merged() {
        let first = BankRecord {
            code: Some("37040044".to_owned()),
            name: Some("Genossenbank".to_owned()),
            bic: None,
            services: vec![],
        };
        let later = BankRecord {
            code: Some("37040044".to_owned()),
            name: Some("Genossenbank (Geschäftsstelle)".to_owned()),
            bic: Some("GENODEF1XXX".to_owned()),
            services: vec![service(
                "HBCI",
                Some("https://fints.example/"),
                Some("PINTAN"),
            )],
        };
        let merged = merge_records(&code(), vec![first.clone(), later.clone()]).expect("a bank");
        // The first record names the bank; the server and the BIC come from
        // the first record that has one.
        assert_eq!(merged.name, "Genossenbank");
        assert_eq!(merged.bic.as_deref(), Some("GENODEF1XXX"));
        assert_eq!(
            merged.server.expect("a server").url,
            "https://fints.example/"
        );
        // Without the later record the bank has no server.
        assert_eq!(
            merge_records(&code(), vec![first]).expect("a bank").server,
            None
        );
        assert_eq!(merge_records(&code(), vec![]), None);
        // A first record with a server keeps it.
        let other = BankRecord {
            services: vec![service(
                "HBCI",
                Some("https://fints.first.example/"),
                Some("PINTAN"),
            )],
            ..BankRecord::default()
        };
        let merged = merge_records(&code(), vec![other, later]).expect("a bank");
        assert_eq!(
            merged.server.expect("a server").url,
            "https://fints.first.example/"
        );
    }

    #[test]
    fn library_errors_become_error_codes() {
        assert_eq!(
            map_error(
                SessionError::Library(GWEN_ERROR_NET_UNREACHABLE),
                None,
                false
            ),
            BankError::Unreachable
        );
        assert_eq!(
            map_error(SessionError::Library(AB_ERROR_NETWORK), None, true),
            BankError::Unreachable
        );
        assert_eq!(
            map_error(SessionError::Library(GWEN_ERROR_SSL_SECURITY), None, false),
            BankError::UntrustedServer
        );
        assert_eq!(
            map_error(SessionError::Library(GWEN_ERROR_USER_ABORTED), None, false),
            BankError::Aborted
        );
        assert_eq!(
            map_error(
                SessionError::Library(-1),
                Some("Konto gesperrt".to_owned()),
                false
            ),
            BankError::Refused {
                text: Some("Konto gesperrt".to_owned())
            }
        );
        assert_eq!(
            map_error(SessionError::Library(3), Some("bad token".to_owned()), true),
            BankError::SetupFailed {
                text: Some("bad token".to_owned())
            }
        );
        assert_eq!(
            map_error(SessionError::Declined(PasswordKind::Pin), None, false),
            BankError::PinNeeded
        );
        assert_eq!(
            map_error(SessionError::Declined(PasswordKind::Tan), None, false),
            BankError::TanNeeded
        );
        assert_eq!(
            map_error(SessionError::Declined(PasswordKind::Other), None, false),
            BankError::Aborted
        );
        assert_eq!(
            map_error(SessionError::UntrustedServer, None, false),
            BankError::UntrustedServer
        );
        assert_eq!(
            map_error(SessionError::AlreadyOpen, None, false),
            BankError::Busy
        );
        assert_eq!(
            map_error(SessionError::Init(-1), None, false),
            BankError::Folder
        );
        assert_eq!(
            map_error(SessionError::Load("gone".to_owned()), None, false),
            BankError::NotInstalled
        );
        assert_eq!(
            map_error(SessionError::Argument, None, true),
            BankError::SetupFailed { text: None }
        );
        assert_eq!(
            map_error(SessionError::BadData("x"), None, false),
            BankError::Refused { text: None }
        );
    }

    #[test]
    fn the_probe_becomes_the_availability() {
        let current = LibraryVersion::new(6, 9, 2);
        let gwen = LibraryVersion::new(5, 14, 1);
        assert_eq!(
            availability(Ok((gwen, current))),
            Availability::Available {
                aqbanking: Version::new(6, 9, 2),
                gwenhywfar: Version::new(5, 14, 1),
            }
        );
        assert_eq!(
            availability(Err(LoadError::TooOld {
                library: kassensturz_aqbanking_sys::Library::Gwenhywfar,
                found: LibraryVersion::new(5, 13, 0),
                minimum: gwen,
            })),
            Availability::TooOld {
                library: Library::Gwenhywfar,
                found: Version::new(5, 13, 0),
                minimum: Version::new(5, 14, 1),
            }
        );
        assert_eq!(
            availability(Err(LoadError::NotFound {
                library: kassensturz_aqbanking_sys::Library::AqBanking,
                reason: "gone".to_owned(),
            })),
            Availability::NotInstalled
        );
    }

    fn amount(fraction: &str) -> AmountRecord {
        AmountRecord {
            fraction: fraction.to_owned(),
            currency: Some("EUR".to_owned()),
        }
    }

    fn account() -> BankAccount {
        BankAccount {
            id: BankAccountId::new("3"),
            login: LoginId::new("7"),
            kind: BankAccountKind::Current,
            iban: None,
            number: None,
            currency: None,
            name: None,
        }
    }

    #[test]
    fn the_booked_balance_is_preferred_and_lines_are_split() {
        let record = FetchRecord {
            accounts: vec![FetchedAccountRecord {
                unique_id: 3,
                iban: None,
                account_number: None,
                bank_code: None,
                currency: Some("EUR".to_owned()),
                balances: vec![
                    BalanceRecord {
                        kind: BalanceKind::Noted,
                        date: Some(DateRecord {
                            year: 2026,
                            month: 10,
                            day: 23,
                        }),
                        amount: amount("373457/100"),
                    },
                    BalanceRecord {
                        kind: BalanceKind::Booked,
                        date: Some(DateRecord {
                            year: 2026,
                            month: 10,
                            day: 22,
                        }),
                        amount: amount("123457/100"),
                    },
                ],
                transactions: vec![
                    TransactionRecord {
                        kind: TransactionKind::Statement,
                        bank_id: Some("T-1".to_owned()),
                        booking_date: Some(DateRecord {
                            year: 2026,
                            month: 10,
                            day: 15,
                        }),
                        value_date: Some(DateRecord {
                            year: 2026,
                            month: 13,
                            day: 1,
                        }),
                        amount: amount("-4999/100"),
                        remote_name: Some("Supermarkt Nord".to_owned()),
                        remote_iban: Some(String::new()),
                        purpose: Some("Einkauf\n\nKarte 1234\n".to_owned()),
                    },
                    TransactionRecord {
                        kind: TransactionKind::Other(9),
                        bank_id: None,
                        booking_date: None,
                        value_date: None,
                        amount: amount("1/100"),
                        remote_name: None,
                        remote_iban: None,
                        purpose: None,
                    },
                    TransactionRecord {
                        kind: TransactionKind::NotedStatement,
                        bank_id: Some(String::new()),
                        booking_date: None,
                        value_date: None,
                        amount: amount("250000/100"),
                        remote_name: None,
                        remote_iban: None,
                        purpose: None,
                    },
                ],
            }],
            messages: vec![],
        };
        let result = fetched(account(), 3, record);
        let balance = result.balance.expect("a balance");
        assert_eq!(balance.amount.fraction, "123457/100");
        assert_eq!(balance.date, Some(Date::new(2026, 10, 22).expect("a date")));
        assert_eq!(result.transactions.len(), 2);
        let first = &result.transactions[0];
        assert!(first.booked);
        assert_eq!(first.bank_id.as_deref(), Some("T-1"));
        assert_eq!(first.value_date, None, "month 13 isn't a date");
        assert_eq!(first.other_party_iban, None);
        assert_eq!(first.purpose, vec!["Einkauf", "Karte 1234"]);
        let pending = &result.transactions[1];
        assert!(!pending.booked);
        assert_eq!(pending.bank_id, None);
        assert!(pending.purpose.is_empty());
        // Only a noted balance: it is taken.
        let record = FetchRecord {
            accounts: vec![FetchedAccountRecord {
                unique_id: 3,
                iban: None,
                account_number: None,
                bank_code: None,
                currency: None,
                balances: vec![BalanceRecord {
                    kind: BalanceKind::Noted,
                    date: None,
                    amount: amount("1/100"),
                }],
                transactions: vec![],
            }],
            messages: vec![],
        };
        let result = fetched(account(), 3, record);
        assert_eq!(result.balance.expect("noted").amount.fraction, "1/100");
        // An answer without the account: nothing new.
        let result = fetched(
            account(),
            3,
            FetchRecord {
                accounts: vec![],
                messages: vec!["No such account".to_owned()],
            },
        );
        assert_eq!(result.balance, None);
        assert!(result.transactions.is_empty());
    }

    #[test]
    fn account_records_become_accounts() {
        let account = bank_account(
            AccountRecord {
                unique_id: 12,
                kind: AccountKind::CreditCard,
                country: Some("de".to_owned()),
                bank_code: Some("37040044".to_owned()),
                account_number: Some("1234567890".to_owned()),
                sub_account_number: None,
                iban: Some(String::new()),
                bic: None,
                currency: Some("EUR".to_owned()),
                name: Some("Kreditkarte".to_owned()),
            },
            &LoginId::new("7"),
        );
        assert_eq!(account.id, BankAccountId::new("12"));
        assert_eq!(account.kind, BankAccountKind::CreditCard);
        assert_eq!(account.iban, None);
        assert_eq!(account.number.as_deref(), Some("1234567890"));
        assert_eq!(
            account.currency,
            Some(CurrencyCode::new("EUR").expect("a code"))
        );
        assert_eq!(account.name.as_deref(), Some("Kreditkarte"));
        let other = bank_account(
            AccountRecord {
                unique_id: 1,
                kind: AccountKind::Unknown(99),
                country: None,
                bank_code: None,
                account_number: None,
                sub_account_number: None,
                iban: None,
                bic: None,
                currency: Some("not a code".to_owned()),
                name: None,
            },
            &LoginId::new("7"),
        );
        assert_eq!(other.kind, BankAccountKind::Other);
        assert_eq!(other.currency, None);
    }
}
