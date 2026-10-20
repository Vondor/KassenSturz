// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! KassenSturz's exporters: writing a book's data to files other programs
//! can read (D-029, D-099).
//!
//! Every format implements one interface, the [`Exporter`] trait, the
//! counterpart of the importers' (D-069). The WASM plugin API mirrors both
//! once the plugin host comes, after 1.0 (D-007, D-071). An exporter only
//! writes: it gets [`ExportData`], which the caller has gathered from a book
//! (`kassensturz-pipeline` does it for the front ends), and never touches a
//! book itself, just as an importer never does (D-091).
//!
//! What users see isn't what a file holds (D-002): amounts are written
//! exactly, with the currency's decimal places, never rounded, and text
//! from a book is written as it is, except where a spreadsheet would take it
//! for a formula (D-099). Errors are codes with parameters
//! ([`ExportError`], D-034, D-084), like everywhere else in KassenSturz.
//!
//! The formats so far are [`CsvExport`] (M2.7); the complete export of a
//! book comes in M2.8.
//!
//! This crate depends only on `kassensturz-core` (`docs/ARCHITECTURE.md`).

#![forbid(unsafe_code)]

mod csv;
mod data;
mod error;
mod exporter;

pub use csv::{CsvExport, CsvExportSettings, DecimalMark, Separator};
pub use data::{AccountTransactions, Counterpart, ExportData, TransactionRow};
pub use error::ExportError;
pub use exporter::{ExportKind, Exporter, ExporterInfo, export, export_file};
