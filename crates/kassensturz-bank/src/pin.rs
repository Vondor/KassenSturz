// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The banking PIN: a value that never shows itself and is wiped when
//! dropped (threat T-02), and the user's three choices of how long to keep
//! it (D-057): ask each time, remember until KassenSturz closes, save in the
//! system keychain. When the bank calls a PIN wrong, it is forgotten at once,
//! so a login is never blocked by repeating it (D-140, threat T-05).

use std::collections::{HashMap, HashSet};
use std::fmt;

use kassensturz_platform::KeychainPlace;
use zeroize::Zeroizing;

use crate::backend::LoginId;
use crate::error::BankError;

/// A PIN. It has no `Display`, its `Debug` shows no value, and the memory
/// is wiped when it is dropped. Only the bank backend reads it, to answer
/// the bank's password request; it never reaches QML (threat T-02).
#[derive(Clone, PartialEq, Eq)]
pub struct Pin(Zeroizing<String>);

impl Pin {
    /// Takes the PIN the user typed. The `String` is moved in, so no copy
    /// outlives this value unwiped; a caller that still holds another copy
    /// has to wipe or drop it itself.
    pub fn new(pin: String) -> Self {
        Self(Zeroizing::new(pin))
    }

    /// The PIN, for the bank backend only.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Pin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Pin(…)")
    }
}

/// How long the user wants a PIN kept (D-057). The default asks each time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PinMode {
    /// Kept only for this one bank session, then forgotten.
    #[default]
    AskEachTime,
    /// Kept in memory until KassenSturz closes, never written anywhere.
    RememberUntilClose,
    /// Saved in the system keychain through `kassensturz-platform` (M4.1),
    /// until the user removes it or the login.
    SaveInKeychain,
}

/// Where saved PINs live: the system keychain, behind `kassensturz-platform`
/// (M4.1), or [`MemoryPinStore`] in tests. Only this crate calls it, with
/// the login's ID as the key. A store never logs or shows a PIN.
pub trait PinStore {
    /// Saves `pin` for `login`, replacing a saved one.
    fn save(&mut self, login: &LoginId, pin: &Pin) -> Result<(), BankError>;
    /// The saved PIN for `login`, if any.
    fn load(&self, login: &LoginId) -> Result<Option<Pin>, BankError>;
    /// Removes the saved PIN for `login`; nothing happens if there is none.
    fn delete(&mut self, login: &LoginId) -> Result<(), BankError>;
    /// Whether the store saves PINs at all: [`NoPinStore`] doesn't.
    fn saves(&self) -> bool {
        true
    }

    /// Gets the place ready to save PINs, asking the user for what it
    /// needs, and says where in it they go, when the user can tell that
    /// place apart from others, e.g. a KeePassXC database (D-167). A store
    /// that needs nothing first answers `None`.
    fn open(&self) -> Result<Option<KeychainPlace>, BankError> {
        Ok(None)
    }
}

/// The store when the user chose to save PINs nowhere (D-163): it holds
/// none, and the session keeps a PIN the user wanted saved only until the
/// app closes instead.
#[derive(Debug, Default)]
pub struct NoPinStore;

impl PinStore for NoPinStore {
    /// Never reached through [`PinSession`], which asks [`Self::saves`]
    /// first; fails like a keychain that can't be reached, so nothing is
    /// ever lost silently.
    fn save(&mut self, _login: &LoginId, _pin: &Pin) -> Result<(), BankError> {
        Err(BankError::Keychain)
    }

    fn load(&self, _login: &LoginId) -> Result<Option<Pin>, BankError> {
        Ok(None)
    }

    fn delete(&mut self, _login: &LoginId) -> Result<(), BankError> {
        Ok(())
    }

    fn saves(&self) -> bool {
        false
    }
}

/// A store in memory, for tests and for a build without a keychain.
#[derive(Debug, Default)]
pub struct MemoryPinStore {
    saved: HashMap<LoginId, Pin>,
    /// When set, every call fails with `bank.keychain`, as a keychain that
    /// can't be reached would.
    pub unavailable: bool,
}

impl MemoryPinStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many PINs are saved.
    pub fn len(&self) -> usize {
        self.saved.len()
    }

    /// Whether no PIN is saved.
    pub fn is_empty(&self) -> bool {
        self.saved.is_empty()
    }

    fn check(&self) -> Result<(), BankError> {
        if self.unavailable {
            Err(BankError::Keychain)
        } else {
            Ok(())
        }
    }
}

