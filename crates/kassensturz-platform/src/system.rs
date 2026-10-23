// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The system keychain, through the `keyring` crate (Q-085, D-146).

use std::fmt;

use zeroize::Zeroize;

use crate::{Keychain, KeychainError, Secret, SecretKey};

/// KassenSturz's app ID (D-049): the name the keychain files secrets
/// under, next to the key.
pub const APP_ID: &str = "io.github.vondor.KassenSturz";

/// The user's system keychain: the Secret Service on Linux, which KWallet
/// (through `ksecretd`) and GNOME Keyring answer; Windows' Credential
/// Manager and the macOS Keychain once those builds come (M7).
///
/// Each secret is one entry of the keychain, filed under [`APP_ID`] and the
/// [`SecretKey`], so the user finds and removes it in the keychain's own
/// manager too (KWalletManager, Seahorse). The keychain decides whether it
/// asks the user to unlock; KassenSturz never gets its password. In the
/// Flatpak the sandbox needs `--talk-name=org.freedesktop.secrets` (D-142,
/// M4.8); without it every call answers [`KeychainError::Unavailable`].
pub struct SystemKeychain {
    service: String,
}

impl SystemKeychain {
    /// The keychain, with secrets filed under [`APP_ID`].
    pub fn new() -> Self {
        Self::for_service(APP_ID)
    }

    /// Files secrets under `service` instead of [`APP_ID`]. For tests, so
    /// a test never touches the entries the app keeps.
    pub fn for_service(service: &str) -> Self {
        Self {
            service: service.to_owned(),
        }
    }

    /// The name secrets are filed under.
    pub fn service(&self) -> &str {
        &self.service
    }

    fn entry(&self, key: &SecretKey) -> Result<keyring::Entry, KeychainError> {
        keyring::Entry::new(&self.service, &key.to_string()).map_err(failure)
    }
}

impl Default for SystemKeychain {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for SystemKeychain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SystemKeychain")
            .field("service", &self.service)
            .finish()
    }
}

impl Keychain for SystemKeychain {
    fn store(&self, key: &SecretKey, secret: &Secret) -> Result<(), KeychainError> {
        self.entry(key)?
            .set_password(secret.expose())
            .map_err(failure)
    }

