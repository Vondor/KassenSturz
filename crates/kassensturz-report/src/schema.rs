// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The property schema of dashboard components (M6a.6, D-278, D-281, D-282).
//!
//! One table says which components exist, which dataset ids each one can
//! show and which properties it has, with the kind and the allowed values
//! of every property. The properties panel is drawn from it, a document is
//! checked against it, and it is all the AI may fill in (`propose_view`,
//! D-282): a name that is not in this table cannot reach a chart.
//!
//! A property has a key and a kind. What the user reads (its label and its
//! help) is looked up by the key in the app (D-034); the table holds no
//! user-facing text.

use crate::ReportError;
use crate::catalog::{CATALOG, DatasetInfo};

/// The longest title, in characters (D-281).
pub const MAX_TEXT_CHARS: usize = 200;

/// A kind of component, by the fixed id documents use (D-278).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentType {
    /// `chart.line`
    ChartLine,
    /// `chart.bar`
    ChartBar,
    /// `chart.stacked_bar`
    ChartStackedBar,
    /// `chart.area`
    ChartArea,
    /// `chart.pie`
    ChartPie,
    /// `chart.donut`
    ChartDonut,
    /// `kpi.card`
    KpiCard,
    /// `progress.bar`
    ProgressBar,
    /// `table.rows`
    TableRows,
}

impl ComponentType {
    /// Every component, in the order the app offers them.
    pub const ALL: [Self; 9] = [
        Self::ChartLine,
        Self::ChartBar,
        Self::ChartStackedBar,
        Self::ChartArea,
        Self::ChartPie,
        Self::ChartDonut,
        Self::KpiCard,
        Self::ProgressBar,
        Self::TableRows,
    ];

    /// The id documents and the AI use.
    pub const fn id(self) -> &'static str {
        match self {
            Self::ChartLine => "chart.line",
            Self::ChartBar => "chart.bar",
            Self::ChartStackedBar => "chart.stacked_bar",
            Self::ChartArea => "chart.area",
            Self::ChartPie => "chart.pie",
            Self::ChartDonut => "chart.donut",
            Self::KpiCard => "kpi.card",
            Self::ProgressBar => "progress.bar",
            Self::TableRows => "table.rows",
        }
    }

    /// The component with this id; `None` for one this version doesn't
    /// know (a document shows a placeholder for it, D-278).
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.id() == id)
    }

    /// Which entry of a dataset's `suits` (catalog) says it fits this
    /// component: an area chart reads like a line, a donut like a pie, a
    /// progress bar like a key figure.
    pub const fn suit(self) -> &'static str {
        match self {
            Self::ChartLine | Self::ChartArea => "line",
            Self::ChartBar => "bar",
            Self::ChartStackedBar => "stacked_bar",
            Self::ChartPie | Self::ChartDonut => "pie",
            Self::KpiCard | Self::ProgressBar => "kpi",
            Self::TableRows => "table",
        }
    }

    /// The datasets this component can show, in the catalog's order.
    pub fn datasets(self) -> impl Iterator<Item = &'static DatasetInfo> {
        CATALOG
            .iter()
            .filter(move |dataset| dataset.suits.contains(&self.suit()))
    }

    /// The properties of this component, in the order the panel lists them.
    pub const fn properties(self) -> &'static [Property] {
        match self {
            Self::ChartLine
            | Self::ChartBar
            | Self::ChartStackedBar
            | Self::ChartArea
            | Self::ChartPie
            | Self::ChartDonut => &CHART,
            Self::KpiCard => &KPI_CARD,
            Self::ProgressBar => &PROGRESS_BAR,
            Self::TableRows => &TABLE,
        }
    }
}

/// Where a property sits in a component of a document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// `params`: what data is asked for.
    Params,
    /// `props`: how it is shown.
    Props,
}

impl Place {
    /// The key of the object that holds the property.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Params => "params",
            Self::Props => "props",
        }
    }
}

/// What a property can be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A short text of at most [`MAX_TEXT_CHARS`] characters, on one line.
    Text,
    /// Yes or no.
    Flag {
        /// The value of a new component.
        default: bool,
    },
    /// One word of a fixed list; the first is the default.
    Choice(&'static [&'static str]),
    /// One of the dataset's groupings (catalog); the first is the default.
    Grouping,
}

