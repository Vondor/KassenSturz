// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the AI settings screens do (M5.7, D-202 to D-205), apart from Qt.
//!
//! The provider (address, model, how its key is kept) and the consent live
//! in the `[AI]` group of this computer's settings file, never in a book
//! (D-168), and are written under the key names `kassensturz-ai` reads
//! (`ProviderSettings::entries`, `ConsentRecord::entries`). The provider's
//! key is only ever in `kassensturz-ai`'s `ProviderKeys`: typed into the
//! screen, handed over once and never read back to it (D-169, threat T-25).
//! This module is compiled only in a build with AI (D-039).

use std::sync::Mutex;

use kassensturz_ai::{
    AiClient, AiError, ConfigFile, ConsentRecord, KeyUse, Place, ProviderAddress, ProviderKeys,
    ProviderSettings, SCRIPTED_MODEL, STATEMENT, consent_missing,
};
use kassensturz_ai_chat::{BuildInfo, DEMO_ENV, DemoModel, sample_request};
use kassensturz_core::{Date, ai_switch};
use kassensturz_platform::{APP_ID, Secret, SecretStorage};

use crate::settings::Settings;

/// The question the example request in the consent screen asks. It is
/// English like the rest of what the model gets (D-189) and holds nothing
/// of the user's.
pub const SAMPLE_QUESTION: &str = "How much did I spend on food last month?";

/// The provider's keys of this run of KassenSturz, shared by these screens
/// and the chat (M5.8), so a key remembered until the app closes is the
/// same key in both.
static KEYS: Mutex<Option<(SecretStorage, ProviderKeys)>> = Mutex::new(None);

/// Runs `f` with the provider keys, saved in the place the Password storage
/// setting names now (D-163, D-169): a change of that setting takes effect
/// here at once, keeping what is remembered in memory.
pub fn with_keys<R>(f: impl FnOnce(&mut ProviderKeys) -> R) -> R {
    with_keys_in(Settings::user().pin_storage(), f)
}

fn with_keys_in<R>(storage: SecretStorage, f: impl FnOnce(&mut ProviderKeys) -> R) -> R {
    let mut held = KEYS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    match held.as_mut() {
        Some((place, keys)) => {
            if *place != storage {
                keys.use_store(storage.keychain(APP_ID));
                *place = storage;
            }
        }
        None => *held = Some((storage, ProviderKeys::new(storage.keychain(APP_ID)))),
    }
    let (_, keys) = held.as_mut().expect("set above");
    f(keys)
}

/// Whether the fake model (`KASSENSTURZ_FAKE_AI`, D-193) is switched on:
/// then nothing is sent anywhere, and "Test connection" answers from the
/// app itself (D-205).
pub fn fake_model() -> bool {
    std::env::var(DEMO_ENV)
        .ok()
        .is_some_and(|value| DemoModel::from_env_value(&value).is_some())
}

/// The consent as the screen shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Consent {
    /// The provider is on this computer, or there is none yet: nothing
    /// leaves the machine, so nothing to agree to (D-175).
    NotNeeded,
    /// The user agreed to what is sent to this provider, on this day.
    Given(Date),
    /// The provider is on another computer and the user hasn't agreed yet
    /// to this address and the current statement.
    Missing,
}

/// What the settings form shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Form {
    /// The address as saved, or empty.
    pub address: String,
    /// The model as saved, or empty.
    pub model: String,
    /// How the key is kept.
    pub key_use: KeyUse,
    /// Where the address points, if it is usable.
    pub place: Option<Place>,
    /// Why the saved address can't be used, if it is odd.
    pub address_error: Option<AiError>,
    /// The consent for the saved provider.
    pub consent: Consent,
}

/// What saving the form did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Saved {
    /// The settings as typed can't be used; nothing was written.
    Refused(AiError),
    /// The settings file couldn't be written; the old settings stay.
    NotWritten,
    /// Written. `key_error` is why a key couldn't be kept or an old one
    /// couldn't be removed, which doesn't undo the settings.
    Written {
        /// Where the provider is.
        place: Place,
        /// Whether the user has to agree before anything is sent.
        consent_needed: bool,
        /// A problem with the key, if any.
        key_error: Option<AiError>,
    },
}

