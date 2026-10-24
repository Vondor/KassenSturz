// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The fake bank: a [`Bank`] in memory for tests and for trying the screens
//! without a real bank (D-055). Its statements are built in code; every
//! IBAN is one of SWIFT's example IBANs, every name and amount is made up
//! (AGENTS.md §3.1 rule 2).
//!
//! It behaves like a real backend in what matters for the code around it:
//! it asks the [`PinSession`] for the PIN, calls the wrong one wrong, can
//! ask for a TAN or an approval in the bank's app through the app's
//! [`BankDialogs`] ([`SecondFactor`], D-151), lists accounts only after a
//! login, filters by date, delivers a pending transaction, and can be told
//! to fail in the ways a real bank does.

use kassensturz_core::{Currency, CurrencyCode, Date};

use crate::availability::{AQBANKING_MINIMUM, Availability, GWENHYWFAR_MINIMUM, Library, Version};
use crate::backend::{
    Bank, BankAccount, BankAccountId, BankAccountKind, Login, LoginId, LoginSetup,
};
use crate::dialogs::{BankDialogs, BankQuestion, QuestionLevel, TanRequest};
use crate::directory::{BankDirectory, BankInfo, CustomerId, FintsServer, LoginKind};
use crate::error::BankError;
use crate::fetched::{Fetched, FetchedAmount, FetchedBalance, FetchedTransaction};
use crate::iban::BankCode;
use crate::pin::{PinAnswer, PinSession};

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};

/// SWIFT's example IBAN for Germany: the fake bank's current account.
pub const CURRENT_ACCOUNT_IBAN: &str = "DE89370400440532013000";
/// The bank code of [`CURRENT_ACCOUNT_IBAN`]: the fake bank's code.
pub const BANK_CODE: &str = "37040044";
/// A bank code the fake directory knows without a FinTS server.
pub const BANK_CODE_WITHOUT_SERVER: &str = "10000000";
/// A bank code the fake directory knows with a server but no login kinds.
pub const BANK_CODE_WITHOUT_LOGIN_KINDS: &str = "20000000";
/// A cooperative bank's code the fake directory knows without a server; its
/// neighbour [`BANK_CODE_NEIGHBOUR`] is on an Atruvia server, so the setup
/// offers that address as a guess.
pub const BANK_CODE_GUESSED: &str = "40000010";
/// The neighbour of [`BANK_CODE_GUESSED`], with an Atruvia server.
pub const BANK_CODE_NEIGHBOUR: &str = "40000011";
/// SWIFT's example IBAN for the Netherlands: the other party of some
/// transactions.
const GROCER_IBAN: &str = "NL91ABNA0417164300";
/// SWIFT's example IBAN for Belgium: the user's savings elsewhere, so a
/// Transfer shows up.
pub const SAVINGS_IBAN: &str = "BE68539007547034";

/// How a fake bank fails on its next call, as a real bank might.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The server can't be reached.
    Unreachable,
    /// The bank answers with an error and this text.
    Refused(String),
    /// The user closes the bank's dialog.
    Aborted,
    /// The bank delivers an amount that isn't whole minor units (a third of
    /// a cent), as broken data would (threat T-09, T-10).
    InexactAmount,
}

/// What the fake bank asks for after the PIN, on every connection, as
/// banks do under the EU's strong customer authentication.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecondFactor {
    /// A TAN shown as text; it accepts [`FakeBank::TAN`].
    Tan,
    /// A TAN shown as a picture, which the app can't show (D-151).
    PictureTan,
    /// An approval in the bank's app: a question whose first button goes
    /// on.
    Approval,
}

#[derive(Clone, Debug)]
struct FakeLogin {
    login: Login,
}

/// The app's dialogs, if it gave the fake bank some.
#[derive(Default)]
struct Dialogs(Option<Box<dyn BankDialogs>>);

impl std::fmt::Debug for Dialogs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.is_some() { "Dialogs" } else { "None" })
    }
}

/// The fake bank. Every login it gets has the same two accounts, a current
/// account and a credit card, with the same made-up transactions.
#[derive(Debug)]
pub struct FakeBank {
    availability: Availability,
    logins: Vec<FakeLogin>,
    next_login: u32,
    failure: Option<Failure>,
    second_factor: Option<SecondFactor>,
    dialogs: Dialogs,
}

