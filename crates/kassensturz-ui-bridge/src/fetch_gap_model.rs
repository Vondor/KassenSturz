// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The limit of the fetch gap warning (M5.U, D-180) as a QML object, for the
//! Settings page. QML gets the choice as the word the settings file keeps
//! ("60", "off"); the page owns every label (D-034).

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;

use crate::fetch_gap::{self, CHOICES};
use crate::settings::Settings;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt's string type.
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        /// How long after the last fetch from the bank the overview warns.
        #[qobject]
        #[qml_element]
        #[qproperty(QString, choice, READ, NOTIFY)]
        type FetchGapSettings = super::FetchGapSettingsRust;
    }

    extern "RustQt" {
        /// Chooses the limit by its word, e.g. "90" or "off", for this
        /// computer from now on. Returns whether it was saved; a word the
        /// page doesn't offer is refused.
        #[qinvokable]
        fn choose(self: Pin<&mut FetchGapSettings>, choice: &QString) -> bool;
    }

    impl cxx_qt::Initialize for FetchGapSettings {}
}

/// The Rust side of [`qobject::FetchGapSettings`].
#[derive(Default)]
pub struct FetchGapSettingsRust {
    choice: QString,
}

impl cxx_qt::Initialize for qobject::FetchGapSettings {
    fn initialize(mut self: Pin<&mut Self>) {
        let shown = shown_choice(&Settings::user().fetch_gap_choice());
        self.as_mut().rust_mut().choice = QString::from(shown);
    }
}

impl qobject::FetchGapSettings {
    fn choose(mut self: Pin<&mut Self>, choice: &QString) -> bool {
        let word = choice.to_string();
        if !CHOICES.contains(&word.as_str()) {
            return false;
        }
        let saved = Settings::user().set_fetch_gap_choice(&word);
        // Shown as chosen even if it couldn't be saved; the page then says
        // that it holds only until KassenSturz closes.
        self.as_mut().rust_mut().choice = QString::from(word.as_str());
        self.as_mut().choice_changed();
        saved
    }
}

/// The choice a stored word stands for: the word if the page offers it, else
/// the default one, as `fetch_gap::limit_of` reads it.
fn shown_choice(stored: &str) -> &'static str {
    CHOICES
        .iter()
        .find(|choice| **choice == stored)
        .or_else(|| {
            CHOICES
                .iter()
                .find(|choice| choice.parse::<u32>().ok() == Some(fetch_gap::DEFAULT_DAYS))
        })
        .copied()
        .unwrap_or(fetch_gap::OFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_word_is_shown_as_the_choice_it_stands_for() {
        for choice in CHOICES {
            assert_eq!(shown_choice(choice), choice);
        }
        // What `limit_of` reads as the default shows as the default.
        let default = fetch_gap::DEFAULT_DAYS.to_string();
        for stored in ["", "7", "soon"] {
            assert_eq!(shown_choice(stored), default, "{stored:?}");
            assert_eq!(fetch_gap::limit_of(stored), fetch_gap::limit_of(&default));
        }
    }
}
