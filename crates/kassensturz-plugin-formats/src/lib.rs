// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Plugin importers and exporters (#791 step 4, D-069, D-852): a plugin
//! with the `importer` or `exporter` extension point, run by the plugin
//! host, behind the same [`Importer`] and [`Exporter`] traits as the
//! built-in formats. Front ends hand them to `kassensturz_import::import`
//! and `kassensturz_export::export` like any other, so the limits and the
//! checks of what an importer returns hold for plugins too.
//!
//! What goes in and out is UTF-8 JSON with amounts as text in minor units,
//! never as floating point (AGENTS.md §3.1 rule 1); `docs/PLUGINS.md`
//! section 8 is the reference for plugin authors. Everything a plugin
//! answers is untrusted: it is read strictly, and anything this crate
//! can't use is refused as `plugin-host.output-invalid`.

mod exporter;
mod importer;
mod intern;
mod read;

pub use exporter::{EXPORTER_INFO, EXPORTER_WRITE, PluginExporter};
pub use importer::{IMPORTER_INFO, IMPORTER_READ, IMPORTER_RECOGNIZE, PluginImporter};

#[cfg(test)]
mod tests;
