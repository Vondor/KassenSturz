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
use crate::catalog::{CATALOG, DatasetInfo, NO_DATASET};
use crate::columns::MAX_COLUMNS;
use crate::formula::MAX_FORMULA_CHARS;

/// The `measure` of a key figure that shows the result of its formula.
pub const FORMULA_MEASURE: &str = "formula";

/// The longest title, in characters (D-281).
pub const MAX_TEXT_CHARS: usize = 200;

/// The most IDs a list property holds (D-281): the series limit.
pub const MAX_IDS: usize = 50;

/// The longest ID in a list property, in characters. An ID is a UUID written
/// as text; the bound only keeps a hostile document small.
const MAX_ID_CHARS: usize = 64;

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
    /// `text.heading`: a line of text that names a section (D-312). It
    /// shows no data, so it is not in [`ComponentType::ALL`].
    Heading,
    /// `text.separator`: a thin line that divides the page (D-334). It
    /// shows no data.
    Separator,
    /// `notice.fetch_gap`: the warnings about accounts that were not
    /// updated from the bank for a long time (D-328). The page's own notices
    /// fill it, not a dataset.
    FetchNotice,
    /// `notice.transfers`: the note that money moved between the user's
    /// own accounts is not counted (D-328). Filled by the page.
    TransfersNote,
    /// `group.period`: a framed area with a period of its own (D-333). The
    /// widgets that lie completely inside it and have no period of their
    /// own show its period. It shows no data.
    PeriodBox,
    /// `card.net_worth`: what all the accounts hold together less what is
    /// owed, a card for each currency (D-329). Today's figure; the page
    /// fills it.
    NetWorth,
    /// `table.income_sources`: where the money of the month came from, by
    /// category (D-336). The page fills it.
    IncomeSources,
    /// `budget.progress`: one bar per budget, or for the budgets of the
    /// chosen categories, from `budget_utilization` (D-357, D-378). It is
    /// in [`ComponentType::LIBRARY`] since M6c.5 draws it (D-381), but not
    /// in [`ComponentType::ALL`]: the AI sees budgets only with the Budgets
    /// row of the permission grid (D-358).
    BudgetProgress,
}

impl ComponentType {
    /// Every component that shows data, in the order the app offers them.
    /// The AI may pick any of these (D-282); the heading is not among them.
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

    /// Every component the grid editor's library offers: [`Self::ALL`] and
    /// the heading (D-312).
    pub const LIBRARY: [Self; 17] = [
        Self::ChartLine,
        Self::ChartBar,
        Self::ChartStackedBar,
        Self::ChartArea,
        Self::ChartPie,
        Self::ChartDonut,
        Self::KpiCard,
        Self::ProgressBar,
        Self::TableRows,
        Self::Heading,
        Self::Separator,
        Self::FetchNotice,
        Self::TransfersNote,
        Self::PeriodBox,
        Self::NetWorth,
        Self::IncomeSources,
        Self::BudgetProgress,
    ];

    /// Whether the component shows no figures of the book: its document names
    /// no dataset (D-312, D-328, D-334).
    pub const fn shows_no_data(self) -> bool {
        matches!(
            self,
            Self::Heading
                | Self::Separator
                | Self::FetchNotice
                | Self::TransfersNote
                | Self::PeriodBox
                | Self::NetWorth
                | Self::IncomeSources
        )
    }

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
            Self::Heading => "text.heading",
            Self::Separator => "text.separator",
            Self::FetchNotice => "notice.fetch_gap",
            Self::TransfersNote => "notice.transfers",
            Self::PeriodBox => "group.period",
            Self::NetWorth => "card.net_worth",
            Self::IncomeSources => "table.income_sources",
            Self::BudgetProgress => "budget.progress",
        }
    }

    /// The component with this id; `None` for one this version doesn't
    /// know (a document shows a placeholder for it, D-278).
    pub fn from_id(id: &str) -> Option<Self> {
        Self::LIBRARY.into_iter().find(|kind| kind.id() == id)
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
            Self::BudgetProgress => "budget",
            Self::Heading
            | Self::Separator
            | Self::FetchNotice
            | Self::TransfersNote
            | Self::PeriodBox
            | Self::NetWorth
            | Self::IncomeSources => "text",
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
            Self::Heading => &HEADING,
            Self::Separator
            | Self::FetchNotice
            | Self::TransfersNote
            | Self::NetWorth
            | Self::IncomeSources => &[],
            Self::PeriodBox => &PERIOD_BOX,
            Self::BudgetProgress => &BUDGET_PROGRESS,
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
    /// A list of up to [`MAX_IDS`] category or group IDs, written as text
    /// (M6b.5). The book decides which of them it still knows.
    Ids,
    /// A formula in the canonical form the book stores (D-295), at most
    /// [`MAX_FORMULA_CHARS`] characters on one line. Empty means the
    /// component shows what its other properties say. Only the length and
    /// the characters are checked here: a formula with a mistake fails on
    /// its own component when it is worked out, not the whole document
    /// (T-41, M6b.7).
    Formula,
    /// A list of up to [`MAX_COLUMNS`] columns, each a name and up to
    /// [`MAX_IDS`] category or group IDs (D-339). Empty means the chart or
    /// table shows what its dataset says; otherwise one series or row for
    /// each column. Names are not empty and differ from each other, ignoring
    /// case, so a formula can use them.
    Columns,
}

