// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The fake keychain: secrets in memory only.

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

use crate::{Keychain, KeychainError, KeychainPlace, Secret, SecretKey};

/// A keychain in memory, for tests and for a session without a system
/// keychain: nothing is written anywhere, and everything is wiped when it
/// is dropped (each [`Secret`] wipes itself).
///
/// It can be told to fail every call with one error, as an unreachable or
/// locked keychain would ([`MemoryKeychain::fail_with`]), so the code on
/// top of it can be tested for that case too.
#[derive(Default)]
pub struct MemoryKeychain {
    secrets: Mutex<HashMap<SecretKey, Secret>>,
    failure: Mutex<Option<KeychainError>>,
    /// The place [`Keychain::open`] gives, as a KeePassXC database does.
    place: Mutex<Option<KeychainPlace>>,
}

impl MemoryKeychain {
    /// An empty keychain that answers every call.
    pub fn new() -> Self {
        Self::default()
    }

    /// From now on every call fails with `error`; `None` makes it answer
    /// again. What is stored stays stored either way.
    pub fn fail_with(&self, error: Option<KeychainError>) {
        *self.failure.lock().expect("no panic holds the failure") = error;
    }

    /// From now on [`Keychain::open`] answers `place`, as a keychain the
    /// user can tell apart from others does (D-167).
    pub fn place(&self, place: Option<KeychainPlace>) {
        *self.place.lock().expect("no panic holds the place") = place;
    }

    /// How many secrets are stored.
    pub fn len(&self) -> usize {
        self.secrets
            .lock()
            .expect("no panic holds the secrets")
            .len()
    }

    /// Whether no secret is stored.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether a secret is stored under `key`. Unlike [`Keychain::read`],
    /// this answers even while the keychain is told to fail, so a test can
    /// check what the keychain holds.
    pub fn contains(&self, key: &SecretKey) -> bool {
        self.secrets
            .lock()
            .expect("no panic holds the secrets")
            .contains_key(key)
    }

    fn check(&self) -> Result<(), KeychainError> {
        match &*self.failure.lock().expect("no panic holds the failure") {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }
}

impl fmt::Debug for MemoryKeychain {
    /// The number of secrets and whether it fails, never a secret or a key.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryKeychain")
            .field("secrets", &self.len())
            .field(
                "failure",
                &self.failure.lock().expect("no panic holds the failure"),
            )
            .finish()
    }
}

impl Keychain for MemoryKeychain {
    fn store(&self, key: &SecretKey, secret: &Secret) -> Result<(), KeychainError> {
        self.check()?;
        self.secrets
            .lock()
            .expect("no panic holds the secrets")
            .insert(key.clone(), secret.clone());
        Ok(())
    }

    fn read(&self, key: &SecretKey) -> Result<Option<Secret>, KeychainError> {
        self.check()?;
        Ok(self
            .secrets
            .lock()
            .expect("no panic holds the secrets")
            .get(key)
            .cloned())
    }

    fn delete(&self, key: &SecretKey) -> Result<(), KeychainError> {
        self.check()?;
        self.secrets
            .lock()
            .expect("no panic holds the secrets")
            .remove(key);
        Ok(())
    }

    fn open(&self) -> Result<Option<KeychainPlace>, KeychainError> {
        self.check()?;
        Ok(self.place.lock().expect("no panic holds the place").clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(id: &str) -> SecretKey {
        SecretKey::new("bank-pin", id).unwrap()
    }

    fn secret(value: &str) -> Secret {
        Secret::new(value.to_owned())
    }

    #[test]
    fn a_stored_secret_is_read_back_and_replaced() {
        let keychain = MemoryKeychain::new();
        assert!(keychain.is_empty());
        assert_eq!(keychain.read(&key("login-1")), Ok(None));

        keychain.store(&key("login-1"), &secret("1234")).unwrap();
        assert_eq!(keychain.read(&key("login-1")), Ok(Some(secret("1234"))));
        assert_eq!(keychain.read(&key("login-2")), Ok(None));
        assert_eq!(keychain.len(), 1);

        keychain.store(&key("login-1"), &secret("5678")).unwrap();
        assert_eq!(keychain.read(&key("login-1")), Ok(Some(secret("5678"))));
        assert_eq!(keychain.len(), 1);
    }

    #[test]
    fn deleting_removes_and_deleting_again_is_fine() {
        let keychain = MemoryKeychain::new();
        keychain.store(&key("login-1"), &secret("1234")).unwrap();
        assert_eq!(keychain.delete(&key("login-1")), Ok(()));
        assert_eq!(keychain.read(&key("login-1")), Ok(None));
        assert_eq!(keychain.delete(&key("login-1")), Ok(()));
        assert_eq!(keychain.delete(&key("never-stored")), Ok(()));
        assert!(keychain.is_empty());
    }

    #[test]
    fn a_failing_keychain_fails_every_call_but_keeps_what_it_has() {
        let keychain = MemoryKeychain::new();
        keychain.store(&key("login-1"), &secret("1234")).unwrap();
        keychain.fail_with(Some(KeychainError::Unavailable));
        assert_eq!(
            keychain.store(&key("login-2"), &secret("0000")),
            Err(KeychainError::Unavailable)
        );
        assert_eq!(
            keychain.read(&key("login-1")),
            Err(KeychainError::Unavailable)
        );
        assert_eq!(
            keychain.delete(&key("login-1")),
            Err(KeychainError::Unavailable)
        );
        assert!(keychain.contains(&key("login-1")));
        assert!(!keychain.contains(&key("login-2")));

        keychain.fail_with(None);
        assert_eq!(keychain.read(&key("login-1")), Ok(Some(secret("1234"))));
    }

    #[test]
    fn debug_shows_no_secret_and_no_key() {
        let keychain = MemoryKeychain::new();
        keychain.store(&key("login-1"), &secret("1234")).unwrap();
        keychain.fail_with(Some(KeychainError::NotText));
        let shown = format!("{keychain:?}");
        assert_eq!(
            shown,
            "MemoryKeychain { secrets: 1, failure: Some(NotText) }"
        );
        assert!(!shown.contains("1234") && !shown.contains("login-1"));
    }

    #[test]
    fn it_is_shared_between_threads() {
        let keychain = std::sync::Arc::new(MemoryKeychain::new());
        let other = std::sync::Arc::clone(&keychain);
        std::thread::spawn(move || other.store(&key("login-1"), &secret("1234")))
            .join()
            .unwrap()
            .unwrap();
        assert_eq!(keychain.read(&key("login-1")), Ok(Some(secret("1234"))));
    }
}
