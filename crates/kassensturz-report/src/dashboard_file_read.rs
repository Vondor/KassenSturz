// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading a dashboard file (M6d.4, D-441, D-442, T-51, T-53;
//! `docs/REPORT_DESIGNER_M6D.md` section 4.3).
//!
//! A dashboard file is untrusted input: someone else may have made it. It
//! is read in two steps.
//!
//! 1. [`IncomingDashboardFile::read`] checks the whole file without the book, in
//!    this order: its size, JSON with the depth limit and no key twice, the
//!    header, `references`, every place that holds an ID in a book (it must
//!    hold a listed reference key, never an ID, so a file can't point at
//!    something in a book by chance), then the dashboard by
//!    [`Document::read`]. A file that fails any check is refused whole, with
//!    the reason; nothing is repaired.
//! 2. [`IncomingDashboardFile::resolve`] looks each reference up by its names in the
//!    open book ([`BookNames`]) and gives the document with the book's IDs,
//!    plus what the summary page lists: the names found, the names not found
//!    with the widgets that used them, every list that now shows more than
//!    its author meant, formula names the book lacks and widgets of a type
//!    this version doesn't know. A name that matches nothing, or more than
//!    one thing, is left out. Nothing is guessed (D-432).
//!
//! Nothing here writes to the book; adding the dashboard is the app's one
//! `add_view` after the user said *Add dashboard*.

use std::collections::{BTreeMap, HashSet};

use kassensturz_core::ViewKind;
use kassensturz_store::Book;
use serde_json::{Map, Value};

use crate::dashboard_file::{
    DASHBOARD_FILE_FORMAT, DASHBOARD_FILE_VERSION, MAX_APPLICATION_CHARS, MAX_DASHBOARD_FILE_BYTES,
    MAX_PATH_NAMES, MAX_REFERENCE_NAME_CHARS, MAX_REFERENCES, Reference,
};
use crate::document::{Scanned, scan};
use crate::{Document, Formula, Item, Kind, MAX_NAME_CHARS, ReportError, Setting};

/// The deepest nesting of objects and arrays in a dashboard file; the
/// dashboard inside it has the document's own limit (D-441).
pub const MAX_DASHBOARD_FILE_DEPTH: usize = 10;

const REFERENCE_PREFIX: &str = "ref:";
/// The longest piece of the file's own text a refusal's path repeats, so a
/// hostile key can't make the message huge.
const MAX_SHOWN_CHARS: usize = 64;
/// The lists of a component's `params` that hold IDs (`Kind::Ids`,
/// `Kind::Accounts`); `columns` holds them in each column's `members`.
const ID_LISTS: [&str; 4] = ["groups", "categories", "accounts", "series"];

fn invalid(reason: &'static str, path: impl Into<String>) -> ReportError {
    ReportError::DashboardFileInvalid {
        reason,
        path: path.into(),
    }
}

/// The file's own text for a path, cut short.
fn shown(text: &str) -> String {
    text.chars().take(MAX_SHOWN_CHARS).collect()
}

/// A refusal of the dashboard inside the file, with its path from the
/// file's top.
fn in_dashboard(error: ReportError) -> ReportError {
    match error {
        ReportError::DocumentInvalid { reason, path } => ReportError::DocumentInvalid {
            reason,
            path: if path.is_empty() {
                "dashboard".to_owned()
            } else {
                format!("dashboard.{path}")
            },
        },
        other => other,
    }
}

/// The number of a reference key: `ref:` and a whole number from 1, written
/// without leading zeros, so each key has exactly one spelling.
fn reference_number(text: &str) -> Option<u32> {
    let digits = text.strip_prefix(REFERENCE_PREFIX)?;
    let canonical = !digits.is_empty()
        && !digits.starts_with('0')
        && digits.chars().all(|c| c.is_ascii_digit());
    canonical.then(|| digits.parse().ok()).flatten()
}

/// A name of an account, category or group: 1 to 200 characters, plain
/// text, no control characters, no spaces around it.
fn check_name(name: &Value, path: &str) -> Result<String, ReportError> {
    let name = name.as_str().ok_or_else(|| invalid("bad-text", path))?;
    if name.is_empty() || name != name.trim() || name.chars().any(char::is_control) {
        return Err(invalid("bad-text", path));
    }
    if name.chars().count() > MAX_REFERENCE_NAME_CHARS {
        return Err(invalid("too-long", path));
    }
    Ok(name.to_owned())
}

