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
            std::env::set_var("LANGUAGE", code);
            std::env::set_var("LC_MESSAGES", locale);
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