impl PinStore for MemoryPinStore {
    fn save(&mut self, login: &LoginId, pin: &Pin) -> Result<(), BankError> {
        self.check()?;
        self.saved.insert(login.clone(), pin.clone());
        Ok(())
    }

    fn load(&self, login: &LoginId) -> Result<Option<Pin>, BankError> {
        self.check()?;
        Ok(self.saved.get(login).cloned())
    }

    fn delete(&mut self, login: &LoginId) -> Result<(), BankError> {
        self.check()?;
        self.saved.remove(login);
        Ok(())
    }
}

/// What the session answers when the bank asks for a PIN.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PinAnswer {
    /// A remembered or saved PIN: the backend sends it.
    Known(Pin),
    /// None is kept, or the bank called the last one wrong: the backend
    /// stops with `bank.pin-needed` and the app asks the user.
    Ask,
}

/// The PINs of one run of KassenSturz: those remembered in memory, with how
/// long, and the way to the saved ones. One session lives as long as the
/// app runs; the backend asks it whenever a bank wants a PIN (D-140's
/// `getPassword`), and tells it when the bank called one wrong.
pub struct PinSession {
    remembered: HashMap<LoginId, (Pin, PinMode)>,
    /// Logins whose last PIN the bank called wrong: no PIN goes out for
    /// them until the user types one again (threat T-05).
    rejected: HashSet<LoginId>,
    store: Box<dyn PinStore>,
}

impl fmt::Debug for PinSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PinSession")
            .field("remembered", &self.remembered.len())
            .field("rejected", &self.rejected.len())
            .finish_non_exhaustive()
    }
}

impl PinSession {
    /// A session with no PIN remembered, over `store` for the saved ones.
    pub fn new(store: Box<dyn PinStore>) -> Self {
        Self {
            remembered: HashMap::new(),
            rejected: HashSet::new(),
            store,
        }
    }

    /// The PIN to send for `login`, if one may be sent: a remembered one
    /// first, then a saved one. After the bank called a PIN wrong, `Ask`
    /// until the user types one.
    pub fn answer(&self, login: &LoginId) -> Result<PinAnswer, BankError> {
        if self.rejected.contains(login) {
            return Ok(PinAnswer::Ask);
        }
        if let Some((pin, _)) = self.remembered.get(login) {
            return Ok(PinAnswer::Known(pin.clone()));
        }
        Ok(self
            .store
            .load(login)?
            .map_or(PinAnswer::Ask, PinAnswer::Known))
    }

    /// Takes the PIN the user typed for `login`, to be kept as `mode`
    /// says. A PIN to save goes to the store at once and is remembered
    /// until the app closes too, so the keychain isn't asked on every
    /// request. When the user chose to save PINs nowhere (D-163), a PIN to
    /// save is only remembered until the app closes.
    pub fn provide(&mut self, login: &LoginId, pin: Pin, mode: PinMode) -> Result<(), BankError> {
        self.rejected.remove(login);
        let mode = self.keepable(mode);
        if mode == PinMode::SaveInKeychain {
            self.store.save(login, &pin)?;
        }
        self.remembered.insert(login.clone(), (pin, mode));
        Ok(())
    }

    /// The bank called the PIN for `login` wrong: the remembered one is
    /// dropped, the saved one deleted, and nothing is sent for this login
    /// until the user types a PIN again (D-140, threat T-05). The keychain's
    /// error, if any, is returned after the PIN is forgotten in memory.
    pub fn rejected(&mut self, login: &LoginId) -> Result<(), BankError> {
        self.remembered.remove(login);
        self.rejected.insert(login.clone());
        self.store.delete(login)
    }

    /// A bank session ended: PINs kept only for it are dropped.
    pub fn end_session(&mut self) {
        self.remembered
            .retain(|_, (_, mode)| *mode != PinMode::AskEachTime);
    }

    /// Moves what is kept for `from` to `to`: the PIN a login got while
    /// it was [`LoginId::pending`] becomes the new login's. A PIN saved in
    /// the store is saved again under `to` and deleted under `from`.
    pub fn rename(&mut self, from: &LoginId, to: &LoginId) -> Result<(), BankError> {
        if let Some((pin, mode)) = self.remembered.remove(from) {
            if mode == PinMode::SaveInKeychain {
                self.store.save(to, &pin)?;
            }
            self.remembered.insert(to.clone(), (pin, mode));
        } else if let Some(pin) = self.store.load(from)? {
            self.store.save(to, &pin)?;
        }
        if self.rejected.remove(from) {
            self.rejected.insert(to.clone());
        }
        self.store.delete(from)
    }

