// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The text encoding of a CAMT.053 file (B-280).
//!
//! XML says how a reader finds it out: a byte order mark decides; without
//! one the first bytes tell UTF-16 from the encodings that start like
//! ASCII; and then the `encoding` of the XML declaration names the
//! encoding. Banks write UTF-8, and some tools write UTF-16 (Windows
//! Notepad's "Unicode") or ISO-8859-1. All of these are read. A file whose
//! bytes contradict its own declaration is refused with a message that says
//! so, instead of being read as one or the other by guess: money may hang on
//! an umlaut. An encoding KassenSturz doesn't read (UTF-32, a Cyrillic or
//! Asian one) is refused by name.

use std::borrow::Cow;

use crate::csv::{TextEncoding, single_byte};
use crate::error::ImportError;

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";
const UTF16_LE_BOM: &[u8] = b"\xFF\xFE";
const UTF16_BE_BOM: &[u8] = b"\xFE\xFF";

/// How the bytes of a file are laid out, from their first bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Layout {
    /// ASCII-compatible; the declaration says which encoding.
    Ascii {
        bom: bool,
    },
    Utf16 {
        little: bool,
        bom: bool,
    },
    Utf32,
}

fn layout(input: &[u8]) -> Layout {
    if input.starts_with(UTF8_BOM) {
        Layout::Ascii { bom: true }
    } else if input.starts_with(b"\xFF\xFE\x00\x00") || input.starts_with(b"\x00\x00\xFE\xFF") {
        Layout::Utf32
    } else if input.starts_with(UTF16_LE_BOM) {
        Layout::Utf16 {
            little: true,
            bom: true,
        }
    } else if input.starts_with(UTF16_BE_BOM) {
        Layout::Utf16 {
            little: false,
            bom: true,
        }
    } else if input.starts_with(b"<\x00\x00\x00") || input.starts_with(b"\x00\x00\x00<") {
        Layout::Utf32
    } else if input.starts_with(b"<\x00?\x00") {
        Layout::Utf16 {
            little: true,
            bom: false,
        }
    } else if input.starts_with(b"\x00<\x00?") {
        Layout::Utf16 {
            little: false,
            bom: false,
        }
    } else {
        Layout::Ascii { bom: false }
    }
}

/// The text of the first bytes of a UTF-16 file, to look for the namespace
/// in, or none if they aren't UTF-16.
pub(super) fn utf16_start(start: &[u8]) -> Option<String> {
    let Layout::Utf16 { little, bom } = layout(start) else {
        return None;
    };
    let body = if bom { &start[2..] } else { start };
    Some(
        units(body, little)
            .map(|unit| unit.unwrap_or('\u{fffd}'))
            .collect(),
    )
}

fn units(body: &[u8], little: bool) -> impl Iterator<Item = Result<char, ()>> + '_ {
    let words = body.as_chunks::<2>().0.iter().map(move |&pair| {
        if little {
            u16::from_le_bytes(pair)
        } else {
            u16::from_be_bytes(pair)
        }
    });
    char::decode_utf16(words).map(|unit| unit.map_err(|_| ()))
}

/// The file's text, whatever its encoding, with no byte order mark.
pub(super) fn decode(input: &[u8]) -> Result<Cow<'_, str>, ImportError> {
    let text = match layout(input) {
        Layout::Utf32 => return Err(unsupported("UTF-32")),
        Layout::Utf16 { little, bom } => Cow::Owned(decode_utf16(input, little, bom)?),
        Layout::Ascii { bom } => {
            let body = if bom { &input[UTF8_BOM.len()..] } else { input };
            decode_ascii_compatible(body, bom)?
        }
    };
    if let Some(at) = text.find('\0') {
        return Err(ImportError::Malformed {
            line: Some(line_at(text.as_bytes(), at)),
        });
    }
    Ok(text)
}

fn decode_utf16(input: &[u8], little: bool, bom: bool) -> Result<String, ImportError> {
    let body = if bom { &input[2..] } else { input };
    let mut text = String::with_capacity(body.len() / 2);
    let mut line = 1_u64;
    if body.len() % 2 != 0 {
        return Err(ImportError::Malformed { line: None });
    }
    for unit in units(body, little) {
        let Ok(char) = unit else {
            return Err(ImportError::Malformed { line: Some(line) });
        };
        if char == '\n' {
            line = line.saturating_add(1);
        }
        text.push(char);
    }
    if let Some(declared) = declared_encoding(&text)? {
        let name = declared.to_ascii_lowercase();
        let fits = match (name.as_str(), little) {
            ("utf-16", _) => true,
            ("utf-16le", little) => little,
            ("utf-16be", little) => !little,
            _ => false,
        };
        if !fits {
            return Err(contradiction(&declared, 1));
        }
    }
    Ok(text)
}