fn only_keys(object: &Map<String, Value>, allowed: &[&str], path: &str) -> Result<(), ReportError> {
    match object.keys().find(|key| !allowed.contains(&key.as_str())) {
        Some(key) => Err(invalid(
            "unknown-property",
            format!("{path}.{}", shown(key)),
        )),
        None => Ok(()),
    }
}

fn read_reference(value: &Value, path: &str) -> Result<Reference, ReportError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid("bad-reference", path))?;
    match object.get("kind").and_then(Value::as_str) {
        Some("category") => {
            only_keys(object, &["kind", "path"], path)?;
            let at = format!("{path}.path");
            let names = object
                .get("path")
                .and_then(Value::as_array)
                .ok_or_else(|| invalid("bad-reference", &at))?;
            if names.is_empty() {
                return Err(invalid("bad-reference", &at));
            }
            if names.len() > MAX_PATH_NAMES {
                return Err(invalid("path-too-long", &at));
            }
            let names = names
                .iter()
                .enumerate()
                .map(|(index, name)| check_name(name, &format!("{at}.{index}")))
                .collect::<Result<_, _>>()?;
            Ok(Reference::Category(names))
        }
        Some(kind @ ("group" | "account")) => {
            only_keys(object, &["kind", "name"], path)?;
            let at = format!("{path}.name");
            let name = check_name(
                object
                    .get("name")
                    .ok_or_else(|| invalid("bad-reference", &at))?,
                &at,
            )?;
            Ok(if kind == "group" {
                Reference::Group(name)
            } else {
                Reference::Account(name)
            })
        }
        _ => Err(invalid("bad-reference", format!("{path}.kind"))),
    }
}

fn read_references(value: &Value) -> Result<BTreeMap<u32, Reference>, ReportError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid("bad-reference", "references"))?;
    if object.len() > MAX_REFERENCES {
        return Err(invalid("too-many-references", "references"));
    }
    object
        .iter()
        .map(|(key, value)| {
            let path = format!("references.{}", shown(key));
            let number = reference_number(key).ok_or_else(|| invalid("bad-reference", &path))?;
            Ok((number, read_reference(value, &path)?))
        })
        .collect()
}

/// What a list of IDs holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Holds {
    /// The user's own accounts.
    Accounts,
    /// Categories and My groups.
    Categories,
}

/// One list of IDs of the dashboard.
struct ListAt {
    /// The dotted path from the file's top.
    path: String,
    /// The list's key: `accounts`, `categories`, `groups`, `series` or
    /// `members`.
    key: &'static str,
    /// The component's index, `None` for the filter bar.
    widget: Option<usize>,
    holds: Holds,
    /// Whether an empty list means "all of them", so leaving every name out
    /// shows more than the file meant. A column's members don't: an empty
    /// column shows nothing.
    empty_shows_all: bool,
}

/// Calls `visit` for every list of IDs in `dashboard`, in the order of the
/// document: the filter bar, then each component's lists and columns. A
/// value of the wrong type is passed over here and refused by
/// [`Document::read`].
fn each_list(
    dashboard: &mut Map<String, Value>,
    mut visit: impl FnMut(&ListAt, &mut Vec<Value>) -> Result<(), ReportError>,
) -> Result<(), ReportError> {
    if let Some(Value::Object(filters)) = dashboard.get_mut("filters") {
        for (key, holds) in [
            ("accounts", Holds::Accounts),
            ("categories", Holds::Categories),
        ] {
            if let Some(Value::Array(list)) = filters.get_mut(key) {
                let at = ListAt {
                    path: format!("dashboard.filters.{key}"),
                    key,
                    widget: None,
                    holds,
                    empty_shows_all: true,
                };
                visit(&at, list)?;
            }
        }
    }
    let Some(Value::Array(components)) = dashboard.get_mut("components") else {
        return Ok(());
    };
    for (index, component) in components.iter_mut().enumerate() {
        let Some(Value::Object(params)) = component.get_mut("params") else {
            continue;
        };
        for key in ID_LISTS {
            if let Some(Value::Array(list)) = params.get_mut(key) {
                let at = ListAt {
                    path: format!("dashboard.components.{index}.params.{key}"),
                    key,
                    widget: Some(index),
                    holds: if key == "accounts" {
                        Holds::Accounts
                    } else {
                        Holds::Categories
                    },
                    empty_shows_all: true,
                };
                visit(&at, list)?;
            }
        }
        if let Some(Value::Array(columns)) = params.get_mut("columns") {
            for (column_index, column) in columns.iter_mut().enumerate() {
                if let Some(Value::Array(list)) = column.get_mut("members") {
                    let at = ListAt {
                        path: format!(
                            "dashboard.components.{index}.params.columns.{column_index}.members"
                        ),
                        key: "members",
                        widget: Some(index),
                        holds: Holds::Categories,
                        empty_shows_all: false,
                    };
                    visit(&at, list)?;
                }
            }
        }
    }
    Ok(())
}

