// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! KMyMoney's `.kmy` file: the whole book as XML, usually compressed with
//! gzip (M2.9).
//!
//! The importer is built clean-room (D-044): from sample files made with
//! KMyMoney and from KMyMoney's public documentation, never from its source
//! code (AGENTS.md §3.1 rule 5). What it takes from the file, and how it
//! decides, is D-101 in `docs/DECISIONS.md`.

mod fraction;
mod gzip;
mod mapping;
mod records;
mod reports;
mod sqlite;
mod xml;

use crate::error::ImportError;
use crate::importer::{ImportKind, Imported, Importer, ImporterInfo, Recognition};
use crate::limits::Limits;

pub use sqlite::KmySqlite;

/// The document type every `.kmy` file declares.
const DOCTYPE: &[u8] = b"<!DOCTYPE KMYMONEY-FILE>";

/// The importer for KMyMoney's `.kmy` files, with the ID `kmy`. It reads a
/// whole book, for a new KassenSturz book.
#[derive(Clone, Copy, Debug, Default)]
pub struct Kmy;

impl Importer for Kmy {
    fn info(&self) -> ImporterInfo {
        ImporterInfo {
            id: "kmy",
            kind: ImportKind::Book,
            extensions: &["kmy"],
        }
    }

    fn recognize(&self, start: &[u8]) -> Recognition {
        let xml = gzip::uncompressed_start(start, crate::importer::RECOGNITION_BYTES);
        let xml = xml.as_deref().unwrap_or(start);
        if find(xml, DOCTYPE) {
            Recognition::Yes
        } else {
            Recognition::No
        }
    }

    fn read(&self, input: &[u8], limits: &Limits) -> Result<Imported, ImportError> {
        let xml = gzip::uncompressed(input, limits)?;
        let records = records::read(&xml, limits)?;
        Ok(Imported::Book(Box::new(mapping::book(records)?)))
    }
}

/// Whether `needle` occurs in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[cfg(test)]
mod tests;