/// One property of a component.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Property {
    /// The key in the document; the app looks the label up by it.
    pub key: &'static str,
    /// Whether it is a parameter or a property.
    pub place: Place,
    /// What it can be.
    pub kind: Kind,
}

/// The value of a property.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Setting {
    /// For [`Kind::Text`].
    Text(String),
    /// For [`Kind::Flag`].
    Flag(bool),
    /// For [`Kind::Choice`] and [`Kind::Grouping`].
    Word(String),
}

const TITLE: Property = Property {
    key: "title",
    place: Place::Props,
    kind: Kind::Text,
};
const GROUPING: Property = Property {
    key: "grouping",
    place: Place::Params,
    kind: Kind::Grouping,
};
const LEGEND: Property = Property {
    key: "legend",
    place: Place::Props,
    kind: Kind::Flag { default: true },
};
const COMPARE: Property = Property {
    key: "compare",
    place: Place::Params,
    kind: Kind::Choice(&["previous_period", "same_last_year"]),
};
const MEASURE: Property = Property {
    key: "measure",
    place: Place::Params,
    kind: Kind::Choice(&["income", "expenses", "net", "savings-rate"]),
};

const CHART: [Property; 3] = [GROUPING, TITLE, LEGEND];
const KPI_CARD: [Property; 3] = [MEASURE, COMPARE, TITLE];
const PROGRESS_BAR: [Property; 2] = [COMPARE, TITLE];
const TABLE: [Property; 2] = [GROUPING, TITLE];

fn invalid(reason: &'static str, path: String) -> ReportError {
    ReportError::DocumentInvalid { reason, path }
}

impl Property {
    /// `params.grouping`-style name of this property, for error reports.
    pub fn path(&self) -> String {
        format!("{}.{}", self.place.key(), self.key)
    }

    /// The words this property can take for `dataset`; empty unless it is a
    /// [`Kind::Choice`] or [`Kind::Grouping`].
    pub fn words(&self, dataset: &DatasetInfo) -> &'static [&'static str] {
        match self.kind {
            Kind::Choice(words) => words,
            Kind::Grouping => dataset.groupings,
            Kind::Text | Kind::Flag { .. } => &[],
        }
    }

    /// The value of a new component.
    pub fn default_setting(&self, dataset: &DatasetInfo) -> Setting {
        match self.kind {
            Kind::Text => Setting::Text(String::new()),
            Kind::Flag { default } => Setting::Flag(default),
            Kind::Choice(_) | Kind::Grouping => Setting::Word(
                self.words(dataset)
                    .first()
                    .copied()
                    .unwrap_or("")
                    .to_owned(),
            ),
        }
    }

    /// Whether `setting` is a value this property can take for `dataset`.
    pub fn check(&self, dataset: &DatasetInfo, setting: &Setting) -> Result<(), ReportError> {
        match (self.kind, setting) {
            (Kind::Text, Setting::Text(text)) => {
                if text.chars().count() > MAX_TEXT_CHARS {
                    Err(invalid("too-long", self.path()))
                } else if text.chars().any(char::is_control) {
                    Err(invalid("bad-text", self.path()))
                } else {
                    Ok(())
                }
            }
            (Kind::Flag { .. }, Setting::Flag(_)) => Ok(()),
            (Kind::Choice(_) | Kind::Grouping, Setting::Word(word)) => {
                if self.words(dataset).contains(&word.as_str()) {
                    Ok(())
                } else {
                    Err(invalid("bad-choice", self.path()))
                }
            }
            _ => Err(invalid("wrong-type", self.path())),
        }
    }
}

/// The dataset `id` if `component` can show it.
pub fn dataset_for(
    component: ComponentType,
    id: &str,
) -> Result<&'static DatasetInfo, ReportError> {
    let dataset = crate::dataset(id).ok_or_else(|| invalid("unknown-dataset", "dataset".into()))?;
    if dataset.suits.contains(&component.suit()) {
        Ok(dataset)
    } else {
        Err(invalid("dataset-not-suited", "dataset".into()))
    }
}
