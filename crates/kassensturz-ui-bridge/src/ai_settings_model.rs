// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The AI settings and consent screens' object (M5.7, D-202 to D-205).
//!
//! Like the switch (`ai_switch_model.rs`), the object exists in every build
//! so the screens are the same, but in a build without AI (D-039) it holds
//! no AI code, is never `available` and answers every call with `ai.off`.
//! The screens that use it are hidden while the AI switch is off (D-013).
//! Everything reaches QML as text and plain objects (D-065); the screens
//! own every sentence (D-034). A key goes in once, from `SecretField`, and
//! nothing here ever hands it back.

use core::pin::Pin;

use cxx_qt::CxxQtType;
#[cfg(feature = "ai")]
use cxx_qt::Threading;
use cxx_qt_lib::{QString, QVariant};

use crate::errors::ErrorText;
use crate::variant::error_object;
#[cfg(feature = "ai")]
use crate::variant::{list, map, text};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// The provider the AI features talk to, how its key is kept, and
        /// the user's consent to what is sent to it.
        #[qobject]
        #[qml_element]
        #[qproperty(bool, available, READ, CONSTANT)]
        #[qproperty(bool, fake, READ, CONSTANT)]
        #[qproperty(bool, testing, READ, NOTIFY)]
        type AiSettings = super::AiSettingsRust;
    }

    impl cxx_qt::Threading for AiSettings {}

    extern "RustQt" {
        /// What the form shows: `address`, `model`, `keyUse` (`none`,
        /// `ask`, `remember` or `save`), `place` (`this`, `other` or empty
        /// when there is no usable address), `addressError` (a code or
        /// empty), `consent` (`notNeeded`, `given` or `missing`) and
        /// `consentDay` (the day agreed, as the user's locale writes it).
        #[qinvokable]
        fn load(self: &AiSettings) -> QVariant;

        /// Checks an address while it is typed: an object with `place`
        /// (`this` or `other`), or an error object.
        #[qinvokable]
        fn check(self: &AiSettings, address: &QString) -> QVariant;

        /// Saves the provider. `key` is what was typed into the key field,
        /// empty to leave the kept key as it is. Answers `saved`; with
        /// `saved` false and a `code`, the settings as typed can't be used;
        /// with `saved` true, `place`, `consentNeeded` and `keyError` (a
        /// code, empty when the key was kept as asked).
        #[qinvokable]
        fn save(
            self: &AiSettings,
            address: &QString,
            model: &QString,
            key_use: &QString,
            key: &QString,
        ) -> QVariant;

        /// Removes the kept key of the saved provider: an object with
        /// `ok`, or an error object.
        #[qinvokable]
        #[cxx_name = "forgetKey"]
        fn forget_key(self: &AiSettings) -> QVariant;

        /// The words of what the consent statement covers, in order, e.g.
        /// `questions`. The screen has one sentence for each.
        #[qinvokable]
        fn statement(self: &AiSettings) -> QVariant;

        /// An example of the first request of a question, with `phrase`
        /// left out of it: an object with `text` and `replaced`, or an
        /// error object. Nothing is sent.
        #[qinvokable]
        fn sample(self: &AiSettings, phrase: &QString) -> QVariant;

        /// The user agrees to what the statement names, for the saved
        /// provider. Returns whether that was saved.
        #[qinvokable]
        #[cxx_name = "giveConsent"]
        fn give_consent(self: &AiSettings) -> bool;

        /// The user takes the consent back. Returns whether that was saved.
        #[qinvokable]
        #[cxx_name = "withdrawConsent"]
        fn withdraw_consent(self: &AiSettings) -> bool;

        /// Asks the saved provider which models it offers, which also shows
        /// that the address and the key work. `key` is a key typed for this
        /// test, empty to use the kept one. The answer comes with
        /// `testFinished`.
        #[qinvokable]
        #[cxx_name = "testConnection"]
        fn test_connection(self: Pin<&mut AiSettings>, key: &QString);

        /// Whether the user's own AI tools may prepare changes through
        /// `kassensturz-mcp`, which the user then confirms in the window
        /// (D-035, D-219). Off until the user turns it on.
        #[qinvokable]
        #[cxx_name = "writeTools"]
        fn write_tools(self: &AiSettings) -> bool;

        /// Turns the write tools on or off. Returns whether that was
        /// saved.
        #[qinvokable]
        #[cxx_name = "setWriteTools"]
        fn set_write_tools(self: &AiSettings, on: bool) -> bool;

        /// The test ended: an object with `models` (a list of names) and
        /// `modelOffered` (whether the saved model is one of them), or an
        /// error object.
        #[qsignal]
        #[cxx_name = "testFinished"]
        fn test_finished(self: Pin<&mut AiSettings>, result: QVariant);
    }

    impl cxx_qt::Initialize for AiSettings {}
}