impl Default for FakeBank {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeBank {
    /// The PIN the fake bank accepts for every login. Made up; the real
    /// bank's PIN is never in this repository (D-055).
    pub const PIN: &'static str = "12345";

    /// The TAN the fake bank accepts when it asks for one. Made up.
    pub const TAN: &'static str = "123456";

    /// An installed fake bank with no logins yet.
    pub fn new() -> Self {
        Self {
            availability: Availability::Available {
                aqbanking: AQBANKING_MINIMUM,
                gwenhywfar: GWENHYWFAR_MINIMUM,
            },
            logins: Vec::new(),
            next_login: 1,
            failure: None,
            second_factor: None,
            dialogs: Dialogs::default(),
        }
    }

    /// Makes the fake bank ask for `factor` after the PIN on every
    /// connection.
    pub fn with_second_factor(mut self, factor: SecondFactor) -> Self {
        self.second_factor = Some(factor);
        self
    }

    /// A fake bank on a computer without AqBanking (D-054).
    pub fn not_installed() -> Self {
        Self {
            availability: Availability::NotInstalled,
            ..Self::new()
        }
    }

    /// A fake bank on a computer whose AqBanking is older than needed
    /// (D-138).
    pub fn too_old() -> Self {
        Self {
            availability: Availability::TooOld {
                library: Library::AqBanking,
                found: Version::new(6, 5, 0),
                minimum: AQBANKING_MINIMUM,
            },
            ..Self::new()
        }
    }

    /// Makes the next call that talks to the bank fail with `failure`.
    pub fn fail_next(&mut self, failure: Failure) {
        self.failure = Some(failure);
    }

    /// The setup for the fake bank's own bank code, with a made-up login
    /// name: what the setup screens would gather for it.
    pub fn setup() -> LoginSetup {
        LoginSetup {
            bank: BankCode::new("DE", BANK_CODE),
            url: FakeDirectory::URL.to_owned(),
            kind: FakeDirectory::PIN_TAN,
            login_name: "test-login".to_owned(),
            customer_id: None,
        }
    }

    fn find(&self, login: &LoginId) -> Result<&FakeLogin, BankError> {
        self.logins
            .iter()
            .find(|fake| &fake.login.id == login)
            .ok_or_else(|| BankError::UnknownLogin(login.clone()))
    }

    /// The accounts of `login`: the current account and the credit card.
    fn accounts_of(login: &LoginId) -> Vec<BankAccount> {
        vec![
            BankAccount {
                id: BankAccountId::new(format!("{login}/current")),
                login: login.clone(),
                kind: BankAccountKind::Current,
                iban: Some(CURRENT_ACCOUNT_IBAN.to_owned()),
                number: Some("0532013000".to_owned()),
                currency: Some(EUR),
                name: Some("Current account".to_owned()),
            },
            BankAccount {
                id: BankAccountId::new(format!("{login}/card")),
                login: login.clone(),
                kind: BankAccountKind::CreditCard,
                iban: None,
                number: Some("4000 0000 0000 0001".to_owned()),
                currency: Some(EUR),
                name: Some("Credit card".to_owned()),
            },
        ]
    }

    /// Talks to the bank for `login`: checks the PIN the way a bank does,
    /// and fails as told. Ends the PIN session's bank session afterwards.
    fn connect(&mut self, login: &LoginId, pins: &mut PinSession) -> Result<(), BankError> {
        self.availability.require()?;
        if let Some(failure) = self.failure.take() {
            return Err(match failure {
                Failure::Unreachable => BankError::Unreachable,
                Failure::Refused(text) => BankError::Refused { text: Some(text) },
                Failure::Aborted => BankError::Aborted,
                Failure::InexactAmount => {
                    // Not a connection failure: put back for the fetch.
                    self.failure = Some(Failure::InexactAmount);
                    return self.check_pin(login, pins);
                }
            });
        }
        self.check_pin(login, pins)?;
        self.check_second_factor()
    }

