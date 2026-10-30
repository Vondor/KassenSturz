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
    AiClient, AiError, ConfigFile, ConsentRecord, FunctionCalling, KeyUse, MIN_CONTEXT_TOKENS,
    ModelInfo, Place, ProviderAddress, ProviderKeys, ProviderSettings, SCRIPTED_MODEL, STATEMENT,
    consent_missing, probe_functions, probe_request_body,
};
use kassensturz_ai_chat::{BuildInfo, DEMO_ENV, DemoModel, demo_provider, sample_request};
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
    demo_model().is_some()
}

/// The fake model `KASSENSTURZ_FAKE_AI` asks for, if it is switched on.
pub fn demo_model() -> Option<DemoModel> {
    std::env::var(DEMO_ENV)
        .ok()
        .and_then(|value| DemoModel::from_env_value(&value))
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
    /// What the last test found out about the saved model, if it was made
    /// for this address and model.
    pub capability: Option<Capability>,
}

/// What "Test connection" found out about a model, as it is kept on this
/// computer with the provider's settings (D-294, D-320), never in a book.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capability {
    /// Whether the model called the test function; `None` when the test
    /// could not tell (not asked, or the request failed).
    pub functions: Option<bool>,
    /// The context size in tokens, when the provider's list said it.
    pub context_tokens: Option<u64>,
}

impl Capability {
    /// Whether the context is smaller than KassenSturz's chat needs.
    pub fn context_small(&self) -> bool {
        self.context_tokens
            .is_some_and(|tokens| tokens < MIN_CONTEXT_TOKENS)
    }
}

/// Whether the test could ask the model to call a function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Functions {
    /// It called the test function.
    Yes,
    /// It did not, or its provider refused a request that offers functions.
    No,
    /// Not asked: no model is chosen yet.
    NoModel,
    /// Not asked: the provider is on another computer and the user has not
    /// agreed yet, and nothing is sent to it before (D-035).
    NoConsent,
    /// The request failed; the models were still listed.
    Failed(AiError),
}

/// What a successful "Test connection" found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    /// The models the provider offers.
    pub models: Vec<String>,
    /// The saved model's context size, when the list says it.
    pub context_tokens: Option<u64>,
    /// Whether the saved model called the test function.
    pub functions: Functions,
}

impl Found {
    fn capability(&self) -> Capability {
        Capability {
            functions: match self.functions {
                Functions::Yes => Some(true),
                Functions::No => Some(false),
                _ => None,
            },
            context_tokens: self.context_tokens,
        }
    }
}

/// The settings keys of the kept test result (D-320): which address and
/// model it is for, and what it found. Written by the settings page only,
/// in the `[AI]` group next to the provider's keys.
const CAPABILITY_FOR_KEY: &str = "CapabilityFor";
const CAPABILITY_FUNCTIONS_KEY: &str = "CapabilityFunctions";
const CAPABILITY_CONTEXT_KEY: &str = "CapabilityContext";

/// Whether the test may ask the model to call a function, and what came of
/// it. `ask` is only run when a model is chosen and, for a provider on
/// another computer, the user has agreed (`consent_missing` false): nothing
/// is sent before (D-035).
fn ask_functions(
    has_model: bool,
    consent_missing: bool,
    ask: impl FnOnce() -> Result<FunctionCalling, AiError>,
) -> Functions {
    if !has_model {
        return Functions::NoModel;
    }
    if consent_missing {
        return Functions::NoConsent;
    }
    match ask() {
        Ok(FunctionCalling::Yes) => Functions::Yes,
        Ok(FunctionCalling::No) => Functions::No,
        Err(AiError::NoConsent) => Functions::NoConsent,
        Err(error) => Functions::Failed(error),
    }
}

