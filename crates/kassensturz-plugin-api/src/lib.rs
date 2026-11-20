// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The plugin API v1 (#791, D-007, D-753).
//!
//! This crate holds what a plugin and KassenSturz agree on before any plugin
//! code runs: the [`Manifest`] a plugin ships (D-847), the [`ApiVersion`] it
//! targets, the [`ExtensionPoint`]s it fills and the [`Permission`]s it asks
//! for (D-848), the plugin file that carries them (D-849), and the data of a
//! theme (D-925). It reads and
//! checks a plugin file and refuses anything it does not fully understand; it
//! runs no plugin. The host that loads and sandboxes the WASM modules is
//! `kassensturz-plugin-host` (D-850).
//!
//! A manifest is untrusted input (AGENTS.md §8): its texts are data that the
//! UI shows as plain text, never as markup or instructions.
//!
//! The API is v1 when it lands; changes during the Beta are a version step,
//! and it freezes at RC with the book format (D-753). From v1 on, a breaking
//! change is a human gate (AGENTS.md §3.2).

mod error;
mod manifest;
mod package;
mod permission;
mod theme;
mod version;

pub use error::PluginError;
pub use manifest::{
    MANIFEST_VERSION, MAX_DESCRIPTION_CHARS, MAX_MANIFEST_BYTES, MAX_NAME_CHARS, Manifest,
    PluginId, PluginVersion,
};
pub use package::{MANIFEST_SECTION, MAX_PLUGIN_BYTES, Package};
pub use permission::{ExtensionPoint, Permission};
pub use theme::{
    Colours, MAX_CORNER_RADIUS, MAX_FONT_CHARS, MAX_THEME_BYTES, MIN_CONTRAST, Rgb, THEME_SECTION,
    THEME_VERSION, Theme,
};
pub use version::{API_VERSION, ApiVersion};

#[cfg(test)]
mod package_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod theme_tests;
