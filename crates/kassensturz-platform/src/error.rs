// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What can go wrong with the keychain: codes without values (D-034).

use std::fmt;

use kassensturz_core::{ErrorReport, Report};

/// The keychain couldn't do what was asked. No variant carries a secret, a
/// key or the keychain library's text: a report, a log line or a panic
/// message that shows one shows nothing worth protecting (threat T-02).
/// The front end turns the code into a message (`docs/ERROR_CODES.md`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum KeychainError {
    /// No keychain answered, or it refused: the Secret Service isn't
    /// running, the wallet is locked and the user didn't unlock it, or the
    /// sandbox has no permission to talk to it (D-142). Nothing was saved,
    /// read or removed.
    Unavailable,
    /// The keychain answered with an error. `kind` names the kind of
    /// error the keychain library reported, for logs; it never holds a
    /// value.
    Failed {
        /// The keychain library's kind of error, e.g. `PlatformFailure`.
        kind: &'static str,
    },
    /// What the keychain holds under the key isn't text, so it can't have
    /// been saved by KassenSturz, or it was damaged. The value is not kept.
    NotText,
    /// The key isn't two short ASCII words (see
    /// [`SecretKey::new`](crate::SecretKey::new)).
    InvalidKey,
}

impl fmt::Display for KeychainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => f.write_str("the system keychain couldn't be reached"),
            Self::Failed { kind } => write!(f, "the system keychain failed ({kind})"),
            Self::NotText => f.write_str("the saved value isn't text"),
            Self::InvalidKey => f.write_str("the key isn't two short ASCII words"),
        }
    }
}

impl std::error::Error for KeychainError {}

impl Report for KeychainError {
    fn report(&self) -> ErrorReport {
        match self {
            Self::Unavailable => ErrorReport::new("platform.keychain-unavailable"),
            Self::Failed { .. } => ErrorReport::new("platform.keychain-failed"),
            Self::NotText => ErrorReport::new("platform.keychain-not-text"),
            Self::InvalidKey => ErrorReport::new("platform.invalid-secret-key"),
        }
    }
}

// The list of error codes lives in `kassensturz-store`'s source, so that the
// crates that must not depend on each other all check their codes the same
// way.
#[cfg(test)]
#[path = "../../kassensturz-store/src/error/list.rs"]
mod list;
#[cfg(test)]
mod tests;