    /// Forgets everything about `login`'s PIN: remembered and saved. For
    /// "Remove saved PIN" in Settings and for removing the login (D-057).
    pub fn forget(&mut self, login: &LoginId) -> Result<(), BankError> {
        self.remembered.remove(login);
        self.rejected.remove(login);
        self.store.delete(login)
    }

    /// Whether a PIN for `login` is saved in the store, for Settings to
    /// show "Remove saved PIN".
    pub fn is_saved(&self, login: &LoginId) -> Result<bool, BankError> {
        Ok(self.store.load(login)?.is_some())
    }

    /// Whether the store saves PINs: the user didn't choose to save them
    /// nowhere (D-163).
    pub fn saves(&self) -> bool {
        self.store.saves()
    }

    /// Gets the place PINs are saved in ready, e.g. connects to KeePassXC
    /// again, and says where they go: [`PinStore::open`] (D-167).
    pub fn open_store(&self) -> Result<Option<KeychainPlace>, BankError> {
        self.store.open()
    }

    /// Saves PINs in `store` from now on, because the user chose another
    /// place for them (D-163). The PINs of `logins` saved in the old place
    /// are deleted there and never copied over: the user types each again
    /// at the next connection and chooses to save it in the new place. A
    /// PIN remembered because it was saved stays remembered until the app
    /// closes. The store is replaced even when the old place can't be
    /// reached; the first error then says so, and the PINs it holds stay
    /// there until the user removes them in that place's own manager.
    pub fn replace_store(
        &mut self,
        store: Box<dyn PinStore>,
        logins: &[LoginId],
    ) -> Result<(), BankError> {
        let mut result = Ok(());
        for login in logins {
            if let Err(error) = self.store.delete(login) {
                result = result.and(Err(error));
            }
        }
        self.store = store;
        for (_, mode) in self.remembered.values_mut() {
            if *mode == PinMode::SaveInKeychain {
                *mode = PinMode::RememberUntilClose;
            }
        }
        result
    }

    /// `mode`, or remembering until the app closes when it asks to save
    /// and the store saves nothing.
    fn keepable(&self, mode: PinMode) -> PinMode {
        if mode == PinMode::SaveInKeychain && !self.store.saves() {
            PinMode::RememberUntilClose
        } else {
            mode
        }
    }

