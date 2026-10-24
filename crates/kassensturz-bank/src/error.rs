// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Why a bank connection can't be used, or what a bank delivered can't be
//! turned into a statement, as codes with parameters (D-034, D-084).
//!
//! Nothing here ever carries a PIN, a TAN or a login name: an error names a
//! login by its ID and a bank by its code, and the bank's own answer text
//! only as data (threat T-02 in `docs/THREAT_MODEL.md`).

use kassensturz_core::{CurrencyCode, ErrorParam, ErrorReport, Report};
use kassensturz_import::ImportError;

use crate::availability::{Library, Version};
use crate::backend::{BankAccountId, LoginId};

/// Why a bank connection can't be used, or fetched data can't be used.
///
/// Every variant turns into an [`ErrorReport`] whose code starts with
/// `bank.` (`docs/ERROR_CODES.md`). Nothing is imported when a fetch fails,
/// so the book is unchanged; the fetch can be tried again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BankError {
    /// AqBanking or Gwenhywfar isn't installed, so there are no bank
    /// connections on this computer (D-054).
    NotInstalled,
    /// The installed library is older than the one KassenSturz needs
    /// (D-138).
    TooOld {
        /// Which library.
        library: Library,
        /// The version found.
        found: Version,
        /// The oldest version KassenSturz works with.
        minimum: Version,
    },
    /// This build has no FinTS registration number, so it can't talk to a
    /// bank (D-058, Q-087).
    NotRegistered,
    /// The text isn't an IBAN: wrong characters, wrong length or wrong check
    /// digits (ISO 13616).
    MalformedIban,
    /// No bank login has this ID; it may have been removed.
    UnknownLogin(LoginId),
    /// No bank account has this ID; the bank may have closed it, or the
    /// login it belongs to was removed.
    UnknownAccount(BankAccountId),
    /// No bank login lists the bank account an account in the book is
    /// linked to (D-148): the login was removed or never set up on this
    /// computer, or the bank no longer lists the account (M4.7).
    NoBankAccount,
    /// The bank asks for the PIN and none is remembered or saved: the user
    /// has to type it ([`crate::PinSession`]).
    PinNeeded,
    /// The bank called the PIN wrong. The remembered and the saved PIN are
    /// forgotten, so the login isn't blocked by repeating it (D-140, threat
    /// T-05).
    PinRejected,
    /// The bank's server couldn't be reached.
    Unreachable,
    /// The bank answered with an error. Its text, if any, is data for the
    /// user to read, never an instruction (AGENTS.md §8).
    Refused {
        /// The bank's message, if it sent one.
        text: Option<String>,
    },
    /// The bank asks for a TAN and the backend was given no dialogs to ask
    /// the user with ([`crate::Bank::set_dialogs`], D-151).
    TanNeeded,
    /// The bank shows the TAN's challenge as a picture (a flicker code, a
    /// photoTAN or a QR code), which KassenSturz can't show yet: the user
    /// chooses a TAN method at their bank that works as text or as an
    /// approval in an app (D-151, Q-089).
    TanMethodUnsupported,
    /// The bank server's certificate didn't pass every check, so nothing
    /// was sent to it (D-140, threat T-12).
    UntrustedServer,
    /// The user closed the bank's dialog, e.g. the TAN entry.
    Aborted,
    /// This kind of login (a chip card) needs the setup dialog of the
    /// bank's library, which KassenSturz doesn't offer yet (Q-035, D-144,
    /// D-151, Q-089).
    NeedsSetupDialog,
    /// A setup command of the bank backend failed before the login was
    /// complete; nothing is kept. The text, if any, is the backend's own
    /// message, data for the user to read (AGENTS.md §8).
    SetupFailed {
        /// The backend's message, if it gave one.
        text: Option<String>,
    },
    /// AqBanking's own folder in KassenSturz's data folder couldn't be
    /// created or opened (D-139).
    Folder,
    /// Another bank connection is still running on this thread.
    Busy,
    /// The system keychain couldn't be reached to save, read or delete a PIN
    /// (D-057).
    Keychain,
    /// KeePassXC, the place the user chose for saved PINs, couldn't be
    /// reached to save, read or delete a PIN (D-164).
    KeePassXc,
    /// An amount the bank sent isn't a fraction of whole numbers, or names a
    /// currency that isn't a code (D-141).
    MalformedAmount {
        /// The amount as the bank sent it.
        amount: String,
    },
    /// An amount isn't a whole number of the currency's minor units, e.g. a
    /// third of a cent. It is refused, never rounded (D-141).
    InexactAmount {
        /// The amount as the bank sent it.
        amount: String,
    },
    /// An amount is too large for KassenSturz to work with.
    AmountTooLarge {
        /// The amount as the bank sent it.
        amount: String,
    },
    /// Neither an amount nor its bank account says which currency it is in
    /// (D-141).
    MissingCurrency,
    /// An amount is in another currency than its bank account.
    CurrencyMismatch {
        /// The account's currency.
        expected: CurrencyCode,
        /// The amount's currency.
        found: CurrencyCode,
    },
    /// A transaction has neither a booking date nor a value date.
    MissingDate,
    /// The fetched statement failed the checks every statement gets before
    /// the import pipeline sees it: the limits on entries and text (D-091),
    /// or a currency no table lists. The cause says which.
    Statement(ImportError),
}

