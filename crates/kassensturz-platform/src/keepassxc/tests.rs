// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The KeePassXC keychain against the made-up KeePassXC in `fake.rs`.

use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::time::Duration;

use super::fake::FakeKeePassXc;
use super::*;
use crate::MemoryKeychain;

/// The system keychain the connection keys go to, shared with the test.
#[derive(Clone)]
struct Links(Arc<MemoryKeychain>);

impl Keychain for Links {
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

const SERVICE: &str = "io.github.vondor.KassenSturz.test";

fn key(id: &str) -> SecretKey {
    SecretKey::new("bank-pin", id).expect("key")
}

fn keychain(fake: &FakeKeePassXc, links: &Links) -> KeePassXcKeychain {
    KeePassXcKeychain::at(SERVICE, Some(fake.socket.clone()), Box::new(links.clone()))
        .waiting(Duration::from_secs(5))
}

fn links() -> Links {
    Links(Arc::new(MemoryKeychain::new()))
}

#[test]
fn a_secret_is_saved_read_back_and_emptied_in_keepassxc() {
    let fake = FakeKeePassXc::start("round-trip");
    let links = links();
    let keychain = keychain(&fake, &links);

    assert_eq!(
        keychain.read(&key("login-1")).map(|s| s.is_some()),
        Ok(false)
    );
    keychain
        .store(&key("login-1"), &Secret::new("0000".to_owned()))
        .expect("stored");
    let read = keychain
        .read(&key("login-1"))
        .expect("read")
        .expect("saved");
    assert_eq!(read.expose(), "0000");
    {
        let state = fake.state();
        assert_eq!(state.entries.len(), 1);
        let entry = &state.entries[0];
        assert_eq!(entry.url, "kassensturz://io.github.vondor.kassensturz.test");
        assert_eq!(entry.login, "bank-pin/login-1");
        // The connection key is kept in the system keychain, per database.
        assert_eq!(links.0.len(), 1);
        assert!(
            links
                .0
                .contains(&SecretKey::new(LINK_KIND, "29234e32274a32276e25666a42").expect("key"))
        );
    }

    // Saving again updates the same entry.
    keychain
        .store(&key("login-1"), &Secret::new("1111".to_owned()))
        .expect("stored");
    assert_eq!(fake.state().entries.len(), 1);
    assert_eq!(
        keychain
            .read(&key("login-1"))
            .expect("read")
            .expect("saved")
            .expose(),
        "1111"
    );
    // Another key is another entry; neither sees the other's.
    assert_eq!(
        keychain.read(&key("login-2")).map(|s| s.is_some()),
        Ok(false)
    );

    keychain.delete(&key("login-1")).expect("deleted");
    assert_eq!(
        keychain.read(&key("login-1")).map(|s| s.is_some()),
        Ok(false)
    );
    // KeePassXC can't delete through this interface: the entry is emptied.
    assert_eq!(fake.state().entries[0].password, "");
    // Deleting what isn't there succeeds and asks nothing.
    let before = fake.state().actions.len();
    keychain.delete(&key("login-1")).expect("nothing to delete");
    keychain.delete(&key("login-9")).expect("nothing to delete");
    assert!(
        !fake.state().actions[before..]
            .iter()
            .any(|action| action == "set-login"),
        "{:?}",
        fake.state().actions
    );
}

#[test]
fn no_secret_travels_in_clear() {
    let fake = FakeKeePassXc::start("clear");
    let keychain = keychain(&fake, &links());
    keychain
        .store(&key("login-1"), &Secret::new("5f3c9a2e".to_owned()))
        .expect("stored");
    keychain.read(&key("login-1")).expect("read");
    let raw = fake.state().raw.join("\n");
    assert!(!raw.contains("5f3c9a2e"), "{raw}");
    assert!(!raw.contains("bank-pin"), "{raw}");
    assert!(raw.contains("\"action\":\"set-login\""), "{raw}");
}

#[test]
fn a_kept_connection_is_used_again_without_asking() {
    let fake = FakeKeePassXc::start("kept");
    let links = links();
    keychain(&fake, &links)
        .store(&key("login-1"), &Secret::new("0000".to_owned()))
        .expect("stored");
    assert_eq!(fake.state().links.len(), 1);

    // A new run: the key from the system keychain is tested, not asked again.
    let next = keychain(&fake, &links);
    assert_eq!(
        next.read(&key("login-1"))
            .expect("read")
            .expect("saved")
            .expose(),
        "0000"
    );
    let state = fake.state();
    assert_eq!(state.links.len(), 1);
    assert_eq!(
        state
            .actions
            .iter()
            .filter(|action| *action == "associate")
            .count(),
        1
    );
    assert!(
        state
            .actions
            .iter()
            .any(|action| action == "test-associate")
    );
}

#[test]
fn a_connection_keepassxc_forgot_is_asked_for_again() {
    let fake = FakeKeePassXc::start("forgotten");
    let links = links();
    keychain(&fake, &links).read(&key("login-1")).expect("read");
    fake.state().links.clear();
    keychain(&fake, &links).read(&key("login-1")).expect("read");
    assert_eq!(fake.state().links.len(), 1);
    assert_eq!(fake.state().links[0].0, "KassenSturz 1");
}

#[test]
fn a_locked_database_is_unlocked_in_keepassxc_first() {
    let fake = FakeKeePassXc::start("locked");
    fake.state().locked = true;
    let keychain = keychain(&fake, &links());
    keychain
        .store(&key("login-1"), &Secret::new("0000".to_owned()))
        .expect("stored after unlocking");
    let state = fake.state();
    assert!(!state.locked);
    assert_eq!(state.entries.len(), 1);
    assert_eq!(
        state
            .actions
            .iter()
            .filter(|action| *action == "get-databasehash")
            .count(),
        2
    );
}

#[test]
fn keepassxc_saying_no_or_not_running_is_unavailable_and_nothing_is_kept() {
    let fake = FakeKeePassXc::start("refused");
    fake.state().refuse_association = true;
    let links = links();
    let refused = keychain(&fake, &links);
    assert_eq!(
        refused.store(&key("login-1"), &Secret::new("0000".to_owned())),
        Err(KeychainError::KeePassXcUnavailable)
    );
    assert!(fake.state().entries.is_empty());
    assert!(links.0.is_empty());

    let missing = KeePassXcKeychain::at(
        SERVICE,
        Some(std::env::temp_dir().join("ks-no-keepassxc-here.socket")),
        Box::new(links.clone()),
    );
    assert_eq!(
        missing.read(&key("login-1")).map(|s| s.is_some()),
        Err(KeychainError::KeePassXcUnavailable)
    );
    let nowhere = KeePassXcKeychain::at(SERVICE, None, Box::new(links));
    assert_eq!(
        nowhere.delete(&key("login-1")),
        Err(KeychainError::KeePassXcUnavailable)
    );
}

#[test]
fn without_a_system_keychain_the_connection_lasts_until_the_app_closes() {
    let fake = FakeKeePassXc::start("no-links");
    let links = links();
    links.0.fail_with(Some(KeychainError::Unavailable));
    let keychain = keychain(&fake, &links);
    keychain
        .store(&key("login-1"), &Secret::new("0000".to_owned()))
        .expect("stored");
    keychain.read(&key("login-1")).expect("read");
    let state = fake.state();
    assert_eq!(
        state
            .actions
            .iter()
            .filter(|action| *action == "associate")
            .count(),
        1
    );
}

#[test]
fn a_lost_connection_is_opened_again_once() {
    let fake = FakeKeePassXc::start("lost");
    let keychain = keychain(&fake, &links());
    keychain.read(&key("login-1")).expect("read");
    // KeePassXC restarts: the old connection is gone.
    if let Some(session) = keychain.session.lock().expect("session").as_mut() {
        let (broken, other_end) = UnixStream::pair().expect("pair");
        drop(other_end);
        session.stream = broken;
    }
    keychain
        .store(&key("login-1"), &Secret::new("0000".to_owned()))
        .expect("stored on a new connection");
    let opened = fake
        .state()
        .actions
        .iter()
        .filter(|action| *action == "change-public-keys")
        .count();
    assert_eq!(opened, 2);
}

#[test]
fn the_nonce_counts_up_like_libsodium() {
    let mut nonce = [0_u8; 24];
    assert_eq!(increment(nonce)[0], 1);
    nonce[0] = 0xff;
    let next = increment(nonce);
    assert_eq!((next[0], next[1]), (0, 1));
    assert_eq!(increment([0xff; 24]), [0; 24]);
}

#[test]
fn the_entry_url_is_the_service_in_lower_case() {
    let keychain = KeePassXcKeychain::at(crate::APP_ID, None, Box::new(links()));
    assert_eq!(keychain.url(), "kassensturz://io.github.vondor.kassensturz");
    assert!(format!("{keychain:?}").starts_with("KeePassXcKeychain"));
}