/// The AI settings of a settings file: written through KConfig, read the
/// way `kassensturz-ai` reads them.
pub struct Store {
    settings: Settings,
    config: ConfigFile,
}

impl Store {
    /// The settings of this user on this computer.
    pub fn user() -> Self {
        Self {
            settings: Settings::user(),
            config: ConfigFile::user(),
        }
    }

    /// The settings in the file at `path`.
    #[cfg(test)]
    pub fn at(path: &std::path::Path) -> Self {
        Self {
            settings: Settings::at(path),
            config: ConfigFile::at(path),
        }
    }

    /// The settings file, read as every AI call reads it.
    pub fn config(&self) -> &ConfigFile {
        &self.config
    }

    /// A client that reads the settings at every call.
    pub fn client(&self) -> AiClient {
        AiClient::new(self.config.clone())
    }

    fn text(&self, key: &str) -> String {
        self.settings.text(ai_switch::GROUP, key)
    }

    pub(crate) fn put(&self, key: &str, value: &str) -> bool {
        self.settings.set_text(ai_switch::GROUP, key, value)
    }

    /// What the form shows now.
    pub fn form(&self) -> Form {
        let address = self.text(kassensturz_ai::ADDRESS_KEY);
        let model = self.text(kassensturz_ai::MODEL_KEY);
        let key_use = KeyUse::from_word(&self.text(kassensturz_ai::KEY_USE_KEY));
        let parsed = ProviderAddress::parse(&address);
        let config = self.config.read();
        let consent = match &parsed {
            Ok(parsed) if parsed.place() == Place::AnotherComputer => match config.consent() {
                Some(consent) if consent.covers(parsed) => Consent::Given(consent.day),
                _ => Consent::Missing,
            },
            _ => Consent::NotNeeded,
        };
        let address_error = match parsed {
            Err(error) if !address.trim().is_empty() => Some(error),
            _ => None,
        };
        Form {
            place: ProviderAddress::parse(&address)
                .ok()
                .map(|address| address.place()),
            address,
            model,
            key_use,
            address_error,
            consent,
        }
    }

    /// Checks an address as typed, for the hint under the field.
    pub fn check_address(address: &str) -> Result<Place, AiError> {
        ProviderAddress::parse(address).map(|address| address.place())
    }

    /// Saves the provider as typed. A `key` typed with it is kept as
    /// `key_use` says; a key kept for an earlier address or in an earlier
    /// way is removed first (D-203), so a changed address never has the old
    /// server's key left behind.
    pub fn save(
        &self,
        keys: &mut ProviderKeys,
        address: &str,
        model: &str,
        key_use: KeyUse,
        key: Option<Secret>,
    ) -> Saved {
        let provider = match ProviderSettings::new(address, model, key_use) {
            Ok(provider) => provider,
            Err(error) => return Saved::Refused(error),
        };
        let earlier = self.config.read().provider().ok().cloned();
        if !provider
            .entries()
            .iter()
            .all(|(name, value)| self.put(name, value))
        {
            return Saved::NotWritten;
        }
        let mut key_error = None;
        if let Some(earlier) = earlier
            && matches!(earlier.key_use, KeyUse::UntilClose | KeyUse::Saved)
            && (earlier.address.origin() != provider.address.origin()
                || earlier.key_use != provider.key_use)
        {
            key_error = keys.forget(&earlier.address).err();
        }
        if let Some(key) = key.filter(|key| !key.is_empty())
            && let Err(error) = keys.keep(&provider.address, provider.key_use, key)
        {
            key_error = Some(error);
        }
        Saved::Written {
            place: provider.address.place(),
            consent_needed: consent_missing(&provider.address, self.config.read().consent()),
            key_error,
        }
    }