/// A dashboard file, read and checked, before its names are looked up.
#[derive(Clone, Debug, PartialEq)]
pub struct IncomingDashboardFile {
    /// The program that wrote it, as the file says; information only, never
    /// used to decide anything.
    pub application: Option<String>,
    /// The references by their number.
    pub references: BTreeMap<u32, Reference>,
    /// The dashboard as the file has it, reference keys in its lists.
    dashboard: Map<String, Value>,
    /// The same dashboard read as a document, for its name and widgets.
    document: Document,
}

impl IncomingDashboardFile {
    /// Reads and checks a dashboard file without the book.
    pub fn read(text: &str) -> Result<Self, ReportError> {
        if text.len() > MAX_DASHBOARD_FILE_BYTES {
            return Err(ReportError::DashboardFileTooBig {
                max_bytes: MAX_DASHBOARD_FILE_BYTES,
            });
        }
        let value: Value = serde_json::from_str(text).map_err(|_| invalid("not-json", ""))?;
        scan(text, MAX_DASHBOARD_FILE_DEPTH).map_err(|found| match found {
            Scanned::TooDeep => invalid("too-deep", ""),
            Scanned::DuplicateKey(key) => invalid("duplicate-key", shown(&key)),
            Scanned::NotJson => invalid("not-json", ""),
        })?;
        let Value::Object(object) = value else {
            return Err(invalid("not-an-object", ""));
        };
        if object.get("format").and_then(Value::as_str) != Some(DASHBOARD_FILE_FORMAT) {
            return Err(invalid("wrong-format", "format"));
        }
        // The version before the keys: a newer file may have keys this
        // version doesn't know, and should say it is newer.
        let version = object
            .get("version")
            .and_then(Value::as_i64)
            .filter(|&version| version >= 1)
            .ok_or_else(|| invalid("bad-version", "version"))?;
        if version > i64::from(DASHBOARD_FILE_VERSION) {
            return Err(ReportError::UnsupportedDashboardFileVersion { version });
        }
        if let Some(key) = object.keys().find(|key| {
            !matches!(
                key.as_str(),
                "format" | "version" | "application" | "references" | "dashboard"
            )
        }) {
            return Err(invalid("unknown-property", shown(key)));
        }
        let application = match object.get("application") {
            None => None,
            Some(Value::String(text)) => {
                if text.chars().any(char::is_control) {
                    return Err(invalid("bad-text", "application"));
                }
                if text.chars().count() > MAX_APPLICATION_CHARS {
                    return Err(invalid("too-long", "application"));
                }
                Some(text.clone())
            }
            Some(_) => return Err(invalid("bad-text", "application")),
        };
        let references = match object.get("references") {
            None => BTreeMap::new(),
            Some(value) => read_references(value)?,
        };
        let Some(Value::Object(dashboard)) = object.get("dashboard") else {
            return Err(invalid("not-an-object", "dashboard"));
        };
        // A file can't change which dashboard opens (D-448).
        if dashboard.contains_key("default") {
            return Err(invalid("unknown-property", "dashboard.default"));
        }
        let mut used = HashSet::new();
        let mut checked = dashboard.clone();
        each_list(&mut checked, |at, list| {
            for (index, item) in list.iter().enumerate() {
                let path = format!("{}.{index}", at.path);
                let text = item
                    .as_str()
                    .ok_or_else(|| invalid("bad-reference", &path))?;
                if !text.starts_with(REFERENCE_PREFIX) {
                    return Err(invalid("id-in-file", &path));
                }
                let number =
                    reference_number(text).ok_or_else(|| invalid("bad-reference", &path))?;
                let fits = matches!(
                    (references.get(&number), at.holds),
                    (Some(Reference::Account(_)), Holds::Accounts)
                        | (
                            Some(Reference::Category(_) | Reference::Group(_)),
                            Holds::Categories
                        )
                );
                if !fits {
                    return Err(invalid("bad-reference", &path));
                }
                used.insert(number);
            }
            Ok(())
        })?;
        if let Some(unused) = references.keys().find(|number| !used.contains(*number)) {
            return Err(invalid(
                "unused-reference",
                format!("references.{REFERENCE_PREFIX}{unused}"),
            ));
        }
        let document =
            Document::read(&Value::Object(dashboard.clone()).to_string()).map_err(in_dashboard)?;
        if document.kind != ViewKind::Dashboard {
            return Err(ReportError::DocumentInvalid {
                reason: "bad-kind",
                path: "dashboard.kind".to_owned(),
            });
        }
        Ok(Self {
            application,
            references,
            dashboard: dashboard.clone(),
            document,
        })
    }

