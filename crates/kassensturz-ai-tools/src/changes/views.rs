// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A chart the AI proposes (M6a.9, D-282).
//!
//! The AI chooses from fixed lists: a dataset, a kind of chart (or `auto`),
//! a grouping, a key figure, a comparison and a period, and writes one
//! plain-text title. It writes no numbers and no document: the choices are
//! put into a [`Component`], which the same property schema checks as one
//! the user made by hand, and the figures the user sees in the preview come
//! from `kassensturz-report`. The chart is saved only when the user confirms
//! the set, and is marked as AI-made in the history like every other change
//! of a set (D-198).

use std::collections::HashSet;

use kassensturz_core::{StoredView, ViewId, ViewKind};
use kassensturz_report::{
    ColumnSetting, Component, ComponentType, Document, FORMULA_MEASURE, Item, MAX_TEXT_CHARS,
    PERIODS, ReportError, Rolling, Setting, read_range_word,
};
use kassensturz_store::{Book, StoreError};
use serde_json::{Map, Value};

use super::Object;
use crate::error::ToolError;
use crate::text::hides_text;

/// The period of a chart that names none.
pub(super) const DEFAULT_PERIOD: &str = "last-12-months";

/// A new chart, as the user reviews it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewView {
    /// Its title, which is also its name in the list of saved charts.
    pub title: String,
    /// The period it shows: one of [`PERIODS`], a rolling period or a
    /// range ([`is_view_period`]).
    pub period: String,
    component: Component,
    /// The document that is saved when the user confirms.
    pub(super) document: String,
}

impl NewView {
    /// The id of the kind of component, e.g. `chart.bar`.
    pub fn type_id(&self) -> &'static str {
        self.component.kind().id()
    }

    /// The id of the dataset it shows.
    pub fn dataset_id(&self) -> &'static str {
        self.component.dataset().id
    }

    /// The component as JSON text, which `ComponentProperties.load` of the
    /// app takes, so the preview draws it like a chart the user made.
    pub fn component_json(&self) -> String {
        self.component.to_value().to_string()
    }
    /// The component, for the figure its answer gives (D-685).
    pub(super) const fn component(&self) -> &Component {
        &self.component
    }
}

/// A saved chart the AI changes (D-681): only what its call named, the
/// rest as the user left it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditView {
    /// The chart.
    pub id: ViewId,
    /// Its name now.
    pub before: String,
    /// Its name after; the same when it isn't renamed.
    pub title: String,
    /// The period it shows after.
    pub period: String,
    component: Component,
    /// The document as the user reviewed it: applying fails if it changed
    /// since.
    reviewed: String,
    /// The document that is saved when the user confirms.
    document: String,
}

impl EditView {
    /// The id of its kind of component after the change.
    pub fn type_id(&self) -> &'static str {
        self.component.kind().id()
    }

    /// The id of the dataset it shows after the change.
    pub fn dataset_id(&self) -> &'static str {
        self.component.dataset().id
    }

    /// The component after the change as JSON text, for the preview.
    pub fn component_json(&self) -> String {
        self.component.to_value().to_string()
    }
    /// The component after the change, for the figure its answer gives
    /// (D-685).
    pub(super) const fn component(&self) -> &Component {
        &self.component
    }
}

/// A saved chart the AI deletes (D-681). Undo brings it back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeleteView {
    /// The chart.
    pub id: ViewId,
    /// Its name.
    pub name: String,
    /// The document as the user reviewed it.
    reviewed: String,
}

