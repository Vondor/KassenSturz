// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The bank abstraction: what every backend offers KassenSturz (D-012,
//! D-060, D-145). The real one speaks FinTS through AqBanking (M4.5); the
//! fake one is for tests ([`crate::FakeBank`], D-055).
//!
//! Everything here only reads. There is no function that starts a transfer,
//! a standing order or a direct debit, and none will be added (D-053,
//! threat T-14).

use std::fmt;

use kassensturz_core::{CurrencyCode, Date};

use crate::availability::Availability;
use crate::dialogs::BankDialogs;
use crate::directory::{BankDirectory, LoginKind};
use crate::error::BankError;
use crate::fetched::Fetched;
use crate::iban::BankCode;
use crate::pin::PinSession;

macro_rules! text_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(String);

        impl $name {
            /// Wraps the backend's ID.
            pub fn new(id: impl Into<String>) -> Self {
                Self(id.into())
            }

            /// The ID as text, for the backend and for the bank link in the
            /// book (Q-086).
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

text_id! {
    /// Names a bank login in its backend, e.g. AqBanking's user ID as text.
    /// It is not the login name the bank knows the user by: that stays in
    /// the backend's own storage (D-139) and never comes out of it.
    LoginId
}

impl LoginId {
    /// The ID a login has while it is being set up and the backend hasn't
    /// named it yet. The app keeps the PIN the user typed in the setup
    /// under this ID ([`crate::PinSession::provide`]); [`Bank::add_login`]
    /// answers the bank from it and moves it to the new login's ID when
    /// the setup worked, or forgets it when it didn't.
    pub fn pending() -> Self {
        Self::new("pending")
    }
}

text_id! {
    /// Names a bank account in its backend, e.g. AqBanking's account ID as
    /// text.
    BankAccountId
}

/// A bank login as the app lists it: which bank, how the user logs in, and
/// its ID. The login name isn't here on purpose (threat T-19).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Login {
    /// Its ID in the backend.
    pub id: LoginId,
    /// The bank it is for.
    pub bank: BankCode,
    /// How the user logs in there.
    pub kind: LoginKind,
}

/// What setting up a login needs, as the setup screens gathered it (M4.6)
/// from the questions of the bank's [`LoginKind`]. The backend keeps it in
/// its own storage; this crate keeps nothing of it after the call. Its
/// `Debug` shows neither the login name nor the customer ID (threat T-19).
#[derive(Clone, PartialEq, Eq)]
pub struct LoginSetup {
    /// The bank.
    pub bank: BankCode,
    /// The FinTS server's address.
    pub url: String,
    /// How the user logs in.
    pub kind: LoginKind,
    /// The login name the bank gave.
    pub login_name: String,
    /// The customer ID the bank gave, if it has one apart from the login
    /// name.
    pub customer_id: Option<String>,
}

impl fmt::Debug for LoginSetup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoginSetup")
            .field("bank", &self.bank)
            .field("url", &self.url)
            .field("kind", &self.kind)
            .field("login_name", &"…")
            .field("customer_id", &self.customer_id.as_ref().map(|_| "…"))
            .finish()
    }
}

/// An account at a bank, as the bank lists it after a login.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BankAccount {
    /// Its ID in the backend.
    pub id: BankAccountId,
    /// The login it is reached through.
    pub login: LoginId,
    /// What kind of account the bank says it is.
    pub kind: BankAccountKind,
    /// Its IBAN, if the bank gives one; a credit card usually has none.
    pub iban: Option<String>,
    /// Any other number the bank gives it by, e.g. an account number or a
    /// card number as the bank shows it.
    pub number: Option<String>,
    /// Its currency, if the bank says. Amounts without a currency of their
    /// own are taken to be in it (D-141).
    pub currency: Option<CurrencyCode>,
    /// The name the bank gives it, e.g. the product's name.
    pub name: Option<String>,
}

