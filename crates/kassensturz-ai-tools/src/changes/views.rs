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

use kassensturz_core::ViewKind;
use kassensturz_report::{
    Component, ComponentType, Document, MAX_TEXT_CHARS, PERIODS, ReportError, Setting,
};
use kassensturz_store::Book;
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
    /// The period it shows, one of [`PERIODS`].
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
}

/// The kind of component `auto` stands for: money by category reads best as
/// a pie, anything else as bars, and a dataset that suits neither as a key
/// figure or a table.
fn automatic(dataset: &str, grouping: Option<&str>) -> Option<ComponentType> {
    let grouping = grouping.or_else(|| {
        kassensturz_report::dataset(dataset).and_then(|info| info.groupings.first().copied())
    });
    let by_category = grouping.is_some_and(|grouping| grouping.ends_with("category"));
    let order: &[ComponentType] = if by_category {
        &[ComponentType::ChartPie, ComponentType::ChartBar]
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
fn title(text: &str) -> Result<String, ToolError> {
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

/// Checks one entry of `new_views` against the book.
pub(super) fn new_view(
    book: &Book,
    value: &Value,
    names: &mut HashSet<String>,
) -> Result<NewView, ToolError> {
    let item = Object::read(
        value,
        &[
            "title", "dataset", "chart", "grouping", "measure", "compare", "period",
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
    choose(&mut component, "grouping", grouping, "grouping")?;
    choose(
        &mut component,
        "measure",
        item.string("measure")?,
        "measure",
    )?;
    choose(
        &mut component,
        "compare",
        item.string("compare")?,
        "compare",
    )?;
    component
        .set("title", Setting::Text(title.clone()))
        .map_err(|_| ToolError::BadArgument { argument: "title" })?;
    let period = item.string("period")?.unwrap_or(DEFAULT_PERIOD);
    if !PERIODS.contains(&period) {
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
    for key in ["grouping", "measure", "compare"] {
        let fits = item.get(key).and_then(Value::as_str).is_none_or(|word| {
            component
                .clone()
                .set(key, Setting::Word(word.to_owned()))
                .is_ok()
        });
        if !fits {
            item.remove(key);
        }
    }
}
