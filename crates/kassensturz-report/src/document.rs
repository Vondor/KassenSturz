// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The dashboard and view document (M6a.7, D-278, D-281): reading it from
//! text the book kept or a file gave, and writing it back.
//!
//! A document is untrusted input. It is checked before anything of it is
//! used: its size, its nesting, two equal keys in one object (which
//! `serde_json` would silently resolve to the last one, so a hostile file
//! could show a reader something other than what the checks saw), the number
//! of components, the format and version, and then every component against
//! the property schema. A component of a type this version doesn't know is
//! kept as it is, so the document still opens and saves without losing it;
//! the screen shows a placeholder for it (D-278).

use std::collections::HashSet;

use kassensturz_core::{Date, ViewKind};
use serde_json::{Map, Value};

use crate::filter::{Dimension, check_list};
use crate::{Asked, Component, Filters, ReportError};

/// The `format` value every document carries.
pub const FORMAT: &str = "kassensturz-dashboard";
/// The version this program writes and the newest it reads. Version 2
/// (D-351) added `filters.accounts`, `filters.categories` and the
/// components' `params.accounts`; version 1 documents are still read.
pub const VERSION: u32 = 2;
/// The largest document, in bytes (D-281).
pub const MAX_BYTES: usize = 1 << 20;
/// The deepest nesting of objects and arrays (D-281).
pub const MAX_DEPTH: usize = 8;
/// The most components in one document (D-281).
pub const MAX_COMPONENTS: usize = 100;

/// The ID of the one component of a view.
const VIEW_COMPONENT_ID: &str = "c1";
/// The longest name or component ID, in characters (D-281).
pub const MAX_NAME_CHARS: usize = 200;
/// The largest number in a component's frame.
const MAX_FRAME: u64 = 1000;

/// The periods a document can name, by the fixed names the screens use.
pub const PERIODS: [&str; 5] = [
    "this-month",
    "last-month",
    "last-12-months",
    "this-year",
    "last-year",
];

/// Where a component sits on a dashboard's grid, in cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    /// The column of the left edge, from 0.
    pub col: u32,
    /// The row of the top edge, from 0.
    pub row: u32,
    /// How many columns wide, at least 1.
    pub cols: u32,
    /// How many rows high, at least 1.
    pub rows: u32,
}

/// What a document holds in one place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    /// A component this version knows.
    Component(Component),
    /// A component of a type this version doesn't know, kept as it was.
    Unknown(Value),
}

/// One component of a document, with its place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The component's ID in the document: letters, digits, `_` and `-`.
    pub id: String,
    /// The component.
    pub item: Item,
    /// Where it sits; always `None` in a view.
    pub frame: Option<Frame>,
}

/// A saved view or dashboard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    /// A view has exactly one component and no frame.
    pub kind: ViewKind,
    /// The name; never empty, at most 200 characters.
    pub name: String,
    /// One of [`PERIODS`] for the whole document, if it names one: the
    /// filter bar's period (`filters.period`).
    pub period: Option<String>,
    /// The filter bar's accounts (`filters.accounts`, D-351): IDs of own
    /// accounts, written as text. Empty narrows nothing.
    pub accounts: Vec<String>,
    /// The filter bar's categories (`filters.categories`, D-351): IDs of
    /// categories or My groups, written as text. Empty narrows nothing.
    pub categories: Vec<String>,
    /// The components, in the order of the document.
    pub entries: Vec<Entry>,
    /// Where a view came from, if another program's report made it
    /// (`origin`, D-450; M6d.6a). Only a view has one.
    pub origin: Option<ViewOrigin>,
}

/// Where a view came from (`origin`, D-450): a KMyMoney report, by its ID in
/// the file, and the day it was imported. A later import of the same file
/// recognises its views by it (M6d.6b). It is plain text and never copied:
/// not onto a dashboard and not into a dashboard file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewOrigin {
    /// The program, `kmymoney` ([`ViewOrigin::KMYMONEY`]), the only one so far.
    pub from: String,
    /// The report's ID in the file: on one line, at most 64 characters.
    pub report: String,
    /// The day it was imported.
    pub imported: Date,
}

impl ViewOrigin {
    /// The `from` of a view made from a KMyMoney report.
    pub const KMYMONEY: &'static str = "kmymoney";

    /// The origin of a view made from the KMyMoney report `report` on the
    /// day `imported`.
    pub fn kmymoney(report: &str, imported: Date) -> Self {
        Self {
            from: Self::KMYMONEY.to_owned(),
            report: report.to_owned(),
            imported,
        }
    }
}

