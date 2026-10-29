// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the properties panel shows for a component (M6a.6, D-278): one field
//! for every property of the component's type in the property schema of
//! `kassensturz-report`, so a property the schema doesn't have has no field
//! and cannot be set. No Qt in this file: `properties_model.rs` hands the
//! result to QML.
//!
//! The panel holds no user-facing text from here: it looks the label and
//! the help of a field up by its key, and the name of a word by the word
//! (D-034). Titles are the user's own text and go back to QML as text.

use kassensturz_report::{Component, ComponentType, Kind, Setting};

/// One field of the panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// The property's key, which the panel looks its label up by.
    pub key: &'static str,
    /// `params` or `props`.
    pub place: &'static str,
    /// `text`, `flag` or `choice`.
    pub kind: &'static str,
    /// The value of a text; empty for the other kinds.
    pub text: String,
    /// The value of a flag; false for the other kinds.
    pub flag: bool,
    /// The value of a choice; empty for the other kinds.
    pub word: String,
    /// The words of a choice, in the order they are offered.
    pub words: Vec<&'static str>,
}

/// The fields of a component, in the order of its schema. A choice with
/// fewer than two words is left out: there is nothing to choose, and the
/// document still holds its one value.
pub fn fields(component: &Component) -> Vec<Field> {
    component
        .settings()
        .filter_map(|(property, setting)| {
            let words = property.words(component.dataset());
            let kind = match property.kind {
                Kind::Text => "text",
                Kind::Flag { .. } => "flag",
                Kind::Choice(_) | Kind::Grouping if words.len() > 1 => "choice",
                Kind::Choice(_) | Kind::Grouping => return None,
            };
            let (mut text, mut flag, mut word) = (String::new(), false, String::new());
            match setting {
                Setting::Text(value) => text.clone_from(value),
                Setting::Flag(value) => flag = *value,
                Setting::Word(value) => word.clone_from(value),
            }
            Some(Field {
                key: property.key,
                place: property.place.key(),
                kind,
                text,
                flag,
                word,
                words: words.to_vec(),
            })
        })
        .collect()
}

/// The ids of the datasets the component's type can show.
pub fn dataset_ids(kind: ComponentType) -> Vec<&'static str> {
    kind.datasets().map(|dataset| dataset.id).collect()
}

/// The component as the text of a JSON document part, for saving.
pub fn json(component: &Component) -> String {
    component.to_value().to_string()
}

/// What the workbench asks `BookCharts` and `BookCards` for, by the fixed
/// names those objects take, so the screen maps nothing itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    /// `chart`, `card`, `progress` or `table`.
    pub shows: &'static str,
    /// The `kind` for `BookCharts.show`; empty for a card or a progress bar.
    pub chart: &'static str,
    /// The `grouping` for `BookCharts.show`.
    pub grouping: &'static str,
    /// The `compare` for `BookCards.show`.
    pub compare: String,
    /// The card to pick from the cards: `income`, `expenses`, `net` or
    /// `savings-rate`.
    pub measure: String,
}

fn word(component: &Component, key: &str) -> String {
    match component.get(key) {
        Some(Setting::Word(word)) => word.clone(),
        _ => String::new(),
    }
}

/// What to ask for to show `component`.
pub fn preview(component: &Component) -> Preview {
    let (shows, chart) = match component.kind() {
        ComponentType::ChartLine => ("chart", "line"),
        ComponentType::ChartBar => ("chart", "bar"),
        ComponentType::ChartStackedBar => ("chart", "stacked-bar"),
        ComponentType::ChartArea => ("chart", "area"),
        ComponentType::ChartPie => ("chart", "pie"),
        ComponentType::ChartDonut => ("chart", "donut"),
        ComponentType::KpiCard => ("card", ""),
        ComponentType::ProgressBar => ("progress", ""),
        // A table shows the values of a chart; a bar chart is the one both
        // datasets can make.
        ComponentType::TableRows => ("table", "bar"),
    };
    let compare = word(component, "compare");
    let measure = match component.kind() {
        ComponentType::ProgressBar => "savings-rate".to_owned(),
        _ => word(component, "measure"),
    };
    Preview {
        shows,
        chart,
        // The schema's words for the dataset's groupings; `BookCharts` takes
        // `top-level` or `each`, and the monthly dataset has one grouping.
        grouping: if word(component, "grouping") == "top_level_category" {
            "top-level"
        } else {
            "each"
        },
        compare: if compare.is_empty() {
            "previous_period".to_owned()
        } else {
            compare
        },
        measure,
    }
}