/// The Rust side of [`qobject::AiSettings`].
#[derive(Default)]
pub struct AiSettingsRust {
    available: bool,
    fake: bool,
    testing: bool,
}

impl cxx_qt::Initialize for qobject::AiSettings {
    fn initialize(mut self: Pin<&mut Self>) {
        let mut state = self.as_mut().rust_mut();
        state.available = cfg!(feature = "ai");
        state.fake = is_fake();
    }
}

#[cfg(feature = "ai")]
fn is_fake() -> bool {
    crate::ai_settings::fake_model()
}

#[cfg(not(feature = "ai"))]
fn is_fake() -> bool {
    false
}

/// The most model names handed to the screen; a provider can list hundreds.
#[cfg(feature = "ai")]
const MAX_MODELS: usize = 100;

/// The calls of the screens in a build without AI: all answer `ai.off`.
#[cfg(not(feature = "ai"))]
mod engine {
    use super::*;

    fn off() -> QVariant {
        error_object(&ErrorText::plain("ai.off"))
    }

    pub(super) fn load() -> QVariant {
        off()
    }

    pub(super) fn check(_address: &str) -> QVariant {
        off()
    }

    pub(super) fn save(_address: &str, _model: &str, _key_use: &str, _key: &str) -> QVariant {
        off()
    }

    pub(super) fn forget_key() -> QVariant {
        off()
    }

    pub(super) fn statement() -> QVariant {
        off()
    }

    pub(super) fn sample(_phrase: &str) -> QVariant {
        off()
    }

    pub(super) fn give_consent() -> bool {
        false
    }

    pub(super) fn withdraw_consent() -> bool {
        false
    }

    /// Reports `ai.off` at once; there is nothing to wait for.
    pub(super) fn test(qobject: Pin<&mut qobject::AiSettings>, _key: &str) {
        qobject.test_finished(off());
    }
}

#[cfg(feature = "ai")]
mod engine {
    use kassensturz_ai::{AiError, KeyUse, Place, ProviderKeys, SCRIPTED_ADDRESS, SCRIPTED_MODEL};
    use kassensturz_core::{Date, Report};
    use kassensturz_platform::Secret;

    use super::*;
    use crate::ai_settings::{self, Consent, Saved, Store, with_keys};
    use crate::format::Locale;
    use crate::number_format::app_locale;

    /// An AI error as the screens get it: its code, and its parameters
    /// (the status of a provider's error) as text.
    pub(super) fn ai_error(error: &AiError) -> ErrorText {
        let report = error.report();
        ErrorText {
            code: report.code,
            params: report
                .params
                .iter()
                .map(|(name, param)| {
                    let text = match param {
                        kassensturz_core::ErrorParam::Number(number) => number.to_string(),
                        kassensturz_core::ErrorParam::Text(text) => text.clone(),
                        other => format!("{other:?}"),
                    };
                    (*name, text)
                })
                .collect(),
        }
    }

