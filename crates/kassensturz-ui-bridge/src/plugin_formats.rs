// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The plugin importers and exporters the user added (#791 step 4b,
//! D-852), for the import and export pages. Each call loads them anew from
//! the plugins folder, so a plugin added or removed a moment ago counts,
//! and one that broke the last time starts fresh.

use std::path::Path;

use kassensturz_plugin_api::ExtensionPoint;
use kassensturz_plugin_formats::{PluginExporter, PluginImporter};

use crate::plugins;

/// The plugin importers, each with the plugin's name, sorted by name.
pub fn importers(ai_enabled: bool) -> Vec<(String, PluginImporter)> {
    plugins::folder()
        .map(|folder| importers_in(&folder, ai_enabled))
        .unwrap_or_default()
}

/// The plugin importers in `folder`.
pub fn importers_in(folder: &Path, ai_enabled: bool) -> Vec<(String, PluginImporter)> {
    plugins::loaded(folder, ExtensionPoint::Importer, ai_enabled)
        .into_iter()
        .filter_map(|(_, plugin)| {
            let name = plugin.manifest().name.clone();
            PluginImporter::new(plugin)
                .ok()
                .map(|importer| (name, importer))
        })
        .collect()
}

/// A plugin exporter as the export page offers it.
pub struct ExporterChoice {
    /// The plugin's file name in the plugins folder, which names it.
    pub file: String,
    /// The plugin's name, shown as plain text.
    pub name: String,
    /// The loaded plugin behind the `Exporter` trait.
    pub exporter: PluginExporter,
}

/// The plugin exporters, sorted by name.
pub fn exporters(ai_enabled: bool) -> Vec<ExporterChoice> {
    plugins::folder()
        .map(|folder| exporters_in(&folder, ai_enabled))
        .unwrap_or_default()
}

/// The plugin exporters in `folder`.
pub fn exporters_in(folder: &Path, ai_enabled: bool) -> Vec<ExporterChoice> {
    plugins::loaded(folder, ExtensionPoint::Exporter, ai_enabled)
        .into_iter()
        .filter_map(|(file, plugin)| {
            let name = plugin.manifest().name.clone();
            PluginExporter::new(plugin)
                .ok()
                .map(|exporter| ExporterChoice {
                    file,
                    name,
                    exporter,
                })
        })
        .collect()
}

/// The plugin exporter of the plugin file `file`.
pub fn exporter(file: &str, ai_enabled: bool) -> Option<ExporterChoice> {
    exporters(ai_enabled)
        .into_iter()
        .find(|choice| choice.file == file)
}
