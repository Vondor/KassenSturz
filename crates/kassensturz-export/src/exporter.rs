// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Exporter interface every format implements (D-007, D-069).

use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;

use crate::data::ExportData;
use crate::error::ExportError;

/// Writes one file format, e.g. CSV.
///
/// Built-in exporters implement it directly; the WASM plugin API mirrors it
/// after 1.0, so a plugin exporter works the same way (D-069, D-071). An
/// exporter only writes: it gets the data and a place to write to, and never
/// sees a book. Settings a format needs, such as CSV's separator, are the
/// exporter value's own, given when it is created, so the interface stays the
/// same for every format.
///
/// Front ends call [`export`] or [`export_file`] rather than
/// [`Exporter::write`], so the data is checked and no half-written file is
/// left behind whatever the exporter does.
pub trait Exporter {
    /// What the exporter writes.
    fn info(&self) -> ExporterInfo;

    /// Writes `data` to `out`, in the order it has, exactly: an amount is
    /// never rounded, and nothing is left out without being said (D-029).
    /// It checks `data` itself ([`ExportData::check`]) as the first thing,
    /// since it can be called directly.
    fn write(&self, data: &ExportData, out: &mut dyn Write) -> Result<(), ExportError>;
}

/// What an exporter writes, for choosing it: in the file dialog, on the
/// command line and in settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExporterInfo {
    /// A stable ID: lowercase ASCII letters, digits and hyphens, e.g.
    /// `csv`. It names the exporter on the command line and in saved
    /// settings, and never changes. The UI shows the format's name, which it
    /// keeps itself (D-034).
    pub id: &'static str,
    /// What it writes.
    pub kind: ExportKind,
    /// The file name extension the format has, lowercase and without the
    /// dot, e.g. `csv`.
    pub extension: &'static str,
}

/// What an exporter writes, known before anything is gathered from the book,
/// so the front end collects the right data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ExportKind {
    /// The transactions of accounts ([`ExportData::Transactions`]).
    Transactions,
}

/// Writes `data` with `exporter` and returns the bytes of the file.
pub fn export(exporter: &dyn Exporter, data: &ExportData) -> Result<Vec<u8>, ExportError> {
    data.check()?;
    let mut bytes = Vec::new();
    exporter.write(data, &mut bytes)?;
    Ok(bytes)
}

/// Writes `data` with `exporter` to a new file at `path`.
///
/// Never overwrites: if something is at `path`, it fails with
/// [`ExportError::AlreadyExists`] and leaves it alone. If writing fails, the
/// file this call began is removed again, so there is either a complete file
/// or none.
pub fn export_file(
    exporter: &dyn Exporter,
    data: &ExportData,
    path: &Path,
) -> Result<(), ExportError> {
    data.check()?;
    let file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let mut out = BufWriter::new(file);
    let written = exporter
        .write(data, &mut out)
        .and_then(|()| out.flush().map_err(ExportError::from))
        .and_then(|()| out.get_ref().sync_all().map_err(ExportError::from));
    if written.is_err() {
        drop(out);
        // Nothing more can be done if even this fails.
        let _ = fs::remove_file(path);
    }
    written
}