    pub(super) fn place_word(place: Place) -> &'static str {
        match place {
            Place::ThisComputer => "this",
            Place::AnotherComputer => "other",
        }
    }

    pub(super) fn load() -> QVariant {
        load_in(&Store::user(), ai_settings::fake_model())
    }

    pub(super) fn check(address: &str) -> QVariant {
        match Store::check_address(address) {
            Ok(place) => map([("place", text(place_word(place)))]),
            Err(error) => error_object(&ai_error(&error)),
        }
    }

    pub(super) fn save(address: &str, model: &str, key_use: &str, key: &str) -> QVariant {
        with_keys(|keys| save_in(&Store::user(), keys, address, model, key_use, key))
    }

    pub(super) fn forget_key() -> QVariant {
        with_keys(|keys| forget_key_in(&Store::user(), keys))
    }

    pub(super) fn give_consent() -> bool {
        let given = Store::user().give_consent(app_locale().today());
        if given {
            crate::note!(ConsentGiven);
        }
        given
    }

    pub(super) fn withdraw_consent() -> bool {
        let withdrawn = Store::user().withdraw_consent();
        if withdrawn {
            crate::note!(ConsentTakenBack);
        }
        withdrawn
    }

    pub(super) fn statement() -> QVariant {
        list(ai_settings::statement_words().into_iter().map(text))
    }

    pub(super) fn sample(phrase: &str) -> QVariant {
        sample_in(&Store::user(), phrase, app_locale().today())
    }

    /// What the form shows, as an object. `fake` is whether the fake model
    /// is on, which adds the address and model its button fills in.
    pub(super) fn load_in(store: &Store, fake: bool) -> QVariant {
        let form = store.form();
        let (consent, day) = match &form.consent {
            Consent::NotNeeded => ("notNeeded", String::new()),
            Consent::Missing => ("missing", String::new()),
            Consent::Given(day) => ("given", app_locale().short_date(*day)),
        };
        let (test_address, test_model) = if fake {
            (SCRIPTED_ADDRESS, SCRIPTED_MODEL)
        } else {
            ("", "")
        };
        map([
            ("testAddress", text(test_address)),
            ("testModel", text(test_model)),
            ("address", text(&form.address)),
            ("model", text(&form.model)),
            ("keyUse", text(form.key_use.as_word())),
            ("place", text(form.place.map_or("", place_word))),
            (
                "addressError",
                text(form.address_error.as_ref().map_or("", |e| e.report().code)),
            ),
            ("consent", text(consent)),
            ("consentDay", text(&day)),
        ])
    }

    pub(super) fn save_in(
        store: &Store,
        keys: &mut ProviderKeys,
        address: &str,
        model: &str,
        key_use: &str,
        key: &str,
    ) -> QVariant {
        let key = (!key.is_empty()).then(|| Secret::new(key.to_owned()));
        match store.save(keys, address, model, KeyUse::from_word(key_use), key) {
            Saved::Refused(error) => {
                let error = ai_error(&error);
                crate::note!(ProviderRefused, None, Some(error.code));
                map([("saved", QVariant::from(&false))]
                    .into_iter()
                    .chain(error_fields(&error)))
            }
            Saved::NotWritten => map([("saved", QVariant::from(&false))]),
            Saved::Written {
                place,
                consent_needed,
                key_error,
            } => {
                crate::note!(ProviderSaved);
                map([
                    ("saved", QVariant::from(&true)),
                    ("place", text(place_word(place))),
                    ("consentNeeded", QVariant::from(&consent_needed)),
                    (
                        "keyError",
                        text(key_error.as_ref().map_or("", |e| e.report().code)),
                    ),
                ])
            }
        }
    }

    fn error_fields(error: &ErrorText) -> Vec<(&'static str, QVariant)> {
        std::iter::once(("code", text(error.code)))
            .chain(
                error
                    .params
                    .iter()
                    .map(|(name, value)| (*name, text(value))),
            )
            .collect()
    }

    pub(super) fn forget_key_in(store: &Store, keys: &mut ProviderKeys) -> QVariant {
        match store.forget_key(keys) {
            Ok(()) => map([("ok", QVariant::from(&true))]),
            Err(error) => error_object(&ai_error(&error)),
        }
    }

    pub(super) fn sample_in(store: &Store, phrase: &str, today: Date) -> QVariant {
        match store.sample(phrase, today) {
            Ok((request, replaced)) => map([
                ("text", text(&request)),
                ("replaced", text(&replaced.to_string())),
            ]),
            Err(error) => error_object(&ai_error(&error)),
        }
    }

    /// What a test found, as an object: the models (at most
    /// [`MAX_MODELS`], sorted) and whether `model` is one of them, or the
    /// error.
    pub(super) fn test_object(result: Result<Vec<String>, AiError>, model: &str) -> QVariant {
        match result {
            Ok(mut models) => {
                let offered = models.iter().any(|name| name == model);
                models.sort();
                map([
                    (
                        "models",
                        list(models.iter().take(MAX_MODELS).map(|name| text(name))),
                    ),
                    ("modelOffered", QVariant::from(&offered)),
                ])
            }
            Err(error) => error_object(&ai_error(&error)),
        }
    }

    /// Starts the test on a thread of its own, since a provider can take
    /// seconds to answer; its answer is queued back to the window thread.
    pub(super) fn test(mut qobject: Pin<&mut qobject::AiSettings>, typed: &str) {
        qobject.as_mut().rust_mut().testing = true;
        qobject.as_mut().testing_changed();
        let thread = qobject.qt_thread();
        let typed = (!typed.is_empty()).then(|| Secret::new(typed.to_owned()));
        std::thread::spawn(move || {
            let store = Store::user();
            // The key is looked up under the lock, the provider is asked
            // without it, so saving elsewhere never waits for a provider.
            let result = with_keys(|keys| store.key_for(keys, typed))
                .and_then(|key| store.test_connection(key, ai_settings::fake_model()));
            let model = store
                .config()
                .read()
                .provider()
                .map(|provider| provider.model.clone())
                .unwrap_or_default();
            match &result {
                Ok(_) => crate::note!(ModelsListed),
                Err(error) => crate::note!(ModelsFailed, None, Some(ai_error(error).code)),
            }
            let answer = test_object(result, &model);
            // A window that is closing has no one left to tell.
            let _ = thread.queue(move |mut qobject| {
                qobject.as_mut().rust_mut().testing = false;
                qobject.as_mut().testing_changed();
                qobject.test_finished(answer);
            });
        });
    }
}