/// One column of a chart or table: what it is called and what is under it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnSetting {
    /// The name; a series' name, and what a formula calls it.
    pub name: String,
    /// The IDs of the categories and groups under it, written as text.
    pub members: Vec<String>,
    /// A formula in the form the book stores, whose amount is the column's
    /// figure (D-341), or empty; then `members` is empty.
    pub formula: String,
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
    /// For [`Kind::Text`] and [`Kind::Formula`].
    Text(String),
    /// For [`Kind::Flag`].
    Flag(bool),
    /// For [`Kind::Choice`] and [`Kind::Grouping`].
    Word(String),
    /// For [`Kind::Ids`].
    Ids(Vec<String>),
    /// For [`Kind::Columns`].
    Columns(Vec<ColumnSetting>),
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
const GROUPS: Property = Property {
    key: "groups",
    place: Place::Params,
    kind: Kind::Ids,
};
/// `params.categories` (D-331): the categories and groups a spending chart or
/// table is limited to. Empty means all of them.
const CATEGORIES: Property = Property {
    key: "categories",
    place: Place::Params,
    kind: Kind::Ids,
};
const SERIES: Property = Property {
    key: "series",
    place: Place::Params,
    kind: Kind::Ids,
};
/// `params.columns` (D-339): the columns a chart or table is made of.
const COLUMNS: Property = Property {
    key: "columns",
    place: Place::Params,
    kind: Kind::Columns,
};
const SHOW_HIDDEN: Property = Property {
    key: "show_hidden",
    place: Place::Props,
    kind: Kind::Flag { default: false },
};
const LEGEND: Property = Property {
    key: "legend",
    place: Place::Props,
    kind: Kind::Flag { default: true },
};
/// `props.formula` (D-289, D-322): the figure of a key figure whose measure is
/// `formula`, or of a progress bar.
const FORMULA: Property = Property {
    key: "formula",
    place: Place::Props,
    kind: Kind::Formula,
};
/// `props.target` (D-289, D-322): the figure a progress bar's formula is
/// measured against; a full bar.
const TARGET: Property = Property {
    key: "target",
    place: Place::Props,
    kind: Kind::Formula,
};
const TEXT: Property = Property {
    key: "text",
    place: Place::Props,
    kind: Kind::Text,
};
/// `props.period` (D-289): `page` takes the period of the page the component
/// is on; the others are the fixed periods of [`crate::PERIODS`].
const PERIOD: Property = Property {
    key: "period",
    place: Place::Props,
    kind: Kind::Choice(&[
        "page",
        "this-month",
        "last-month",
        "last-12-months",
        "this-year",
        "last-year",
    ]),
};
/// The period of a period box (D-333): one of the fixed periods, never
/// `page`, because a box that followed the page would change nothing.
const BOX_PERIOD: Property = Property {
    key: "period",
    place: Place::Props,
    kind: Kind::Choice(&[
        "this-month",
        "last-month",
        "last-12-months",
        "this-year",
        "last-year",
    ]),
};
const COMPARE: Property = Property {
    key: "compare",
    place: Place::Params,
    kind: Kind::Choice(&["previous_period", "same_last_year"]),
};
/// `params.measure`: the figure a key figure card shows. Its words are the
/// dataset's `measures` (catalog, D-368), so each dataset offers its own
/// figures; [`Property::words`] looks them up.
const MEASURE: Property = Property {
    key: MEASURE_KEY,
    place: Place::Params,
    kind: Kind::Choice(&[]),
};
const MEASURE_KEY: &str = "measure";

const CHART: [Property; 9] = [
    GROUPING,
    GROUPS,
    CATEGORIES,
    SERIES,
    COLUMNS,
    TITLE,
    LEGEND,
    SHOW_HIDDEN,
    PERIOD,
];
const KPI_CARD: [Property; 5] = [MEASURE, COMPARE, FORMULA, TITLE, PERIOD];
const PROGRESS_BAR: [Property; 5] = [COMPARE, FORMULA, TARGET, TITLE, PERIOD];
const TABLE: [Property; 7] = [
    GROUPING,
    GROUPS,
    CATEGORIES,
    COLUMNS,
    TITLE,
    SHOW_HIDDEN,
    PERIOD,
];
const HEADING: [Property; 1] = [TEXT];
const PERIOD_BOX: [Property; 2] = [TITLE, BOX_PERIOD];
/// The budget bars (D-357): which categories' budgets (empty: every
/// budget), a title and the period whose last day (or today, if earlier)
/// the bars are asked for (D-346).
const BUDGET_PROGRESS: [Property; 3] = [CATEGORIES, TITLE, PERIOD];

