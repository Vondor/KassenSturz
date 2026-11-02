// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Getting a chart description in, as the properties panel of a custom chart
//! sees it (M6e.3, D-505, D-508). There are two ways in, a pasted text and an
//! opened file, and one check for both: [`check`] runs the parser of
//! `kassensturz-report` and says what the panel needs to show, whether the
//! description can be kept, which dataset it names, whether anything can be
//! drawn and which parts are not supported here. No Qt in this file:
//! `chart_spec_model.rs` hands the result to QML.
//!
//! A description is the user's own text and may be hostile: it is never run,
//! never fetched and shown only as plain text. A file is read with a bound
//! on its size, so a huge or endless one costs bounded memory and time.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use kassensturz_report::ReportError;
use kassensturz_report::spec::{self, MAX_SPEC_BYTES};

/// What the panel knows about a description after reading it.
#[derive(Debug)]
pub struct Check {
    /// Why the description can't be kept; `None` when the parser accepted it
    /// (with or without parts that are not supported here).
    pub refused: Option<ReportError>,
    /// The dataset it names (`data.name`); empty when it names none or was
    /// refused.
    pub dataset: String,
    /// Whether something can be drawn: a dataset, a mark and what it needs
    /// are left after the unsupported parts (D-508).
    pub can_draw: bool,
    /// The parts that are not supported here, in the order of the text: the
    /// path (the user's own text) and the fixed word of the reason.
    pub unsupported: Vec<(String, &'static str)>,
}

/// Reads `text` as a description would be read when it is saved.
pub fn check(text: &str) -> Check {
    match spec::read(text) {
        Err(error) => Check {
            refused: Some(error),
            dataset: String::new(),
            can_draw: false,
            unsupported: Vec::new(),
        },
        Ok(read) => {
            let unsupported = read
                .unsupported()
                .iter()
                .map(|part| (part.path().to_owned(), part.reason()))
                .collect();
            let (chart, _) = read.into_parts();
            Check {
                refused: None,
                dataset: chart
                    .as_ref()
                    .map(|chart| chart.dataset().id().to_owned())
                    .unwrap_or_default(),
                can_draw: chart.is_some(),
                unsupported,
            }
        }
    }
}

/// The text of the file at `path` (D-505): at most [`MAX_SPEC_BYTES`] bytes
/// are kept and one more is read to tell, UTF-8 only, with a leading
/// byte-order mark taken off. Nothing else is remembered of the file.
///
/// # Errors
///
/// `report.spec-too-big` (`bytes`) for a longer file, `report.spec-invalid`
/// with `not-utf8` for bytes that are not UTF-8, and with `unreadable` for a
/// file that is not a regular file or can't be read.
pub fn read_file(path: &Path) -> Result<String, ReportError> {
    let unreadable = || ReportError::SpecInvalid {
        reason: "unreadable",
        path: String::new(),
    };
    // A pipe or a device could block or never end; only a regular file is
    // read. It is asked before the file is opened, because opening a named
    // pipe already waits for a writer, which would hang the window (M6e.4,
    // M6-B92). It is asked again of the opened file, in case the name was
    // pointed elsewhere in between.
    if !std::fs::metadata(path).is_ok_and(|meta| meta.is_file()) {
        return Err(unreadable());
    }
    let file = File::open(path).map_err(|_| unreadable())?;
    if !file.metadata().is_ok_and(|meta| meta.is_file()) {
        return Err(unreadable());
    }
    let limit = u64::try_from(MAX_SPEC_BYTES)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut bytes = Vec::new();
    file.take(limit)
        .read_to_end(&mut bytes)
        .map_err(|_| unreadable())?;
    if bytes.len() > MAX_SPEC_BYTES {
        return Err(ReportError::SpecTooBig { limit: "bytes" });
    }
    let text = String::from_utf8(bytes).map_err(|_| ReportError::SpecInvalid {
        reason: "not-utf8",
        path: String::new(),
    })?;
    Ok(text
        .strip_prefix('\u{feff}')
        .map_or(text.clone(), str::to_owned))
}