impl qobject::AiSettings {
    fn load(&self) -> QVariant {
        engine::load()
    }

    fn check(&self, address: &QString) -> QVariant {
        engine::check(&address.to_string())
    }

    fn save(
        &self,
        address: &QString,
        model: &QString,
        key_use: &QString,
        key: &QString,
    ) -> QVariant {
        engine::save(
            &address.to_string(),
            &model.to_string(),
            &key_use.to_string(),
            &key.to_string(),
        )
    }

    fn forget_key(&self) -> QVariant {
        engine::forget_key()
    }

    fn statement(&self) -> QVariant {
        engine::statement()
    }

    fn sample(&self, phrase: &QString) -> QVariant {
        engine::sample(&phrase.to_string())
    }

    fn give_consent(&self) -> bool {
        engine::give_consent()
    }

    fn withdraw_consent(&self) -> bool {
        engine::withdraw_consent()
    }

    fn test_connection(self: Pin<&mut Self>, key: &QString) {
        if self.testing {
            return;
        }
        engine::test(self, &key.to_string());
    }

    fn write_tools(&self) -> bool {
        write_tools_in(&crate::settings::Settings::user())
    }

    fn set_write_tools(&self, on: bool) -> bool {
        keep_write_tools(&crate::settings::Settings::user(), on)
    }
}

/// Whether `settings` turn the MCP write tools on; always false in a build
/// without AI. AI itself is the other switch (D-013): the MCP server and
/// the window read both.
#[cfg(feature = "ai")]
fn write_tools_in(settings: &crate::settings::Settings) -> bool {
    use kassensturz_core::ai_switch::{GROUP, ON, WRITE_TOOLS_KEY};
    settings.text(GROUP, WRITE_TOOLS_KEY) == ON
}

#[cfg(not(feature = "ai"))]
fn write_tools_in(_settings: &crate::settings::Settings) -> bool {
    false
}

/// Keeps the write tools' toggle in `settings`. Returns whether that was
/// saved.
#[cfg(feature = "ai")]
fn keep_write_tools(settings: &crate::settings::Settings, on: bool) -> bool {
    use kassensturz_core::ai_switch::{GROUP, ON, WRITE_TOOLS_KEY};
    settings.set_text(GROUP, WRITE_TOOLS_KEY, if on { ON } else { "false" })
}

#[cfg(not(feature = "ai"))]
fn keep_write_tools(_settings: &crate::settings::Settings, _on: bool) -> bool {
    false
}

#[cfg(test)]
mod write_tools_tests {
    use super::*;
    use crate::settings::Settings;
    use crate::test_folder::TestFolder;

    #[cfg(feature = "ai")]
    #[test]
    fn the_write_tools_are_off_until_turned_on_and_the_mcp_server_reads_them() {
        use kassensturz_core::ai_switch::write_tools_in as server_reads;
        let folder = TestFolder::new();
        let file = folder.join("kassensturzrc");
        let settings = Settings::at(&file);
        assert!(!write_tools_in(&settings));
        assert!(settings.set_text("AI", "Enabled", "true"));

        assert!(keep_write_tools(&settings, true));
        assert!(write_tools_in(&Settings::at(&file)));
        // `kassensturz-mcp` and the window's check read the file KConfig
        // wrote, with no Qt.
        let written = std::fs::read_to_string(&file).expect("settings file written");
        assert!(server_reads(&written), "{written}");

        assert!(keep_write_tools(&settings, false));
        assert!(!write_tools_in(&settings));
        let written = std::fs::read_to_string(&file).expect("settings file written");
        assert!(!server_reads(&written), "{written}");
    }