/// The kind of component `auto` stands for: money by category reads best as
/// a pie, budgets as their bars, anything else as bars or, for a figure
/// over time such as what you are worth, a line (D-352), and a dataset that
/// suits none of them as a key figure or a table. Since the datasets suit
/// more kinds (D-666), the financial ones name their own first choice, so
/// `auto` still picks what it picked before: a line of what you are worth,
/// bars of the money in and out, today's balances as a key figure.
pub(super) fn automatic(dataset: &str, grouping: Option<&str>) -> Option<ComponentType> {
    let grouping = grouping.or_else(|| {
        kassensturz_report::dataset(dataset).and_then(|info| info.groupings.first().copied())
    });
    let by_category = grouping.is_some_and(|grouping| grouping.ends_with("category"));
    let order: &[ComponentType] = if by_category {
        &[ComponentType::ChartPie, ComponentType::ChartBar]
    } else if kassensturz_report::dataset(dataset)
        .is_some_and(|info| info.suits.contains(&ComponentType::BudgetProgress.suit()))
    {
        &[ComponentType::BudgetProgress]
    } else if dataset == kassensturz_report::NET_WORTH_OVER_TIME {
        &[ComponentType::ChartLine]
    } else if dataset == kassensturz_report::CASH_FLOW_BY_MONTH {
        &[ComponentType::ChartBar]
    } else if dataset == kassensturz_report::ACCOUNT_BALANCES {
        &[ComponentType::KpiCard]
    } else {
        &[
            ComponentType::ChartStackedBar,
            ComponentType::ChartBar,
            ComponentType::ChartLine,
        ]
    };
    order
        .iter()
        .chain(&[ComponentType::KpiCard, ComponentType::TableRows])
        .copied()
        .find(|kind| kind.datasets().any(|info| info.id == dataset))
}

/// The title the AI wrote: one line of plain text, the longest the property
/// allows.
pub(super) fn title(text: &str) -> Result<String, ToolError> {
    let text = text.trim();
    let bad = text.is_empty()
        || text.chars().count() > MAX_TEXT_CHARS
        || text.chars().any(|c| c.is_control() || hides_text(c));
    if bad {
        return Err(ToolError::BadArgument { argument: "title" });
    }
    Ok(text.to_owned())
}

/// Sets a property the AI named, which the schema may refuse for this kind
/// of component (a key figure has no grouping).
fn choose(
    component: &mut Component,
    key: &str,
    word: Option<&str>,
    argument: &'static str,
) -> Result<(), ToolError> {
    match word {
        None => Ok(()),
        Some(word) => component
            .set(key, Setting::Word(word.to_owned()))
            .map_err(|_| ToolError::BadArgument { argument }),
    }
}