    /// Removes the key kept for the saved provider, in memory and where it
    /// was saved. Fails with `ai.no-provider` when none is set up.
    pub fn forget_key(&self, keys: &mut ProviderKeys) -> Result<(), AiError> {
        let config = self.config.read();
        keys.forget(&config.provider()?.address)
    }

    /// Records that the user agreed to what is sent to the saved provider,
    /// on `today`, to the current statement. Returns whether it was
    /// written; nothing is recorded for a provider that isn't usable.
    pub fn give_consent(&self, today: Date) -> bool {
        let config = self.config.read();
        let Ok(provider) = config.provider() else {
            return false;
        };
        ConsentRecord::give(&provider.address, today)
            .entries()
            .iter()
            .all(|(name, value)| self.put(name, value))
    }

    /// Takes the consent back: nothing goes to a provider on another
    /// computer until the user agrees again. Returns whether it was
    /// written.
    pub fn withdraw_consent(&self) -> bool {
        [
            kassensturz_ai::CONSENT_ADDRESS_KEY,
            kassensturz_ai::CONSENT_STATEMENT_KEY,
            kassensturz_ai::CONSENT_DAY_KEY,
        ]
        .iter()
        .all(|name| self.put(name, ""))
    }

    /// The key for a test: the one typed for it, else the one kept for the
    /// saved provider, if it needs one.
    pub fn key_for_test(
        &self,
        keys: &ProviderKeys,
        typed: Option<Secret>,
    ) -> Result<Option<Secret>, AiError> {
        if let Some(key) = typed.filter(|key| !key.is_empty()) {
            return Ok(Some(key));
        }
        let config = self.config.read();
        let provider = config.provider()?;
        keys.get(&provider.address, provider.key_use)
    }

    /// The models the saved provider offers, which also shows that the
    /// address and the key work. Sends only the key, nothing from a book
    /// (D-182). With `fake` (the fake model is on), nothing is sent anywhere
    /// and the app's own test model is "offered" (D-205).
    pub fn test_connection(&self, key: Option<Secret>, fake: bool) -> Result<Vec<String>, AiError> {
        let config = self.config.read();
        if !config.enabled() {
            return Err(AiError::Off);
        }
        let provider = config.provider()?;
        if fake {
            if provider.key_use != KeyUse::None && key.is_none() {
                return Err(AiError::KeyNeeded);
            }
            return Ok(vec![SCRIPTED_MODEL.to_owned()]);
        }
        self.client().list_models(key.as_ref())
    }

    /// An example of the first request of a question to the saved
    /// provider, exactly as the chat builds it, with every `phrase` left
    /// out of it (D-204). Nothing is sent. Also how many places the phrase
    /// was found in.
    pub fn sample(&self, phrase: &str, today: Date) -> Result<(String, usize), AiError> {
        let build = BuildInfo::new(crate::book_session::app_version(), build_commit());
        let client = self.client();
        let mut request = sample_request(&client, &build, today, SAMPLE_QUESTION)?;
        let replaced = request.redact(phrase)?;
        Ok((request.preview().to_owned(), replaced))
    }
}

/// The commit the build is made from, empty when it isn't known.
fn build_commit() -> &'static str {
    option_env!("KASSENSTURZ_BUILD_COMMIT").unwrap_or("")
}

