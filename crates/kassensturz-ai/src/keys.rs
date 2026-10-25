// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The provider's key: typed each time, remembered until KassenSturz
//! closes, or saved where the Password storage setting says (D-169,
//! D-184; threat T-25).
//!
//! A key is a [`Secret`] and reaches the keychain only through
//! `kassensturz-platform` (AGENTS.md §3.1 rule 7). A saved key is filed as
//! `ai-key/<server>`, the provider's scheme, host and port
//! ([`ProviderAddress::origin`]), so a key goes only to the server it was
//! saved for: changing the address to another server never sends the old
//! key there.

use std::collections::HashMap;

use kassensturz_platform::{Keychain, KeychainError, Secret, SecretKey};

use crate::address::ProviderAddress;
use crate::config::KeyUse;
use crate::error::AiError;

/// The kind part of the keychain key.
const KIND: &str = "ai-key";

/// The provider keys of one run of KassenSturz: those remembered in memory
/// and the way to the saved ones.
pub struct ProviderKeys {
    remembered: HashMap<String, Secret>,
    store: Option<Box<dyn Keychain>>,
}

impl std::fmt::Debug for ProviderKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProviderKeys")
    }
}

impl ProviderKeys {
    /// Keys over `store`, the place the Password storage setting names
    /// (`SecretStorage::keychain`, D-163), or `None` when the user chose
    /// to save secrets nowhere.
    pub fn new(store: Option<Box<dyn Keychain>>) -> Self {
        Self {
            remembered: HashMap::new(),
            store,
        }
    }

    fn key(address: &ProviderAddress) -> Result<SecretKey, AiError> {
        SecretKey::new(KIND, &address.origin()).map_err(keychain_error)
    }

    /// The key to send to `address`, if one is kept: `None` when the
    /// provider needs none or the user has to type it now.
    pub fn get(
        &self,
        address: &ProviderAddress,
        key_use: KeyUse,
    ) -> Result<Option<Secret>, AiError> {
        match key_use {
            KeyUse::None | KeyUse::AskEachTime => Ok(None),
            KeyUse::UntilClose => Ok(self.remembered.get(&address.origin()).cloned()),
            KeyUse::Saved => {
                if let Some(key) = self.remembered.get(&address.origin()) {
                    return Ok(Some(key.clone()));
                }
                match &self.store {
                    Some(store) => store.read(&Self::key(address)?).map_err(keychain_error),
                    None => Ok(None),
                }
            }
        }
    }

    /// Keeps the key the user typed for `address` as `key_use` says: not at
    /// all, until KassenSturz closes, or saved. With no place to save it,
    /// a key to be saved is remembered until KassenSturz closes, as a PIN
    /// is (D-163).
    pub fn keep(
        &mut self,
        address: &ProviderAddress,
        key_use: KeyUse,
        key: Secret,
    ) -> Result<(), AiError> {
        match key_use {
            KeyUse::None | KeyUse::AskEachTime => Ok(()),
            KeyUse::UntilClose => {
                self.remembered.insert(address.origin(), key);
                Ok(())
            }
            KeyUse::Saved => match &self.store {
                Some(store) => store
                    .store(&Self::key(address)?, &key)
                    .map_err(keychain_error),
                None => {
                    self.remembered.insert(address.origin(), key);
                    Ok(())
                }
            },
        }
    }

    /// Forgets the key for `address`, in memory and where it was saved.
    pub fn forget(&mut self, address: &ProviderAddress) -> Result<(), AiError> {
        self.remembered.remove(&address.origin());
        match &self.store {
            Some(store) => store.delete(&Self::key(address)?).map_err(keychain_error),
            None => Ok(()),
        }
    }
}