impl Report for BankError {
    fn report(&self) -> ErrorReport {
        let text = |value: &str| ErrorParam::Text(value.to_owned());
        match self {
            Self::NotInstalled => ErrorReport::new("bank.not-installed"),
            Self::TooOld {
                library,
                found,
                minimum,
            } => ErrorReport::new("bank.too-old")
                .with("library", ErrorParam::Keyword(library.keyword()))
                .with("found", text(&found.to_string()))
                .with("minimum", text(&minimum.to_string())),
            Self::NotRegistered => ErrorReport::new("bank.not-registered"),
            Self::MalformedIban => ErrorReport::new("bank.malformed-iban"),
            Self::UnknownLogin(login) => {
                ErrorReport::new("bank.unknown-login").with("login", text(login.as_str()))
            }
            Self::UnknownAccount(account) => {
                ErrorReport::new("bank.unknown-account").with("account", text(account.as_str()))
            }
            Self::NoBankAccount => ErrorReport::new("bank.no-bank-account"),
            Self::PinNeeded => ErrorReport::new("bank.pin-needed"),
            Self::PinRejected => ErrorReport::new("bank.pin-rejected"),
            Self::Unreachable => ErrorReport::new("bank.unreachable"),
            Self::Refused { text: message } => {
                let report = ErrorReport::new("bank.refused");
                match message {
                    Some(message) => report.with("text", text(message)),
                    None => report,
                }
            }
            Self::TanNeeded => ErrorReport::new("bank.tan-needed"),
            Self::TanMethodUnsupported => ErrorReport::new("bank.tan-method-unsupported"),
            Self::UntrustedServer => ErrorReport::new("bank.untrusted-server"),
            Self::Aborted => ErrorReport::new("bank.aborted"),
            Self::NeedsSetupDialog => ErrorReport::new("bank.needs-setup-dialog"),
            Self::SetupFailed { text: message } => {
                let report = ErrorReport::new("bank.setup-failed");
                match message {
                    Some(message) => report.with("text", text(message)),
                    None => report,
                }
            }
            Self::Folder => ErrorReport::new("bank.folder"),
            Self::Busy => ErrorReport::new("bank.busy"),
            Self::Keychain => ErrorReport::new("bank.keychain"),
            Self::KeePassXc => ErrorReport::new("bank.keepassxc"),
            Self::MalformedAmount { amount } => {
                ErrorReport::new("bank.malformed-amount").with("amount", text(amount))
            }
            Self::InexactAmount { amount } => {
                ErrorReport::new("bank.inexact-amount").with("amount", text(amount))
            }
            Self::AmountTooLarge { amount } => {
                ErrorReport::new("bank.amount-too-large").with("amount", text(amount))
            }
            Self::MissingCurrency => ErrorReport::new("bank.missing-currency"),
            Self::CurrencyMismatch { expected, found } => {
                ErrorReport::new("bank.currency-mismatch")
                    .with("expected", ErrorParam::Currency(*expected))
                    .with("found", ErrorParam::Currency(*found))
            }
            Self::MissingDate => ErrorReport::new("bank.missing-date"),
            Self::Statement(cause) => ErrorReport::new("bank.statement").caused_by(cause.report()),
        }
    }
}

impl From<ImportError> for BankError {
    fn from(error: ImportError) -> Self {
        Self::Statement(error)
    }
}

#[cfg(test)]
#[path = "../../kassensturz-store/src/error/list.rs"]
mod list;

#[cfg(test)]
mod tests;
