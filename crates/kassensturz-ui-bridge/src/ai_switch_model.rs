// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The one AI switch (D-013, D-168) as a QML object.
//!
//! `enabled` is the single property every AI element in the screens hides
//! behind, so while it is false nothing AI-related is shown, not even grayed
//! out; only the toggle in Settings, which shows itself when `available`,
//! stays. A build without AI (D-039) is never `available` and never
//! `enabled`; the object exists so the screens are the same in both builds,
//! but it holds no AI code. The switch is kept in the app's settings file
//! for this computer, never in a book (D-168).

use core::pin::Pin;

use cxx_qt::CxxQtType;

use crate::settings::Settings;

#[cxx_qt::bridge]
pub mod qobject {
    extern "RustQt" {
        /// Whether AI features are switched on.
        #[qobject]
        #[qml_element]
        #[qproperty(bool, available, READ, CONSTANT)]
        #[qproperty(bool, enabled, READ, NOTIFY)]
        type AiSwitch = super::AiSwitchRust;
    }

    extern "RustQt" {
        /// Switches AI features on or off. Returns whether that was saved;
        /// when it wasn't, or when this build has no AI, the switch stays as
        /// it was.
        #[qinvokable]
        #[cxx_name = "switchTo"]
        fn switch_to(self: Pin<&mut AiSwitch>, on: bool) -> bool;
    }

    impl cxx_qt::Initialize for AiSwitch {}
}

/// The Rust side of [`qobject::AiSwitch`].
#[derive(Default)]
pub struct AiSwitchRust {
    available: bool,
    enabled: bool,
}

impl cxx_qt::Initialize for qobject::AiSwitch {
    fn initialize(mut self: Pin<&mut Self>) {
        let mut model = self.as_mut().rust_mut();
        model.available = cfg!(feature = "ai");
        model.enabled = is_on(&Settings::user());
    }
}

impl qobject::AiSwitch {
    fn switch_to(mut self: Pin<&mut Self>, on: bool) -> bool {
        if !cfg!(feature = "ai") || !write(&Settings::user(), on) {
            return false;
        }
        if self.enabled != on {
            if on {
                crate::note!(AiSwitchedOn);
            } else {
                crate::note!(AiSwitchedOff);
            }
            self.as_mut().rust_mut().enabled = on;
            self.as_mut().enabled_changed();
        }
        true
    }
}

/// Whether `settings` switch AI on; always false in a build without AI.
#[cfg(feature = "ai")]
fn is_on(settings: &Settings) -> bool {
    use kassensturz_core::ai_switch::{ENABLED_KEY, GROUP, ON};
    settings.text(GROUP, ENABLED_KEY) == ON
}

#[cfg(not(feature = "ai"))]
fn is_on(_settings: &Settings) -> bool {
    false
}

/// Keeps the switch in `settings`. Returns whether that was saved.
#[cfg(feature = "ai")]
fn write(settings: &Settings, on: bool) -> bool {
    use kassensturz_core::ai_switch::{ENABLED_KEY, GROUP, ON};
    settings.set_text(GROUP, ENABLED_KEY, if on { ON } else { "false" })
}

#[cfg(not(feature = "ai"))]
fn write(_settings: &Settings, _on: bool) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_folder::TestFolder;

    #[cfg(feature = "ai")]
    #[test]
    fn the_switch_is_off_until_written_and_programs_without_qt_read_it() {
        let folder = TestFolder::new();
        let file = folder.join("kassensturzrc");
        let settings = Settings::at(&file);
        assert!(!is_on(&settings));

        assert!(write(&settings, true));
        assert!(is_on(&Settings::at(&file)));
        // `kassensturz-mcp` reads the file KConfig wrote, with no Qt.
        let written = std::fs::read_to_string(&file).expect("settings file written");
        assert!(written.contains("[AI]\nEnabled=true"), "{written}");
        assert!(kassensturz_core::ai_switch::enabled_at(&file));

        assert!(write(&settings, false));
        assert!(!is_on(&settings));
        assert!(!kassensturz_core::ai_switch::enabled_at(&file));
    }

    #[cfg(not(feature = "ai"))]
    #[test]
    fn a_build_without_ai_is_never_on_and_saves_nothing() {
        let folder = TestFolder::new();
        let file = folder.join("kassensturzrc");
        let settings = Settings::at(&file);
        assert!(!write(&settings, true));
        assert!(!is_on(&settings));
        assert!(!file.exists());
    }
}
