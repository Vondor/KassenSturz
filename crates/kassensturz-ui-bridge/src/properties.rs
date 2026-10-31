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

use kassensturz_report::{
    ColumnSetting, Component, ComponentType, FORMULA_MEASURE, Kind, NumberForm, Setting,
};

use crate::formulas::users_form;

/// One field of the panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// The property's key, which the panel looks its label up by.
    pub key: &'static str,
    /// `params` or `props`.
    pub place: &'static str,
    /// `text`, `formula`, `flag`, `choice` or `ids`.
    pub kind: &'static str,
    /// The value of a text, or a formula as the user's number format writes
    /// it; empty for the other kinds.
    pub text: String,
    /// The value of a flag; false for the other kinds.
    pub flag: bool,
    /// The value of a choice; empty for the other kinds.
    pub word: String,
    /// The words of a choice, in the order they are offered.
    pub words: Vec<&'static str>,
    /// The IDs of a list of groups or categories; empty for the other kinds.
    pub ids: Vec<String>,
}

/// The fields of a component, in the order of its schema. A choice with
/// fewer than two words is left out: there is nothing to choose, and the
/// document still holds its one value. A formula is shown in the form of
/// the user's number format `form` (D-295).
pub fn fields(component: &Component, form: NumberForm) -> Vec<Field> {
    let uses = uses_formula(component);
    let has_formula = uses && !text_of(component, "formula").is_empty();
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
            // The columns have a board of their own, not a field (D-340);
            // the view carries them as `columns`.
            if property.key == "columns" {
                return None;
            }
            // The period of a component is offered by the grid editor's
            // panel (M6b.3); the workbench has its own period.
            if property.key == "period" {
                return None;
            }
            // The formula of a key figure is used when its measure says so,
            // that of a progress bar always; a bar measures it against its
            // target. A figure that is a formula has no comparison (M6b.7).
            let hidden = match property.key {
                "formula" => component.kind() == ComponentType::KpiCard && !uses,
                "target" => !has_formula,
                "compare" => uses,
                _ => false,
            };
            if hidden {
                return None;
            }
            // A list of IDs is offered only where it is used: the groups of a
            // chart that groups by My groups, the series of a chart over time.
            if let (Kind::Ids, Setting::Ids(_)) = (&property.kind, setting) {
                let wanted = match property.key {
                    "groups" => matches!(
                        component.get("grouping"),
                        Some(Setting::Word(word)) if word == kassensturz_report::GROUPS_GROUPING
                    ),
                    "categories" => {
                        component.dataset().id == kassensturz_report::SPENDING_BY_CATEGORY
                    }
                    _ => component.dataset().id == kassensturz_report::CATEGORY_OVER_TIME,
                };
                if !wanted {
                    return None;
                }
            }
            let words: Vec<&'static str> = property.words(component.dataset()).to_vec();
            let kind = match property.kind {
                Kind::Text => "text",
                Kind::Formula => "formula",
                Kind::Flag { .. } => "flag",
                Kind::Choice(_) | Kind::Grouping if words.len() > 1 => "choice",
                Kind::Ids => "ids",
                // The accounts get their field with the filter bar (M6c.7).
                Kind::Accounts | Kind::Columns | Kind::Choice(_) | Kind::Grouping => return None,
            };
            let (mut text, mut flag, mut word) = (String::new(), false, String::new());
            let mut chosen = Vec::new();
            match setting {
                Setting::Text(value) if property.kind == Kind::Formula => {
                    text = users_form(form, value);
                }
                Setting::Text(value) => text.clone_from(value),
                Setting::Flag(value) => flag = *value,
                Setting::Word(value) => word.clone_from(value),
                Setting::Ids(ids) => chosen.clone_from(ids),
                Setting::Columns(_) => {}
            }
            Some(Field {
                key: property.key,
                place: property.place.key(),
                kind,
                text,
                flag,
                word,
                words,
                ids: chosen,
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
    /// The IDs for `BookCharts.showWith` that limit a spending component to
    /// these categories and groups (D-331), separated by commas; empty for
    /// all.
    pub only: String,
    /// Whether the hidden categories are shown anyway.
    pub show_hidden: bool,
    /// The columns of a chart or table as the document writes them (a JSON
    /// list of `{"name", "members"}`), for `BookCharts.showWith`; empty for
    /// none (D-339).
    pub columns: String,
    /// The `compare` for `BookCards.show`.
    pub compare: String,
    /// The card to pick from the cards: `income`, `expenses`, `net` or
    /// `savings-rate`; `net_worth`, `assets` or `liabilities` for the
    /// balances of the accounts.
    pub measure: String,
    /// Whether a card or a progress bar shows the result of a formula
    /// instead of a card of the cards (M6b.7).
    pub uses_formula: bool,
    /// The formula in the form the book stores; empty for none.
    pub formula: String,
    /// What a full progress bar stands for, a formula in the form the book
    /// stores; empty for none.
    pub target: String,
}

/// The columns of a chart or table as the document writes them: a JSON list
/// of `{"name": ..., "members": [...], "formula": "..."}`. `None` for a component that has no
/// columns property (D-339). `show` writes a column's formula for whoever
/// reads the text: in the user's number format for the screen, as stored for
/// the charts (D-341).
pub fn columns_json(component: &Component, show: &dyn Fn(&str) -> String) -> Option<String> {
    match component.get("columns") {
        Some(Setting::Columns(columns)) => Some(
            serde_json::Value::Array(
                columns
                    .iter()
                    .map(|column| {
                        serde_json::json!({
                            "name": column.name,
                            "members": column.members,
                            "formula": show(&column.formula),
                        })
                    })
                    .collect(),
            )
            .to_string(),
        ),
        _ => None,
    }
}

/// The columns a JSON list of `{"name", "members", "formula"}` says, or `None` when
/// the text is no such list.
pub fn read_columns(
    json: &str,
    keep: &dyn Fn(&str) -> Option<String>,
) -> Option<Vec<ColumnSetting>> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    value
        .as_array()?
        .iter()
        .map(|entry| {
            Some(ColumnSetting {
                name: entry.get("name")?.as_str()?.to_owned(),
                members: entry
                    .get("members")?
                    .as_array()?
                    .iter()
                    .map(|id| id.as_str().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()?,
                // Optional: a column with members has none (D-341). `keep`
                // gives the form the book stores, or `None` when the text is
                // no formula, which refuses the whole list.
                formula: match entry.get("formula").map(|formula| formula.as_str()) {
                    None => String::new(),
                    Some(formula) => keep(formula?)?,
                },
            })
        })
        .collect()
}

/// The text of a formula property in the form the book stores, empty when
/// the component has none.
fn text_of(component: &Component, key: &str) -> String {
    match component.get(key) {
        Some(Setting::Text(formula)) => formula.trim().to_owned(),
        _ => String::new(),
    }
}

/// Whether the component shows the result of a formula: a key figure whose
/// measure is `formula`, or a progress bar that has one (M6b.7). A bar
/// without one shows the share you kept.
fn uses_formula(component: &Component) -> bool {
    match component.kind() {
        ComponentType::KpiCard => {
            matches!(component.get("measure"), Some(Setting::Word(word)) if word == FORMULA_MEASURE)
        }
        ComponentType::ProgressBar => !text_of(component, "formula").is_empty(),
        _ => false,
    }
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
        ComponentType::Separator => ("separator", ""),
        ComponentType::FetchNotice => ("fetch-notice", ""),
        ComponentType::TransfersNote => ("transfers-note", ""),
        ComponentType::PeriodBox => ("period-box", ""),
        ComponentType::NetWorth => ("net-worth", ""),
        ComponentType::IncomeSources => ("income-sources", ""),
        // Not in the library before M6c.5 draws it (D-378).
        ComponentType::BudgetProgress => ("budget-progress", ""),
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
            // The balances of the accounts as what you have, owe and are
            // worth (M6c.2).
            "account_kind" => "account-kind",
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
        only: ids(component, "categories"),
        show_hidden: matches!(component.get("show_hidden"), Some(Setting::Flag(true))),
        columns: columns_json(component, &str::to_owned)
            .filter(|json| json != "[]")
            .unwrap_or_default(),
        compare: if compare.is_empty() {
            "previous_period".to_owned()
        } else {
            compare
        },
        measure,
        uses_formula: uses_formula(component),
        formula: text_of(component, "formula"),
        target: text_of(component, "target"),
    }
}
