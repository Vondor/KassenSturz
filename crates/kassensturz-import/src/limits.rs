// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Limits for reading untrusted files (AGENTS.md §8, D-091).

use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::error::ImportError;

/// How much a file may ask of KassenSturz, so a damaged or malicious file
/// can't exhaust memory or the stack.
///
/// The defaults are far above what a household's files need: a year of a
/// busy account in CAMT.053 is a few megabytes with a few thousand entries,
/// nested about ten levels deep.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The most bytes a file may have. For a compressed file, such as
    /// KMyMoney's `.kmy` (M2.9), the limit applies to the uncompressed
    /// content too, so the importer stops decompressing there.
    pub max_file_bytes: u64,
    /// The most levels of nesting, e.g. of XML elements, an importer reads.
    pub max_depth: usize,
    /// The most entries all statements of a file may have together.
    pub max_entries: usize,
    /// The most bytes of one text, such as a name or the remittance
    /// information.
    pub max_text_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_file_bytes: 128 * 1024 * 1024,
            max_depth: 64,
            max_entries: 1_000_000,
            max_text_bytes: 16 * 1024,
        }
    }
}

impl Limits {
    /// Fails if a file of `bytes` is larger than allowed.
    pub fn check_file_size(&self, bytes: usize) -> Result<(), ImportError> {
        // A usize always fits into 64 bits on the platforms KassenSturz
        // supports; if it didn't, the file would be too large anyway.
        let bytes = u64::try_from(bytes).unwrap_or(u64::MAX);
        if bytes > self.max_file_bytes {
            Err(ImportError::FileTooLarge {
                max_bytes: self.max_file_bytes,
            })
        } else {
            Ok(())
        }
    }

    /// Fails if `depth` levels of nesting are more than allowed. An importer
    /// calls it before it goes one level deeper; `line` is where in the file
    /// that is, if the importer knows.
    pub fn check_depth(&self, depth: usize, line: Option<u64>) -> Result<(), ImportError> {
        if depth > self.max_depth {
            Err(ImportError::TooDeeplyNested {
                max: self.max_depth,
                line,
            })
        } else {
            Ok(())
        }
    }

    /// Fails if `count` entries are more than allowed. An importer calls it
    /// as it adds entries, so it stops before holding too many.
    pub fn check_entries(&self, count: usize) -> Result<(), ImportError> {
        if count > self.max_entries {
            Err(ImportError::TooManyEntries {
                max: self.max_entries,
            })
        } else {
            Ok(())
        }
    }

    /// Fails if `text` is longer than allowed; `line` is where in the file
    /// it is, if known.
    pub fn check_text(&self, text: &str, line: Option<u64>) -> Result<(), ImportError> {
        if text.len() > self.max_text_bytes {
            Err(ImportError::TextTooLong {
                max_bytes: self.max_text_bytes,
                line,
            })
        } else {
            Ok(())
        }
    }
}

/// Reads the file at `path`, refusing one larger than
/// `limits.max_file_bytes` without reading more than one byte beyond it.
pub fn read_file(path: &Path, limits: &Limits) -> Result<Vec<u8>, ImportError> {
    let file = File::open(path).map_err(|error| ImportError::Io(error.kind()))?;
    let mut content = Vec::new();
    file.take(limits.max_file_bytes.saturating_add(1))
        .read_to_end(&mut content)
        .map_err(|error| ImportError::Io(error.kind()))?;
    limits.check_file_size(content.len())?;
    Ok(content)
}

#[cfg(test)]
mod tests;