/// The model's entry in a list, also when the provider writes it with the
/// `models/` prefix of Gemini's list (D-209).
fn find_model<'a>(infos: &'a [ModelInfo], model: &str) -> Option<&'a ModelInfo> {
    infos
        .iter()
        .find(|info| info.id == model || info.id.strip_prefix("models/") == Some(model))
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
        let capability = self.kept_capability(&address, &model);
        Form {
            capability,
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

    fn capability_target(address: &str, model: &str) -> String {
        format!("{address}|{model}")
    }

    /// What the last test found, if it was made for this address and model.
    fn kept_capability(&self, address: &str, model: &str) -> Option<Capability> {
        if model.is_empty()
            || self.text(CAPABILITY_FOR_KEY) != Self::capability_target(address, model)
        {
            return None;
        }
        let capability = Capability {
            functions: match self.text(CAPABILITY_FUNCTIONS_KEY).as_str() {
                "yes" => Some(true),
                "no" => Some(false),
                _ => None,
            },
            context_tokens: self
                .text(CAPABILITY_CONTEXT_KEY)
                .parse()
                .ok()
                .filter(|tokens| *tokens > 0),
        };
        (capability != Capability::default()).then_some(capability)
    }

    /// Keeps what a test found for the saved address and model, replacing
    /// the earlier result. Best effort: the test's answer is shown either
    /// way.
    fn keep_capability(&self, address: &str, model: &str, capability: Capability) {
        let functions = match capability.functions {
            Some(true) => "yes",
            Some(false) => "no",
            None => "",
        };
        let context = capability
            .context_tokens
            .map(|tokens| tokens.to_string())
            .unwrap_or_default();
        let _ = self.put(CAPABILITY_FOR_KEY, &Self::capability_target(address, model))
            && self.put(CAPABILITY_FUNCTIONS_KEY, functions)
            && self.put(CAPABILITY_CONTEXT_KEY, &context);
    }

    /// The exact request the function test sends to the saved model, for
    /// the screen to show before it runs (D-204). Nothing is sent. Empty
    /// while no model is saved.
    pub fn capability_request(&self) -> String {
        let model = self.text(kassensturz_ai::MODEL_KEY);
        if model.is_empty() {
            String::new()
        } else {
            probe_request_body(&model)
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
        let provider = match ProviderSettings::with_optional_model(address, model, key_use) {
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

    /// The key for a test or a question: the one typed for it, else the one
    /// kept for the saved provider, if it needs one.
    pub fn key_for(
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

    /// Asks the saved provider for its models, which also shows that the
    /// address and the key work, and then what the saved model can do
    /// (D-294, D-320): its context size where the list says it, and whether
    /// it calls a function (one fixed, made-up test function, nothing from
    /// a book, T-45). The listing sends only the key (D-182); the function
    /// test is a request like any other and needs the agreement of a
    /// provider on another computer first (D-035). With `fake` (the fake
    /// model is on), nothing is sent anywhere and the fake model answers
    /// (D-205). What was found is kept with the provider's settings, for
    /// the address and model it was found for.
    pub fn test_connection(
        &self,
        key: Option<Secret>,
        fake: Option<DemoModel>,
    ) -> Result<Found, AiError> {
        let config = self.config.read();
        if !config.enabled() {
            return Err(AiError::Off);
        }
        let provider = config.connection()?;
        let has_model = !provider.model.is_empty();
        let (infos, functions) = match fake {
            Some(model) => {
                if provider.key_use != KeyUse::None && key.is_none() {
                    return Err(AiError::KeyNeeded);
                }
                let infos = vec![ModelInfo {
                    id: SCRIPTED_MODEL.to_owned(),
                    context_tokens: None,
                }];
                let functions = ask_functions(has_model, false, || {
                    probe_functions(&demo_provider(self.config.clone(), model), None)
                });
                (infos, functions)
            }
            None => {
                let client = self.client();
                let infos = client.list_model_infos(key.as_ref())?;
                let agreed = !consent_missing(&provider.address, config.consent());
                let functions = ask_functions(has_model, !agreed, || {
                    probe_functions(&client, key.as_ref())
                });
                (infos, functions)
            }
        };
        let found = Found {
            context_tokens: find_model(&infos, &provider.model)
                .and_then(|info| info.context_tokens),
            models: infos.into_iter().map(|info| info.id).collect(),
            functions,
        };
        if !provider.model.is_empty() {
            self.keep_capability(
                provider.address.as_str(),
                &provider.model,
                found.capability(),
            );
        }
        Ok(found)
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
pub(crate) fn build_commit() -> &'static str {
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
    fn an_address_without_a_model_is_saved_and_can_list_models_but_not_chat() {
        let (_folder, _file, store) = on();
        let (_, mut keys) = keys();
        let saved = store.save(&mut keys, "http://127.0.0.1:1/v1", "  ", KeyUse::None, None);
        assert!(matches!(saved, Saved::Written { .. }), "{saved:?}");
        // The test reaches the server (nothing listens on port 1) ...
        assert_eq!(store.test_connection(None, None), Err(AiError::Unreachable));
        // ... but there is no provider to ask a question yet.
        assert_eq!(store.sample("", TODAY), Err(AiError::NoProvider));
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
            store.key_for(&keys, None).unwrap(),
            Some(secret("test-key-ui-1"))
        );
        assert_eq!(
            store.key_for(&keys, Some(secret("typed"))).unwrap(),
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
        assert_eq!(store.key_for(&keys, None).unwrap(), None);

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
        assert_eq!(store.test_connection(None, None), Err(AiError::Off));
        assert!(store.put(ai_switch::ENABLED_KEY, ai_switch::ON));
        assert_eq!(store.test_connection(None, None), Err(AiError::NoProvider));
        let (_, mut keys) = keys();

        // Nothing is listening on port 1 of this computer.
        store.save(&mut keys, "http://127.0.0.1:1/v1", "m", KeyUse::None, None);
        assert_eq!(store.test_connection(None, None), Err(AiError::Unreachable));

        // The fake model never connects, and keeps the key rules.
        let found = store.test_connection(None, Some(DemoModel::Functions));
        assert_eq!(
            found.map(|found| found.models),
            Ok(vec![SCRIPTED_MODEL.to_owned()])
        );
        store.save(
            &mut keys,
            "http://127.0.0.1:1/v1",
            "m",
            KeyUse::AskEachTime,
            None,
        );
        assert_eq!(
            store.test_connection(None, Some(DemoModel::Functions)),
            Err(AiError::KeyNeeded)
        );
        let found =
            store.test_connection(Some(secret("test-key-ui-5")), Some(DemoModel::Functions));
        assert_eq!(
            found.map(|found| found.models),
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
        // The example holds what a real question would hold now: with
        // nothing allowed yet (D-252), no function is offered.
        assert!(
            !preview.contains("search_transactions"),
            "nothing is read until the user allows it"
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

    /// The first line and the body of each request a test server got.
    type RequestLog = Arc<Mutex<Vec<(String, String)>>>;

    /// A server on this computer that answers one connection after the
    /// other with the given JSON bodies and logs each request's first line
    /// and body, so the tests see what really went out.
    fn serve(answers: Vec<(u16, String)>) -> (String, RequestLog) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
        let address = format!(
            "http://127.0.0.1:{}/v1",
            listener.local_addr().unwrap().port()
        );
        let log = Arc::new(Mutex::new(Vec::new()));
        let seen = log.clone();
        std::thread::spawn(move || {
            for (status, body) in answers {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
                let mut raw = Vec::new();
                let mut buffer = [0u8; 4096];
                let mut wanted = None;
                while let Ok(n) = stream.read(&mut buffer) {
                    raw.extend_from_slice(&buffer[..n]);
                    let text = String::from_utf8_lossy(&raw).into_owned();
                    if let Some((head, rest)) = text.split_once("\r\n\r\n") {
                        let length = *wanted.get_or_insert_with(|| {
                            head.lines()
                                .find_map(|line| {
                                    line.to_ascii_lowercase()
                                        .strip_prefix("content-length:")
                                        .and_then(|v| v.trim().parse::<usize>().ok())
                                })
                                .unwrap_or(0)
                        });
                        if rest.len() >= length || n == 0 {
                            break;
                        }
                    } else if n == 0 {
                        break;
                    }
                }
                let text = String::from_utf8_lossy(&raw).into_owned();
                let line = text.lines().next().unwrap_or_default().to_owned();
                let request = text
                    .split_once("\r\n\r\n")
                    .map_or("", |(_, rest)| rest)
                    .to_owned();
                seen.lock().unwrap().push((line, request));
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\n\
                         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                );
            }
        });
        (address, log)
    }

    fn calls_ping() -> String {
        serde_json::json!({"choices": [{"finish_reason": "tool_calls", "message": {"tool_calls": [
            {"id": "c1", "type": "function", "function": {
                "name": kassensturz_ai::PROBE_FUNCTION, "arguments": "{\"word\":\"ping\"}"}}]}}]})
        .to_string()
    }

    fn says_words() -> String {
        r#"{"choices":[{"finish_reason":"stop","message":{"content":"I will."}}]}"#.to_owned()
    }

    #[test]
    fn the_test_says_what_the_model_can_do_and_keeps_it_for_that_model() {
        let models = r#"{"data":[{"id":"small","context_length":4096},{"id":"other"}]}"#;
        let (address, log) = serve(vec![(200, models.to_owned()), (200, calls_ping())]);
        let (_folder, _file, store) = on();
        let (_, mut keys) = keys();
        store.save(&mut keys, &address, "small", KeyUse::None, None);
        assert_eq!(store.form().capability, None, "nothing found yet");
        let found = store.test_connection(None, None).expect("a found result");
        assert_eq!(found.functions, Functions::Yes);
        assert_eq!(found.context_tokens, Some(4096));
        assert_eq!(found.models, ["small", "other"]);
        // What went out: the list, then exactly the request the screen shows.
        let sent = log.lock().unwrap().clone();
        assert_eq!(sent[0].0, "GET /v1/models HTTP/1.1");
        assert_eq!(sent[1].0, "POST /v1/chat/completions HTTP/1.1");
        assert_eq!(sent[1].1, store.capability_request());
        // Kept with the provider's settings, for this model only.
        let kept = store.form().capability.expect("kept");
        assert_eq!(kept.functions, Some(true));
        assert_eq!(kept.context_tokens, Some(4096));
        assert!(kept.context_small());
        store.save(&mut keys, &address, "other", KeyUse::None, None);
        assert_eq!(store.form().capability, None, "another model is untested");
        store.save(&mut keys, &address, "small", KeyUse::None, None);
        assert_eq!(
            store.form().capability,
            Some(kept),
            "back to the tested one"
        );
    }

    #[test]
    fn a_model_that_answers_in_words_is_a_no_and_a_later_test_replaces_the_result() {
        let models = r#"{"data":[{"id":"m"}]}"#;
        let (address, _log) = serve(vec![
            (200, models.to_owned()),
            (200, says_words()),
            (200, models.to_owned()),
            (200, calls_ping()),
        ]);
        let (_folder, _file, store) = on();
        let (_, mut keys) = keys();
        store.save(&mut keys, &address, "m", KeyUse::None, None);
        let found = store.test_connection(None, None).unwrap();
        assert_eq!(found.functions, Functions::No);
        assert_eq!(found.context_tokens, None, "not guessed");
        assert_eq!(store.form().capability.unwrap().functions, Some(false));
        store.test_connection(None, None).unwrap();
        assert_eq!(store.form().capability.unwrap().functions, Some(true));
    }

    #[test]
    fn a_failed_function_test_still_lists_the_models_and_is_not_kept_as_an_answer() {
        let models = r#"{"data":[{"id":"m"}]}"#;
        let (address, _log) = serve(vec![(200, models.to_owned()), (503, "{}".to_owned())]);
        let (_folder, _file, store) = on();
        let (_, mut keys) = keys();
        store.save(&mut keys, &address, "m", KeyUse::None, None);
        let found = store.test_connection(None, None).unwrap();
        assert_eq!(found.models, ["m"]);
        assert_eq!(
            found.functions,
            Functions::Failed(AiError::ProviderFailed { status: 503 })
        );
        assert_eq!(store.form().capability, None);
    }

    #[test]
    fn without_a_model_only_the_list_is_asked() {
        let (address, log) = serve(vec![(200, r#"{"data":[{"id":"m"}]}"#.to_owned())]);
        let (_folder, _file, store) = on();
        let (_, mut keys) = keys();
        store.save(&mut keys, &address, "", KeyUse::None, None);
        let found = store.test_connection(None, None).unwrap();
        assert_eq!(found.functions, Functions::NoModel);
        assert_eq!(log.lock().unwrap().len(), 1, "no chat request went out");
        assert_eq!(store.capability_request(), "");
    }

    #[test]
    fn nothing_is_asked_before_a_model_is_chosen_or_the_user_agreed() {
        // The list needs no consent (D-182); the function test does for a
        // provider on another computer (D-035), so `ask` must not even run.
        let never = || -> Result<FunctionCalling, AiError> { panic!("nothing may be sent") };
        assert_eq!(ask_functions(false, false, never), Functions::NoModel);
        assert_eq!(ask_functions(false, true, never), Functions::NoModel);
        assert_eq!(ask_functions(true, true, never), Functions::NoConsent);
        assert_eq!(
            ask_functions(true, false, || Ok(FunctionCalling::Yes)),
            Functions::Yes
        );
        assert_eq!(
            ask_functions(true, false, || Err(AiError::NoConsent)),
            Functions::NoConsent,
            "the client's own check of the consent is the same answer"
        );
    }

    #[test]
    fn the_fake_model_answers_the_function_test_without_connecting() {
        let (_folder, _file, store) = on();
        let (_, mut keys) = keys();
        store.save(
            &mut keys,
            kassensturz_ai::SCRIPTED_ADDRESS,
            SCRIPTED_MODEL,
            KeyUse::None,
            None,
        );
        let yes = store
            .test_connection(None, Some(DemoModel::Functions))
            .unwrap();
        assert_eq!(yes.functions, Functions::Yes);
        assert_eq!(yes.context_tokens, None);
        let no = store
            .test_connection(None, Some(DemoModel::NoFunctions))
            .unwrap();
        assert_eq!(no.functions, Functions::No);
        assert_eq!(store.form().capability.unwrap().functions, Some(false));
    }

    #[test]
    fn with_ai_off_nothing_is_asked_and_nothing_is_kept() {
        let folder = TestFolder::new();
        let store = Store::at(&folder.join("kassensturzrc"));
        assert_eq!(
            store.test_connection(None, Some(DemoModel::Functions)),
            Err(AiError::Off)
        );
        assert_eq!(store.form().capability, None);
    }
}
