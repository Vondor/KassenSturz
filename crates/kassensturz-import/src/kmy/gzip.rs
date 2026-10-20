// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The gzip layer of a `.kmy` file: KMyMoney compresses its XML with gzip
//! when it saves (D-102). The importer reads the XML uncompressed too, e.g.
//! after the user unpacked it.

use std::io::Read;

use flate2::read::GzDecoder;

use crate::error::ImportError;
use crate::limits::Limits;

/// The first two bytes of every gzip file (RFC 1952).
const MAGIC: [u8; 2] = [0x1f, 0x8b];

/// Whether `input` starts like a gzip file.
pub(super) fn is_gzip(input: &[u8]) -> bool {
    input.starts_with(&MAGIC)
}

/// The XML of a `.kmy` file: `input` uncompressed if it is gzip, otherwise
/// `input` itself. The uncompressed content is held to
/// `limits.max_file_bytes` too, and decompressing stops one byte past it,
/// so a small file can't unpack into one that exhausts memory (D-091). A
/// damaged gzip stream, or one whose checksum doesn't match, is
/// `import.malformed`.
pub(super) fn uncompressed<'a>(
    input: &'a [u8],
    limits: &Limits,
) -> Result<std::borrow::Cow<'a, [u8]>, ImportError> {
    if !is_gzip(input) {
        return Ok(std::borrow::Cow::Borrowed(input));
    }
    let mut content = Vec::new();
    GzDecoder::new(input)
        .take(limits.max_file_bytes.saturating_add(1))
        .read_to_end(&mut content)
        .map_err(|_| ImportError::Malformed { line: None })?;
    limits.check_file_size(content.len())?;
    Ok(std::borrow::Cow::Owned(content))
}

/// As much of the start of `start`'s uncompressed content as it holds, at
/// most `max` bytes, for recognizing a file from its first bytes: a gzip
/// stream cut off there is no error. `None` if `start` isn't gzip or its
/// stream is damaged before anything comes out.
pub(super) fn uncompressed_start(start: &[u8], max: usize) -> Option<Vec<u8>> {
    if !is_gzip(start) {
        return None;
    }
    let mut decoder = GzDecoder::new(start);
    let mut content = vec![0; max];
    let mut filled = 0;
    while filled < max {
        match decoder.read(&mut content[filled..]) {
            Ok(0) | Err(_) => break,
            Ok(read) => filled += read,
        }
    }
    content.truncate(filled);
    (filled > 0).then_some(content)
}

#[cfg(test)]
mod tests;
