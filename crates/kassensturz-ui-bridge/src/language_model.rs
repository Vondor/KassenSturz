// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The language setting (D-127) as a QML object, for the Settings page.
//!
//! QML gets the choice as the word the settings file keeps for it, e.g.
//! "de", and whether it differs from the language in effect since the app
//! started, so the page can say that the change applies at the next start;
//! the page owns every label (D-034).

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;

use crate::language::{self, Language};
use crate::number_format::Choice;
use crate::settings::Settings;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt's string type.
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        /// The user's choice of language.
        #[qobject]
        #[qml_element]
        #[qproperty(QString, choice, READ, NOTIFY)]
        #[qproperty(bool, restart_needed, cxx_name = "restartNeeded", READ, NOTIFY)]
        type LanguageSettings = super::LanguageSettingsRust;
    }

    extern "RustQt" {
        /// Chooses `choice`, e.g. "de" or "system", from the next start on.
        /// Returns whether the choice was saved; a word that isn't a
        /// choice isn't.
        #[qinvokable]
        fn choose(self: Pin<&mut LanguageSettings>, choice: &QString) -> bool;
    }

    impl cxx_qt::Initialize for LanguageSettings {}
}

/// The Rust side of [`qobject::LanguageSettings`].
#[derive(Default)]
pub struct LanguageSettingsRust {
    choice: QString,
    restart_needed: bool,
}

impl cxx_qt::Initialize for qobject::LanguageSettings {
    fn initialize(self: Pin<&mut Self>) {
        self.show(Language::read(&Settings::user()));
    }
}

impl qobject::LanguageSettings {
    fn choose(self: Pin<&mut Self>, choice: &QString) -> bool {
        let choice = choice.to_string();
        if !Language::CHOICES.iter().any(|(_, key)| *key == choice) {
            return false;
        }
        let language = Language::from_key(&choice);
        let saved = language.write(&Settings::user());
        self.show(language);
        saved
    }

    /// Shows `language` as the choice.
    fn show(mut self: Pin<&mut Self>, language: Language) {
        let mut model = self.as_mut().rust_mut();
        model.choice = QString::from(language.key());
        model.restart_needed = language != language::at_start();
        self.as_mut().choice_changed();
        self.as_mut().restart_needed_changed();
    }
}