    #[cfg(not(feature = "ai"))]
    #[test]
    fn a_build_without_ai_never_turns_the_write_tools_on() {
        let folder = TestFolder::new();
        let file = folder.join("kassensturzrc");
        let settings = Settings::at(&file);
        assert!(!keep_write_tools(&settings, true));
        assert!(!write_tools_in(&settings));
        assert!(!file.exists());
    }
}

#[cfg(all(test, feature = "ai"))]
mod tests {
    use std::sync::Arc;

    use cxx_qt_lib::QVariant;
    use kassensturz_ai::{AiError, ProviderKeys};
    use kassensturz_core::{Date, ai_switch};
    use kassensturz_platform::{Keychain, KeychainError, MemoryKeychain, Secret, SecretKey};

    use super::engine::{forget_key_in, load_in, sample_in, save_in, statement, test_object};
    use crate::ai_settings::Store;
    use crate::test_folder::TestFolder;
    use crate::variant::read_back::{elements, flag_of, keys, list_of, object, text_of};

    const TODAY: Date = Date::constant(2026, 10, 25);

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

    fn setup() -> (TestFolder, Store, Arc<MemoryKeychain>, ProviderKeys) {
        let folder = TestFolder::new();
        let store = Store::at(&folder.join("kassensturzrc"));
        let memory = Arc::new(MemoryKeychain::new());
        let keys = ProviderKeys::new(Some(Box::new(Shared(memory.clone()))));
        (folder, store, memory, keys)
    }

    /// Every text in an object, the way QML would read each property.
    fn all_texts(value: &QVariant) -> Vec<String> {
        keys(value)
            .into_iter()
            .filter_map(|key| {
                object(value)
                    .get(&cxx_qt_lib::QString::from(key.as_str()))
                    .and_then(|v| v.value::<cxx_qt_lib::QString>())
                    .map(|t| t.to_string())
            })
            .collect()
    }

    #[test]
    fn the_form_reads_back_as_qml_reads_it() {
        let (_folder, store, _memory, mut kept) = setup();
        let empty = load_in(&store, false);
        assert_eq!(
            keys(&empty),
            [
                "address",
                "addressError",
                "consent",
                "consentDay",
                "keyUse",
                "model",
                "place",
                "testAddress",
                "testModel"
            ]
        );
        assert_eq!(text_of(&empty, "address"), "");
        assert_eq!(text_of(&empty, "place"), "");
        assert_eq!(text_of(&empty, "consent"), "notNeeded");
        assert_eq!(text_of(&empty, "keyUse"), "none");
        assert_eq!(text_of(&empty, "testAddress"), "");

        save_in(
            &store,
            &mut kept,
            "https://api.example.com/v1",
            "big",
            "save",
            "",
        );
        let form = load_in(&store, true);
        assert_eq!(text_of(&form, "address"), "https://api.example.com/v1");
        assert_eq!(text_of(&form, "model"), "big");
        assert_eq!(text_of(&form, "keyUse"), "save");
        assert_eq!(text_of(&form, "place"), "other");
        assert_eq!(text_of(&form, "consent"), "missing");
        assert_eq!(text_of(&form, "consentDay"), "");
        assert_eq!(text_of(&form, "testModel"), "kassensturz-scripted-model");
        assert!(text_of(&form, "testAddress").starts_with("http://localhost"));

        assert!(store.give_consent(TODAY));
        let form = load_in(&store, false);
        assert_eq!(text_of(&form, "consent"), "given");
        assert!(
            text_of(&form, "consentDay").contains("25"),
            "the day, as the locale writes it"
        );

        // An address that was saved by hand and can't be used.
        assert!(store.put(kassensturz_ai::ADDRESS_KEY, "http://192.168.1.2:1/v1"));
        let odd = load_in(&store, false);
        assert_eq!(text_of(&odd, "addressError"), "ai.plain-http");
        assert_eq!(text_of(&odd, "place"), "");
    }

