// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The layout setting (#791 step 7, D-927) as a QML object, for Settings ›
//! Look, and the switches of the chosen layout for `Look`.
//!
//! QML gets the chosen layout's ID (empty for KassenSturz's own), its
//! switches, and the installed layout plugins; the page owns every label
//! (D-034), and a plugin's name is its own text, shown as plain text. A
//! choice applies at once.

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QVariant};

use crate::density::{self, Installed};
use crate::settings::Settings;
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
        /// The user's choice of layout.
        #[qobject]
        #[qml_element]
        #[qproperty(QString, choice, READ, NOTIFY)]
        #[qproperty(bool, compact_lists, cxx_name = "compactLists", READ, NOTIFY)]
        #[qproperty(bool, hide_hints, cxx_name = "hideHints", READ, NOTIFY)]
        #[qproperty(bool, compact_tiles, cxx_name = "compactTiles", READ, NOTIFY)]
        type DensitySettings = super::DensitySettingsRust;
    }

    extern "RustQt" {
        /// The layout plugins there are, sorted by name: each `{ id, name }`.
        #[qinvokable]
        fn layouts(self: &DensitySettings) -> QVariant;

        /// Chooses the layout plugin with `id`, or KassenSturz's own for an
        /// empty one, at once. Returns whether it was saved; an ID no
        /// installed layout plugin has isn't.
        #[qinvokable]
        fn choose(self: Pin<&mut DensitySettings>, id: &QString) -> bool;
    }

    impl cxx_qt::Initialize for DensitySettings {}
}

/// The Rust side of [`qobject::DensitySettings`].
#[derive(Default)]
pub struct DensitySettingsRust {
    choice: QString,
    compact_lists: bool,
    hide_hints: bool,
    compact_tiles: bool,
}

impl cxx_qt::Initialize for qobject::DensitySettings {
    fn initialize(self: Pin<&mut Self>) {
        self.show(density::chosen(&Settings::user()), &installed());
    }
}

/// The installed layout plugins, for QML.
fn layouts_object(installed: &[Installed]) -> QVariant {
    list(
        installed
            .iter()
            .map(|found| map([("id", text(&found.id)), ("name", text(&found.name))])),
    )
}

fn installed() -> Vec<Installed> {
    crate::plugins::folder()
        .map(|folder| density::installed(&folder))
        .unwrap_or_default()
}

impl qobject::DensitySettings {
    fn layouts(&self) -> QVariant {
        layouts_object(&installed())
    }

    fn choose(self: Pin<&mut Self>, id: &QString) -> bool {
        let id = id.to_string();
        let id = (!id.is_empty()).then_some(id);
        let installed = installed();
        if let Some(id) = &id
            && !installed.iter().any(|found| &found.id == id)
        {
            return false;
        }
        let saved = density::choose(&Settings::user(), id.as_deref());
        self.show(id, &installed);
        saved
    }

    /// Shows `id` as the choice and turns on its switches.
    fn show(mut self: Pin<&mut Self>, id: Option<String>, installed: &[Installed]) {
        let switches = density::switches(installed, id.as_deref());
        let mut model = self.as_mut().rust_mut();
        model.choice = QString::from(id.as_deref().unwrap_or_default());
        model.compact_lists = switches.compact_lists;
        model.hide_hints = switches.hide_hints;
        model.compact_tiles = switches.compact_tiles;
        self.as_mut().choice_changed();
        self.as_mut().compact_lists_changed();
        self.as_mut().hide_hints_changed();
        self.as_mut().compact_tiles_changed();
    }
}

#[cfg(test)]
mod tests {
    use kassensturz_plugin_api::Density;

    use super::*;
    use crate::variant::read_back::{elements, text_of};

    #[test]
    fn the_layouts_read_back_the_way_qml_reads_them() {
        let object = layouts_object(&[Installed {
            id: "org.example.compact".into(),
            name: "Compact".into(),
            density: Density::default(),
        }]);
        let items = elements(&object).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(text_of(&items[0], "id"), "org.example.compact");
        assert_eq!(text_of(&items[0], "name"), "Compact");
    }
}