    /// Asks for the second factor, if the fake bank wants one, the way the
    /// real backend does: through the dialogs, or not at all.
    fn check_second_factor(&mut self) -> Result<(), BankError> {
        let Some(factor) = self.second_factor else {
            return Ok(());
        };
        if factor == SecondFactor::PictureTan {
            return Err(BankError::TanMethodUnsupported);
        }
        let Some(dialogs) = self.dialogs.0.as_mut() else {
            return Err(BankError::TanNeeded);
        };
        match factor {
            SecondFactor::Tan => {
                let tan = dialogs
                    .tan(&TanRequest {
                        title: "TAN".to_owned(),
                        challenge: "Type the TAN your TAN app shows for this login.".to_owned(),
                        min_len: 6,
                        max_len: 6,
                    })
                    .ok_or(BankError::Aborted)?;
                if tan.as_str() == Self::TAN {
                    Ok(())
                } else {
                    Err(BankError::Refused {
                        text: Some("9941 TAN wrong (made-up message of the fake bank)".to_owned()),
                    })
                }
            }
            SecondFactor::Approval => match dialogs.question(&BankQuestion {
                level: QuestionLevel::Info,
                title: "Waiting for approval".to_owned(),
                text: "Approve this login in your banking app, then choose Approved.".to_owned(),
                buttons: vec!["Approved".to_owned(), "Abort".to_owned()],
            }) {
                Some(0) => Ok(()),
                _ => Err(BankError::Aborted),
            },
            SecondFactor::PictureTan => Err(BankError::TanMethodUnsupported),
        }
    }

    fn check_pin(&self, login: &LoginId, pins: &mut PinSession) -> Result<(), BankError> {
        match pins.answer(login)? {
            PinAnswer::Ask => Err(BankError::PinNeeded),
            PinAnswer::Known(pin) if pin.as_str() == Self::PIN => Ok(()),
            PinAnswer::Known(_) => {
                pins.rejected(login)?;
                Err(BankError::PinRejected)
            }
        }
    }

    /// The current account's made-up transactions, in the bank's order.
    fn current_account_transactions() -> Vec<FetchedTransaction> {
        let eur = Currency::new(EUR, 2).expect("two decimal places");
        let booked = |day: i8,
                      minor_units: i64,
                      id: &str,
                      name: &str,
                      iban: Option<&str>,
                      purpose: &[&str]| {
            FetchedTransaction {
                booked: true,
                bank_id: Some(id.to_owned()),
                booking_date: Some(october(day)),
                value_date: Some(october(day)),
                amount: FetchedAmount::from_minor_units(minor_units, eur),
                other_party_name: Some(name.to_owned()),
                other_party_iban: iban.map(str::to_owned),
                purpose: purpose.iter().map(|line| (*line).to_owned()).collect(),
            }
        };
        vec![
            booked(
                1,
                250_000,
                "FAKE-2026-10-0001",
                "Example Employer",
                None,
                &["Salary October 2026"],
            ),
            booked(
                2,
                -4_523,
                "FAKE-2026-10-0002",
                "Example Grocer",
                Some(GROCER_IBAN),
                &["Card payment 01.10.2026", "Thank you"],
            ),
            booked(
                5,
                -20_000,
                "FAKE-2026-10-0003",
                "Own savings",
                Some(SAVINGS_IBAN),
                &["Monthly saving"],
            ),
            FetchedTransaction {
                booked: false,
                bank_id: None,
                booking_date: None,
                value_date: Some(october(9)),
                amount: FetchedAmount::from_minor_units(-1_999, eur),
                other_party_name: Some("Example Fuel Station".to_owned()),
                other_party_iban: None,
                purpose: vec!["Card payment, not booked yet".to_owned()],
            },
        ]
    }

    /// The credit card's made-up transactions: debt, so negative.
    fn credit_card_transactions() -> Vec<FetchedTransaction> {
        let eur = Currency::new(EUR, 2).expect("two decimal places");
        let charge =
            |day: i8, minor_units: i64, id: &str, name: &str, purpose: &str| FetchedTransaction {
                booked: true,
                bank_id: Some(id.to_owned()),
                booking_date: Some(october(day)),
                value_date: None,
                amount: FetchedAmount::from_minor_units(minor_units, eur),
                other_party_name: Some(name.to_owned()),
                other_party_iban: None,
                purpose: vec![purpose.to_owned()],
            };
        vec![
            charge(3, -8_990, "FAKE-CARD-0001", "Example Bookshop", "Books"),
            charge(7, -3_355, "FAKE-CARD-0002", "Example Railway", "Ticket"),
        ]
    }
}

/// A day in October 2026.
fn october(day: i8) -> Date {
    Date::new(2026, 10, day).expect("a valid date")
}

impl Bank for FakeBank {
    fn availability(&self) -> Availability {
        self.availability
    }