    /// The dashboard's name as the file has it.
    pub fn name(&self) -> &str {
        &self.document.name
    }

    /// The dashboard's widgets, in the order of the document.
    pub fn widgets(&self) -> Vec<Widget> {
        self.document.entries.iter().map(Widget::of).collect()
    }

    /// Looks the references up in the book and gives the document with the
    /// book's IDs and what the summary lists.
    pub fn resolve(&self, names: &BookNames) -> Result<Resolved, ReportError> {
        let looked_up: BTreeMap<u32, Result<String, bool>> = self
            .references
            .iter()
            .map(|(number, reference)| (*number, names.find(reference)))
            .collect();
        let widgets = self.widgets();
        let mut missing: BTreeMap<u32, MissingName> = BTreeMap::new();
        let mut shows_more = Vec::new();
        let mut dashboard = self.dashboard.clone();
        each_list(&mut dashboard, |at, list| {
            let mut kept: Vec<Value> = Vec::new();
            let mut seen = HashSet::new();
            for item in list.iter() {
                // Checked by `read`: a listed reference key.
                let Some(number) = item.as_str().and_then(reference_number) else {
                    continue;
                };
                match &looked_up[&number] {
                    Ok(id) => {
                        // Two names can find the same thing, e.g. written
                        // with other capitals; a list names it once.
                        if seen.insert(id.to_lowercase()) {
                            kept.push(Value::String(id.clone()));
                        }
                    }
                    Err(several) => {
                        let note = missing.entry(number).or_insert_with(|| MissingName {
                            reference: self.references[&number].clone(),
                            several: *several,
                            filter_bar: false,
                            widgets: Vec::new(),
                        });
                        match at.widget {
                            None => note.filter_bar = true,
                            Some(index) => {
                                if !note.widgets.contains(&widgets[index]) {
                                    note.widgets.push(widgets[index].clone());
                                }
                            }
                        }
                    }
                }
            }
            if at.empty_shows_all && !list.is_empty() && kept.is_empty() {
                shows_more.push(ShowsMore {
                    widget: at.widget.map(|index| widgets[index].clone()),
                    list: at.key,
                    named: list.len(),
                });
            }
            *list = kept;
            Ok(())
        })?;
        let document =
            Document::read(&Value::Object(dashboard).to_string()).map_err(in_dashboard)?;
        let mut formula_names = Vec::new();
        let mut unknown = Vec::new();
        for (entry, widget) in document.entries.iter().zip(&widgets) {
            let component = match &entry.item {
                Item::Component(component) => component,
                Item::Unknown(_) => {
                    unknown.push(widget.clone());
                    continue;
                }
            };
            let mut columns: Vec<String> = Vec::new();
            let mut formulas: Vec<&str> = Vec::new();
            for (property, setting) in component.settings() {
                match (property.kind, setting) {
                    (Kind::Formula, Setting::Text(text)) => formulas.push(text),
                    (Kind::Columns, Setting::Columns(list)) => {
                        for column in list {
                            columns.push(column.name.trim().to_lowercase());
                            formulas.push(&column.formula);
                        }
                    }
                    _ => {}
                }
            }
            for text in formulas.into_iter().filter(|text| !text.is_empty()) {
                // A formula with a mistake shows it on its widget anyway.
                let Ok(formula) = Formula::parse(text) else {
                    continue;
                };
                for name in formula.name_texts() {
                    let lower = name.trim().to_lowercase();
                    let listed = formula_names
                        .iter()
                        .any(|note: &FormulaName| note.widget == *widget && note.name == name);
                    if !columns.contains(&lower) && !names.knows_formula_name(name) && !listed {
                        formula_names.push(FormulaName {
                            widget: widget.clone(),
                            name: name.to_owned(),
                        });
                    }
                }
            }
        }
        let found = self
            .references
            .iter()
            .filter(|(number, _)| looked_up[*number].is_ok())
            .map(|(_, reference)| reference.clone())
            .collect();
        Ok(Resolved {
            document,
            found,
            missing: missing.into_values().collect(),
            shows_more,
            formula_names,
            unknown,
        })
    }
}