/// What kind of account a bank account is, as far as the bank says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BankAccountKind {
    /// A current (checking) account.
    Current,
    /// A savings account.
    Savings,
    /// A credit card, whose statements come as its own kind of fetch at the
    /// bank but as the same [`Fetched`] here (D-029).
    CreditCard,
    /// Anything else, or not said.
    Other,
}

/// A bank backend: AqBanking (M4.5) or the fake bank for tests.
///
/// A backend only reads. It has no function that moves money, and the
/// bindings it is built on declare none (D-053, threat T-14). PINs come
/// from the [`PinSession`] it is handed: when the session has none, the
/// backend fails with [`BankError::PinNeeded`] and the app asks the user,
/// then calls again; when the bank calls a PIN wrong, the backend tells the
/// session ([`PinSession::rejected`]) and fails with
/// [`BankError::PinRejected`] (D-140).
pub trait Bank {
    /// Whether the backend can be used on this computer (D-054).
    fn availability(&self) -> Availability;

    /// Where banks are looked up for the setup screens (D-145).
    fn directory(&self) -> &dyn BankDirectory;

    /// Gives the backend the app's dialogs for what a bank asks while a
    /// call runs: a TAN, or a question such as an approval in the bank's
    /// app (D-151). Without them a TAN request ends the call with
    /// [`BankError::TanNeeded`].
    fn set_dialogs(&mut self, dialogs: Box<dyn BankDialogs>);

    /// The logins set up in this backend.
    fn logins(&self) -> Result<Vec<Login>, BankError>;

    /// Sets up a login and fetches its accounts from the bank, which needs
    /// the PIN: the one kept under [`LoginId::pending`], or, for a
    /// backend that names the login beforehand, under that name. Returns
    /// the new login's ID, which now holds the PIN. On failure nothing is
    /// kept, not even the PIN (D-057).
    fn add_login(
        &mut self,
        setup: &LoginSetup,
        pins: &mut PinSession,
    ) -> Result<LoginId, BankError>;

    /// Removes a login and its accounts from the backend. Call
    /// [`remove_login`] instead, which also forgets the PIN (D-057).
    fn remove_login(&mut self, login: &LoginId) -> Result<(), BankError>;

    /// The accounts the bank lists for `login`.
    fn accounts(&self, login: &LoginId) -> Result<Vec<BankAccount>, BankError>;

    /// Fetches the balance and the transactions of `account` from `since`
    /// on (the bank decides how far back it goes without a date), as the
    /// bank delivered them. Converting them into a statement is
    /// [`crate::statement`]'s job, so every backend is converted the same
    /// way.
    fn fetch(
        &mut self,
        account: &BankAccountId,
        since: Option<Date>,
        pins: &mut PinSession,
    ) -> Result<Fetched, BankError>;
}

/// Removes `login` from `bank` and forgets its PIN, remembered or saved
/// (D-057: a saved PIN goes with its login). The PIN is forgotten even when
/// the backend fails, so no PIN outlives an attempt to remove its login.
pub fn remove_login(
    bank: &mut dyn Bank,
    login: &LoginId,
    pins: &mut PinSession,
) -> Result<(), BankError> {
    let removed = bank.remove_login(login);
    pins.forget(login)?;
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directory::CustomerId;

    #[test]
    fn a_setup_shows_no_login_name_or_customer_id() {
        let setup = LoginSetup {
            bank: BankCode::new("DE", "37040044"),
            url: "https://fints.testbank.example/".to_owned(),
            kind: LoginKind::PinTan {
                customer_id: CustomerId::Separate,
            },
            login_name: "made-up-login-name".to_owned(),
            customer_id: Some("made-up-customer-id".to_owned()),
        };
        let debug = format!("{setup:?}");
        assert!(!debug.contains("made-up"), "{debug}");
        assert!(debug.contains("37040044"), "{debug}");
    }

    #[test]
    fn ids_are_text() {
        let id = LoginId::new("7");
        assert_eq!(id.as_str(), "7");
        assert_eq!(id.to_string(), "7");
        assert_eq!(BankAccountId::new(String::from("a")).as_str(), "a");
    }
}