/// Sets a list of ids the AI named (`categories`, `accounts`), which the
/// schema may refuse for this kind of component. The ids were checked
/// against the book and the grid by `propose_view` (D-353); an edited call
/// keeps the ones the model sent.
fn choose_ids(
    component: &mut Component,
    key: &'static str,
    ids: &[Value],
) -> Result<(), ToolError> {
    if ids.is_empty() {
        return Ok(());
    }
    let ids = ids
        .iter()
        .map(|id| id.as_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()
        .ok_or(ToolError::BadArgument { argument: key })?;
    component
        .set(key, Setting::Ids(ids))
        .map_err(|_| ToolError::BadArgument { argument: key })
}

/// Sets the chart's columns (US3.1b, D-687) from the item's `columns`: each
/// `{name, formula}` or `{name, categories}`, checked against the book by
/// `propose_view`; the schema checks the names and the lengths again. An
/// empty list takes the columns away. Only spending is made of columns.
fn choose_columns(component: &mut Component, value: Option<&Value>) -> Result<(), ToolError> {
    const BAD: ToolError = ToolError::BadArgument {
        argument: "columns",
    };
    let Some(value) = value else {
        return Ok(());
    };
    let columns = value
        .as_array()
        .ok_or(BAD)?
        .iter()
        .map(|entry| {
            let entry = entry.as_object()?;
            let name = entry.get("name")?.as_str()?.to_owned();
            let formula = match entry.get("formula") {
                Some(formula) => formula.as_str()?.to_owned(),
                None => String::new(),
            };
            let members = match entry.get("categories") {
                Some(ids) => ids
                    .as_array()?
                    .iter()
                    .map(|id| id.as_str().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()?,
                None => Vec::new(),
            };
            let known = entry
                .keys()
                .all(|key| ["name", "formula", "categories"].contains(&key.as_str()));
            (known && formula.is_empty() != members.is_empty()).then_some(ColumnSetting {
                name,
                members,
                formula,
            })
        })
        .collect::<Option<Vec<_>>>()
        .ok_or(BAD)?;
    if !columns.is_empty() && component.dataset().id != kassensturz_report::SPENDING_BY_CATEGORY {
        return Err(BAD);
    }
    component
        .set("columns", Setting::Columns(columns))
        .map_err(|_| BAD)
}

/// The settings of a chart the AI prepares besides its kind and dataset,
/// in the order they are set: words, then lists of ids, then formulas
/// (D-352, D-650).
const WORDS: [&str; 5] = ["grouping", "measure", "compare", "average", "orientation"];
const ID_LISTS: [&str; 6] = [
    "categories",
    "accounts",
    "income_categories",
    "income_accounts",
    "expenses_categories",
    "expenses_accounts",
];
const FORMULAS: [&str; 2] = ["formula", "target"];

/// Whether `period` is one a saved chart can have: a fixed one, one that
/// moves with today (D-644, D-645) or a typed range (D-332).
pub(super) fn is_view_period(period: &str) -> bool {
    PERIODS.contains(&period)
        || Rolling::read(period).is_some()
        || read_range_word(period).is_some()
}

/// Checks one entry of `new_views` against the book: the item
/// `propose_view` made of a model's call (D-352), or that item as the user
/// edited it (D-297).
pub(super) fn new_view(
    book: &Book,
    value: &Value,
    names: &mut HashSet<String>,
) -> Result<NewView, ToolError> {
    let item = Object::read(
        value,
        &[
            "title",
            "dataset",
            "chart",
            "grouping",
            "measure",
            "compare",
            "average",
            "orientation",
            "period",
            "categories",
            "accounts",
            "income_categories",
            "income_accounts",
            "expenses_categories",
            "expenses_accounts",
            "formula",
            "target",
            "columns",
        ],
        "new_views",
    )?;
    let title = title(item.required_string("title")?)?;
    let dataset = item.required_string("dataset")?;
    // The AI knows no groups and no hide list in M6b, so what uses them is
    // refused even if the model names it.
    if !kassensturz_report::dataset(dataset).is_some_and(|info| info.for_ai) {
        return Err(ToolError::BadArgument {
            argument: "dataset",
        });
    }
    let grouping = item.string("grouping")?;
    if grouping == Some(kassensturz_report::GROUPS_GROUPING) {
        return Err(ToolError::BadArgument {
            argument: "grouping",
        });
    }
    let kind = match item.required_string("chart")? {
        "auto" => automatic(dataset, grouping),
        id => ComponentType::from_id(id),
    }
    .ok_or(ToolError::BadArgument { argument: "chart" })?;
    // The dataset exists, so a refusal means the chart doesn't suit it.
    let mut component = Component::new(kind, dataset)
        .map_err(|_: ReportError| ToolError::BadArgument { argument: "chart" })?;
    let formula = item.string("formula")?;
    let target = item.string("target")?;
    if target.is_some() && formula.is_none() {
        return Err(ToolError::BadArgument { argument: "target" });
    }
    // A key figure with a formula shows the formula (D-289): its measure is
    // `formula`, whether the AI named it or not.
    let mut measure = item.string("measure")?;
    if formula.is_some() && kind == ComponentType::KpiCard {
        match measure {
            None | Some(FORMULA_MEASURE) => measure = Some(FORMULA_MEASURE),
            Some(_) => {
                return Err(ToolError::BadArgument {
                    argument: "measure",
                });
            }
        }
    }
    for key in WORDS {
        let word = match key {
            "grouping" => grouping,
            "measure" => measure,
            _ => item.string(key)?,
        };
        choose(&mut component, key, word, key)?;
    }
    for key in ID_LISTS {
        choose_ids(&mut component, key, item.array(key)?)?;
    }
    choose_columns(&mut component, item.get("columns"))?;
    for (key, text) in FORMULAS.into_iter().zip([formula, target]) {
        if let Some(text) = text {
            component
                .set(key, Setting::Text(text.to_owned()))
                .map_err(|_| ToolError::BadArgument { argument: key })?;
        }
    }
    component
        .set("title", Setting::Text(title.clone()))
        .map_err(|_| ToolError::BadArgument { argument: "title" })?;
    let period = item.string("period")?.unwrap_or(DEFAULT_PERIOD);
    if !is_view_period(period) {
        return Err(ToolError::BadArgument { argument: "period" });
    }
    // The name is the title; no two saved charts share one, ignoring
    // capitals, which the book checks again when it saves.
    let key = title.to_lowercase();
    let taken = book
        .views(Some(ViewKind::View))?
        .iter()
        .any(|view| view.name.to_lowercase() == key);
    if taken || !names.insert(key) {
        return Err(ToolError::NameTaken);
    }
    let document = Document::view(&title, Some(period), component.clone()).to_json();
    Document::read(&document).map_err(|_| ToolError::BadArgument { argument: "title" })?;
    Ok(NewView {
        title,
        period: period.to_owned(),
        component,
        document,
    })
}

/// Drops the settings a chart's new kind doesn't take, after the user chose
/// another kind on the review page (D-297): a key figure's measure means
/// nothing to a bar chart. The checks then judge the kind itself.
pub(super) fn keep_fitting(item: &mut Map<String, Value>) {
    let dataset = item.get("dataset").and_then(Value::as_str).unwrap_or("");
    let grouping = item.get("grouping").and_then(Value::as_str);
    let kind = match item.get("chart").and_then(Value::as_str) {
        Some("auto") => automatic(dataset, grouping),
        Some(id) => ComponentType::from_id(id),
        None => None,
    };
    let Some(component) = kind.and_then(|kind| Component::new(kind, dataset).ok()) else {
        return;
    };
    for key in WORDS.into_iter().chain(ID_LISTS).chain(FORMULAS) {
        let fits = match item.get(key) {
            None => true,
            Some(Value::String(text)) if FORMULAS.contains(&key) => component
                .clone()
                .set(key, Setting::Text(text.clone()))
                .is_ok(),
            Some(Value::String(word)) => component
                .clone()
                .set(key, Setting::Word(word.clone()))
                .is_ok(),
            Some(Value::Array(ids)) => {
                ids.is_empty() || choose_ids(&mut component.clone(), key, ids).is_ok()
            }
            Some(_) => false,
        };
        if !fits {
            item.remove(key);
        }
    }
}

/// A chart saved under Statistics, by the ID in `item`'s `view_id`: a
/// dashboard or an ID the book doesn't have is unknown (D-681).
fn saved_chart(book: &Book, item: &Object<'_>) -> Result<StoredView, ToolError> {
    let id = item
        .uuid("view_id")?
        .map(ViewId::from_bytes)
        .ok_or(ToolError::BadArgument {
            argument: "view_id",
        })?;
    let view = match book.view(id) {
        Err(StoreError::UnknownView(_)) => return Err(ToolError::UnknownView),
        other => other?,
    };
    if view.kind != ViewKind::View {
        return Err(ToolError::UnknownView);
    }
    Ok(view)
}

/// A saved chart's document and its one component, or why the AI can't
/// change it: a chart of a newer version or of a kind this version doesn't
/// know is kept as it is.
fn chart_of(view: &StoredView) -> Result<(Document, Component), ToolError> {
    let unreadable = || ToolError::NotChangeable {
        argument: "view_id",
        reason: "unreadable",
    };
    let document = Document::read(&view.document).map_err(|_| unreadable())?;
    match document.entries.first().map(|entry| &entry.item) {
        Some(Item::Component(component)) => {
            let component = component.clone();
            Ok((document, component))
        }
        _ => Err(unreadable()),
    }
}

/// The keys of an item of `edit_views`: those of `new_views` and the
/// chart's ID.
const EDIT_KEYS: [&str; 19] = [
    "view_id",
    "title",
    "dataset",
    "chart",
    "grouping",
    "measure",
    "compare",
    "average",
    "orientation",
    "period",
    "categories",
    "accounts",
    "income_categories",
    "income_accounts",
    "expenses_categories",
    "expenses_accounts",
    "formula",
    "target",
    "columns",
];

/// The `compare` word for no comparison, which puts the chart's
/// comparison back to its default.
const NO_COMPARISON: &str = "none";

/// Checks one entry of `edit_views`, the item `propose_view` made of a call
/// with `view_id` (D-681), against the book: the saved chart takes each
/// setting the item names, through the same schema as a chart the user
/// changes by hand, and keeps every other one. `names` holds the names of
/// the charts the set saves, so two of them can't take one name; `touched`
/// the charts it changes or deletes.
pub(super) fn edit_view(
    book: &Book,
    value: &Value,
    names: &mut HashSet<String>,
    touched: &mut HashSet<ViewId>,
) -> Result<EditView, ToolError> {
    let item = Object::read(value, &EDIT_KEYS, "edit_views")?;
    let view = saved_chart(book, &item)?;
    if !touched.insert(view.id) {
        return Err(ToolError::BadArgument {
            argument: "view_id",
        });
    }
    let (mut document, mut component) = chart_of(&view)?;
    let dataset = item.string("dataset")?;
    if let Some(dataset) = dataset
        && !kassensturz_report::dataset(dataset).is_some_and(|info| info.for_ai)
    {
        return Err(ToolError::BadArgument {
            argument: "dataset",
        });
    }
    let grouping = item.string("grouping")?;
    if grouping == Some(kassensturz_report::GROUPS_GROUPING) {
        return Err(ToolError::BadArgument {
            argument: "grouping",
        });
    }
    if let Some(chart) = item.string("chart")? {
        let shown = dataset.unwrap_or(component.dataset().id);
        let kind = match chart {
            "auto" => automatic(shown, grouping),
            id => ComponentType::from_id(id),
        }
        .ok_or(ToolError::BadArgument { argument: "chart" })?;
        if kind != component.kind() {
            component
                .change_kind(kind)
                .map_err(|_: ReportError| ToolError::BadArgument { argument: "chart" })?;
        }
    }
    if let Some(dataset) = dataset {
        component
            .use_dataset(dataset)
            .map_err(|_: ReportError| ToolError::BadArgument {
                argument: if item.get("chart").is_some() {
                    "chart"
                } else {
                    "dataset"
                },
            })?;
    }
    let kind = component.kind();
    let defaults = Component::new(kind, component.dataset().id)
        .map_err(|_: ReportError| ToolError::BadArgument { argument: "chart" })?;
    let formula = item.string("formula")?;
    let mut measure = item.string("measure")?;
    if formula.is_some() && kind == ComponentType::KpiCard {
        match measure {
            None | Some(FORMULA_MEASURE) => measure = Some(FORMULA_MEASURE),
            Some(_) => {
                return Err(ToolError::BadArgument {
                    argument: "measure",
                });
            }
        }
    }
    for key in WORDS {
        let word = match key {
            "grouping" => grouping,
            "measure" => measure,
            _ => item.string(key)?,
        };
        let Some(word) = word else { continue };
        let setting = match (key, defaults.get(key)) {
            ("compare", Some(default)) if word == NO_COMPARISON => default.clone(),
            _ => Setting::Word(word.to_owned()),
        };
        component
            .set(key, setting)
            .map_err(|_| ToolError::BadArgument { argument: key })?;
    }
    for key in ID_LISTS {
        if item.get(key).is_none() {
            continue;
        }
        let ids = item
            .array(key)?
            .iter()
            .map(|id| id.as_str().map(str::to_owned))
            .collect::<Option<Vec<_>>>()
            .ok_or(ToolError::BadArgument { argument: key })?;
        component
            .set(key, Setting::Ids(ids))
            .map_err(|_| ToolError::BadArgument { argument: key })?;
    }
    choose_columns(&mut component, item.get("columns"))?;
    for key in FORMULAS {
        let text = if key == "formula" {
            formula
        } else {
            item.string(key)?
        };
        if let Some(text) = text {
            component
                .set(key, Setting::Text(text.to_owned()))
                .map_err(|_| ToolError::BadArgument { argument: key })?;
        }
    }
    let title = match item.string("title")? {
        Some(text) => {
            let text = title(text)?;
            component
                .set("title", Setting::Text(text.clone()))
                .map_err(|_| ToolError::BadArgument { argument: "title" })?;
            text
        }
        None => document.name.clone(),
    };
    let period = match item.string("period")? {
        Some("page") => super::view_call::default_period(component.dataset().id).to_owned(),
        Some(period) if is_view_period(period) => period.to_owned(),
        Some(_) => return Err(ToolError::BadArgument { argument: "period" }),
        None => document
            .period
            .clone()
            .unwrap_or_else(|| super::view_call::default_period(component.dataset().id).to_owned()),
    };
    // A new name is checked as a new chart's is, against the others.
    let key = title.to_lowercase();
    if key != document.name.to_lowercase() {
        let taken = book
            .views(Some(ViewKind::View))?
            .iter()
            .any(|other| other.id != view.id && other.name.to_lowercase() == key);
        if taken {
            return Err(ToolError::NameTaken);
        }
    }
    if !names.insert(key) {
        return Err(ToolError::NameTaken);
    }
    let before = document.name.clone();
    document.name.clone_from(&title);
    if item.get("period").is_some() || document.period.is_some() {
        document.period = Some(period.clone());
    }
    if let Some(entry) = document.entries.first_mut() {
        entry.item = Item::Component(component.clone());
    }
    let text = document.to_json();
    Document::read(&text).map_err(|_| ToolError::BadArgument { argument: "title" })?;
    Ok(EditView {
        id: view.id,
        before,
        title,
        period,
        component,
        reviewed: view.document,
        document: text,
    })
}

/// Checks one entry of `delete_views` against the book (D-681).
pub(super) fn delete_view(
    book: &Book,
    value: &Value,
    touched: &mut HashSet<ViewId>,
) -> Result<DeleteView, ToolError> {
    let item = Object::read(value, &["view_id"], "delete_views")?;
    let view = saved_chart(book, &item)?;
    if !touched.insert(view.id) {
        return Err(ToolError::NotChangeable {
            argument: "view_id",
            reason: "also-deleted",
        });
    }
    Ok(DeleteView {
        id: view.id,
        name: view.name,
        reviewed: view.document,
    })
}

/// The saved chart `id` as the user reviewed it, or the set is out of date:
/// the user changed or deleted it since.
fn unchanged(book: &Book, id: ViewId, reviewed: &str) -> Result<(), super::ChangeError> {
    match book.view(id) {
        Err(StoreError::UnknownView(_)) => Err(super::ChangeError::OutOfDate),
        Err(error) => Err(error.into()),
        Ok(view) if view.document != reviewed => Err(super::ChangeError::OutOfDate),
        Ok(_) => Ok(()),
    }
}

/// Saves a changed chart, as the user's own change of it would.
pub(super) fn apply_edit_view(book: &mut Book, edit: &EditView) -> Result<(), super::ChangeError> {
    unchanged(book, edit.id, &edit.reviewed)?;
    book.edit_view(edit.id, &edit.document)?;
    Ok(())
}

/// Deletes a saved chart; Undo brings it back.
pub(super) fn apply_delete_view(
    book: &mut Book,
    delete: &DeleteView,
) -> Result<(), super::ChangeError> {
    unchanged(book, delete.id, &delete.reviewed)?;
    book.remove_view(delete.id)?;
    Ok(())
}
