// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! KMyMoney's saved reports (M6d.6a, D-444): a `REPORT` record with its
//! settings as attributes and its filters as elements inside it. A `.kmy`
//! file keeps them in its `REPORTS` section, a SQLite file keeps each one's
//! record as XML text in a row of `kmmReportConfig`, as the maintainer's
//! sample files show (D-431). Both are read here into the same
//! [`BookReport`], so both formats give the same views.
//!
//! The records are taken as they are: what a setting means is decided by
//! the mapping onto a saved view (`kassensturz-pipeline`), never here.

use super::xml::{self, Element};
use crate::book::{BookReport, BookReportPart};
use crate::error::ImportError;
use crate::limits::Limits;

/// The name of a report's record.
pub(super) const REPORT: &str = "REPORT";

/// The report a `REPORT` element starts, without its filters yet. A record
/// without an `id` is `import.malformed`: nothing could tell it apart.
pub(super) fn report(element: &Element) -> Result<BookReport, ImportError> {
    let key = element.get("id").ok_or(ImportError::Malformed {
        line: Some(element.line),
    })?;
    Ok(BookReport {
        key: key.to_owned(),
        name: element.get("name").unwrap_or_default().to_owned(),
        settings: element.attributes.clone(),
        parts: Vec::new(),
    })
}

/// The element `element` inside a report, as one of its parts.
pub(super) fn part(element: &Element) -> BookReportPart {
    BookReportPart {
        name: element.name.clone(),
        settings: element.attributes.clone(),
    }
}

/// The reports in the XML text of one row of `kmmReportConfig`: a
/// `REPORTS` element around one `REPORT` record, as the samples show.
/// Anything else is `import.malformed`.
pub(super) fn from_row(xml: &[u8], limits: &Limits) -> Result<Vec<BookReport>, ImportError> {
    let mut reports: Vec<BookReport> = Vec::new();
    xml::read_elements(xml, limits, |path, element| {
        let malformed = || ImportError::Malformed {
            line: Some(element.line),
        };
        let names: Vec<&str> = path.iter().map(String::as_str).collect();
        match names.as_slice() {
            [] if element.name == "REPORTS" => {}
            ["REPORTS"] if element.name == REPORT => reports.push(report(element)?),
            ["REPORTS", REPORT, ..] => {
                reports
                    .last_mut()
                    .ok_or_else(malformed)?
                    .parts
                    .push(part(element));
            }
            _ => return Err(malformed()),
        }
        Ok(())
    })?;
    Ok(reports)
}

#[cfg(test)]
mod tests;