/// The keychain's error as `ai.keepassxc` when KeePassXC was the place
/// (D-164), else `ai.keychain`, without the keychain's own text.
fn keychain_error(error: KeychainError) -> AiError {
    match error {
        KeychainError::KeePassXcUnavailable => AiError::KeePassXc,
        _ => AiError::Keychain,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use kassensturz_platform::MemoryKeychain;

    use super::*;

    /// A store the test can look into after handing it over.
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

    fn address(text: &str) -> ProviderAddress {
        ProviderAddress::parse(text).unwrap()
    }

    fn secret(text: &str) -> Secret {
        Secret::new(text.to_owned())
    }

    #[test]
    fn a_saved_key_is_filed_under_its_server() {
        let memory = Arc::new(MemoryKeychain::new());
        let mut keys = ProviderKeys::new(Some(Box::new(Shared(memory.clone()))));
        let cloud = address("https://api.example.com/v1");
        keys.keep(&cloud, KeyUse::Saved, secret("test-key-1"))
            .unwrap();
        assert_eq!(memory.len(), 1);
        let filed = SecretKey::new("ai-key", "https:api.example.com:443").unwrap();
        assert_eq!(memory.read(&filed).unwrap(), Some(secret("test-key-1")));

        // A fresh run reads it from the store.
        let fresh = ProviderKeys::new(Some(Box::new(Shared(memory.clone()))));
        assert_eq!(
            fresh.get(&cloud, KeyUse::Saved).unwrap(),
            Some(secret("test-key-1"))
        );
        // Another path on the same server is the same server.
        assert!(
            fresh
                .get(&address("https://api.example.com/v2"), KeyUse::Saved)
                .unwrap()
                .is_some()
        );
        // Another server never gets it.
        assert_eq!(
            fresh
                .get(&address("https://other.example.com/v1"), KeyUse::Saved)
                .unwrap(),
            None
        );
        // Typed each time: never read from the store.
        assert_eq!(fresh.get(&cloud, KeyUse::AskEachTime).unwrap(), None);

        keys.forget(&cloud).unwrap();
        assert_eq!(memory.len(), 0);
    }

    #[test]
    fn remembered_keys_stay_in_memory() {
        let memory = Arc::new(MemoryKeychain::new());
        let mut keys = ProviderKeys::new(Some(Box::new(Shared(memory.clone()))));
        let local = address("http://localhost:1234/v1");
        keys.keep(&local, KeyUse::UntilClose, secret("test-key-2"))
            .unwrap();
        keys.keep(&local, KeyUse::AskEachTime, secret("test-key-3"))
            .unwrap();
        assert_eq!(memory.len(), 0);
        assert_eq!(
            keys.get(&local, KeyUse::UntilClose).unwrap(),
            Some(secret("test-key-2"))
        );
        keys.forget(&local).unwrap();
        assert_eq!(keys.get(&local, KeyUse::UntilClose).unwrap(), None);
    }

    #[test]
    fn with_nowhere_to_save_a_key_is_remembered() {
        let mut keys = ProviderKeys::new(None);
        let cloud = address("https://api.example.com/v1");
        keys.keep(&cloud, KeyUse::Saved, secret("test-key-4"))
            .unwrap();
        assert_eq!(
            keys.get(&cloud, KeyUse::Saved).unwrap(),
            Some(secret("test-key-4"))
        );
    }

    #[test]
    fn keychain_errors_are_codes() {
        let memory = Arc::new(MemoryKeychain::new());
        memory.fail_with(Some(KeychainError::Unavailable));
        let mut keys = ProviderKeys::new(Some(Box::new(Shared(memory.clone()))));
        let cloud = address("https://api.example.com/v1");
        assert_eq!(
            keys.keep(&cloud, KeyUse::Saved, secret("k")),
            Err(AiError::Keychain)
        );
        memory.fail_with(Some(KeychainError::KeePassXcUnavailable));
        assert_eq!(keys.get(&cloud, KeyUse::Saved), Err(AiError::KeePassXc));
        assert_eq!(format!("{keys:?}"), "ProviderKeys");
    }
}
