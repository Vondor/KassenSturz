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
            // Only a dataset that leaves hidden categories out has anything to
            // show anyway.
            if property.key == "show_hidden"
                && component.dataset().id != kassensturz_report::SPENDING_BY_CATEGORY
            {
                return None;
            }
            // The period of a component is offered by the grid editor's
            // panel (M6b.3); the workbench has its own period.
            if property.key == "period" {
                return None;
            }
            // My groups are chosen on their own page (M6b.5), so the grouping
            // that shows them is not offered here until a group can be picked.
            let words: Vec<&'static str> = property
                .words(component.dataset())
                .iter()
                .copied()
                .filter(|&word| word != kassensturz_report::GROUPS_GROUPING)
                .collect();
            let kind = match property.kind {
                Kind::Text => "text",
                Kind::Flag { .. } => "flag",
                Kind::Choice(_) | Kind::Grouping if words.len() > 1 => "choice",
                // The lists of IDs are chosen on the My groups page, not here.
                Kind::Choice(_) | Kind::Grouping | Kind::Ids => return None,
            };
            let (mut text, mut flag, mut word) = (String::new(), false, String::new());
            match setting {
                Setting::Text(value) => text.clone_from(value),
                Setting::Flag(value) => flag = *value,
                Setting::Word(value) => word.clone_from(value),
                Setting::Ids(_) => {}
            }
            Some(Field {
                key: property.key,
                place: property.place.key(),
                kind,
                text,
                flag,
                word,
                words,
            })
        })
        .collect()
}

/// The period choice of a component (`page` or one of the fixed periods),
/// which the grid editor's panel offers and the workbench does not
/// (D-313): the chosen word and the words in the order they are offered.
/// `None` for a component without a period (a heading).
pub fn period(component: &Component) -> Option<(String, Vec<&'static str>)> {
    let (property, setting) = component
        .settings()
        .find(|(property, _)| property.key == "period")?;
    let Setting::Word(word) = setting else {
        return None;
    };
    Some((word.clone(), property.words(component.dataset()).to_vec()))
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
    /// `chart`, `card`, `progress`, `table` or `heading`.
    pub shows: &'static str,
    /// The `kind` for `BookCharts.show`; empty for a card or a progress bar.
    pub chart: &'static str,
    /// The `grouping` for `BookCharts.show`.
    pub grouping: &'static str,
    /// The IDs for `BookCharts.showWith`, separated by commas: the groups
    /// of a spending component, the series of a chart over time.
    pub chosen: String,
    /// Whether the hidden categories are shown anyway.
    pub show_hidden: bool,
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

fn ids(component: &Component, key: &str) -> String {
    match component.get(key) {
        Some(Setting::Ids(ids)) => ids.join(","),
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
        // Text only: nothing is asked of the book.
        ComponentType::Heading => ("heading", ""),
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
        grouping: match word(component, "grouping").as_str() {
            "top_level_category" => "top-level",
            "groups" => "groups",
            _ => "each",
        },
        chosen: ids(
            component,
            if component.dataset().id == kassensturz_report::CATEGORY_OVER_TIME {
                "series"
            } else {
                "groups"
            },
        ),
        show_hidden: matches!(component.get("show_hidden"), Some(Setting::Flag(true))),
        compare: if compare.is_empty() {
            "previous_period".to_owned()
        } else {
            compare
        },
        measure,
    }
}