/// The words of what the statement covers, in the order the screen reads
/// them out (D-204).
pub fn statement_words() -> Vec<&'static str> {
    STATEMENT.iter().map(|item| item.as_word()).collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use kassensturz_ai::{AiError, KeyUse, Place, SCRIPTED_MODEL};
    use kassensturz_core::ai_switch;
    use kassensturz_platform::{Keychain, KeychainError, MemoryKeychain, Secret, SecretKey};

    use super::*;
    use crate::test_folder::TestFolder;

    const TODAY: Date = Date::constant(2026, 10, 25);

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

    fn keys() -> (Arc<MemoryKeychain>, ProviderKeys) {
        let memory = Arc::new(MemoryKeychain::new());
        let keys = ProviderKeys::new(Some(Box::new(Shared(memory.clone()))));
        (memory, keys)
    }

    fn secret(text: &str) -> Secret {
        Secret::new(text.to_owned())
    }

    /// A settings file with AI switched on, as the toggle writes it.
    fn on() -> (TestFolder, std::path::PathBuf, Store) {
        let folder = TestFolder::new();
        let file = folder.join("kassensturzrc");
        let store = Store::at(&file);
        assert!(store.put(ai_switch::ENABLED_KEY, ai_switch::ON));
        (folder, file, store)
    }

    fn written(place: Place, consent_needed: bool) -> Saved {
        Saved::Written {
            place,
            consent_needed,
            key_error: None,
        }
    }

    #[test]
    fn what_is_saved_is_what_kassensturz_ai_reads() {
        let (_folder, file, store) = on();
        let (_, mut keys) = keys();
        assert_eq!(
            store.save(
                &mut keys,
                " http://localhost:11434/v1/ ",
                " qwen3 ",
                KeyUse::None,
                None
            ),
            written(Place::ThisComputer, false)
        );
        // The reader of M5.2 (`AiConfig`) gets the provider the page saved.
        let config = store.config().read();
        let provider = config.provider().expect("a provider");
        assert_eq!(provider.address.as_str(), "http://localhost:11434/v1");
        assert_eq!(provider.model, "qwen3");
        assert_eq!(provider.key_use, KeyUse::None);
        assert!(
            config.enabled(),
            "saving a provider leaves the switch alone"
        );
        let form = store.form();
        assert_eq!(form.address, "http://localhost:11434/v1");
        assert_eq!(form.model, "qwen3");
        assert_eq!(form.place, Some(Place::ThisComputer));
        assert_eq!(form.consent, Consent::NotNeeded);
        assert_eq!(form.address_error, None);
        // As KConfig wrote it, in the group the switch is in.
        let text = std::fs::read_to_string(&file).expect("settings file");
        assert!(text.contains("[AI]"), "{text}");
        assert!(text.contains("ProviderModel=qwen3"), "{text}");
    }

    #[test]
    fn a_refused_provider_writes_nothing() {
        let (_folder, file, store) = on();
        let (_, mut keys) = keys();
        for (address, model, error) in [
            ("http://192.168.1.2:11434/v1", "m", AiError::PlainHttp),
            ("ftp://example.com", "m", AiError::BadAddress),
            ("https://api.example.com/v1", "  ", AiError::NoProvider),
            ("", "m", AiError::NoProvider),
        ] {
            assert_eq!(
                store.save(&mut keys, address, model, KeyUse::None, None),
                Saved::Refused(error),
                "{address} {model}"
            );
        }
        let text = std::fs::read_to_string(&file).expect("settings file");
        assert!(!text.contains("ProviderAddress"), "{text}");
    }

    #[test]
    fn another_computer_is_asked_again_for_every_new_address() {
        let (_folder, _file, store) = on();
        let (_, mut keys) = keys();
        assert_eq!(
            store.save(
                &mut keys,
                "https://api.example.com/v1",
                "big",
                KeyUse::None,
                None
            ),
            written(Place::AnotherComputer, true)
        );
        assert_eq!(store.form().consent, Consent::Missing);
        // M5.2's client refuses to send until the consent is given: the
        // check comes before any connection.
        let client = store.client();
        let request = client
            .prepare(vec![kassensturz_ai::Message::user("Hi")], Vec::new())
            .expect("a request");
        assert_eq!(client.send(&request, None), Err(AiError::NoConsent));

        assert!(store.give_consent(TODAY));
        assert_eq!(store.form().consent, Consent::Given(TODAY));
        let config = store.config().read();
        let consent = config.consent().expect("a consent");
        assert!(consent.covers(&config.provider().unwrap().address));

        // The same address again: still agreed. Another address: asked.
        assert_eq!(
            store.save(
                &mut keys,
                "https://api.example.com/v1",
                "other",
                KeyUse::None,
                None
            ),
            written(Place::AnotherComputer, false)
        );
        assert_eq!(
            store.save(
                &mut keys,
                "https://elsewhere.example.org/v1",
                "big",
                KeyUse::None,
                None
            ),
            written(Place::AnotherComputer, true)
        );
        assert_eq!(store.form().consent, Consent::Missing);

        // Taking it back: nothing covers the address.
        assert!(store.give_consent(TODAY));
        assert!(store.withdraw_consent());
        assert_eq!(store.form().consent, Consent::Missing);
        assert_eq!(store.config().read().consent(), None);
        let request = client
            .prepare(vec![kassensturz_ai::Message::user("Hi")], Vec::new())
            .expect("a request");
        assert_eq!(client.send(&request, None), Err(AiError::NoConsent));
    }

    #[test]
    fn a_provider_on_this_computer_needs_no_consent_and_none_is_recorded_for_nothing() {
        let (_folder, _file, store) = on();
        assert!(!store.give_consent(TODAY), "no provider yet");
        let (_, mut keys) = keys();
        store.save(
            &mut keys,
            "http://127.0.0.1:1234/v1",
            "m",
            KeyUse::None,
            None,
        );
        assert_eq!(store.form().consent, Consent::NotNeeded);
        assert!(store.withdraw_consent(), "nothing to take back is fine");
    }

    #[test]
    fn a_key_is_kept_as_chosen_and_never_in_the_settings_file() {
        let (_folder, file, store) = on();
        let (memory, mut keys) = keys();
        let cloud = "https://api.example.com/v1";
        store.save(
            &mut keys,
            cloud,
            "big",
            KeyUse::Saved,
            Some(secret("test-key-ui-1")),
        );
        assert_eq!(memory.len(), 1);
        let filed = SecretKey::new("ai-key", "https:api.example.com:443").unwrap();
        assert_eq!(memory.read(&filed).unwrap(), Some(secret("test-key-ui-1")));
        let text = std::fs::read_to_string(&file).expect("settings file");
        assert!(!text.contains("test-key-ui-1"), "{text}");
        assert!(text.contains("ProviderKey=save"), "{text}");

        // Saving again with the field empty keeps the saved key.
        store.save(&mut keys, cloud, "other", KeyUse::Saved, None);
        assert_eq!(memory.len(), 1);

        // The test gets the kept key unless one is typed for it.
        assert_eq!(
            store.key_for_test(&keys, None).unwrap(),
            Some(secret("test-key-ui-1"))
        );
        assert_eq!(
            store.key_for_test(&keys, Some(secret("typed"))).unwrap(),
            Some(secret("typed"))
        );

        // Another server never keeps the old server's key.
        store.save(
            &mut keys,
            "https://other.example.com/v1",
            "big",
            KeyUse::Saved,
            None,
        );
        assert_eq!(memory.len(), 0);
        assert_eq!(store.key_for_test(&keys, None).unwrap(), None);

        // Neither does a change to typing it each time.
        store.save(
            &mut keys,
            cloud,
            "big",
            KeyUse::Saved,
            Some(secret("test-key-ui-2")),
        );
        assert_eq!(memory.len(), 1);
        store.save(&mut keys, cloud, "big", KeyUse::AskEachTime, None);
        assert_eq!(memory.len(), 0);

        // Removing on request.
        store.save(
            &mut keys,
            cloud,
            "big",
            KeyUse::Saved,
            Some(secret("test-key-ui-3")),
        );
        store.forget_key(&mut keys).expect("removed");
        assert_eq!(memory.len(), 0);
        let text = std::fs::read_to_string(&file).expect("settings file");
        assert!(!text.contains("test-key-ui"), "{text}");
    }

    #[test]
    fn a_key_that_cannot_be_kept_is_reported_and_the_settings_stay_saved() {
        let (_folder, _file, store) = on();
        let (memory, mut keys) = keys();
        memory.fail_with(Some(KeychainError::Unavailable));
        let saved = store.save(
            &mut keys,
            "https://api.example.com/v1",
            "big",
            KeyUse::Saved,
            Some(secret("test-key-ui-4")),
        );
        assert_eq!(
            saved,
            Saved::Written {
                place: Place::AnotherComputer,
                consent_needed: true,
                key_error: Some(AiError::Keychain)
            }
        );
        assert_eq!(store.form().address, "https://api.example.com/v1");
        assert!(!format!("{saved:?}").contains("test-key-ui-4"));
    }

    #[test]
    fn the_test_asks_the_saved_provider_and_needs_ai_on() {
        let folder = TestFolder::new();
        let store = Store::at(&folder.join("kassensturzrc"));
        assert_eq!(store.test_connection(None, false), Err(AiError::Off));
        assert!(store.put(ai_switch::ENABLED_KEY, ai_switch::ON));
        assert_eq!(store.test_connection(None, false), Err(AiError::NoProvider));
        let (_, mut keys) = keys();

        // Nothing is listening on port 1 of this computer.
        store.save(&mut keys, "http://127.0.0.1:1/v1", "m", KeyUse::None, None);
        assert_eq!(
            store.test_connection(None, false),
            Err(AiError::Unreachable)
        );

        // The fake model never connects, and keeps the key rules.
        assert_eq!(
            store.test_connection(None, true),
            Ok(vec![SCRIPTED_MODEL.to_owned()])
        );
        store.save(
            &mut keys,
            "http://127.0.0.1:1/v1",
            "m",
            KeyUse::AskEachTime,
            None,
        );
        assert_eq!(store.test_connection(None, true), Err(AiError::KeyNeeded));
        assert_eq!(
            store.test_connection(Some(secret("test-key-ui-5")), true),
            Ok(vec![SCRIPTED_MODEL.to_owned()])
        );
    }

    #[test]
    fn the_example_request_is_the_real_first_request_and_can_be_redacted() {
        let (_folder, _file, store) = on();
        let (_, mut keys) = keys();
        assert_eq!(store.sample("", TODAY), Err(AiError::NoProvider));
        store.save(
            &mut keys,
            "https://api.example.com/v1",
            "big",
            KeyUse::Saved,
            Some(secret("test-key-ui-6")),
        );
        let (preview, replaced) = store.sample("", TODAY).expect("a sample");
        assert_eq!(replaced, 0);
        assert!(preview.contains(SAMPLE_QUESTION), "{preview}");
        assert!(preview.contains("\"big\""), "{preview}");
        assert!(
            preview.contains("search_transactions"),
            "the functions are offered"
        );
        assert!(
            !preview.contains("test-key-ui-6"),
            "the key is added after the preview"
        );

        let (preview, replaced) = store.sample("food", TODAY).expect("a sample");
        assert!(replaced >= 1);
        assert!(!preview.contains("food"), "{preview}");

        // AI off: nothing is built.
        assert!(store.put(ai_switch::ENABLED_KEY, "false"));
        assert_eq!(store.sample("", TODAY), Err(AiError::Off));
    }

    #[test]
    fn the_statement_is_the_four_things_the_page_has_sentences_for() {
        assert_eq!(
            statement_words(),
            ["questions", "book-data", "instructions", "key"]
        );
    }

    #[test]
    fn a_new_place_for_saved_keys_keeps_what_the_app_remembers() {
        // Only `Nowhere`: the system keychain isn't used by tests.
        with_keys_in(SecretStorage::Nowhere, |keys| {
            let address = ProviderAddress::parse("http://localhost:1234/v1").unwrap();
            keys.keep(&address, KeyUse::UntilClose, secret("test-key-ui-7"))
                .unwrap();
        });
        with_keys_in(SecretStorage::Nowhere, |keys| {
            let address = ProviderAddress::parse("http://localhost:1234/v1").unwrap();
            assert_eq!(
                keys.get(&address, KeyUse::UntilClose).unwrap(),
                Some(secret("test-key-ui-7")),
                "the same keys serve the settings and the chat"
            );
            keys.forget(&address).unwrap();
        });
    }
}