    fn directory(&self) -> &dyn BankDirectory {
        &FakeDirectory
    }

    fn set_dialogs(&mut self, dialogs: Box<dyn BankDialogs>) {
        self.dialogs = Dialogs(Some(dialogs));
    }

    fn logins(&self) -> Result<Vec<Login>, BankError> {
        self.availability.require()?;
        Ok(self.logins.iter().map(|fake| fake.login.clone()).collect())
    }

    fn add_login(
        &mut self,
        setup: &LoginSetup,
        pins: &mut PinSession,
    ) -> Result<LoginId, BankError> {
        self.availability.require()?;
        let id = LoginId::new(format!("fake-login-{}", self.next_login));
        // The PIN typed in the setup is kept as pending until the login
        // has its ID, as with the real backend.
        if pins.answer(&id)? == PinAnswer::Ask && pins.is_remembered(&LoginId::pending()) {
            pins.rename(&LoginId::pending(), &id)?;
        }
        if let Err(error) = self.connect(&id, pins) {
            // Nothing is kept of a login that didn't work, including its
            // PIN (D-057).
            pins.forget(&id)?;
            return Err(error);
        }
        pins.end_session();
        self.next_login += 1;
        self.logins.push(FakeLogin {
            login: Login {
                id: id.clone(),
                bank: setup.bank.clone(),
                kind: setup.kind,
            },
        });
        Ok(id)
    }

    fn remove_login(&mut self, login: &LoginId) -> Result<(), BankError> {
        self.availability.require()?;
        self.find(login)?;
        self.logins.retain(|fake| &fake.login.id != login);
        Ok(())
    }

    fn accounts(&self, login: &LoginId) -> Result<Vec<BankAccount>, BankError> {
        self.availability.require()?;
        self.find(login)?;
        Ok(Self::accounts_of(login))
    }

    fn fetch(
        &mut self,
        account: &BankAccountId,
        since: Option<Date>,
        pins: &mut PinSession,
    ) -> Result<Fetched, BankError> {
        self.availability.require()?;
        let account = self
            .logins
            .iter()
            .flat_map(|fake| Self::accounts_of(&fake.login.id))
            .find(|candidate| &candidate.id == account)
            .ok_or_else(|| BankError::UnknownAccount(account.clone()))?;
        self.connect(&account.login, pins)?;
        pins.end_session();
        let eur = Currency::new(EUR, 2).expect("two decimal places");
        let (mut transactions, balance) = match account.kind {
            BankAccountKind::CreditCard => (
                Self::credit_card_transactions(),
                FetchedBalance {
                    amount: FetchedAmount::from_minor_units(-12_345, eur),
                    date: Some(october(9)),
                },
            ),
            _ => (
                Self::current_account_transactions(),
                FetchedBalance {
                    // 1,000.00 before October plus the booked entries.
                    amount: FetchedAmount::from_minor_units(
                        100_000 + 250_000 - 4_523 - 20_000,
                        eur,
                    ),
                    date: Some(october(9)),
                },
            ),
        };
        if let Some(since) = since {
            transactions.retain(|transaction| {
                transaction
                    .booking_date
                    .or(transaction.value_date)
                    .is_none_or(|date| date >= since)
            });
        }
        if self.failure.take() == Some(Failure::InexactAmount)
            && let Some(transaction) = transactions.first_mut()
        {
            transaction.amount.fraction = "1/3".to_owned();
        }
        Ok(Fetched {
            account,
            balance: Some(balance),
            transactions,
        })
    }
}

/// The fake bank's directory: its own bank, one without a server, one
/// without known login kinds, and a cooperative bank next to one on an
/// Atruvia server, so every setup state can be tried.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FakeDirectory;

impl FakeDirectory {
    /// The fake bank's server address (`.example` is reserved for
    /// examples, so it never reaches anything).
    pub const URL: &'static str = "https://fints.testbank.example/";
    /// How the fake bank's customers log in.
    pub const PIN_TAN: LoginKind = LoginKind::PinTan {
        customer_id: CustomerId::SameAsLoginName,
    };
    /// The Atruvia server of [`BANK_CODE_NEIGHBOUR`], which the setup offers
    /// for [`BANK_CODE_GUESSED`] as a guess.
    pub const ATRUVIA_URL: &'static str = "https://fints1.atruvia.de/cgi-bin/hbciservlet";

