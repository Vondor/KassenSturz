// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved PINs in the system keychain (D-057, D-147): the [`PinStore`] over
//! `kassensturz-platform`'s [`Keychain`], the only way a PIN reaches the
//! keychain (AGENTS.md §3.1 rule 7).
//!
//! The key is `bank-pin/<login ID>`, so a saved PIN goes with its login
//! and nothing else of KassenSturz's collides with it. Every keychain
//! error becomes `bank.keychain`, without the keychain's own text: the
//! user is told to unlock the keychain or to type the PIN each time
//! (threat T-02).

use kassensturz_platform::{Keychain, KeychainError, Secret, SecretKey};

use crate::backend::LoginId;
use crate::error::BankError;
use crate::pin::{Pin, PinStore};

/// The kind part of the keychain key.
const KIND: &str = "bank-pin";

/// A [`PinStore`] in the system keychain.
pub struct KeychainPinStore {
    keychain: Box<dyn Keychain>,
}

impl std::fmt::Debug for KeychainPinStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("KeychainPinStore")
    }
}

impl KeychainPinStore {
    /// A store over `keychain`: the system's, or a
    /// [`kassensturz_platform::MemoryKeychain`] in tests.
    pub fn new(keychain: Box<dyn Keychain>) -> Self {
        Self { keychain }
    }

    fn key(login: &LoginId) -> Result<SecretKey, BankError> {
        SecretKey::new(KIND, login.as_str()).map_err(bank_error)
    }
}

impl PinStore for KeychainPinStore {
    fn save(&mut self, login: &LoginId, pin: &Pin) -> Result<(), BankError> {
        self.keychain
            .store(&Self::key(login)?, &Secret::new(pin.as_str().to_owned()))
            .map_err(bank_error)
    }

    fn load(&self, login: &LoginId) -> Result<Option<Pin>, BankError> {
        self.keychain
            .read(&Self::key(login)?)
            .map(|secret| secret.map(|secret| Pin::new(secret.expose().to_owned())))
            .map_err(bank_error)
    }

    fn delete(&mut self, login: &LoginId) -> Result<(), BankError> {
        self.keychain.delete(&Self::key(login)?).map_err(bank_error)
    }
}

/// The keychain's error as `bank.keepassxc` when KeePassXC was the place
/// (D-164), else `bank.keychain`, without the keychain's own text.
fn bank_error(error: KeychainError) -> BankError {
    match error {
        KeychainError::KeePassXcUnavailable => BankError::KeePassXc,
        _ => BankError::Keychain,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use kassensturz_platform::{KeychainError, MemoryKeychain};

    use super::*;
    use crate::pin::{PinAnswer, PinMode, PinSession};

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

    fn login() -> LoginId {
        LoginId::new("7")
    }

    #[test]
    fn a_saved_pin_lives_in_the_keychain_under_the_logins_key() {
        let keychain = Shared(Arc::new(MemoryKeychain::new()));
        let mut session =
            PinSession::new(Box::new(KeychainPinStore::new(Box::new(keychain.clone()))));
        session
            .provide(
                &login(),
                Pin::new("0000".to_owned()),
                PinMode::SaveInKeychain,
            )
            .expect("saved");
        let key = SecretKey::new("bank-pin", "7").expect("a valid key");
        assert!(keychain.0.contains(&key));
        assert_eq!(keychain.0.len(), 1);
        // A new run of the app reads it back.
        let next_run = PinSession::new(Box::new(KeychainPinStore::new(Box::new(keychain.clone()))));
        assert_eq!(
            next_run.answer(&login()),
            Ok(PinAnswer::Known(Pin::new("0000".to_owned())))
        );
        session.forget(&login()).expect("forgotten");
        assert!(keychain.0.is_empty());
    }

    #[test]
    fn a_keychain_that_fails_gives_the_bank_code_without_its_text() {
        let keychain = Shared(Arc::new(MemoryKeychain::new()));
        keychain.0.fail_with(Some(KeychainError::Unavailable));
        let mut store = KeychainPinStore::new(Box::new(keychain));
        assert_eq!(
            store.save(&login(), &Pin::new("0000".to_owned())),
            Err(BankError::Keychain)
        );
        assert_eq!(store.load(&login()), Err(BankError::Keychain));
        assert_eq!(store.delete(&login()), Err(BankError::Keychain));
        // A login ID the keychain can't take is the same error.
        let store = KeychainPinStore::new(Box::new(MemoryKeychain::new()));
        assert_eq!(
            store.load(&LoginId::new("has a space")),
            Err(BankError::Keychain)
        );
    }
}
