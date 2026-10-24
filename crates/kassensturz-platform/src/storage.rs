// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where secrets the user chose to save are kept (D-163): the user's
//! choice in Settings, and which choices this build offers on this system.

use crate::{Keychain, SystemKeychain};

/// Where KassenSturz keeps the secrets the user chose to save, such as a
/// banking PIN (D-057). The user picks it in Settings; [`Self::System`]
/// is the default, the others are advanced choices (D-163).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SecretStorage {
    /// The system keychain ([`SystemKeychain`]): on Linux whichever
    /// program provides the desktop's Secret Service (KWallet, GNOME
    /// Keyring or KeePassXC, D-150), Windows' Credential Manager, the
    /// macOS Keychain.
    #[default]
    System,
    /// Nowhere: nothing is saved, and KassenSturz doesn't offer to. A
    /// secret is typed each time or remembered until the app closes.
    Nowhere,
}

impl SecretStorage {
    /// Every choice this build offers on this system, the default first.
    /// What a later build adds for one system only is left out on the
    /// others.
    pub const fn available() -> &'static [Self] {
        &[Self::System, Self::Nowhere]
    }

    /// The word the choice is kept under in the app's settings.
    pub const fn key(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Nowhere => "nowhere",
        }
    }

    /// The choice kept under `key`. A word this build doesn't know, or
    /// one for a choice it doesn't offer here, gives the default, so a
    /// settings file from another build never leaves the user without a
    /// place.
    pub fn from_key(key: &str) -> Self {
        Self::available()
            .iter()
            .copied()
            .find(|storage| storage.key() == key)
            .unwrap_or_default()
    }

    /// Whether the choice is one of the advanced ones, shown one step
    /// away from the default.
    pub const fn is_advanced(self) -> bool {
        !matches!(self, Self::System)
    }

    /// The keychain that keeps secrets filed under `service` (usually
    /// [`crate::APP_ID`]), or `None` when nothing is to be saved.
    pub fn keychain(self, service: &str) -> Option<Box<dyn Keychain>> {
        match self {
            Self::System => Some(Box::new(SystemKeychain::for_service(service))),
            Self::Nowhere => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::APP_ID;

    #[test]
    fn the_system_keychain_is_the_default_and_comes_first() {
        assert_eq!(SecretStorage::default(), SecretStorage::System);
        assert_eq!(SecretStorage::available()[0], SecretStorage::System);
        assert!(!SecretStorage::System.is_advanced());
        assert!(SecretStorage::Nowhere.is_advanced());
    }

    #[test]
    fn every_choice_comes_back_from_its_key() {
        for &storage in SecretStorage::available() {
            assert_eq!(SecretStorage::from_key(storage.key()), storage);
        }
        assert_eq!(SecretStorage::System.key(), "system");
        assert_eq!(SecretStorage::Nowhere.key(), "nowhere");
    }

    #[test]
    fn an_unknown_or_empty_key_gives_the_system_keychain() {
        assert_eq!(SecretStorage::from_key(""), SecretStorage::System);
        assert_eq!(SecretStorage::from_key("vault"), SecretStorage::System);
        assert_eq!(SecretStorage::from_key("Nowhere"), SecretStorage::System);
    }

    #[test]
    fn nowhere_has_no_keychain() {
        assert!(SecretStorage::Nowhere.keychain(APP_ID).is_none());
        assert!(SecretStorage::System.keychain(APP_ID).is_some());
    }
}
