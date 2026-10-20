// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The XML of a `.kmy` file, read as a stream of elements with their
//! attributes, where KMyMoney keeps its data.
//!
//! The file is untrusted (AGENTS.md §8): it is read one event at a time,
//! never as a tree; its document type may only name KMyMoney's, without
//! any definitions, so no entity is ever expanded or fetched; and the
//! limits on nesting and text are kept while reading (D-091).

use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

use crate::error::ImportError;
use crate::limits::Limits;

/// The only document type a `.kmy` file may declare: KMyMoney's name for
/// its files, nothing more.
const DOCTYPE: &str = "KMYMONEY-FILE";

/// One element: its name, its attributes in the file's order, and where
/// in the file it starts.
pub(super) struct Element {
    /// The element's name, e.g. `TRANSACTION`.
    pub name: String,
    /// Its attributes as names and values, the values unescaped.
    pub attributes: Vec<(String, String)>,
    /// The line it starts on, counted from 1.
    pub line: u64,
}

impl Element {
    /// The value of the attribute `name`, if the element has it.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

/// Reads `xml` and calls `visit` for every element, with the names of the
/// elements it sits inside, outermost first. Text between elements must be
/// whitespace: KMyMoney keeps everything in attributes.
pub(super) fn read_elements(
    xml: &[u8],
    limits: &Limits,
    mut visit: impl FnMut(&[String], &Element) -> Result<(), ImportError>,
) -> Result<(), ImportError> {
    let text = std::str::from_utf8(xml).map_err(|error| ImportError::Malformed {
        line: Some(line_at(xml, error.valid_up_to())),
    })?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let offset = xml.len() - text.len();
    let mut reader = Reader::from_str(text);
    let mut path: Vec<String> = Vec::new();
    let mut root_closed = false;
    let mut lines = Lines::new(xml);
    let mut line = |reader: &Reader<&[u8]>| {
        let position = usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX);
        lines.at(position.saturating_add(offset))
    };
    loop {
        let event = reader.read_event();
        let at = line(&reader);
        let event = event.map_err(|_| ImportError::Malformed { line: Some(at) })?;
        let malformed = || ImportError::Malformed { line: Some(at) };
        match event {
            Event::Start(start) | Event::Empty(start) if root_closed => {
                let _ = start;
                return Err(malformed());
            }
            Event::Start(start) => {
                limits.check_depth(path.len() + 1, Some(at))?;
                let element = element(&start, limits, at)?;
                visit(&path, &element)?;
                path.push(element.name);
            }
            Event::Empty(start) => {
                limits.check_depth(path.len() + 1, Some(at))?;
                let element = element(&start, limits, at)?;
                visit(&path, &element)?;
                if path.is_empty() {
                    root_closed = true;
                }
            }
            Event::End(_) => {
                // The reader checks that end tags match their start tags.
                path.pop().ok_or_else(malformed)?;
                if path.is_empty() {
                    root_closed = true;
                }
            }
            Event::Text(text) => {
                if !text.xml10_content().trim().is_empty() {
                    return Err(malformed());
                }
            }
            Event::CData(_) | Event::GeneralRef(_) => return Err(malformed()),
            Event::Decl(declaration) => {
                let version = declaration.version().is_ok_and(|version| version == "1.0");
                let encoding = match declaration.encoding() {
                    None => true,
                    // KMyMoney writes UTF-8, and the bytes were read as such.
                    Some(encoding) => {
                        encoding.is_ok_and(|encoding| encoding.eq_ignore_ascii_case("utf-8"))
                    }
                };
                if !(version && encoding) || !path.is_empty() {
                    return Err(malformed());
                }
            }
            // Only KMyMoney's name, with no definitions that could declare
            // entities, before the first element.
            Event::DocType(doctype) => {
                if doctype.xml10_content().trim() != DOCTYPE || !path.is_empty() || root_closed {
                    return Err(malformed());
                }
            }
            Event::Comment(_) | Event::PI(_) => {}
            Event::Eof => break,
        }
    }
    if !path.is_empty() || !root_closed {
        return Err(ImportError::Malformed {
            line: Some(line_at(xml, xml.len())),
        });
    }
    Ok(())
}

/// The element `start` begins, with its attributes unescaped and within
/// the limits.
fn element(start: &BytesStart<'_>, limits: &Limits, line: u64) -> Result<Element, ImportError> {
    let malformed = || ImportError::Malformed { line: Some(line) };
    let name = start.name().into_inner().to_owned();
    let mut attributes = Vec::new();
    for attribute in start.attributes() {
        let attribute = attribute.map_err(|_| malformed())?;
        let key = attribute.key.into_inner().to_owned();
        // The five predefined entities and character references only: the
        // file defines no others.
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|_| malformed())?
            .into_owned();
        limits.check_text(&value, Some(line))?;
        attributes.push((key, value));
    }
    Ok(Element {
        name,
        attributes,
        line,
    })
}

/// Finds the lines of positions that only move forward, counting the
/// line breaks between one and the next, so a large file is read once.
struct Lines<'a> {
    input: &'a [u8],
    position: usize,
    line: u64,
}

impl<'a> Lines<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            position: 0,
            line: 1,
        }
    }

    /// The line, counted from 1, of the byte at `position`, which must not
    /// be before the one asked for last.
    fn at(&mut self, position: usize) -> u64 {
        let position = position.min(self.input.len());
        if position < self.position {
            return line_at(self.input, position);
        }
        let breaks = self.input[self.position..position]
            .iter()
            .filter(|&&byte| byte == b'\n')
            .count();
        self.line = self
            .line
            .saturating_add(u64::try_from(breaks).unwrap_or(u64::MAX));
        self.position = position;
        self.line
    }
}

/// The line, counted from 1, of the byte at `position`.
fn line_at(input: &[u8], position: usize) -> u64 {
    let before = &input[..position.min(input.len())];
    let breaks = before.iter().filter(|&&byte| byte == b'\n').count();
    u64::try_from(breaks).unwrap_or(u64::MAX).saturating_add(1)
}

#[cfg(test)]
mod tests;
