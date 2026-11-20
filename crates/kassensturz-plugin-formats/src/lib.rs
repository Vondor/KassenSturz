// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Plugin importers and exporters (#791 step 4, D-069, D-852), reports
//! and automations (step 5, D-926), and AI provider translators (step 8,
//! D-928, with the `ai` feature). An importer or exporter is a plugin
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

#[cfg(feature = "ai")]
mod ai_provider;
mod automation;
mod exporter;
mod importer;
mod intern;
mod read;
mod report;

#[cfg(feature = "ai")]
pub use ai_provider::{AI_INFO, AI_MODELS, AI_REPLY, AI_REQUEST, PluginAiProvider};
pub use automation::{AUTOMATION_AFTER_IMPORT, NewTransaction, PluginAutomation, Suggestion};
pub use exporter::{EXPORTER_INFO, EXPORTER_WRITE, PluginExporter};
pub use importer::{IMPORTER_INFO, IMPORTER_READ, IMPORTER_RECOGNIZE, PluginImporter};
pub use report::{
    Cell, Column, ColumnKind, MAX_CELL_CHARS, MAX_COLUMNS, MAX_NAME_CHARS, MAX_REPORTS, MAX_ROWS,
    PluginReport, REPORT_INFO, REPORT_RUN, ReportInfo, ReportTable,
};

#[cfg(all(test, feature = "ai"))]
mod ai_provider_tests;
#[cfg(test)]
mod report_tests;
#[cfg(test)]
mod tests;
