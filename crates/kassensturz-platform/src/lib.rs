// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What differs between desktops, behind traits (`docs/ARCHITECTURE.md`):
//! in M4.1 the system keychain, where a banking PIN is saved only when the
//! user chooses so (D-057, AGENTS.md §3.1 rule 7).
//!
//! The [`Keychain`] trait stores, reads and deletes one [`Secret`] under a
//! [`SecretKey`]. [`SystemKeychain`] is the real one: through the `keyring`
//! crate (the maintainer's choice for Q-085, D-147) it reaches the Secret
//! Service on Linux, which KWallet and GNOME Keyring answer, and later
//! Windows' Credential Manager and the macOS Keychain (M7).
//! [`MemoryKeychain`] is the fake for tests and for a session without a
//! keychain: it keeps secrets in memory only and can be told to fail like
//! an unreachable keychain.
//!
//! [`SecretStorage`] is the user's choice of where saved secrets go: the
//! system keychain by default, or, as advanced choices, a KeePassXC database
//! through KeePassXC's browser interface ([`KeePassXcKeychain`], on Linux,
//! D-164) or nowhere (D-163).
//!
//! A [`Secret`] never shows its value, not even in `Debug`, and wipes its
//! memory when dropped (threat T-02 in `docs/THREAT_MODEL.md`). Errors are
//! codes without values ([`KeychainError`], D-034): no error, log or report
//! ever carries a secret, and the keychain library's own errors, some of
//! which carry the stored bytes, are turned into KassenSturz's codes before
//! they go anywhere.
//!
//! This crate depends only on `kassensturz-core` and the keychain library.

#![forbid(unsafe_code)]

mod error;
#[cfg(unix)]
mod keepassxc;
mod key;
mod memory;
mod secret;
mod storage;
mod system;

pub use error::KeychainError;
#[cfg(unix)]
pub use keepassxc::KeePassXcKeychain;
pub use key::SecretKey;
pub use memory::MemoryKeychain;
pub use secret::Secret;
pub use storage::SecretStorage;
pub use system::{APP_ID, SystemKeychain};

/// A place that keeps secrets for the user, such as a saved banking PIN
/// (D-057): the system keychain, or a fake in tests.
///
/// Every secret is stored under a [`SecretKey`]. Storing under a key that
/// already has a secret replaces it; deleting a key that has none does
/// nothing and succeeds, so a secret can always be removed at once (D-140,
/// threat T-05). An implementation never logs, prints or copies a secret
/// beyond what the keychain itself needs.
pub trait Keychain: Send + Sync {
    /// Saves `secret` under `key`, replacing a saved one.
    fn store(&self, key: &SecretKey, secret: &Secret) -> Result<(), KeychainError>;

    /// The secret saved under `key`, or `None` when there is none.
    fn read(&self, key: &SecretKey) -> Result<Option<Secret>, KeychainError>;

    /// Removes the secret saved under `key`; nothing happens if there is
    /// none.
    fn delete(&self, key: &SecretKey) -> Result<(), KeychainError>;

    /// Gets the place ready to save secrets, asking the user for what it
    /// needs (e.g. to unlock a KeePassXC database and allow KassenSturz),
    /// and says where in it secrets go, when the user can tell that place
    /// apart from others (D-166). A keychain that needs nothing first
    /// answers `None`.
    fn open(&self) -> Result<Option<KeychainPlace>, KeychainError> {
        Ok(None)
    }
}

/// Where in a keychain the user can see saved secrets (D-166): e.g. a
/// KeePassXC database's name and the group the entries go into. Names the
/// user gave, shown as they are; never a secret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeychainPlace {
    /// The keychain's name, e.g. the KeePassXC database's.
    pub name: String,
    /// The group or folder in it the secrets go into.
    pub group: String,
}