fn decode_ascii_compatible(body: &[u8], bom: bool) -> Result<Cow<'_, str>, ImportError> {
    let declared = declared_encoding_in_bytes(body)?;
    let name = declared.as_ref().map(|name| name.to_ascii_lowercase());
    let shown = declared.as_deref().unwrap_or("UTF-8");
    let single = match name.as_deref() {
        None | Some("utf-8") => None,
        Some("iso-8859-1" | "iso_8859-1" | "latin1" | "latin-1" | "windows-1252" | "cp1252") => {
            Some(TextEncoding::Windows1252)
        }
        Some("iso-8859-15" | "iso_8859-15" | "latin9" | "latin-9") => {
            Some(TextEncoding::Iso8859_15)
        }
        Some("utf-16" | "utf-16le" | "utf-16be") => return Err(contradiction(shown, 1)),
        Some("utf-32" | "ucs-4") => return Err(unsupported("UTF-32")),
        Some(_) => return Err(unsupported(shown)),
    };
    match single {
        None => std::str::from_utf8(body)
            .map(Cow::Borrowed)
            .map_err(|error| contradiction(shown, line_at(body, error.valid_up_to()))),
        Some(encoding) => {
            if bom {
                // A UTF-8 byte order mark and a declaration of another
                // encoding.
                return Err(contradiction(shown, 1));
            }
            // Bytes that are valid UTF-8 with a non-ASCII character in them
            // are very probably UTF-8 text with a wrong declaration;
            // reading them as single bytes would turn each umlaut into
            // two wrong characters.
            if !body.is_ascii() && std::str::from_utf8(body).is_ok() {
                return Err(contradiction(shown, 1));
            }
            single_byte(body, encoding)
                .map(Cow::Owned)
                .map_err(|error| match error {
                    ImportError::WrongEncoding { line, .. } => ImportError::Malformed { line },
                    other => other,
                })
        }
    }
}

/// The `encoding` of the XML declaration at the start of `body`, which is
/// ASCII-compatible text.
fn declared_encoding_in_bytes(body: &[u8]) -> Result<Option<String>, ImportError> {
    let head = &body[..body.len().min(256)];
    // The declaration is ASCII up to its end; look at the bytes up to `?>`.
    let end = head.windows(2).position(|pair| pair == b"?>");
    let Some(end) = end else {
        return Ok(None);
    };
    let Ok(declaration) = std::str::from_utf8(&head[..end]) else {
        // Not ASCII: the file is damaged or in an encoding the declaration
        // can't be read in; the declaration is then not trusted.
        return Err(ImportError::Malformed { line: Some(1) });
    };
    declared_encoding(&format!("{declaration}?>"))
}

/// The `encoding` of the XML declaration at the start of `text`.
fn declared_encoding(text: &str) -> Result<Option<String>, ImportError> {
    let Some(rest) = text.strip_prefix("<?xml") else {
        return Ok(None);
    };
    let Some(end) = rest.find("?>") else {
        return Ok(None);
    };
    let declaration = &rest[..end];
    let Some(at) = declaration.find("encoding") else {
        return Ok(None);
    };
    let after = declaration[at + "encoding".len()..].trim_start();
    let Some(after) = after.strip_prefix('=') else {
        return Err(ImportError::Malformed { line: Some(1) });
    };
    let after = after.trim_start();
    let mut chars = after.chars();
    let quote = chars.next();
    if !matches!(quote, Some('"' | '\'')) {
        return Err(ImportError::Malformed { line: Some(1) });
    }
    let value: String = chars.take_while(|&char| Some(char) != quote).collect();
    if value.is_empty() || !value.is_ascii() {
        return Err(ImportError::Malformed { line: Some(1) });
    }
    Ok(Some(value))
}

fn contradiction(declared: &str, line: u64) -> ImportError {
    ImportError::EncodingContradiction {
        declared: declared.to_owned(),
        line: Some(line),
    }
}

fn unsupported(found: &str) -> ImportError {
    ImportError::UnsupportedEncoding {
        found: found.to_owned(),
    }
}

/// The line, counted from 1, of the byte at `position`.
fn line_at(input: &[u8], position: usize) -> u64 {
    let before = &input[..position.min(input.len())];
    let breaks = before.iter().filter(|&&byte| byte == b'\n').count();
    u64::try_from(breaks).unwrap_or(u64::MAX).saturating_add(1)
}
