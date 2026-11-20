// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! UI density plugins (#791 step 7, D-756, D-927): the user picks one as
//! the "Layout" in Settings › Look, the choice is kept as the plugin's ID
//! under `Look/Density` (like `Look/Theme`, D-924), and it applies at once.
//!
//! A plugin only turns on switches that KassenSturz defines; the QML reads
//! them from `Look`. No switch can hide a warning, an error or a question
//! before a change (`density_audit`).

use std::fs;
use std::path::Path;

use kassensturz_plugin_api::{Density, ExtensionPoint, Package};

use crate::settings::Settings;

/// The settings group and key of the chosen layout.
const GROUP: &str = "Look";
const KEY: &str = "Density";

/// A UI density plugin in the plugin folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    /// Its ID, which the settings keep.
    pub id: String,
    /// Its name, as its manifest has it.
    pub name: String,
    /// Its switches.
    pub density: Density,
}

/// Every usable UI density plugin in `folder`, sorted by name.
pub fn installed(folder: &Path) -> Vec<Installed> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<Installed> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            let bytes = crate::plugins::read(&entry.path()).ok()?;
            let package = Package::read(&bytes).ok()?;
            if !package
                .manifest
                .extension_points
                .contains(&ExtensionPoint::UiDensity)
            {
                return None;
            }
            Some(Installed {
                id: package.manifest.id.as_str().to_owned(),
                name: package.manifest.name,
                density: package.density?,
            })
        })
        .collect();
    found.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });
    found
}

/// The ID of the chosen layout, or `None` for KassenSturz's own.
pub fn chosen(settings: &Settings) -> Option<String> {
    Some(settings.text(GROUP, KEY)).filter(|id| !id.is_empty())
}

/// Chooses the layout plugin with `id`, or KassenSturz's own for `None`.
/// Returns whether it was saved.
pub fn choose(settings: &Settings, id: Option<&str>) -> bool {
    settings.set_text(GROUP, KEY, id.unwrap_or_default())
}

/// The switches of the layout `id` among `installed`; all off for
/// KassenSturz's own, or when that plugin is gone.
pub fn switches(installed: &[Installed], id: Option<&str>) -> Density {
    id.and_then(|id| installed.iter().find(|found| found.id == id))
        .map(|found| found.density)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::test_folder::TestFolder;
    use crate::test_plugins;

    fn compact() -> serde_json::Value {
        json!({"density_version": 1, "switches": {"compact_lists": true, "hide_hints": true}})
    }

    #[test]
    fn finds_the_density_plugins_in_the_folder() {
        let folder = TestFolder::new();
        fs::write(
            folder.join("b.wasm"),
            test_plugins::density_file("org.example.compact", "Compact", &compact()),
        )
        .unwrap();
        fs::write(
            folder.join("a.wasm"),
            test_plugins::density_file(
                "org.example.airy",
                "airy",
                &json!({"density_version": 1, "switches": {}}),
            ),
        )
        .unwrap();
        // A theme, an importer and a broken file: all left out.
        fs::write(
            folder.join("c.wasm"),
            test_plugins::theme_file(
                "org.example.green",
                "Green",
                &json!({
                    "theme_version": 1,
                    "light": {"background": "#ffffff", "text": "#202020", "accent": "#2e7d32"},
                    "dark": {"background": "#1b1b1b", "text": "#eeeeee", "accent": "#81c784"},
                }),
            ),
        )
        .unwrap();
        fs::write(
            folder.join("d.wasm"),
            test_plugins::file("org.example.reader", "Reader", &["importer"], "(module)"),
        )
        .unwrap();
        fs::write(folder.join("e.wasm"), b"not a plugin").unwrap();
        let found = installed(folder.path());
        let ids: Vec<&str> = found.iter().map(|found| found.id.as_str()).collect();
        assert_eq!(ids, ["org.example.airy", "org.example.compact"]);
        assert_eq!(found[1].name, "Compact");
        assert_eq!(
            switches(&found, Some("org.example.compact")),
            Density {
                compact_lists: true,
                hide_hints: true,
                compact_tiles: false,
            }
        );
        // KassenSturz's own, and a plugin that is gone, turn nothing on.
        assert_eq!(switches(&found, None), Density::default());
        assert_eq!(
            switches(&found, Some("org.example.gone")),
            Density::default()
        );
    }

    #[test]
    fn the_choice_is_kept_under_look_density() {
        let folder = TestFolder::new();
        let file = folder.join("kassensturzrc");
        let settings = Settings::at(&file);
        assert_eq!(chosen(&settings), None);
        assert!(choose(&settings, Some("org.example.compact")));
        assert_eq!(
            chosen(&Settings::at(&file)),
            Some("org.example.compact".into())
        );
        let written = fs::read_to_string(&file).unwrap();
        assert!(
            written.contains("[Look]\nDensity=org.example.compact"),
            "{written}"
        );
        assert!(choose(&settings, None));
        assert_eq!(chosen(&Settings::at(&file)), None);
    }
}
