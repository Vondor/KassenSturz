// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The theme setting (#791 step 6, D-924, D-925) as a QML object, for
//! Settings › Look.
//!
//! QML gets the chosen theme's ID (empty for KassenSturz's own look),
//! whether it differs from the one in effect since the start, the corner
//! radius in effect, and the installed themes with their colours as
//! `#rrggbb` text for the small previews; the page owns every label
//! (D-034), and a theme's name is the plugin's own text, shown as plain
//! text.

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QVariant};
use kassensturz_plugin_api::Colours;

use crate::settings::Settings;
use crate::theme::{self, Installed};
use crate::variant::{list, map, text};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt's string type.
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qvariant.h");
        /// Qt's variant type.
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// The user's choice of theme.
        #[qobject]
        #[qml_element]
        #[qproperty(QString, choice, READ, NOTIFY)]
        #[qproperty(bool, restart_needed, cxx_name = "restartNeeded", READ, NOTIFY)]
        #[qproperty(i32, corner_radius, cxx_name = "cornerRadius", READ, CONSTANT)]
        type ThemeSettings = super::ThemeSettingsRust;
    }

    extern "RustQt" {
        /// The theme plugins there are, sorted by name: each `{ id, name,
        /// light, dark }`, a look being `{ background, text, accent }` as
        /// `#rrggbb`.
        #[qinvokable]
        fn themes(self: &ThemeSettings) -> QVariant;

        /// Chooses the theme with `id`, or KassenSturz's own look for an
        /// empty one, from the next start on. Returns whether it was saved;
        /// an ID no installed theme has isn't.
        #[qinvokable]
        fn choose(self: Pin<&mut ThemeSettings>, id: &QString) -> bool;
    }

    impl cxx_qt::Initialize for ThemeSettings {}
}

/// The Rust side of [`qobject::ThemeSettings`].
pub struct ThemeSettingsRust {
    choice: QString,
    restart_needed: bool,
    /// The corner radius in effect, -1 for KassenSturz's own.
    corner_radius: i32,
}

impl Default for ThemeSettingsRust {
    fn default() -> Self {
        Self {
            choice: QString::default(),
            restart_needed: false,
            corner_radius: theme::in_effect().corner_radius.map_or(-1, i32::from),
        }
    }
}

impl cxx_qt::Initialize for qobject::ThemeSettings {
    fn initialize(self: Pin<&mut Self>) {
        self.show(theme::chosen(&Settings::user()));
    }
}

/// The installed themes, for QML.
fn themes_object(themes: &[Installed]) -> QVariant {
    let look = |colours: &Colours| {
        map([
            ("background", text(&colours.background.hex())),
            ("text", text(&colours.text.hex())),
            ("accent", text(&colours.accent.hex())),
        ])
    };
    list(themes.iter().map(|installed| {
        map([
            ("id", text(&installed.id)),
            ("name", text(&installed.name)),
            ("light", look(&installed.theme.light)),
            ("dark", look(&installed.theme.dark)),
        ])
    }))
}

fn installed() -> Vec<Installed> {
    crate::plugins::folder()
        .map(|folder| theme::installed(&folder))
        .unwrap_or_default()
}

impl qobject::ThemeSettings {
    fn themes(&self) -> QVariant {
        themes_object(&installed())
    }

    fn choose(self: Pin<&mut Self>, id: &QString) -> bool {
        let id = id.to_string();
        let id = (!id.is_empty()).then_some(id);
        if let Some(id) = &id
            && !installed().iter().any(|theme| &theme.id == id)
        {
            return false;
        }
        let saved = theme::choose(&Settings::user(), id.as_deref());
        self.show(id);
        saved
    }

    /// Shows `id` as the choice.
    fn show(mut self: Pin<&mut Self>, id: Option<String>) {
        let restart_needed = id != theme::in_effect().id;
        let mut model = self.as_mut().rust_mut();
        model.choice = QString::from(id.as_deref().unwrap_or_default());
        model.restart_needed = restart_needed;
        self.as_mut().choice_changed();
        self.as_mut().restart_needed_changed();
    }
}

#[cfg(test)]
mod tests {
    use kassensturz_plugin_api::Theme;

    use super::*;
    use crate::variant::read_back::{elements, object_of, text_of};

    #[test]
    fn the_themes_read_back_the_way_qml_reads_them() {
        let theme = Theme::parse(
            br##"{"theme_version": 1,
                "light": {"background": "#F4FAF4", "text": "#1B1F1B", "accent": "#2E7D32"},
                "dark": {"background": "#121A13", "text": "#E3EDE3", "accent": "#81C784"}}"##,
        )
        .unwrap();
        let object = themes_object(&[Installed {
            id: "org.example.green".into(),
            name: "Green".into(),
            theme,
        }]);
        let items = elements(&object).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(text_of(&items[0], "id"), "org.example.green");
        assert_eq!(text_of(&items[0], "name"), "Green");
        let light = object_of(&items[0], "light");
        assert_eq!(text_of(&light, "background"), "#f4faf4");
        assert_eq!(text_of(&light, "text"), "#1b1f1b");
        assert_eq!(text_of(&light, "accent"), "#2e7d32");
        assert_eq!(text_of(&object_of(&items[0], "dark"), "accent"), "#81c784");
    }
}