    /// Whether a PIN for `login` is remembered in memory, however long.
    pub fn is_remembered(&self, login: &LoginId) -> bool {
        self.remembered.contains_key(login)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn login() -> LoginId {
        LoginId::new("login-1")
    }

    fn pin(text: &str) -> Pin {
        Pin::new(text.to_owned())
    }

    fn session() -> PinSession {
        PinSession::new(Box::new(MemoryPinStore::new()))
    }

    #[test]
    fn a_pin_never_shows_its_value() {
        let pin = pin("0000");
        assert_eq!(format!("{pin:?}"), "Pin(…)");
        let mut session = session();
        session
            .provide(&login(), pin, PinMode::RememberUntilClose)
            .expect("provided");
        let debug = format!("{session:?}");
        assert!(!debug.contains("0000"), "{debug}");
        assert!(debug.starts_with("PinSession {"), "{debug}");
    }

    #[test]
    fn without_a_pin_the_user_is_asked() {
        let session = session();
        assert_eq!(session.answer(&login()), Ok(PinAnswer::Ask));
        assert_eq!(session.is_saved(&login()), Ok(false));
        assert!(!session.is_remembered(&login()));
    }

    #[test]
    fn a_pin_for_one_session_is_dropped_when_it_ends() {
        let mut session = session();
        session
            .provide(&login(), pin("0000"), PinMode::AskEachTime)
            .expect("provided");
        assert_eq!(session.answer(&login()), Ok(PinAnswer::Known(pin("0000"))));
        session.end_session();
        assert_eq!(session.answer(&login()), Ok(PinAnswer::Ask));
        assert_eq!(session.is_saved(&login()), Ok(false));
    }

    #[test]
    fn a_remembered_pin_outlives_the_session_but_is_not_saved() {
        let mut session = session();
        session
            .provide(&login(), pin("0000"), PinMode::RememberUntilClose)
            .expect("provided");
        session.end_session();
        assert_eq!(session.answer(&login()), Ok(PinAnswer::Known(pin("0000"))));
        assert_eq!(session.is_saved(&login()), Ok(false));
        assert!(session.is_remembered(&login()));
    }

    #[test]
    fn a_saved_pin_is_in_the_store_and_answers_in_a_new_run() {
        let mut session = session();
        session
            .provide(&login(), pin("0000"), PinMode::SaveInKeychain)
            .expect("provided");
        assert_eq!(session.is_saved(&login()), Ok(true));
        // A new run of the app: nothing remembered, the same store.
        let store = std::mem::replace(&mut session.store, Box::new(MemoryPinStore::new()));
        let next_run = PinSession::new(store);
        assert_eq!(next_run.answer(&login()), Ok(PinAnswer::Known(pin("0000"))));
        assert!(!next_run.is_remembered(&login()));
    }

    #[test]
    fn a_rejected_pin_is_forgotten_everywhere_and_not_sent_again() {
        let mut session = session();
        session
            .provide(&login(), pin("0000"), PinMode::SaveInKeychain)
            .expect("provided");
        session.rejected(&login()).expect("forgotten");
        assert_eq!(session.answer(&login()), Ok(PinAnswer::Ask));
        assert_eq!(session.is_saved(&login()), Ok(false));
        assert!(!session.is_remembered(&login()));
        // Typing a new PIN lifts the block.
        session
            .provide(&login(), pin("1111"), PinMode::AskEachTime)
            .expect("provided");
        assert_eq!(session.answer(&login()), Ok(PinAnswer::Known(pin("1111"))));
    }

    #[test]
    fn the_rejection_forgets_in_memory_even_when_the_keychain_fails() {
        let mut store = MemoryPinStore::new();
        store
            .save(&login(), &pin("0000"))
            .expect("saved while reachable");
        store.unavailable = true;
        let mut session = PinSession::new(Box::new(store));
        session
            .provide(&login(), pin("0000"), PinMode::RememberUntilClose)
            .expect("remembering needs no keychain");
        assert_eq!(session.rejected(&login()), Err(BankError::Keychain));
        assert!(!session.is_remembered(&login()));
        // Blocked, even though the keychain still has the old PIN.
        assert_eq!(session.answer(&login()), Ok(PinAnswer::Ask));
    }

    #[test]
    fn saving_fails_cleanly_without_a_keychain() {
        let mut store = MemoryPinStore::new();
        store.unavailable = true;
        let mut session = PinSession::new(Box::new(store));
        assert_eq!(
            session.provide(&login(), pin("0000"), PinMode::SaveInKeychain),
            Err(BankError::Keychain)
        );
        assert!(!session.is_remembered(&login()));
        assert_eq!(session.is_saved(&login()), Err(BankError::Keychain));
    }

    #[test]
    fn forgetting_removes_the_saved_and_the_remembered_pin() {
        let mut session = session();
        session
            .provide(&login(), pin("0000"), PinMode::SaveInKeychain)
            .expect("provided");
        session.forget(&login()).expect("forgotten");
        assert_eq!(session.answer(&login()), Ok(PinAnswer::Ask));
        assert_eq!(session.is_saved(&login()), Ok(false));
    }

    #[test]
    fn a_pending_pin_is_moved_to_the_new_login() {
        let mut session = session();
        let pending = LoginId::pending();
        session
            .provide(&pending, pin("0000"), PinMode::SaveInKeychain)
            .expect("provided");
        session.rename(&pending, &login()).expect("renamed");
        assert_eq!(session.answer(&pending), Ok(PinAnswer::Ask));
        assert_eq!(session.is_saved(&pending), Ok(false));
        assert_eq!(session.answer(&login()), Ok(PinAnswer::Known(pin("0000"))));
        assert_eq!(session.is_saved(&login()), Ok(true));
        // Renaming a login without a PIN changes nothing.
        session
            .rename(&LoginId::new("nobody"), &LoginId::new("still nobody"))
            .expect("nothing to move");
        assert_eq!(
            session.answer(&LoginId::new("still nobody")),
            Ok(PinAnswer::Ask)
        );
    }

    #[test]
    fn asking_each_time_is_the_default() {
        assert_eq!(PinMode::default(), PinMode::AskEachTime);
    }

    /// A memory store the test can look into after the session took it.
    #[derive(Clone, Default)]
    struct Shared(std::sync::Arc<std::sync::Mutex<MemoryPinStore>>);

    impl PinStore for Shared {
        fn save(&mut self, login: &LoginId, pin: &Pin) -> Result<(), BankError> {
            self.0.lock().expect("store").save(login, pin)
        }

        fn load(&self, login: &LoginId) -> Result<Option<Pin>, BankError> {
            self.0.lock().expect("store").load(login)
        }

        fn delete(&mut self, login: &LoginId) -> Result<(), BankError> {
            self.0.lock().expect("store").delete(login)
        }
    }

    #[test]
    fn saving_nowhere_only_remembers_the_pin_until_the_app_closes() {
        let mut session = PinSession::new(Box::new(NoPinStore));
        assert!(!session.saves());
        session
            .provide(&login(), pin("0000"), PinMode::SaveInKeychain)
            .expect("remembered instead");
        assert_eq!(session.answer(&login()), Ok(PinAnswer::Known(pin("0000"))));
        assert_eq!(session.is_saved(&login()), Ok(false));
        // It outlives the bank session, like a remembered PIN.
        session.end_session();
        assert!(session.is_remembered(&login()));
        // A login set up while pending keeps it under its new ID.
        let pending = LoginId::pending();
        session
            .provide(&pending, pin("1111"), PinMode::SaveInKeychain)
            .expect("remembered instead");
        let new = LoginId::new("login-2");
        session.rename(&pending, &new).expect("moved");
        assert_eq!(session.answer(&new), Ok(PinAnswer::Known(pin("1111"))));
        assert_eq!(session.is_saved(&new), Ok(false));
        session.forget(&new).expect("forgotten");
        assert_eq!(session.answer(&new), Ok(PinAnswer::Ask));
    }

    #[test]
    fn a_new_place_starts_empty_and_the_old_one_is_emptied() {
        let old = Shared::default();
        let new = Shared::default();
        let mut session = PinSession::new(Box::new(old.clone()));
        let other = LoginId::new("login-2");
        session
            .provide(&login(), pin("0000"), PinMode::SaveInKeychain)
            .expect("saved");
        session
            .provide(&other, pin("1111"), PinMode::SaveInKeychain)
            .expect("saved");
        assert_eq!(old.0.lock().expect("store").len(), 2);

        session
            .replace_store(Box::new(new.clone()), &[login(), other.clone()])
            .expect("replaced");

        // Removed from the old place, not copied to the new one.
        assert!(old.0.lock().expect("store").is_empty());
        assert!(new.0.lock().expect("store").is_empty());
        assert!(session.saves());
        assert_eq!(session.is_saved(&login()), Ok(false));
        // Still remembered until the app closes, now only in memory.
        assert_eq!(session.answer(&login()), Ok(PinAnswer::Known(pin("0000"))));
        session
            .rename(&login(), &LoginId::new("login-3"))
            .expect("moved");
        assert!(new.0.lock().expect("store").is_empty());
        // The next PIN to save goes to the new place.
        session
            .provide(&other, pin("2222"), PinMode::SaveInKeychain)
            .expect("saved");
        assert_eq!(
            new.0.lock().expect("store").load(&other),
            Ok(Some(pin("2222")))
        );
    }

    #[test]
    fn an_old_place_out_of_reach_is_still_left_and_says_so() {
        let old = Shared::default();
        let mut session = PinSession::new(Box::new(old.clone()));
        session
            .provide(&login(), pin("0000"), PinMode::SaveInKeychain)
            .expect("saved");
        old.0.lock().expect("store").unavailable = true;

        let result = session.replace_store(Box::new(NoPinStore), &[login()]);

        assert_eq!(result, Err(BankError::Keychain));
        assert!(!session.saves());
        assert_eq!(session.is_saved(&login()), Ok(false));
        old.0.lock().expect("store").unavailable = false;
        assert_eq!(old.0.lock().expect("store").len(), 1);
    }

    #[test]
    fn the_empty_store_saves_nothing() {
        let mut store = NoPinStore;
        assert!(!store.saves());
        assert_eq!(store.save(&login(), &pin("0000")), Err(BankError::Keychain));
        assert_eq!(store.load(&login()), Ok(None));
        assert_eq!(store.delete(&login()), Ok(()));
        assert!(MemoryPinStore::new().saves());
    }
}
