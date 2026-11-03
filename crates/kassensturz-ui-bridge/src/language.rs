// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The language of the app (D-034, D-127): the system's by default, or the
//! one the user chose in Settings.
//!
//! The choice is kept with KConfig like the other settings of this computer
//! (D-112). KI18n picks its language from Qt, which reads the `LANGUAGE`
//! environment variable, but only when the system's own locale isn't the
//! plain "C" one; `LC_MESSAGES` is read in that case too. A choice is
//! therefore applied by setting both variables before Qt starts, which changes
//! only the language of the messages, not how dates and amounts are written.
//! KI18n can't switch while the app runs (see `Main.qml`), so a new choice
//! applies at the next start.

use std::sync::OnceLock;

use crate::number_format::Choice;
use crate::settings::Settings;

/// The group of the settings file that holds the language.
const GENERAL: &str = "General";

/// A language the user can choose.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    /// The language of the system.
    #[default]
    System,
    /// English, the language the strings are written in.
    English,
    /// German.
    German,
}

impl Choice for Language {
    const SETTING: &'static str = "Language";
    const CHOICES: &'static [(Self, &'static str)] = &[
        (Self::System, "system"),
        (Self::English, "en"),
        (Self::German, "de"),
    ];
}

impl Language {
    /// The language the user chose, from `settings`.
    pub fn read(settings: &Settings) -> Self {
        Self::from_key(&settings.text(GENERAL, Self::SETTING))
    }

    /// Keeps the choice in `settings`. Returns whether that was saved.
    pub fn write(self, settings: &Settings) -> bool {
        settings.set_text(GENERAL, Self::SETTING, self.key())
    }

    /// The code KI18n knows the language by; none for the system's.
    pub fn code(self) -> Option<&'static str> {
        match self {
            Self::System => None,
            _ => Some(self.key()),
        }
    }

    /// The language the screens are in, for the questions of the chat's
    /// buttons: the choice, or for the system's the language of its
    /// environment. `None` for any other language, so the model then
    /// answers in the language of the question (D-232).
    #[cfg(feature = "ai")]
    pub fn answer_language(self) -> Option<kassensturz_ai_chat::AnswerLanguage> {
        use kassensturz_ai_chat::AnswerLanguage;
        match self {
            Self::English => Some(AnswerLanguage::English),
            Self::German => Some(AnswerLanguage::German),
            Self::System => ["LC_ALL", "LC_MESSAGES", "LANGUAGE", "LANG"]
                .iter()
                .filter_map(|name| std::env::var(name).ok())
                .find(|value| !value.is_empty() && value != "C" && value != "POSIX")
                .and_then(|value| {
                    let code = value.to_ascii_lowercase();
                    if code.starts_with("de") {
                        Some(AnswerLanguage::German)
                    } else if code.starts_with("en") {
                        Some(AnswerLanguage::English)
                    } else {
                        None
                    }
                }),
        }
    }

    /// A locale in the language, for `LC_MESSAGES`; none for the system's.
    fn messages_locale(self) -> Option<&'static str> {
        match self {
            Self::System => None,
            Self::English => Some("en_US.UTF-8"),
            Self::German => Some("de_DE.UTF-8"),
        }
    }
}

/// The language that was in effect when the app started.
static AT_START: OnceLock<Language> = OnceLock::new();

/// Applies the user's choice of language, if there is one. It runs first in
/// `main`, before Qt starts, because KI18n reads `LANGUAGE` when it first
/// translates a string.
pub fn apply_at_start() {
    let language = Language::read(&Settings::user());
    // Set only here, once.
    let _ = AT_START.set(language);
    if let (Some(code), Some(locale)) = (language.code(), language.messages_locale()) {
        // SAFETY: nothing but `main`'s first statements runs yet, so no other
        // thread reads or writes the environment.
        unsafe {
            set_for_qt("LANGUAGE", code);
            set_for_qt("LC_MESSAGES", locale);
        }
    }
}

