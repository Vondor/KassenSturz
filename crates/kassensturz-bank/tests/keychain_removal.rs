// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! #505 (M6-B188): a removed bank login leaves no entry that still holds a
//! PIN, also in a keychain that cannot delete entries. KeePassXC's browser
//! interface is such a place: `delete` empties the entry's password and the
//! entry stays, so `bank-pin/<id>` and `bank-pin/pending` remain listed.
//! An empty password counts as no secret. The test reads only whether a
//! password is empty, never its value. Synthetic data only.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use kassensturz_bank::{
    Bank, BankCode, FakeBank, KeychainPinStore, LoginId, LoginSetup, Pin, PinMode, PinSession,
    remove_login,
};
use kassensturz_bank::{CustomerId, FintsVersion, LoginKind};
use kassensturz_platform::{Keychain, KeychainError, Secret, SecretKey};

/// A keychain that behaves as KeePassXC does through its browser interface.
#[derive(Clone, Default)]
struct EmptyingKeychain(Arc<Mutex<BTreeMap<String, String>>>);

impl EmptyingKeychain {
    fn keys(&self) -> Vec<String> {
        self.0.lock().expect("entries").keys().cloned().collect()
    }

    /// The entries whose password is not empty.
    fn holding_a_secret(&self) -> Vec<String> {
        self.0
            .lock()
            .expect("entries")
            .iter()
            .filter(|(_, password)| !password.is_empty())
            .map(|(key, _)| key.clone())
            .collect()
    }
}

impl Keychain for EmptyingKeychain {
    fn store(&self, key: &SecretKey, secret: &Secret) -> Result<(), KeychainError> {
        self.0
            .lock()
            .expect("entries")
            .insert(key.to_string(), secret.expose().to_owned());
        Ok(())
    }

    fn read(&self, key: &SecretKey) -> Result<Option<Secret>, KeychainError> {
        // An emptied entry counts as no secret.
        Ok(self
            .0
            .lock()
            .expect("entries")
            .get(&key.to_string())
            .filter(|password| !password.is_empty())
            .map(|password| Secret::new(password.clone())))
    }

    fn delete(&self, key: &SecretKey) -> Result<(), KeychainError> {
        if let Some(password) = self.0.lock().expect("entries").get_mut(&key.to_string()) {
            password.clear();
        }
        Ok(())
    }
}

fn setup() -> LoginSetup {
    LoginSetup {
        bank: BankCode::new("DE", "37040044"),
        url: "https://fints.testbank.example/".to_owned(),
        kind: LoginKind::PinTan {
            customer_id: CustomerId::SameAsLoginName,
        },
        login_name: "made-up-login".to_owned(),
        customer_id: None,
        fints_version: FintsVersion::default(),
        tan_method: None,
    }
}

/// Sets up a login as the setup page does: the PIN typed is saved in the
/// keychain under `pending`, then the bank gives the login its ID.
fn set_up(bank: &mut FakeBank, pins: &mut PinSession, pin: &str) -> Result<LoginId, ()> {
    pins.provide(
        &LoginId::pending(),
        Pin::new(pin.to_owned()),
        PinMode::SaveInKeychain,
    )
    .expect("provided");
    bank.add_login(&setup(), pins).map_err(|_| ())
}

#[test]
fn a_removed_login_leaves_no_entry_that_holds_a_pin() {
    let keychain = EmptyingKeychain::default();
    let mut pins = PinSession::new(Box::new(KeychainPinStore::new(Box::new(keychain.clone()))));
    let mut bank = FakeBank::new();

    // A setup that fails (wrong PIN) keeps nothing of the PIN.
    assert!(set_up(&mut bank, &mut pins, "00000").is_err());
    assert_eq!(
        keychain.holding_a_secret(),
        Vec::<String>::new(),
        "{:?}",
        keychain.keys()
    );

    let first = set_up(&mut bank, &mut pins, FakeBank::PIN).expect("set up");
    let second = set_up(&mut bank, &mut pins, FakeBank::PIN).expect("set up");
    let held = keychain.holding_a_secret();
    assert!(
        !held.iter().any(|key| key.ends_with("/pending")),
        "{held:?}"
    );
    assert_eq!(held.len(), 2, "{held:?}");

    remove_login(&mut bank, &first, &mut pins).expect("removed");
    assert_eq!(
        keychain.holding_a_secret(),
        [format!("bank-pin/{second}")],
        "only the login that is kept"
    );
    remove_login(&mut bank, &second, &mut pins).expect("removed");
    assert!(
        keychain.holding_a_secret().is_empty(),
        "no entry holds a secret, entries listed: {:?}",
        keychain.keys()
    );
}