/// A widget of a file's dashboard, so the summary can name it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Widget {
    /// Its ID in the document.
    pub id: String,
    /// Its type as the document writes it, e.g. `chart.donut`; for a type
    /// this version doesn't know, the file's text, cut short.
    pub kind: String,
    /// Its title, empty when it has none.
    pub title: String,
}

impl Widget {
    fn of(entry: &crate::Entry) -> Self {
        let (kind, title) = match &entry.item {
            Item::Component(component) => (
                component.kind().id().to_owned(),
                match component.get("title") {
                    Some(Setting::Text(title)) => title.clone(),
                    _ => String::new(),
                },
            ),
            Item::Unknown(value) => (
                shown(value.get("type").and_then(Value::as_str).unwrap_or("")),
                value
                    .get("props")
                    .and_then(|props| props.get("title"))
                    .and_then(Value::as_str)
                    .map(shown)
                    .unwrap_or_default(),
            ),
        };
        Self {
            id: entry.id.clone(),
            kind,
            title,
        }
    }
}

/// A name of the file this book has nothing for (keyword `not-found`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingName {
    /// What the file names.
    pub reference: Reference,
    /// Whether it matched more than one thing, so none was taken.
    pub several: bool,
    /// Whether the filter bar used it.
    pub filter_bar: bool,
    /// The widgets that used it, in the order of the document.
    pub widgets: Vec<Widget>,
}

/// A list that named things, of which this book has none, so it now narrows
/// nothing and shows more than its author meant (keyword `shows-more`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShowsMore {
    /// The widget, `None` for the filter bar.
    pub widget: Option<Widget>,
    /// Which list: `accounts`, `categories`, `groups` or `series`.
    pub list: &'static str,
    /// How many things it named in the file.
    pub named: usize,
}

/// A name a widget's formula uses that this book has no category or group
/// for; the widget shows its mistake as any formula does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormulaName {
    /// The widget.
    pub widget: Widget,
    /// The name as the formula writes it.
    pub name: String,
}

/// A dashboard file read into a book: the document to add and what the
/// summary page lists before anything is added.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    /// The dashboard with this book's IDs; what *Add dashboard* stores.
    pub document: Document,
    /// The names this book has, in the order of the references.
    pub found: Vec<Reference>,
    /// The names it lacks, in the order of the references.
    pub missing: Vec<MissingName>,
    /// The lists that now show more, in the order of the document.
    pub shows_more: Vec<ShowsMore>,
    /// The formula names it lacks, in the order of the document.
    pub formula_names: Vec<FormulaName>,
    /// The widgets of a type this version doesn't know, shown as the
    /// placeholder (D-278).
    pub unknown: Vec<Widget>,
}