    /// Every bank the directory knows, by code.
    const CODES: [&'static str; 5] = [
        BANK_CODE,
        BANK_CODE_WITHOUT_SERVER,
        BANK_CODE_WITHOUT_LOGIN_KINDS,
        BANK_CODE_GUESSED,
        BANK_CODE_NEIGHBOUR,
    ];
}

impl BankDirectory for FakeDirectory {
    fn lookup(&self, code: &BankCode) -> Option<BankInfo> {
        if code.country() != "DE" {
            return None;
        }
        let info = |name: &str, bic: Option<&str>, server: Option<FintsServer>| BankInfo {
            code: code.clone(),
            name: name.to_owned(),
            bic: bic.map(str::to_owned),
            server,
        };
        match code.code() {
            BANK_CODE => Some(info(
                "Testbank",
                Some("TESTDEFFXXX"),
                Some(FintsServer {
                    url: Self::URL.to_owned(),
                    login_kinds: Some(vec![Self::PIN_TAN, LoginKind::ChipCard]),
                }),
            )),
            BANK_CODE_WITHOUT_SERVER => Some(info("Bank without FinTS", None, None)),
            BANK_CODE_WITHOUT_LOGIN_KINDS => Some(info(
                "Bank with unknown login",
                None,
                Some(FintsServer {
                    url: "https://fints.unknown-login.example/".to_owned(),
                    login_kinds: None,
                }),
            )),
            BANK_CODE_GUESSED => Some(info(
                "Cooperative bank without FinTS",
                Some("GENODEF1XXX"),
                None,
            )),
            BANK_CODE_NEIGHBOUR => Some(info(
                "Cooperative bank next door",
                Some("GENODEF1YYY"),
                Some(FintsServer {
                    url: Self::ATRUVIA_URL.to_owned(),
                    login_kinds: Some(vec![LoginKind::PinTan {
                        customer_id: CustomerId::Unknown,
                    }]),
                }),
            )),
            _ => None,
        }
    }

