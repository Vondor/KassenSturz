// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The dashboard file, version 1 (M6d.3, D-441, D-443): one dashboard
//! written so it can be given to another book. `docs/DASHBOARD_FILE.md` is
//! the format; this module writes it.
//!
//! The file holds no money and no ID of the book (D-432). Every place of the
//! document that holds an ID (the schema's ID lists, the members of
//! `params.columns`, `filters.accounts` and `filters.categories`) holds a
//! reference key `ref:N` instead, and `references` says what each key names:
//! a category by its path of names from the top, a group or an account by
//! its name. An ID the book no longer knows, or whose name the reader would
//! refuse, is left out and counted (D-458), so the file is always one the
//! reader takes.
//!
//! The same dashboard in the same book gives the same file byte for byte:
//! keys in a fixed order, two spaces per level, references numbered in the
//! order they are first met.

use std::collections::HashMap;
use std::fmt::Write as _;

use kassensturz_store::Book;
use serde_json::{Map, Value};

use crate::{ComponentType, Document, Kind, ReportError};

/// The `format` value every dashboard file carries.
pub const DASHBOARD_FILE_FORMAT: &str = "kassensturz-dashboard-file";
/// The version this program writes and the newest it reads.
pub const DASHBOARD_FILE_VERSION: u32 = 1;
/// The largest file, in bytes.
pub const MAX_DASHBOARD_FILE_BYTES: usize = 1 << 20;
/// The most references in one file.
pub const MAX_REFERENCES: usize = 500;
/// The most names in a category's path.
pub const MAX_PATH_NAMES: usize = 16;
/// The longest name in a reference, in characters.
pub const MAX_REFERENCE_NAME_CHARS: usize = 200;
/// The longest `application` text, in characters.
pub const MAX_APPLICATION_CHARS: usize = 100;
/// What the suggested file name ends with.
pub const DASHBOARD_FILE_EXTENSION: &str = ".dashboard.json";

/// What a reference key names.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Reference {
    /// A category, by the names from the top-level category down.
    Category(Vec<String>),
    /// One of My groups, by its name.
    Group(String),
    /// One of the user's own accounts, by its name.
    Account(String),
}

/// A dashboard file as written by [`write_dashboard_file`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DashboardFile {
    /// The file's text: UTF-8, no byte order mark, a line break at the end.
    pub text: String,
    /// How many references the file holds.
    pub references: usize,
    /// How many IDs were left out, because the book no longer has what they
    /// named or its name can't be written (D-458).
    pub left_out: usize,
}

/// A name the reader takes: 1 to 200 characters, no control characters and
/// no spaces around it.
fn writable(name: &str) -> bool {
    !name.is_empty()
        && name == name.trim()
        && name.chars().count() <= MAX_REFERENCE_NAME_CHARS
        && !name.chars().any(char::is_control)
}

/// Which list an ID sits in, and so what it may name.
#[derive(Clone, Copy)]
enum List {
    /// The user's own accounts.
    Accounts,
    /// Categories and My groups.
    Categories,
}

/// The book's names by ID, written as text in small letters.
struct Names {
    accounts: HashMap<String, String>,
    categories: HashMap<String, Vec<String>>,
    groups: HashMap<String, String>,
}

impl Names {
    fn read(book: &Book) -> Result<Self, ReportError> {
        let accounts = book
            .all_accounts()?
            .into_iter()
            .filter(|account| account.kind.is_account())
            .map(|account| (account.id.to_string(), account.name))
            .collect();
        let categories = book
            .categories()?
            .into_iter()
            .map(|entry| (entry.category.id.to_string(), entry.path))
            .collect();
        let groups = book
            .category_groups()?
            .into_iter()
            .map(|group| (group.id.to_string(), group.name))
            .collect();
        Ok(Self {
            accounts,
            categories,
            groups,
        })
    }

    /// What `id` names in a list of `list`, if it can be written.
    fn reference(&self, id: &str, list: List) -> Option<Reference> {
        let id = id.to_ascii_lowercase();
        let reference = match list {
            List::Accounts => Reference::Account(self.accounts.get(&id)?.clone()),
            List::Categories => match self.categories.get(&id) {
                Some(path) => Reference::Category(path.clone()),
                None => Reference::Group(self.groups.get(&id)?.clone()),
            },
        };
        let fits = match &reference {
            Reference::Category(path) => {
                !path.is_empty()
                    && path.len() <= MAX_PATH_NAMES
                    && path.iter().all(|name| writable(name))
            }
            Reference::Group(name) | Reference::Account(name) => writable(name),
        };
        fits.then_some(reference)
    }
}

/// The references met so far, in order.
struct Refs<'a> {
    names: &'a Names,
    keys: HashMap<Reference, String>,
    order: Vec<(String, Reference)>,
    left_out: usize,
}

