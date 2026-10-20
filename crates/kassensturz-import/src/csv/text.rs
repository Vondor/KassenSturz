// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A CSV file's text: decoding it from its encoding and splitting it into
//! rows and fields.

use std::borrow::Cow;

use crate::csv::settings::{MAX_COLUMNS, TextEncoding};
use crate::error::ImportError;

/// The UTF-8 byte order mark, which some programs write first.
const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// Windows-1252's characters for the bytes 0x80 to 0x9F, as in the Unicode
/// Consortium's mapping table `MICSFT/WINDOWS/CP1252.TXT`; `None` where it
/// has none (0x81, 0x8D, 0x8F, 0x90, 0x9D). From 0xA0 on, each byte is the
/// character with the same number, as in ISO-8859-1.
const WINDOWS_1252: [Option<char>; 32] = [
    Some('\u{20AC}'),
    None,
    Some('\u{201A}'),
    Some('\u{0192}'),
    Some('\u{201E}'),
    Some('\u{2026}'),
    Some('\u{2020}'),
    Some('\u{2021}'),
    Some('\u{02C6}'),
    Some('\u{2030}'),
    Some('\u{0160}'),
    Some('\u{2039}'),
    Some('\u{0152}'),
    None,
    Some('\u{017D}'),
    None,
    None,
    Some('\u{2018}'),
    Some('\u{2019}'),
    Some('\u{201C}'),
    Some('\u{201D}'),
    Some('\u{2022}'),
    Some('\u{2013}'),
    Some('\u{2014}'),
    Some('\u{02DC}'),
    Some('\u{2122}'),
    Some('\u{0161}'),
    Some('\u{203A}'),
    Some('\u{0153}'),
    None,
    Some('\u{017E}'),
    Some('\u{0178}'),
];

/// The bytes where ISO-8859-15 differs from ISO-8859-1, and its characters
/// there, as in the Unicode Consortium's mapping table
/// `ISO8859/8859-15.TXT`.
const ISO_8859_15: [(u8, char); 8] = [
    (0xA4, '\u{20AC}'),
    (0xA6, '\u{0160}'),
    (0xA8, '\u{0161}'),
    (0xB4, '\u{017D}'),
    (0xB8, '\u{017E}'),
    (0xBC, '\u{0152}'),
    (0xBD, '\u{0153}'),
    (0xBE, '\u{0178}'),
];

/// The file's text, in `encoding`, or, without one, in UTF-8 if it is
/// valid UTF-8 and in Windows-1252 otherwise. A UTF-8 byte order mark
/// means UTF-8 whatever was chosen, and is left out.
///
/// A byte the encoding has no character for fails with
/// `import.wrong-encoding` and its line, so the user can choose another;
/// a NUL character, which no text file has but UTF-16 and binary files
/// have plenty of, fails with `import.malformed`.
pub(super) fn decode(
    input: &[u8],
    encoding: Option<TextEncoding>,
) -> Result<Cow<'_, str>, ImportError> {
    let (input, encoding) = match input.strip_prefix(UTF8_BOM) {
        Some(rest) => (rest, Some(TextEncoding::Utf8)),
        None => (input, encoding),
    };
    let text = match encoding {
        Some(TextEncoding::Utf8) => Cow::Borrowed(std::str::from_utf8(input).map_err(|error| {
            ImportError::WrongEncoding {
                encoding: TextEncoding::Utf8,
                line: Some(line_at(input, error.valid_up_to())),
            }
        })?),
        None => match std::str::from_utf8(input) {
            Ok(text) => Cow::Borrowed(text),
            Err(_) => Cow::Owned(single_byte(input, TextEncoding::Windows1252)?),
        },
        Some(encoding) => Cow::Owned(single_byte(input, encoding)?),
    };
    if let Some(at) = text.find('\0') {
        return Err(ImportError::Malformed {
            line: Some(line_at(text.as_bytes(), at)),
        });
    }
    Ok(text)
}

/// `input` read in a single-byte encoding.
fn single_byte(input: &[u8], encoding: TextEncoding) -> Result<String, ImportError> {
    let mut text = String::with_capacity(input.len());
    for (at, &byte) in input.iter().enumerate() {
        let char = match (encoding, byte) {
            (_, 0x00..=0x7F) => Some(char::from(byte)),
            (TextEncoding::Windows1252, 0x80..=0x9F) => WINDOWS_1252[usize::from(byte - 0x80)],
            // ISO-8859-15 has no characters here, only control codes no
            // text uses; a file with these bytes is in another encoding,
            // usually Windows-1252.
            (TextEncoding::Iso8859_15, 0x80..=0x9F) => None,
            (TextEncoding::Iso8859_15, _) => Some(
                ISO_8859_15
                    .iter()
                    .find(|&&(from, _)| from == byte)
                    .map_or(char::from(byte), |&(_, char)| char),
            ),
            (TextEncoding::Windows1252 | TextEncoding::Utf8, _) => Some(char::from(byte)),
        };
        text.push(char.ok_or_else(|| ImportError::WrongEncoding {
            encoding,
            line: Some(line_at(input, at)),
        })?);
    }
    Ok(text)
}

/// The line, counted from 1, of the byte at `position`.
pub(super) fn line_at(input: &[u8], position: usize) -> u64 {
    let before = &input[..position.min(input.len())];
    count_lines(before).saturating_add(1)
}