    fn banks_starting_with(&self, country: &str, prefix: &str) -> Vec<BankInfo> {
        Self::CODES
            .iter()
            .filter(|code| code.starts_with(prefix))
            .filter_map(|code| self.lookup(&BankCode::new(country, code)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directory::{Setup, setup_for};
    use crate::iban::Iban;
    use crate::pin::{MemoryPinStore, Pin, PinMode};

    fn session() -> PinSession {
        PinSession::new(Box::new(MemoryPinStore::new()))
    }

    fn bank_with_login() -> (FakeBank, LoginId, PinSession) {
        let mut bank = FakeBank::new();
        let mut pins = session();
        let id = LoginId::new("fake-login-1");
        pins.provide(
            &id,
            Pin::new(FakeBank::PIN.to_owned()),
            PinMode::RememberUntilClose,
        )
        .expect("provided");
        let login = bank
            .add_login(&FakeBank::setup(), &mut pins)
            .expect("login added");
        assert_eq!(login, id);
        (bank, login, pins)
    }

    #[test]
    fn a_login_needs_the_pin_and_then_lists_its_accounts() {
        let mut bank = FakeBank::new();
        let mut pins = session();
        assert_eq!(
            bank.add_login(&FakeBank::setup(), &mut pins),
            Err(BankError::PinNeeded)
        );
        assert_eq!(bank.logins(), Ok(vec![]));
        let (bank, login, _) = bank_with_login();
        assert_eq!(
            bank.logins(),
            Ok(vec![Login {
                id: login.clone(),
                bank: BankCode::new("DE", BANK_CODE),
                kind: FakeDirectory::PIN_TAN,
            }])
        );
        let accounts = bank.accounts(&login).expect("accounts");
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].iban.as_deref(), Some(CURRENT_ACCOUNT_IBAN));
        assert_eq!(accounts[1].kind, BankAccountKind::CreditCard);
        assert_eq!(
            bank.accounts(&LoginId::new("nobody")),
            Err(BankError::UnknownLogin(LoginId::new("nobody")))
        );
    }

    #[test]
    fn a_wrong_pin_is_rejected_and_forgotten() {
        let mut bank = FakeBank::new();
        let mut pins = session();
        let id = LoginId::new("fake-login-1");
        pins.provide(&id, Pin::new("00000".to_owned()), PinMode::SaveInKeychain)
            .expect("provided");
        assert_eq!(
            bank.add_login(&FakeBank::setup(), &mut pins),
            Err(BankError::PinRejected)
        );
        assert_eq!(pins.is_saved(&id), Ok(false));
        assert!(!pins.is_remembered(&id));
        assert_eq!(bank.logins(), Ok(vec![]));
        // The same ID is used for the next try, so a typed PIN works.
        pins.provide(
            &id,
            Pin::new(FakeBank::PIN.to_owned()),
            PinMode::AskEachTime,
        )
        .expect("provided");
        assert_eq!(
            bank.add_login(&FakeBank::setup(), &mut pins),
            Ok(id.clone())
        );
        // Asked each time: dropped after the bank session.
        assert!(!pins.is_remembered(&id));
    }

    #[test]
    fn a_fetch_delivers_the_made_up_statement_and_filters_by_date() {
        let (mut bank, login, mut pins) = bank_with_login();
        let accounts = bank.accounts(&login).expect("accounts");
        let fetched = bank
            .fetch(&accounts[0].id, None, &mut pins)
            .expect("fetched");
        assert_eq!(fetched.transactions.len(), 4);
        assert_eq!(fetched.transactions.iter().filter(|t| t.booked).count(), 3);
        assert_eq!(
            fetched.balance.as_ref().map(|b| b.amount.fraction.as_str()),
            Some("325477/100")
        );
        let fetched = bank
            .fetch(&accounts[0].id, Some(october(5)), &mut pins)
            .expect("fetched");
        assert_eq!(fetched.transactions.len(), 2);
        let card = bank
            .fetch(&accounts[1].id, None, &mut pins)
            .expect("fetched");
        assert_eq!(card.transactions.len(), 2);
        assert_eq!(card.account.kind, BankAccountKind::CreditCard);
        assert_eq!(
            bank.fetch(&BankAccountId::new("none"), None, &mut pins),
            Err(BankError::UnknownAccount(BankAccountId::new("none")))
        );
    }

    #[test]
    fn failures_happen_once_and_on_the_next_call() {
        let (mut bank, login, mut pins) = bank_with_login();
        let account = bank.accounts(&login).expect("accounts")[0].id.clone();
        for (failure, error) in [
            (Failure::Unreachable, BankError::Unreachable),
            (
                Failure::Refused("9010: made-up bank message".to_owned()),
                BankError::Refused {
                    text: Some("9010: made-up bank message".to_owned()),
                },
            ),
            (Failure::Aborted, BankError::Aborted),
        ] {
            bank.fail_next(failure);
            assert_eq!(bank.fetch(&account, None, &mut pins), Err(error));
            assert!(bank.fetch(&account, None, &mut pins).is_ok());
        }
        bank.fail_next(Failure::InexactAmount);
        let fetched = bank.fetch(&account, None, &mut pins).expect("fetched");
        assert_eq!(fetched.transactions[0].amount.fraction, "1/3");
        let fetched = bank.fetch(&account, None, &mut pins).expect("fetched");
        assert_eq!(fetched.transactions[0].amount.fraction, "250000/100");
    }

    #[test]
    fn without_aqbanking_every_call_says_so() {
        let mut pins = session();
        for (mut bank, error) in [
            (FakeBank::not_installed(), BankError::NotInstalled),
            (
                FakeBank::too_old(),
                BankError::TooOld {
                    library: Library::AqBanking,
                    found: Version::new(6, 5, 0),
                    minimum: AQBANKING_MINIMUM,
                },
            ),
        ] {
            assert!(!bank.availability().is_available());
            assert_eq!(bank.logins(), Err(error.clone()));
            assert_eq!(
                bank.add_login(&FakeBank::setup(), &mut pins),
                Err(error.clone())
            );
            assert_eq!(
                bank.fetch(&BankAccountId::new("x"), None, &mut pins),
                Err(error)
            );
        }
    }

    #[test]
    fn removing_a_login_forgets_its_pin() {
        let (mut bank, login, mut pins) = bank_with_login();
        assert!(pins.is_remembered(&login));
        crate::backend::remove_login(&mut bank, &login, &mut pins).expect("removed");
        assert!(!pins.is_remembered(&login));
        assert_eq!(bank.logins(), Ok(vec![]));
        assert_eq!(
            crate::backend::remove_login(&mut bank, &login, &mut pins),
            Err(BankError::UnknownLogin(login))
        );
    }

    #[test]
    fn the_fake_directory_offers_every_setup_state() {
        let bank = FakeBank::new();
        let iban = Iban::parse(CURRENT_ACCOUNT_IBAN).expect("an IBAN");
        match setup_for(&iban, bank.directory()) {
            Setup::Ready {
                bank,
                url,
                login_kinds,
                account_number,
            } => {
                assert_eq!(bank.name, "Testbank");
                assert_eq!(url, FakeDirectory::URL);
                assert_eq!(
                    login_kinds,
                    vec![FakeDirectory::PIN_TAN, LoginKind::ChipCard]
                );
                assert_eq!(account_number.as_deref(), Some("0532013000"));
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            FakeDirectory.lookup(&BankCode::new("DE", BANK_CODE_WITHOUT_SERVER)),
            Some(BankInfo { server: None, .. })
        ));
        assert!(matches!(
            FakeDirectory.lookup(&BankCode::new("DE", BANK_CODE_WITHOUT_LOGIN_KINDS)),
            Some(BankInfo {
                server: Some(FintsServer {
                    login_kinds: None,
                    ..
                }),
                ..
            })
        ));
        assert_eq!(FakeDirectory.lookup(&BankCode::new("DE", "99999999")), None);
        assert_eq!(FakeDirectory.lookup(&BankCode::new("AT", BANK_CODE)), None);
        // A cooperative bank without a server gets its neighbour's.
        let iban = Iban::parse("DE38400000100000000001").expect("an IBAN");
        match setup_for(&iban, bank.directory()) {
            Setup::GuessedServer { bank, url, .. } => {
                assert_eq!(bank.name, "Cooperative bank without FinTS");
                assert_eq!(url, FakeDirectory::ATRUVIA_URL);
            }
            other => panic!("{other:?}"),
        }
    }

    /// Dialogs with fixed answers.
    struct Answers {
        tan: Option<&'static str>,
        button: Option<usize>,
    }

    impl BankDialogs for Answers {
        fn tan(&mut self, _request: &TanRequest) -> Option<crate::Tan> {
            self.tan.map(|tan| crate::Tan::new(tan.to_owned()))
        }

        fn question(&mut self, _question: &BankQuestion) -> Option<usize> {
            self.button
        }
    }

    #[test]
    fn a_second_factor_is_asked_through_the_dialogs() {
        let setup = FakeBank::setup();
        let pin = || {
            let mut pins = PinSession::new(Box::new(crate::MemoryPinStore::new()));
            pins.provide(
                &LoginId::pending(),
                crate::Pin::new(FakeBank::PIN.to_owned()),
                crate::PinMode::AskEachTime,
            )
            .expect("provided");
            pins
        };
        let cases = [
            (SecondFactor::Tan, None, Err(BankError::TanNeeded)),
            (
                SecondFactor::Tan,
                Some(Answers {
                    tan: Some(FakeBank::TAN),
                    button: None,
                }),
                Ok(()),
            ),
            (
                SecondFactor::Tan,
                Some(Answers {
                    tan: Some("000000"),
                    button: None,
                }),
                Err(BankError::Refused {
                    text: Some("9941 TAN wrong (made-up message of the fake bank)".to_owned()),
                }),
            ),
            (
                SecondFactor::Tan,
                Some(Answers {
                    tan: None,
                    button: None,
                }),
                Err(BankError::Aborted),
            ),
            (
                SecondFactor::Approval,
                Some(Answers {
                    tan: None,
                    button: Some(0),
                }),
                Ok(()),
            ),
            (
                SecondFactor::Approval,
                Some(Answers {
                    tan: None,
                    button: Some(1),
                }),
                Err(BankError::Aborted),
            ),
            (
                SecondFactor::PictureTan,
                Some(Answers {
                    tan: Some(FakeBank::TAN),
                    button: None,
                }),
                Err(BankError::TanMethodUnsupported),
            ),
        ];
        for (factor, answers, expected) in cases {
            let mut bank = FakeBank::new().with_second_factor(factor);
            if let Some(answers) = answers {
                bank.set_dialogs(Box::new(answers));
            }
            let added = bank.add_login(&setup, &mut pin()).map(|_| ());
            assert_eq!(added, expected, "{factor:?}");
            assert_eq!(
                bank.logins().map(|logins| logins.len()),
                Ok(usize::from(expected.is_ok()))
            );
        }
    }
}