/// The document's `origin` (D-450): exactly `from`, `report` and
/// `imported`, the program one this version knows, the ID short and on one
/// line, the day an ISO date.
fn read_origin(value: &Value) -> Result<ViewOrigin, ReportError> {
    let bad = || invalid("bad-origin", "origin");
    let object = value.as_object().ok_or_else(bad)?;
    if object.len() != 3 {
        return Err(bad());
    }
    let from = text_of(object, "from").map_err(|_| bad())?;
    let report = text_of(object, "report").map_err(|_| bad())?;
    let imported = text_of(object, "imported").map_err(|_| bad())?;
    if from != ViewOrigin::KMYMONEY
        || report.is_empty()
        || report.chars().count() > MAX_ID_CHARS
        || report.chars().any(char::is_control)
        || imported.len() != 10
    {
        return Err(bad());
    }
    Ok(ViewOrigin {
        from: from.to_owned(),
        report: report.to_owned(),
        imported: imported.parse().map_err(|_| bad())?,
    })
}

fn invalid(reason: &'static str, path: &str) -> ReportError {
    ReportError::DocumentInvalid {
        reason,
        path: path.to_owned(),
    }
}

/// Refuses text in which an object has the same key twice or the nesting is
/// deeper than [`MAX_DEPTH`]. The text is valid JSON already, so only the
/// brackets, commas and strings need to be followed.
fn guard(text: &str) -> Result<(), ReportError> {
    // One entry per open bracket: the keys seen so far if it is an object.
    let mut open: Vec<Option<HashSet<String>>> = Vec::new();
    // Whether the next string is a key of the innermost object.
    let mut key_next = false;
    let mut chars = text.char_indices();
    while let Some((start, c)) = chars.next() {
        match c {
            '{' | '[' => {
                if open.len() >= MAX_DEPTH {
                    return Err(invalid("too-deep", ""));
                }
                key_next = c == '{';
                open.push((c == '{').then(HashSet::new));
            }
            '}' | ']' => {
                open.pop();
                key_next = false;
            }
            ',' => key_next = matches!(open.last(), Some(Some(_))),
            '"' => {
                let mut escaped = false;
                let mut end = start;
                for (at, c) in chars.by_ref() {
                    end = at;
                    if escaped {
                        escaped = false;
                    } else if c == '\\' {
                        escaped = true;
                    } else if c == '"' {
                        break;
                    }
                }
                if key_next {
                    key_next = false;
                    // Decoded, so "a" and "a" are the same key.
                    let key: String = serde_json::from_str(&text[start..=end])
                        .map_err(|_| invalid("not-json", ""))?;
                    if let Some(Some(keys)) = open.last_mut()
                        && !keys.insert(key.clone())
                    {
                        return Err(invalid("duplicate-key", &key));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn text_of<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a str, ReportError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("wrong-type", key))
}

fn read_frame(value: &Value, path: &str) -> Result<Frame, ReportError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid("wrong-type", path))?;
    if let Some(key) = object
        .keys()
        .find(|key| !matches!(key.as_str(), "col" | "row" | "cols" | "rows"))
    {
        return Err(invalid("unknown-property", &format!("{path}.{key}")));
    }
    let cell = |key: &str, least: u64| -> Result<u32, ReportError> {
        let at = format!("{path}.{key}");
        let number = object
            .get(key)
            .and_then(Value::as_u64)
            .ok_or_else(|| invalid("wrong-type", &at))?;
        if (least..=MAX_FRAME).contains(&number) {
            Ok(u32::try_from(number).unwrap_or(0))
        } else {
            Err(invalid("bad-frame", &at))
        }
    };
    Ok(Frame {
        col: cell("col", 0)?,
        row: cell("row", 0)?,
        cols: cell("cols", 1)?,
        rows: cell("rows", 1)?,
    })
}

fn read_entry(value: &Value, index: usize) -> Result<Entry, ReportError> {
    let path = format!("components.{index}");
    let object = value
        .as_object()
        .ok_or_else(|| invalid("wrong-type", &path))?;
    let id = text_of(object, "id").map_err(|_| invalid("bad-id", &format!("{path}.id")))?;
    let valid_id = !id.is_empty()
        && id.chars().count() <= MAX_NAME_CHARS
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !valid_id {
        return Err(invalid("bad-id", &format!("{path}.id")));
    }
    let frame = object
        .get("frame")
        .map(|frame| read_frame(frame, &format!("{path}.frame")))
        .transpose()?;
    let known = crate::ComponentType::from_id(text_of(object, "type")?).is_some();
    let item = if known {
        Item::Component(Component::from_value(value)?)
    } else {
        Item::Unknown(value.clone())
    };
    Ok(Entry {
        id: id.to_owned(),
        item,
        frame,
    })
}

/// The longest ID in a filter's list, in characters, as in a component's
/// list (D-281).
const MAX_ID_CHARS: usize = 64;

/// One list of the filter bar: text IDs, each short and on one line, at most
/// [`crate::MAX_FILTER_IDS`] of them and none twice.
fn read_ids(value: &Value, dimension: Dimension) -> Result<Vec<String>, ReportError> {
    let path = format!("filters.{}", dimension.word());
    let ids = value
        .as_array()
        .ok_or_else(|| invalid("wrong-type", &path))?
        .iter()
        .map(|id| id.as_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| invalid("wrong-type", &path))?;
    if ids.iter().any(|id| {
        id.is_empty() || id.chars().count() > MAX_ID_CHARS || id.chars().any(char::is_control)
    }) {
        return Err(invalid("bad-text", &path));
    }
    check_list(dimension, &ids)?;
    Ok(ids)
}

/// The document's `filters`: a period, accounts and categories, each
/// optional.
fn read_filters(value: &Value) -> Result<Filters, ReportError> {
    let filters = value
        .as_object()
        .ok_or_else(|| invalid("wrong-type", "filters"))?;
    if let Some(key) = filters
        .keys()
        .find(|key| !matches!(key.as_str(), "period" | "accounts" | "categories"))
    {
        return Err(invalid("unknown-property", &format!("filters.{key}")));
    }
    let period = match filters.get("period") {
        None => None,
        Some(Value::String(period))
            if PERIODS.contains(&period.as_str()) || crate::read_range_word(period).is_some() =>
        {
            Some(period.clone())
        }
        Some(_) => return Err(invalid("bad-choice", "filters.period")),
    };
    let list = |dimension: Dimension| {
        filters
            .get(dimension.word())
            .map_or(Ok(Vec::new()), |value| read_ids(value, dimension))
    };
    Ok(Filters {
        period,
        accounts: list(Dimension::Accounts)?,
        categories: list(Dimension::Categories)?,
    })
}

impl Document {
    /// A view: one component, no frame. The name and period are checked
    /// when the text is read back with [`Document::read`].
    pub fn view(name: &str, period: Option<&str>, component: Component) -> Self {
        Self {
            kind: ViewKind::View,
            name: name.to_owned(),
            period: period.map(str::to_owned),
            accounts: Vec::new(),
            categories: Vec::new(),
            entries: vec![Entry {
                id: VIEW_COMPONENT_ID.to_owned(),
                item: Item::Component(component),
                frame: None,
            }],
            origin: None,
        }
    }

    /// Reads and checks a document.
    pub fn read(text: &str) -> Result<Self, ReportError> {
        if text.len() > MAX_BYTES {
            return Err(ReportError::DocumentTooBig {
                max_bytes: MAX_BYTES,
            });
        }
        let value: Value = serde_json::from_str(text).map_err(|_| invalid("not-json", ""))?;
        guard(text)?;
        let object = value
            .as_object()
            .ok_or_else(|| invalid("not-an-object", ""))?;
        if let Some(key) = object.keys().find(|key| {
            !matches!(
                key.as_str(),
                "format" | "version" | "kind" | "name" | "filters" | "components" | "origin"
            )
        }) {
            return Err(invalid("unknown-property", key));
        }
        if object.get("format").and_then(Value::as_str) != Some(FORMAT) {
            return Err(invalid("wrong-format", "format"));
        }
        let version = object
            .get("version")
            .and_then(Value::as_i64)
            .filter(|&version| version >= 1)
            .ok_or_else(|| invalid("bad-version", "version"))?;
        if version > i64::from(VERSION) {
            return Err(ReportError::UnsupportedDocumentVersion { version });
        }
        let kind = match object.get("kind").and_then(Value::as_str) {
            Some("view") => ViewKind::View,
            Some("dashboard") => ViewKind::Dashboard,
            _ => return Err(invalid("bad-kind", "kind")),
        };
        let name = text_of(object, "name").map_err(|_| invalid("bad-name", "name"))?;
        if name.is_empty() || name != name.trim() || name.chars().count() > MAX_NAME_CHARS {
            return Err(invalid("bad-name", "name"));
        }
        let Filters {
            period,
            accounts,
            categories,
        } = match object.get("filters") {
            None => Filters::default(),
            Some(filters) => read_filters(filters)?,
        };
        let components = match object.get("components") {
            None => &Vec::new(),
            Some(Value::Array(components)) => components,
            Some(_) => return Err(invalid("wrong-type", "components")),
        };
        if components.len() > MAX_COMPONENTS {
            return Err(invalid("too-many-components", "components"));
        }
        let entries: Vec<Entry> = components
            .iter()
            .enumerate()
            .map(|(index, component)| read_entry(component, index))
            .collect::<Result<_, _>>()?;
        let mut ids = HashSet::new();
        if let Some(position) = entries.iter().position(|entry| !ids.insert(&entry.id)) {
            return Err(invalid(
                "duplicate-id",
                &format!("components.{position}.id"),
            ));
        }
        if kind == ViewKind::Dashboard {
            crate::grid::check(&entries)?;
        }
        let origin = match object.get("origin") {
            None => None,
            Some(_) if kind != ViewKind::View => return Err(invalid("bad-origin", "origin")),
            Some(value) => Some(read_origin(value)?),
        };
        if kind == ViewKind::View {
            if entries.len() != 1 {
                return Err(invalid("view-needs-one-component", "components"));
            }
            if entries[0].frame.is_some() {
                return Err(invalid("view-has-frame", "components.0.frame"));
            }
        }
        Ok(Self {
            kind,
            name: name.to_owned(),
            period,
            accounts,
            categories,
            entries,
            origin,
        })
    }

    /// What the document's filter bar holds (`filters`, D-351).
    pub fn filters(&self) -> Filters {
        Filters {
            period: self.period.clone(),
            accounts: self.accounts.clone(),
            categories: self.categories.clone(),
        }
    }

    /// What the component with ID `id` asks for (D-350): its own settings,
    /// else for the period the period box it lies completely in, else what
    /// `bar` holds, the document's own [`Document::filters`] or a choice the
    /// user made for now. `None` for an ID the document doesn't have, a
    /// component of a type this version doesn't know, and one that shows no
    /// figures.
    pub fn asked(&self, id: &str, bar: &Filters) -> Option<Asked> {
        let entry = self.entries.iter().find(|entry| entry.id == id)?;
        let Item::Component(component) = &entry.item else {
            return None;
        };
        let in_box = entry.frame.and_then(|frame| {
            self.entries.iter().find_map(|other| match &other.item {
                Item::Component(period_box)
                    if crate::grid::is_box(&other.item)
                        && other.id != entry.id
                        && other.frame.is_some_and(|outer| outer.contains(frame)) =>
                {
                    match period_box.get("period") {
                        Some(crate::Setting::Word(word)) => Some(word.as_str()),
                        _ => None,
                    }
                }
                _ => None,
            })
        });
        Asked::new(component, in_box, bar)
    }

    /// The document as the text the book keeps. Keys are in alphabetical
    /// order, so the same document always gives the same text.
    pub fn to_json(&self) -> String {
        let mut object = Map::new();
        object.insert("format".into(), FORMAT.into());
        object.insert("version".into(), VERSION.into());
        object.insert(
            "kind".into(),
            match self.kind {
                ViewKind::View => "view",
                ViewKind::Dashboard => "dashboard",
            }
            .into(),
        );
        object.insert("name".into(), self.name.clone().into());
        let mut filters = Map::new();
        if let Some(period) = &self.period {
            filters.insert("period".into(), period.clone().into());
        }
        for (key, ids) in [
            ("accounts", &self.accounts),
            ("categories", &self.categories),
        ] {
            if !ids.is_empty() {
                let ids = ids.iter().cloned().map(Value::String).collect();
                filters.insert(key.into(), Value::Array(ids));
            }
        }
        if !filters.is_empty() {
            object.insert("filters".into(), Value::Object(filters));
        }
        let entries = self.entries.iter().map(|entry| {
            let mut value = match &entry.item {
                Item::Component(component) => component.to_value(),
                Item::Unknown(value) => value.clone(),
            };
            if let Value::Object(map) = &mut value {
                map.insert("id".into(), entry.id.clone().into());
                map.remove("frame");
                if let Some(frame) = entry.frame {
                    let mut cells = Map::new();
                    for (key, number) in [
                        ("col", frame.col),
                        ("row", frame.row),
                        ("cols", frame.cols),
                        ("rows", frame.rows),
                    ] {
                        cells.insert(key.into(), number.into());
                    }
                    map.insert("frame".into(), Value::Object(cells));
                }
            }
            value
        });
        object.insert("components".into(), Value::Array(entries.collect()));
        if let Some(origin) = &self.origin {
            let mut from = Map::new();
            from.insert("from".into(), origin.from.clone().into());
            from.insert("report".into(), origin.report.clone().into());
            from.insert("imported".into(), origin.imported.to_string().into());
            object.insert("origin".into(), Value::Object(from));
        }
        Value::Object(object).to_string()
    }
}