/// Sets an environment variable so that Qt and KI18n see it.
///
/// On Windows, Rust's `set_var` changes only the environment of Windows
/// itself, while Qt's `qgetenv`, which KI18n reads the language with, asks
/// the C runtime's copy of it, taken when the program started. So the choice
/// was saved but the app stayed in the system's language (M6-B151). The C
/// runtime's `_putenv_s` changes both, and Qt's libraries share that runtime
/// (`ucrtbase.dll`) with the program.
///
/// # Safety
///
/// As for `std::env::set_var`: no other thread may read or write the
/// environment at the same time.
unsafe fn set_for_qt(name: &str, value: &str) {
    // SAFETY: the caller guarantees that no other thread uses the environment.
    unsafe { std::env::set_var(name, value) };
    #[cfg(windows)]
    {
        use std::ffi::{CString, c_char, c_int};
        unsafe extern "C" {
            fn _putenv_s(name: *const c_char, value: *const c_char) -> c_int;
        }
        // Names and values here are fixed ASCII without NUL characters.
        if let (Ok(name), Ok(value)) = (CString::new(name), CString::new(value)) {
            // SAFETY: both are NUL-terminated strings that live for the
            // call; the caller guarantees that no other thread uses the
            // environment.
            unsafe { _putenv_s(name.as_ptr(), value.as_ptr()) };
        }
    }
}

/// The language in effect since the app started.
pub fn at_start() -> Language {
    AT_START.get().copied().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::test_folder::TestFolder;

    #[cfg(feature = "ai")]
    #[test]
    fn a_chosen_language_is_the_one_the_chat_buttons_ask_the_answer_in() {
        use kassensturz_ai_chat::AnswerLanguage;
        assert_eq!(
            Language::German.answer_language(),
            Some(AnswerLanguage::German)
        );
        assert_eq!(
            Language::English.answer_language(),
            Some(AnswerLanguage::English)
        );
    }

    #[test]
    fn keeps_the_choice_in_the_settings_file() {
        let folder = TestFolder::new();
        let file = folder.join("kassensturzrc");
        let settings = Settings::at(&file);
        assert_eq!(Language::read(&settings), Language::System);

        assert!(Language::German.write(&settings));
        assert_eq!(Language::read(&Settings::at(&file)), Language::German);
        let written = std::fs::read_to_string(&file).expect("settings file written");
        assert!(written.contains("[General]\nLanguage=de"), "{written}");
    }

    #[test]
    fn a_word_it_doesnt_know_reads_as_the_system_language() {
        let folder = TestFolder::new();
        let file = folder.join("kassensturzrc");
        let settings = Settings::at(&file);
        assert!(settings.set_text(GENERAL, "Language", "klingon"));
        assert_eq!(Language::read(&settings), Language::System);
    }

    /// What KI18n reads on Windows: the C runtime's copy of the
    /// environment, not Windows' own (M6-B151).
    #[cfg(windows)]
    #[test]
    fn the_c_runtime_sees_a_variable_set_for_qt() {
        use std::ffi::{CStr, CString, c_char};
        unsafe extern "C" {
            fn getenv(name: *const c_char) -> *const c_char;
        }
        let name = "KASSENSTURZ_LANGUAGE_TEST";
        // SAFETY: only this test uses this variable.
        unsafe { set_for_qt(name, "en") };
        let c_name = CString::new(name).expect("no NUL");
        // SAFETY: a NUL-terminated name; the result is read at once.
        let value = unsafe { getenv(c_name.as_ptr()) };
        assert!(!value.is_null(), "the C runtime doesn't see {name}");
        // SAFETY: getenv returned a NUL-terminated string.
        assert_eq!(unsafe { CStr::from_ptr(value) }.to_str(), Ok("en"));
    }

    #[test]
    fn only_a_chosen_language_has_a_code() {
        assert_eq!(Language::System.code(), None);
        assert_eq!(Language::English.code(), Some("en"));
        assert_eq!(Language::German.code(), Some("de"));
    }

    #[test]
    fn a_messages_locale_starts_with_its_code() {
        for (language, key) in Language::CHOICES {
            match language.messages_locale() {
                None => assert_eq!(language.code(), None),
                Some(locale) => assert!(locale.starts_with(&format!("{key}_")), "{locale}"),
            }
        }
    }

    /// Where steps meet: KI18n finds a language by its code in `po/`, so a
    /// language the user can choose needs a translation there (English is
    /// the source language and has none).
    #[test]
    fn every_language_to_choose_has_a_translation() {
        let po = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../po");
        for (language, key) in Language::CHOICES {
            if matches!(language, Language::System | Language::English) {
                continue;
            }
            assert!(
                po.join(key).join("kassensturz.po").is_file(),
                "no translation for {key}"
            );
        }
    }
}
