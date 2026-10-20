// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A small JSON writer that keeps keys in the order given, so the file reads
//! like `docs/EXPORT_FORMAT.md` shows it, and lays it out for people as well
//! as programs: a list or object that holds no other non-empty one stays on
//! one line, everything else gets a line per entry, indented by two spaces.

use std::io::{self, Write};

use kassensturz_core::Date;

/// A JSON value, with an object's keys in the order they are written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Json {
    Null,
    Bool(bool),
    Number(u64),
    Text(String),
    List(Vec<Json>),
    Object(Vec<(&'static str, Json)>),
}

impl Json {
    pub(crate) fn text(text: &str) -> Self {
        Self::Text(text.to_owned())
    }

    pub(crate) fn optional_text(text: Option<&str>) -> Self {
        text.map_or(Self::Null, Self::text)
    }

    /// An ID in the usual hyphenated form.
    pub(crate) fn id(id: impl ToString) -> Self {
        Self::Text(id.to_string())
    }

    pub(crate) fn optional_id(id: Option<impl ToString>) -> Self {
        id.map_or(Self::Null, Self::id)
    }

    /// An ISO 8601 date, e.g. `2026-10-05` (D-061).
    pub(crate) fn date(date: Date) -> Self {
        Self::Text(date.to_string())
    }

    pub(crate) fn optional_date(date: Option<Date>) -> Self {
        date.map_or(Self::Null, Self::date)
    }

    /// Writes the value as the whole of a file, without a line break at the
    /// end.
    pub(crate) fn write(&self, out: &mut dyn Write) -> io::Result<()> {
        self.write_at(out, 0)
    }

    /// Writes the value at the current position of a line indented by
    /// `indent` spaces.
    fn write_at(&self, out: &mut dyn Write, indent: usize) -> io::Result<()> {
        match self {
            Self::Null => out.write_all(b"null"),
            Self::Bool(value) => write!(out, "{value}"),
            Self::Number(value) => write!(out, "{value}"),
            // serde_json escapes quotes, backslashes and control characters.
            Self::Text(text) => serde_json::to_writer(&mut *out, text).map_err(io::Error::from),
            Self::List(items) => self.write_container(
                out,
                indent,
                b"[",
                b"]",
                items.len(),
                |out, index, indent| items[index].write_at(out, indent),
            ),
            Self::Object(fields) => self.write_container(
                out,
                indent,
                b"{",
                b"}",
                fields.len(),
                |out, index, indent| {
                    let (key, value) = &fields[index];
                    serde_json::to_writer(&mut *out, key).map_err(io::Error::from)?;
                    out.write_all(b": ")?;
                    value.write_at(out, indent)
                },
            ),
        }
    }

    /// Writes a list or object of `len` entries, each written by `entry`.
    fn write_container(
        &self,
        out: &mut dyn Write,
        indent: usize,
        open: &[u8],
        close: &[u8],
        len: usize,
        entry: impl Fn(&mut dyn Write, usize, usize) -> io::Result<()>,
    ) -> io::Result<()> {
        out.write_all(open)?;
        if self.on_one_line() {
            for index in 0..len {
                if index > 0 {
                    out.write_all(b", ")?;
                }
                entry(out, index, indent)?;
            }
        } else {
            for index in 0..len {
                out.write_all(if index > 0 { b",\n" } else { b"\n" })?;
                write!(out, "{:width$}", "", width = indent + 2)?;
                entry(out, index, indent + 2)?;
            }
            write!(out, "\n{:width$}", "", width = indent)?;
        }
        out.write_all(close)
    }

    /// Whether the value is written on one line: it holds no list or object
    /// that has entries.
    fn on_one_line(&self) -> bool {
        let nested = |value: &Self| match value {
            Self::List(items) => !items.is_empty(),
            Self::Object(fields) => !fields.is_empty(),
            _ => false,
        };
        match self {
            Self::List(items) => !items.iter().any(nested),
            Self::Object(fields) => !fields.iter().any(|(_, value)| nested(value)),
            _ => true,
        }
    }
}