fn invalid(reason: &'static str, path: String) -> ReportError {
    ReportError::DocumentInvalid { reason, path }
}

impl Property {
    /// `params.grouping`-style name of this property, for error reports.
    pub fn path(&self) -> String {
        format!("{}.{}", self.place.key(), self.key)
    }

    /// The words this property can take for `dataset`; empty unless it is a
    /// [`Kind::Choice`] or [`Kind::Grouping`]. A key figure's `measure`
    /// takes the dataset's own figures (D-368).
    pub fn words(&self, dataset: &DatasetInfo) -> &'static [&'static str] {
        match self.kind {
            Kind::Choice(_) if self.key == MEASURE_KEY && self.place == Place::Params => {
                dataset.measures
            }
            Kind::Choice(words) => words,
            Kind::Grouping => dataset.groupings,
            Kind::Text | Kind::Flag { .. } | Kind::Ids | Kind::Formula | Kind::Columns => &[],
        }
    }

    /// The value of a new component.
    pub fn default_setting(&self, dataset: &DatasetInfo) -> Setting {
        match self.kind {
            Kind::Text | Kind::Formula => Setting::Text(String::new()),
            Kind::Flag { default } => Setting::Flag(default),
            Kind::Ids => Setting::Ids(Vec::new()),
            Kind::Columns => Setting::Columns(Vec::new()),
            Kind::Choice(_) | Kind::Grouping => Setting::Word(
                self.words(dataset)
                    .first()
                    .copied()
                    .unwrap_or("")
                    .to_owned(),
            ),
        }
    }

    fn check_ids(&self, ids: &[String], most: usize) -> Result<(), ReportError> {
        if ids.len() > most {
            Err(invalid("too-many", self.path()))
        } else if ids.iter().any(|id| {
            id.is_empty() || id.chars().count() > MAX_ID_CHARS || id.chars().any(char::is_control)
        }) {
            Err(invalid("bad-text", self.path()))
        } else {
            Ok(())
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
            (Kind::Formula, Setting::Text(text)) => {
                if text.chars().count() > MAX_FORMULA_CHARS {
                    Err(invalid("too-long", self.path()))
                } else if text.chars().any(char::is_control) {
                    Err(invalid("bad-text", self.path()))
                } else {
                    Ok(())
                }
            }
            (Kind::Flag { .. }, Setting::Flag(_)) => Ok(()),
            (Kind::Ids, Setting::Ids(ids)) => self.check_ids(ids, MAX_IDS),
            (Kind::Columns, Setting::Columns(columns)) => {
                if columns.len() > MAX_COLUMNS {
                    return Err(invalid("too-many", self.path()));
                }
                let mut names: Vec<String> = Vec::new();
                for column in columns {
                    let name = column.name.trim();
                    if name.is_empty()
                        || column.name.chars().count() > MAX_TEXT_CHARS
                        || column.name.chars().any(char::is_control)
                    {
                        return Err(invalid("bad-text", self.path()));
                    }
                    let lower = name.to_lowercase();
                    if names.contains(&lower) {
                        return Err(invalid("duplicate-name", self.path()));
                    }
                    names.push(lower);
                    self.check_ids(&column.members, MAX_IDS)?;
                    // A column is made of members or of a formula, not both.
                    if !column.formula.is_empty() {
                        if !column.members.is_empty() {
                            return Err(invalid("wrong-type", self.path()));
                        }
                        if column.formula.chars().count() > MAX_FORMULA_CHARS {
                            return Err(invalid("too-long", self.path()));
                        }
                        if column.formula.chars().any(char::is_control) {
                            return Err(invalid("bad-text", self.path()));
                        }
                    }
                }
                Ok(())
            }
            (Kind::Choice(_) | Kind::Grouping, Setting::Word(word)) => {
                // A period can also be a typed range of days (D-332).
                if self.words(dataset).contains(&word.as_str())
                    || (self.key == "period" && crate::read_range_word(word).is_some())
                {
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
    if component.shows_no_data() {
        return if id.is_empty() {
            Ok(&NO_DATASET)
        } else {
            Err(invalid("dataset-not-suited", "dataset".into()))
        };
    }
    let dataset = crate::dataset(id).ok_or_else(|| invalid("unknown-dataset", "dataset".into()))?;
    if dataset.suits.contains(&component.suit()) {
        Ok(dataset)
    } else {
        Err(invalid("dataset-not-suited", "dataset".into()))
    }
}