    #[test]
    fn saving_reads_back_as_qml_reads_it_and_hands_no_key_back() {
        let (_folder, store, memory, mut kept) = setup();
        let key = "test-key-ui-9";
        let done = save_in(
            &store,
            &mut kept,
            "https://api.example.com/v1",
            "big",
            "save",
            key,
        );
        assert_eq!(keys(&done), ["consentNeeded", "keyError", "place", "saved"]);
        assert!(flag_of(&done, "saved"));
        assert!(flag_of(&done, "consentNeeded"));
        assert_eq!(text_of(&done, "place"), "other");
        assert_eq!(text_of(&done, "keyError"), "");
        assert_eq!(memory.len(), 1);

        let refused = save_in(
            &store,
            &mut kept,
            "http://192.168.1.2:1/v1",
            "big",
            "none",
            "",
        );
        assert_eq!(keys(&refused), ["code", "saved"]);
        assert!(!flag_of(&refused, "saved"));
        assert_eq!(text_of(&refused, "code"), "ai.plain-http");

        memory.fail_with(Some(KeychainError::Unavailable));
        let failed = save_in(
            &store,
            &mut kept,
            "https://api.example.com/v1",
            "big",
            "save",
            key,
        );
        assert!(flag_of(&failed, "saved"));
        assert_eq!(text_of(&failed, "keyError"), "ai.keychain");
        memory.fail_with(None);

        // No object the screens get holds the key.
        let everything = [
            done,
            refused,
            failed,
            load_in(&store, true),
            forget_key_in(&store, &mut kept),
            sample_in(&store, "", TODAY),
            test_object(Err(AiError::KeyRejected), "big"),
        ];
        for object in &everything {
            for text in all_texts(object) {
                assert!(!text.contains(key), "{text}");
            }
        }
        let file = std::fs::read_to_string(_folder.join("kassensturzrc")).unwrap();
        assert!(!file.contains(key));
    }

    #[test]
    fn removing_a_key_and_the_other_answers_read_back() {
        let (_folder, store, memory, mut kept) = setup();
        save_in(
            &store,
            &mut kept,
            "https://api.example.com/v1",
            "big",
            "save",
            "test-key-ui-10",
        );
        let removed = forget_key_in(&store, &mut kept);
        assert!(flag_of(&removed, "ok"));
        assert_eq!(memory.len(), 0);

        let empty = Store::at(&_folder.join("nothing"));
        let none = forget_key_in(&empty, &mut kept);
        assert_eq!(text_of(&none, "code"), "ai.no-provider");

        let words: Vec<String> = elements(&statement())
            .expect("a list")
            .iter()
            .map(|word| word.value::<cxx_qt_lib::QString>().unwrap().to_string())
            .collect();
        assert_eq!(words, ["questions", "book-data", "instructions", "key"]);
    }

    #[test]
    fn the_example_request_and_the_test_result_read_back() {
        let (_folder, store, _memory, mut kept) = setup();
        assert!(store.put(ai_switch::ENABLED_KEY, ai_switch::ON));
        let off = sample_in(&Store::at(&_folder.join("nothing")), "", TODAY);
        assert_eq!(text_of(&off, "code"), "ai.off");

        save_in(
            &store,
            &mut kept,
            "https://api.example.com/v1",
            "big",
            "none",
            "",
        );
        let example = sample_in(&store, "", TODAY);
        assert_eq!(keys(&example), ["replaced", "text"]);
        assert_eq!(text_of(&example, "replaced"), "0");
        assert!(text_of(&example, "text").contains("How much did I spend on food"));
        let redacted = sample_in(&store, "food", TODAY);
        assert!(text_of(&redacted, "replaced").parse::<u32>().unwrap() >= 1);
        assert!(!text_of(&redacted, "text").contains("food"));

        let found = test_object(Ok(vec!["b".into(), "big".into(), "a".into()]), "big");
        assert_eq!(keys(&found), ["modelOffered", "models"]);
        assert!(flag_of(&found, "modelOffered"));
        let models: Vec<String> = list_of(&found, "models")
            .iter()
            .map(|name| name.value::<cxx_qt_lib::QString>().unwrap().to_string())
            .collect();
        assert_eq!(models, ["a", "b", "big"], "sorted");
        let missing = test_object(Ok(vec!["a".into()]), "big");
        assert!(!flag_of(&missing, "modelOffered"));

        let many: Vec<String> = (0..300).map(|n| format!("model-{n:03}")).collect();
        assert_eq!(list_of(&test_object(Ok(many), "x"), "models").len(), 100);

        let failed = test_object(Err(AiError::ProviderFailed { status: 503 }), "big");
        assert_eq!(text_of(&failed, "code"), "ai.provider-failed");
        assert_eq!(text_of(&failed, "status"), "503");
        let plain = test_object(Err(AiError::Unreachable), "big");
        assert_eq!(keys(&plain), ["code"]);
    }
}