    fn read(&self, key: &SecretKey) -> Result<Option<Secret>, KeychainError> {
        match self.entry(key)?.get_password() {
            Ok(value) => Ok(Some(Secret::new(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(failure(error)),
        }
    }

    fn delete(&self, key: &SecretKey) -> Result<(), KeychainError> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(failure(error)),
        }
    }
}

/// Turns the library's error into KassenSturz's code, keeping no value:
/// two of the library's errors carry the bytes the keychain holds, which
/// are wiped here instead of travelling on in an error (threat T-02).
fn failure(error: keyring::Error) -> KeychainError {
    use keyring::Error;
    match error {
        Error::NoDefaultStore | Error::NoStorageAccess(_) => KeychainError::Unavailable,
        Error::BadEncoding(mut bytes) | Error::BadDataFormat(mut bytes, _) => {
            bytes.zeroize();
            KeychainError::NotText
        }
        Error::NoEntry => KeychainError::Failed { kind: "NoEntry" },
        Error::PlatformFailure(_) => KeychainError::Failed {
            kind: "PlatformFailure",
        },
        Error::BadStoreFormat(_) => KeychainError::Failed {
            kind: "BadStoreFormat",
        },
        Error::TooLong(..) => KeychainError::Failed { kind: "TooLong" },
        Error::Invalid(..) => KeychainError::Failed { kind: "Invalid" },
        Error::Ambiguous(_) => KeychainError::Failed { kind: "Ambiguous" },
        Error::NotSupportedByStore(_) => KeychainError::Failed {
            kind: "NotSupportedByStore",
        },
        // The library may add kinds; none of them gets a value through.
        _ => KeychainError::Failed { kind: "Other" },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn platform_error() -> Box<dyn std::error::Error + Send + Sync> {
        Box::new(std::io::Error::other("made up"))
    }

    #[test]
    fn every_library_error_becomes_a_code_without_a_value() {
        use keyring::Error;
        let cases = [
            (Error::NoDefaultStore, KeychainError::Unavailable),
            (
                Error::NoStorageAccess(platform_error()),
                KeychainError::Unavailable,
            ),
            (
                Error::BadEncoding(b"\xff1234".to_vec()),
                KeychainError::NotText,
            ),
            (
                Error::BadDataFormat(b"1234".to_vec(), platform_error()),
                KeychainError::NotText,
            ),
            (Error::NoEntry, KeychainError::Failed { kind: "NoEntry" }),
            (
                Error::PlatformFailure(platform_error()),
                KeychainError::Failed {
                    kind: "PlatformFailure",
                },
            ),
            (
                Error::BadStoreFormat("x".to_owned()),
                KeychainError::Failed {
                    kind: "BadStoreFormat",
                },
            ),
            (
                Error::TooLong("x".to_owned(), 1),
                KeychainError::Failed { kind: "TooLong" },
            ),
            (
                Error::Invalid("x".to_owned(), "y".to_owned()),
                KeychainError::Failed { kind: "Invalid" },
            ),
            (
                Error::Ambiguous(Vec::new()),
                KeychainError::Failed { kind: "Ambiguous" },
            ),
            (
                Error::NotSupportedByStore("x".to_owned()),
                KeychainError::Failed {
                    kind: "NotSupportedByStore",
                },
            ),
        ];
        for (library, expected) in cases {
            let shown = format!("{library:?}");
            let mapped = failure(library);
            assert_eq!(mapped, expected, "{shown}");
            assert!(!format!("{mapped:?}").contains("1234"), "{shown}");
        }
    }

    #[test]
    fn secrets_are_filed_under_the_app_id_of_the_flatpak() {
        // The same ID names the app's folders and its Flatpak (D-049), so a
        // keychain manager shows which app an entry belongs to.
        let manifest = include_str!("../../../packaging/flatpak/io.github.vondor.KassenSturz.yml");
        assert!(manifest.contains(&format!("\nid: {APP_ID}\n")), "{APP_ID}");
        assert_eq!(SystemKeychain::new().service(), APP_ID);
        assert_eq!(SystemKeychain::default().service(), APP_ID);
        assert_eq!(
            format!("{:?}", SystemKeychain::new()),
            format!("SystemKeychain {{ service: {APP_ID:?} }}")
        );
    }

    #[test]
    fn a_key_never_stored_is_none_or_the_keychain_is_unavailable() {
        // Runs everywhere: on a desktop with a keychain the entry doesn't
        // exist, in CI nothing answers. Either way no value is read.
        let keychain = SystemKeychain::for_service(TEST_SERVICE);
        let key = SecretKey::new("test", "never-stored").unwrap();
        match keychain.read(&key) {
            Ok(None) => println!("a keychain answered: no entry"),
            Err(KeychainError::Unavailable) => println!("no keychain answered"),
            Err(KeychainError::Failed { kind }) => {
                println!("the keychain refused the request: {kind}");
            }
            other => panic!("{other:?}"),
        }
    }

    /// Filed apart from the app's entries, so a test never touches them.
    const TEST_SERVICE: &str = "io.github.vondor.KassenSturz.test";

    /// The round trip on a real keychain, for a desktop with one: run with
    /// `cargo test -p kassensturz-platform -- --ignored`, natively or
    /// through `tools/sdk.sh`, whose sandbox may talk to the Secret
    /// Service. The secret is made up and removed again; it isn't run in
    /// CI, where no keychain answers. Checked on the maintainer's Plasma
    /// 6.6.6, where `ksecretd` answers as the Secret Service (D-146).
    #[test]
    #[ignore = "needs a system keychain: run with --ignored on a desktop"]
    fn a_secret_survives_the_round_trip_through_the_system_keychain() {
        let keychain = SystemKeychain::for_service(TEST_SERVICE);
        let key = SecretKey::new("test", &format!("round-trip-{}", std::process::id())).unwrap();
        let secret = Secret::new("0000".to_owned());

        match keychain.read(&key) {
            Ok(None) => {}
            Err(KeychainError::Unavailable) => panic!(
                "no keychain answered: this test needs a desktop whose Secret Service \
                 (KWallet, GNOME Keyring) is running and reachable; a sandbox without \
                 --talk-name=org.freedesktop.secrets hides it"
            ),
            other => panic!("{other:?}"),
        }
        keychain.store(&key, &secret).unwrap();
        assert_eq!(keychain.read(&key), Ok(Some(secret)));
        keychain
            .store(&key, &Secret::new("1111".to_owned()))
            .unwrap();
        assert_eq!(
            keychain.read(&key),
            Ok(Some(Secret::new("1111".to_owned())))
        );
        keychain.delete(&key).unwrap();
        assert_eq!(keychain.read(&key), Ok(None));
        assert_eq!(keychain.delete(&key), Ok(()));
    }
}