/// The names of a book's categories, groups and accounts, with their IDs as
/// text, for looking a file's references up.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BookNames {
    /// Each category's ID and its names from the top down.
    categories: Vec<(String, Vec<String>)>,
    /// Each group's ID and name.
    groups: Vec<(String, String)>,
    /// Each own account's ID, name and whether it is closed.
    accounts: Vec<(String, String, bool)>,
}

/// Names compare as the book compares them: ignoring capitals.
fn same(left: &str, right: &str) -> bool {
    left.to_lowercase() == right.to_lowercase()
}

/// The one ID among `matches`, `Err(true)` for several, `Err(false)` for
/// none.
fn the_one<'a>(mut matches: impl Iterator<Item = &'a String>) -> Result<String, bool> {
    match (matches.next(), matches.next()) {
        (Some(id), None) => Ok(id.clone()),
        (Some(_), Some(_)) => Err(true),
        (None, _) => Err(false),
    }
}

impl BookNames {
    /// The names of the open book.
    pub fn read(book: &Book) -> Result<Self, ReportError> {
        Ok(Self {
            categories: book
                .categories()?
                .into_iter()
                .map(|entry| (entry.category.id.to_string(), entry.path))
                .collect(),
            groups: book
                .category_groups()?
                .into_iter()
                .map(|group| (group.id.to_string(), group.name))
                .collect(),
            accounts: book
                .accounts(true)?
                .into_iter()
                .map(|account| (account.id.to_string(), account.name, account.closed))
                .collect(),
        })
    }

    /// The ID of what `reference` names: a category by its whole path from
    /// the top, never by its last name alone; a group by its name; an
    /// account by its name, open accounts first, then closed ones.
    /// `Err(true)` when it matches more than one, `Err(false)` for none.
    fn find(&self, reference: &Reference) -> Result<String, bool> {
        match reference {
            Reference::Category(path) => the_one(
                self.categories
                    .iter()
                    .filter(|(_, names)| {
                        names.len() == path.len()
                            && names
                                .iter()
                                .zip(path)
                                .all(|(left, right)| same(left, right))
                    })
                    .map(|(id, _)| id),
            ),
            Reference::Group(name) => the_one(
                self.groups
                    .iter()
                    .filter(|(_, group)| same(group, name))
                    .map(|(id, _)| id),
            ),
            Reference::Account(name) => {
                let named = |closed: bool| {
                    self.accounts
                        .iter()
                        .filter(move |(_, account, is_closed)| {
                            *is_closed == closed && same(account, name)
                        })
                        .map(|(id, _, _)| id)
                };
                match the_one(named(false)) {
                    Err(false) => the_one(named(true)),
                    open => open,
                }
            }
        }
    }

    /// Whether a formula's name fits a group or category of the book, as a
    /// formula looks names up: a group's name, a category's name, or the end
    /// of its path written with `>`.
    fn knows_formula_name(&self, text: &str) -> bool {
        let whole = text.trim().to_lowercase();
        let parts: Vec<String> = whole
            .split('>')
            .map(|part| part.trim().to_owned())
            .collect();
        self.groups
            .iter()
            .any(|(_, name)| name.to_lowercase() == whole)
            || self.categories.iter().any(|(_, path)| {
                let path: Vec<String> = path.iter().map(|name| name.to_lowercase()).collect();
                path.last() == Some(&whole) || (parts.len() > 1 && path.ends_with(&parts))
            })
    }
}

/// `name`, or when a dashboard has it already (ignoring capitals) the first
/// free *Name (2)*, *Name (3)*, …, cut so it stays within 200 characters.
pub fn free_dashboard_name(name: &str, taken: &[String]) -> String {
    let taken: HashSet<String> = taken.iter().map(|name| name.to_lowercase()).collect();
    if !taken.contains(&name.to_lowercase()) {
        return name.to_owned();
    }
    (2..)
        .map(|number| {
            let suffix = format!(" ({number})");
            let room = MAX_NAME_CHARS - suffix.chars().count();
            let base: String = name.chars().take(room).collect();
            format!("{}{suffix}", base.trim_end())
        })
        .find(|candidate| !taken.contains(&candidate.to_lowercase()))
        .unwrap_or_else(|| name.to_owned())
}