/// How many line breaks `bytes` has.
fn count_lines(bytes: &[u8]) -> u64 {
    let breaks = bytes.iter().filter(|&&byte| byte == b'\n').count();
    u64::try_from(breaks).unwrap_or(u64::MAX)
}

/// `text` without its first `count` lines.
pub(super) fn without_first_lines(text: &str, count: usize) -> &str {
    let mut rest = text;
    for _ in 0..count {
        match rest.find('\n') {
            Some(at) => rest = &rest[at + 1..],
            None => return "",
        }
    }
    rest
}

/// `text` without its last `count` lines that aren't empty, and without
/// the empty lines after them.
pub(super) fn without_last_lines(text: &str, count: usize) -> &str {
    let mut end = text.len();
    let mut skipped = 0;
    while skipped < count && end > 0 {
        let before = &text[..end];
        let before = before.strip_suffix('\n').unwrap_or(before);
        let start = before.rfind('\n').map_or(0, |at| at + 1);
        if !before[start..].trim().is_empty() {
            skipped += 1;
        }
        end = start;
    }
    &text[..end]
}

/// One row of a CSV file.
pub(super) struct Row<'a> {
    /// The line the row starts on, counted from 1.
    pub line: u64,
    /// Its fields, as written, with quotes taken out.
    pub fields: Vec<Cow<'a, str>>,
}

impl Row<'_> {
    /// The field at `index`, counted from 0, or an empty one if the row is
    /// shorter: some programs leave out empty fields at the end of a row.
    pub fn field(&self, index: usize) -> &str {
        self.fields.get(index).map_or("", |field| field)
    }

    /// Whether every field is empty or spaces only, like a separating row.
    pub fn is_empty(&self) -> bool {
        self.fields.iter().all(|field| field.trim().is_empty())
    }
}

/// The rows of a CSV file's text, one at a time, as RFC 4180 describes
/// them: fields separated by the delimiter, rows ending with a line break
/// (`\n` or `\r\n`), and a field in double quotes holding delimiters, line
/// breaks and doubled quotes (`""`) as text.
pub(super) struct Rows<'a> {
    text: &'a str,
    position: usize,
    line: u64,
    delimiter: char,
}

impl<'a> Rows<'a> {
    /// The rows of `text`, whose first line is `first_line` of the file.
    pub fn new(text: &'a str, delimiter: char, first_line: u64) -> Self {
        Self {
            text,
            position: 0,
            line: first_line,
            delimiter,
        }
    }

    /// The next row, or `None` at the end of the text. A quote that is
    /// never closed, or text after a closing quote, fails with
    /// `import.malformed` and the row's line, as does a row with more than
    /// [`MAX_COLUMNS`] fields.
    pub fn next_row(&mut self) -> Option<Result<Row<'a>, ImportError>> {
        if self.position >= self.text.len() {
            return None;
        }
        let line = self.line;
        let mut fields = Vec::new();
        loop {
            if fields.len() == MAX_COLUMNS {
                return Some(Err(ImportError::Malformed { line: Some(line) }));
            }
            let field = if self.rest().starts_with('"') {
                match self.quoted() {
                    Some(field) => field,
                    None => {
                        // Nothing after a broken row can be read reliably.
                        self.position = self.text.len();
                        return Some(Err(ImportError::Malformed { line: Some(line) }));
                    }
                }
            } else {
                self.unquoted()
            };
            fields.push(field);
            let rest = self.rest();
            if rest.starts_with(self.delimiter) {
                self.position += self.delimiter.len_utf8();
            } else {
                // A line break or the end: unquoted() and quoted() stop at
                // nothing else.
                if let Some(after) = rest
                    .strip_prefix("\r\n")
                    .or_else(|| rest.strip_prefix('\n'))
                {
                    self.position = self.text.len() - after.len();
                    self.line = self.line.saturating_add(1);
                }
                return Some(Ok(Row { line, fields }));
            }
        }
    }

    fn rest(&self) -> &'a str {
        &self.text[self.position..]
    }

    /// A field without quotes, up to the next delimiter or line break; a
    /// `\r` before the line break is part of the line break.
    fn unquoted(&mut self) -> Cow<'a, str> {
        let rest = self.rest();
        let end = rest.find([self.delimiter, '\n']).unwrap_or(rest.len());
        let mut field = &rest[..end];
        if rest[end..].starts_with('\n') {
            field = field.strip_suffix('\r').unwrap_or(field);
        }
        // next_row takes the delimiter or the line break that follows.
        self.position += field.len();
        Cow::Borrowed(field)
    }

    /// A field in double quotes, or `None` if its quote isn't closed or
    /// something other than a delimiter or line break follows it.
    fn quoted(&mut self) -> Option<Cow<'a, str>> {
        let text = self.text;
        let mut start = self.position + 1;
        let mut value = String::new();
        loop {
            let quote = start + text[start..].find('"')?;
            value.push_str(&text[start..quote]);
            self.line = self
                .line
                .saturating_add(count_lines(&text.as_bytes()[start..quote]));
            if text[quote + 1..].starts_with('"') {
                value.push('"');
                start = quote + 2;
            } else {
                self.position = quote + 1;
                break;
            }
        }
        let rest = self.rest();
        let ends = rest.is_empty()
            || rest.starts_with(self.delimiter)
            || rest.starts_with('\n')
            || rest.starts_with("\r\n");
        ends.then_some(Cow::Owned(value))
    }
}
