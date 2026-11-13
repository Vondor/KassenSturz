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
//!
//! The same keys hold the seal of each consent the user gave in
//! KassenSturz's own consent screen, filed as `ai-consent/<server>`
//! (B-252): only a consent in the settings file that matches its seal
//! counts, so another program can't agree for the user by writing to the
//! file. With nowhere to save secrets, the seal is kept in memory until
//! KassenSturz closes, and the user is asked again at the next start.

use std::collections::HashMap;

use kassensturz_platform::{Keychain, KeychainError, Secret, SecretKey};

use crate::address::ProviderAddress;
use crate::config::KeyUse;
use crate::consent::ConsentRecord;
use crate::error::AiError;

/// The kind part of the keychain key.
const KIND: &str = "ai-key";

/// The kind part of the keychain key of a consent's seal (B-252).
const CONSENT_KIND: &str = "ai-consent";

/// The provider keys of one run of KassenSturz: those remembered in memory
/// and the way to the saved ones.
pub struct ProviderKeys {
    remembered: HashMap<String, Secret>,
    /// Consents sealed in memory, by origin, while there is nowhere to save
    /// secrets (B-252).
    sealed: HashMap<String, ConsentRecord>,
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
            sealed: HashMap::new(),
            store,
        }
    }

    /// Changes the place saved keys go to, e.g. after the Password storage
    /// setting changed (D-163). The keys remembered in memory stay; keys
    /// saved in the old place stay there, as for a PIN they are neither
    /// copied nor read from it any more.
    pub fn use_store(&mut self, store: Option<Box<dyn Keychain>>) {
        self.store = store;
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

    /// Deletes the key saved for `address` from the place it is saved in
    /// now, because the user chose another place (D-163, D-575): never
    /// copied over, but remembered until KassenSturz closes, as a PIN is.
    /// When it can't be read or deleted, e.g. a locked keychain, the error
    /// says why and the key stays where it is, so the switch can be
    /// refused. [`ProviderKeys::use_store`] then moves to the new place.
    pub fn remove_saved(&mut self, address: &ProviderAddress) -> Result<(), AiError> {
        let Some(store) = &self.store else {
            return Ok(());
        };
        let key = Self::key(address)?;
        let saved = store.read(&key).map_err(keychain_error)?;
        store.delete(&key).map_err(keychain_error)?;
        if let Some(saved) = saved {
            self.remembered.entry(address.origin()).or_insert(saved);
        }
        Ok(())
    }

    fn consent_key(address: &ProviderAddress) -> Result<SecretKey, AiError> {
        SecretKey::new(CONSENT_KIND, &address.origin()).map_err(keychain_error)
    }

    /// Seals the consent the user just gave in KassenSturz's consent screen
    /// (B-252): saved where the Password storage setting says, filed as
    /// `ai-consent/<server>`, or with nowhere to save secrets remembered
    /// until KassenSturz closes, so the user is asked again at the next
    /// start. Only a consent in the settings file that matches its seal
    /// counts ([`crate::confirmed`]).
    pub fn seal_consent(&mut self, consent: &ConsentRecord) -> Result<(), AiError> {
        let origin = consent.address.origin();
        match &self.store {
            Some(store) => store
                .store(
                    &Self::consent_key(&consent.address)?,
                    &Secret::new(consent.seal_text()),
                )
                .map_err(keychain_error),
            None => {
                self.sealed.insert(origin, consent.clone());
                Ok(())
            }
        }
    }

    /// The consent sealed for `address`, if any. A seal that can't be read,
    /// e.g. a locked keychain, counts as none: the user is asked again.
    pub fn sealed_consent(&self, address: &ProviderAddress) -> Option<ConsentRecord> {
        if let Some(sealed) = self.sealed.get(&address.origin()) {
            return Some(sealed.clone());
        }
        let store = self.store.as_ref()?;
        let secret = store.read(&Self::consent_key(address).ok()?).ok()??;
        ConsentRecord::from_seal_text(secret.expose())
    }

    /// Breaks the seal of the consent for `address`, in memory and where it
    /// was saved: the user took the consent back.
    pub fn unseal_consent(&mut self, address: &ProviderAddress) -> Result<(), AiError> {
        self.sealed.remove(&address.origin());
        match &self.store {
            Some(store) => store
                .delete(&Self::consent_key(address)?)
                .map_err(keychain_error),
            None => Ok(()),
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
        KeychainError::KeePassXcUnavailable | KeychainError::KeePassXcAnswer => AiError::KeePassXc,
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
    fn leaving_a_place_deletes_the_saved_key_there_and_remembers_it() {
        // M6-B134, D-575: switching the Password storage takes the AI key
        // out of the old place, like the PINs, and keeps it until close.
        let old = Arc::new(MemoryKeychain::new());
        let new = Arc::new(MemoryKeychain::new());
        let mut keys = ProviderKeys::new(Some(Box::new(Shared(old.clone()))));
        let cloud = address("https://api.example.com/v1");
        keys.keep(&cloud, KeyUse::Saved, secret("test-key-b134"))
            .unwrap();

        old.fail_with(Some(KeychainError::Unavailable));
        assert_eq!(keys.remove_saved(&cloud), Err(AiError::Keychain));
        old.fail_with(None);
        assert_eq!(old.len(), 1, "a locked place keeps the key, and says so");

        keys.remove_saved(&cloud).unwrap();
        assert!(old.is_empty());
        keys.use_store(Some(Box::new(Shared(new.clone()))));
        assert!(new.is_empty(), "never copied");
        assert_eq!(
            keys.get(&cloud, KeyUse::Saved).unwrap(),
            Some(secret("test-key-b134")),
            "remembered until KassenSturz closes"
        );
        // Nothing saved, or no place to save in: nothing to do.
        keys.remove_saved(&address("https://other.example.com/v1"))
            .unwrap();
        let mut nowhere = ProviderKeys::new(None);
        nowhere.remove_saved(&cloud).unwrap();
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
    fn a_new_place_keeps_what_is_remembered_and_reads_only_there() {
        let first = Arc::new(MemoryKeychain::new());
        let second = Arc::new(MemoryKeychain::new());
        let mut keys = ProviderKeys::new(Some(Box::new(Shared(first.clone()))));
        let cloud = address("https://api.example.com/v1");
        let local = address("http://localhost:1234/v1");
        keys.keep(&cloud, KeyUse::Saved, secret("test-key-5"))
            .unwrap();
        keys.keep(&local, KeyUse::UntilClose, secret("test-key-6"))
            .unwrap();

        keys.use_store(Some(Box::new(Shared(second.clone()))));
        assert_eq!(first.len(), 1, "the old place is left as it was");
        assert_eq!(
            keys.get(&cloud, KeyUse::Saved).unwrap(),
            None,
            "a key saved in the old place isn't read from it"
        );
        assert_eq!(
            keys.get(&local, KeyUse::UntilClose).unwrap(),
            Some(secret("test-key-6"))
        );
        keys.keep(&cloud, KeyUse::Saved, secret("test-key-7"))
            .unwrap();
        assert_eq!(second.len(), 1);
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