impl Refs<'_> {
    /// Replaces each ID of the list `value` by its reference key, leaving
    /// out what can't be written.
    fn map_list(&mut self, value: Option<&mut Value>, list: List) {
        let Some(Value::Array(ids)) = value else {
            return;
        };
        let old = std::mem::take(ids);
        for id in old {
            match id.as_str().and_then(|id| self.key(id, list)) {
                Some(key) if !ids.contains(&Value::String(key.clone())) => {
                    ids.push(Value::String(key));
                }
                // Two IDs that name the same thing: the list keeps one.
                Some(_) => {}
                None => self.left_out += 1,
            }
        }
    }

    fn key(&mut self, id: &str, list: List) -> Option<String> {
        let reference = self.names.reference(id, list)?;
        if let Some(key) = self.keys.get(&reference) {
            return Some(key.clone());
        }
        if self.order.len() >= MAX_REFERENCES {
            return None;
        }
        let key = format!("ref:{}", self.order.len() + 1);
        self.keys.insert(reference.clone(), key.clone());
        self.order.push((key.clone(), reference));
        Some(key)
    }
}

/// Replaces the IDs of one component, in the order the file shows them:
/// `params` before `props`, keys alphabetically.
fn map_component(refs: &mut Refs<'_>, component: &mut Value) {
    let Some(kind) = component
        .get("type")
        .and_then(Value::as_str)
        .and_then(ComponentType::from_id)
    else {
        // A type this version doesn't know is kept as it is (D-278).
        return;
    };
    let mut properties: Vec<_> = kind.properties().iter().collect();
    properties.sort_by_key(|property| (property.place.key(), property.key));
    for property in properties {
        let value = component
            .get_mut(property.place.key())
            .and_then(|place| place.get_mut(property.key));
        match property.kind {
            Kind::Ids => refs.map_list(value, List::Categories),
            Kind::Accounts => refs.map_list(value, List::Accounts),
            Kind::Columns => {
                if let Some(Value::Array(columns)) = value {
                    for column in columns {
                        refs.map_list(column.get_mut("members"), List::Categories);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Where in the file a value sits, which decides the order of its keys.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ctx {
    File,
    References,
    Reference,
    Document,
    Filters,
    Components,
    Component,
    Params,
    Columns,
    Column,
    Frame,
    Other,
}

impl Ctx {
    /// The keys that come first, in this order; the others follow
    /// alphabetically.
    const fn first(self) -> &'static [&'static str] {
        match self {
            Self::File => &[
                "format",
                "version",
                "application",
                "references",
                "dashboard",
            ],
            Self::Reference => &["kind", "path", "name"],
            Self::Document => &[
                "format",
                "version",
                "kind",
                "name",
                "template",
                "filters",
                "components",
            ],
            Self::Filters => &["period", "accounts", "categories"],
            Self::Component => &["id", "type", "dataset", "params", "props", "frame"],
            Self::Column => &["name", "members", "formula"],
            Self::Frame => &["col", "row", "cols", "rows"],
            _ => &[],
        }
    }

    fn child(self, key: &str) -> Self {
        match (self, key) {
            (Self::File, "references") => Self::References,
            (Self::File, "dashboard") => Self::Document,
            (Self::References, _) => Self::Reference,
            (Self::Document, "filters") => Self::Filters,
            (Self::Document, "components") => Self::Components,
            (Self::Component, "params") => Self::Params,
            (Self::Component, "frame") => Self::Frame,
            (Self::Params, "columns") => Self::Columns,
            _ => Self::Other,
        }
    }

    const fn element(self) -> Self {
        match self {
            Self::Components => Self::Component,
            Self::Columns => Self::Column,
            _ => Self::Other,
        }
    }

    /// The keys of `object` in the order the file writes them.
    fn order(self, object: &Map<String, Value>) -> Vec<&String> {
        let mut keys: Vec<&String> = object.keys().collect();
        if self == Self::References {
            // `ref:2` before `ref:10`.
            keys.sort_by_key(|key| {
                key.strip_prefix("ref:")
                    .and_then(|number| number.parse::<usize>().ok())
            });
            return keys;
        }
        let first = self.first();
        keys.sort_by_key(|key| {
            (
                first
                    .iter()
                    .position(|known| known == key)
                    .unwrap_or(first.len()),
                key.as_str(),
            )
        });
        keys
    }
}

/// Writes `value` as the file shows it: two spaces per level.
fn write_value(out: &mut String, value: &Value, ctx: Ctx, level: usize) {
    let indent = |out: &mut String, level: usize| out.push_str(&"  ".repeat(level));
    match value {
        Value::Object(object) if !object.is_empty() => {
            out.push_str("{\n");
            let keys = ctx.order(object);
            for (index, key) in keys.iter().enumerate() {
                indent(out, level + 1);
                out.push_str(&Value::String((*key).clone()).to_string());
                out.push_str(": ");
                write_value(out, &object[key.as_str()], ctx.child(key), level + 1);
                out.push_str(if index + 1 < keys.len() { ",\n" } else { "\n" });
            }
            indent(out, level);
            out.push('}');
        }
        Value::Array(items) if !items.is_empty() => {
            out.push_str("[\n");
            for (index, item) in items.iter().enumerate() {
                indent(out, level + 1);
                write_value(out, item, ctx.element(), level + 1);
                out.push_str(if index + 1 < items.len() { ",\n" } else { "\n" });
            }
            indent(out, level);
            out.push(']');
        }
        // Empty objects and arrays, text, numbers, yes and no.
        other => {
            let _ = write!(out, "{other}");
        }
    }
}

fn reference_value(reference: &Reference) -> Value {
    let mut object = Map::new();
    let (kind, key, value) = match reference {
        Reference::Category(path) => (
            "category",
            "path",
            Value::Array(path.iter().cloned().map(Value::String).collect()),
        ),
        Reference::Group(name) => ("group", "name", Value::String(name.clone())),
        Reference::Account(name) => ("account", "name", Value::String(name.clone())),
    };
    object.insert("kind".into(), kind.into());
    object.insert(key.into(), value);
    Value::Object(object)
}

/// `application` as the file may hold it: plain text of at most
/// [`MAX_APPLICATION_CHARS`] characters on one line.
fn application_text(application: &str) -> String {
    let text: String = application
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_APPLICATION_CHARS)
        .collect();
    text.trim().to_owned()
}

/// Writes `document`, a dashboard of `book`, as a dashboard file.
/// `application` names the program and build that writes it; it is
/// information only. The document's default mark (D-448) is never written.
/// Fails with [`ReportError::DocumentTooBig`] if the file would be bigger
/// than [`MAX_DASHBOARD_FILE_BYTES`], and with the book's error if its names
/// can't be read.
pub fn write_dashboard_file(
    book: &Book,
    document: &Document,
    application: &str,
) -> Result<DashboardFile, ReportError> {
    let names = Names::read(book)?;
    let mut refs = Refs {
        names: &names,
        keys: HashMap::new(),
        order: Vec::new(),
        left_out: 0,
    };
    let mut dashboard: Value =
        serde_json::from_str(&document.to_json()).expect("a document writes valid JSON");
    let Value::Object(object) = &mut dashboard else {
        unreachable!("a document is an object")
    };
    // A file never changes which dashboard opens (D-441, D-448), and never
    // says where a view came from (D-450).
    object.remove("default");
    object.remove("origin");
    // The filter bar comes before the components in the file.
    if let Some(Value::Object(filters)) = object.get_mut("filters") {
        refs.map_list(filters.get_mut("accounts"), List::Accounts);
        refs.map_list(filters.get_mut("categories"), List::Categories);
        filters.retain(|_, value| value.as_array().is_none_or(|list| !list.is_empty()));
        if filters.is_empty() {
            object.remove("filters");
        }
    }
    if let Some(Value::Array(components)) = object.get_mut("components") {
        for component in components {
            map_component(&mut refs, component);
        }
    }
    let mut references = Map::new();
    for (key, reference) in &refs.order {
        references.insert(key.clone(), reference_value(reference));
    }
    let mut file = Map::new();
    file.insert("format".into(), DASHBOARD_FILE_FORMAT.into());
    file.insert("version".into(), DASHBOARD_FILE_VERSION.into());
    file.insert("application".into(), application_text(application).into());
    file.insert("references".into(), Value::Object(references));
    file.insert("dashboard".into(), dashboard);
    let mut text = String::new();
    write_value(&mut text, &Value::Object(file), Ctx::File, 0);
    text.push('\n');
    if text.len() > MAX_DASHBOARD_FILE_BYTES {
        return Err(ReportError::DocumentTooBig {
            max_bytes: MAX_DASHBOARD_FILE_BYTES,
        });
    }
    Ok(DashboardFile {
        text,
        references: refs.order.len(),
        left_out: refs.left_out,
    })
}

/// The file name suggested for a dashboard called `name`: the name with
/// [`DASHBOARD_FILE_EXTENSION`], each character a file system refuses
/// replaced by `_`.
pub fn dashboard_file_name(name: &str) -> String {
    let stem: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    // A name of dots alone would name a folder.
    let stem = stem.trim();
    let stem = if stem.chars().all(|c| c == '.') {
        "_"
    } else {
        stem
    };
    format!("{stem}{DASHBOARD_FILE_EXTENSION}")
}
