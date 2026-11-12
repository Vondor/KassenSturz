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
use crate::filter::MAX_FILTER_IDS;
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
    /// `notice.unsorted`: the note about the transactions that have no
    /// category yet, counted across the whole book, Transfers left out, with
    /// the newest of them to open (US1.9, #535, D-637). Filled by the page.
    /// It shows nothing while there is none.
    UnsortedNote,
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
    /// `bank.update_all`: one button that fetches every linked account from
    /// its bank and books what came (D-446, D-449). It is in
    /// [`ComponentType::LIBRARY`], never in [`ComponentType::ALL`]: only the
    /// user places and starts it, never the AI (T-55). It shows no data.
    BankUpdateAll,
    /// `page.period`: the period header, the month the page shows with the
    /// way to the months before and after it and back to this one (D-452,
    /// D-474). It shows no data and has no settings; the page fills it. A
    /// dashboard may hold any number (D-532).
    PeriodHeader,
    /// `page.filters`: the filter bar, the page's period, accounts and
    /// categories (D-351, D-452, D-474). Its choices stay the document's
    /// `filters`; they apply only while the dashboard holds the bar, so
    /// nothing narrows a page without showing it (D-475). It shows no data
    /// and has no settings. A dashboard may hold any number (D-532).
    FilterBar,
    /// `page.tabs`: the tabs of the book's dashboards, one per dashboard in
    /// the book's order, to switch between them (D-436, D-438, D-529). It
    /// shows no data and has no settings; the page fills it, and it shows
    /// nothing while the book has one dashboard. A dashboard may hold any
    /// number (D-532).
    DashboardTabs,
    /// `chart.custom`: a chart the user describes in a short text, a subset
    /// of the Vega-Lite language (M6e, D-504). The text is `props.spec`; the
    /// dataset it shows is the one it names, written to `dataset` whenever
    /// the text is saved. It is in [`ComponentType::LIBRARY`] and never in
    /// [`ComponentType::ALL`]: the AI does not write or see descriptions
    /// (Q-352).
    CustomChart,
    /// `chart.candlestick`: the balance of the chosen accounts as candles,
    /// one per day, week or month of the period: where it started, its
    /// highest, its lowest and where it ended (US4.10, #699, D-718, D-719).
    /// It is in [`ComponentType::LIBRARY`] and not in [`ComponentType::ALL`]:
    /// the AI does not prepare it.
    ChartCandlestick,
    /// `table.transactions`: single transactions of the chosen accounts,
    /// categories and period, newest first, like an account's list (US4.7,
    /// D-716). It only shows them; a click opens the transaction. Its own
    /// bridge object fills it, so its document names no dataset, and like
    /// the income sources it takes every shared filter. It is in
    /// [`ComponentType::LIBRARY`], never in [`ComponentType::ALL`]: the AI
    /// neither proposes nor reads it.
    Transactions,
    /// `notice.alarms`: the warnings tile, a list of rules the user sets
    /// that says which of them a budget or an account crosses today (US4.9,
    /// D-721). It shows no dataset; its own bridge object fills it. It is in
    /// [`ComponentType::LIBRARY`] and never in [`ComponentType::ALL`]: the AI
    /// neither proposes it nor sees its rules.
    Alarms,
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
    pub const LIBRARY: [Self; 26] = [
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
        Self::UnsortedNote,
        Self::PeriodBox,
        Self::NetWorth,
        Self::IncomeSources,
        Self::BudgetProgress,
        Self::BankUpdateAll,
        Self::PeriodHeader,
        Self::FilterBar,
        Self::DashboardTabs,
        Self::CustomChart,
        Self::ChartCandlestick,
        Self::Transactions,
        Self::Alarms,
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
                | Self::UnsortedNote
                | Self::PeriodBox
                | Self::NetWorth
                | Self::IncomeSources
                | Self::BankUpdateAll
                | Self::PeriodHeader
                | Self::FilterBar
                | Self::DashboardTabs
                | Self::Transactions
                | Self::Alarms
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
            Self::UnsortedNote => "notice.unsorted",
            Self::PeriodBox => "group.period",
            Self::NetWorth => "card.net_worth",
            Self::IncomeSources => "table.income_sources",
            Self::BudgetProgress => "budget.progress",
            Self::BankUpdateAll => "bank.update_all",
            Self::PeriodHeader => "page.period",
            Self::FilterBar => "page.filters",
            Self::DashboardTabs => "page.tabs",
            Self::CustomChart => "chart.custom",
            Self::ChartCandlestick => "chart.candlestick",
            Self::Transactions => "table.transactions",
            Self::Alarms => "notice.alarms",
        }
    }

    /// The component with this id; `None` for one this version doesn't
    /// know (a document shows a placeholder for it, D-278).
    pub fn from_id(id: &str) -> Option<Self> {
        Self::LIBRARY.into_iter().find(|kind| kind.id() == id)
    }

    /// Which entry of a dataset's `suits` (catalog) says it fits this
    /// component: an area chart reads like a line, a donut like a pie. A
    /// progress bar has a fit of its own (D-370): the figures of the
    /// balances do not make one.
    pub const fn suit(self) -> &'static str {
        match self {
            Self::ChartLine | Self::ChartArea => "line",
            Self::ChartBar => "bar",
            Self::ChartStackedBar => "stacked_bar",
            Self::ChartPie | Self::ChartDonut => "pie",
            Self::KpiCard => "kpi",
            Self::ProgressBar => "progress",
            Self::TableRows => "table",
            Self::BudgetProgress => "budget",
            // A custom chart can show every dataset of the spec (see
            // `ComponentType::datasets`), so no dataset lists this fit.
            Self::CustomChart => "custom",
            Self::ChartCandlestick => "candlestick",
            Self::Heading
            | Self::Separator
            | Self::FetchNotice
            | Self::TransfersNote
            | Self::UnsortedNote
            | Self::PeriodBox
            | Self::NetWorth
            | Self::IncomeSources
            | Self::BankUpdateAll
            | Self::PeriodHeader
            | Self::FilterBar
            | Self::DashboardTabs
            | Self::Transactions
            | Self::Alarms => "text",
        }
    }

    /// The datasets this component can show, in the catalog's order.
    pub fn datasets(self) -> impl Iterator<Item = &'static DatasetInfo> {
        CATALOG.iter().filter(move |dataset| self.shows(dataset))
    }

    /// The dataset a new component of this type starts with: the first it
    /// can show, except that a key figure card and stacked bars start with
    /// money in and out by month, as they did before spending suited them
    /// too (D-666). `None` for a type without data.
    pub fn first_dataset(self) -> Option<&'static DatasetInfo> {
        let preferred = match self {
            Self::KpiCard | Self::ChartStackedBar => Some(crate::INCOME_EXPENSE_BY_MONTH),
            _ => None,
        };
        preferred
            .and_then(|id| self.datasets().find(|dataset| dataset.id == id))
            .or_else(|| self.datasets().next())
    }

    /// Whether this component can show `dataset`: the dataset lists its
    /// [`ComponentType::suit`], or, for a custom chart, a description can name it
    /// (`spec::SPEC_DATASETS`).
    pub fn shows(self, dataset: &DatasetInfo) -> bool {
        if self == Self::CustomChart {
            crate::spec::spec_dataset(dataset.id).is_some()
        } else {
            dataset.suits.contains(&self.suit())
        }
    }

    /// The properties of this component, in the order the panel lists them.
    pub const fn properties(self) -> &'static [Property] {
        match self {
            Self::ChartLine | Self::ChartArea | Self::ChartPie | Self::ChartDonut => &CHART,
            Self::ChartBar => &BAR_CHART,
            Self::ChartStackedBar => &STACKED_BAR,
            Self::KpiCard => &KPI_CARD,
            Self::ProgressBar => &PROGRESS_BAR,
            Self::TableRows => &TABLE,
            Self::Heading => &HEADING,
            Self::Separator
            | Self::FetchNotice
            | Self::TransfersNote
            | Self::UnsortedNote
            | Self::PeriodHeader
            | Self::FilterBar
            | Self::DashboardTabs => &[],
            Self::NetWorth | Self::IncomeSources => &NOTES_ONLY,
            Self::PeriodBox => &PERIOD_BOX,
            Self::BudgetProgress => &BUDGET_PROGRESS,
            Self::BankUpdateAll => &UPDATE_ALL,
            Self::CustomChart => &CUSTOM_CHART,
            Self::ChartCandlestick => &CANDLESTICK,
            Self::Transactions => &TRANSACTIONS,
            Self::Alarms => &ALARMS,
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
    /// A list of up to [`MAX_FILTER_IDS`] IDs of the user's own accounts,
    /// written as text (D-351): the accounts a component counts on its own.
    /// Empty means it follows the page's filter bar. The book decides
    /// which of them it still knows, when the filter is applied.
    Accounts,
    /// A list of up to [`MAX_FILTER_IDS`] budget IDs, written as text
    /// (D-642): the budgets the budget bars show. Empty means every budget.
    /// The book decides which of them it still knows; a dashboard file
    /// leaves them out, since another book has budgets of its own.
    Budgets,
    /// A list of up to [`MAX_FILTER_IDS`] tag IDs, written as text (US5.2,
    /// D-714): only the parts that carry one of them count. Empty means
    /// every part, whatever its tags. The book decides which of them it
    /// still knows, when the filter is applied; a dashboard file leaves
    /// them out, since another book has tags of its own.
    Tags,
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
    /// A chart description (M6e.3, D-504): the text of a custom chart, at
    /// most [`crate::spec::MAX_SPEC_BYTES`] bytes on any number of lines, which
    /// [`crate::spec::read`] must accept. What it holds is the user's own
    /// text; nothing of it is changed.
    Spec,
    /// The rules of a warnings tile (US4.9, D-722): at most
    /// [`crate::MAX_ALARMS`], each checked by [`crate::AlarmRule::check`].
    /// They name accounts and budgets by the book's IDs, so a dashboard
    /// file leaves them out (D-726).
    Alarms,
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
    /// For [`Kind::Ids`], [`Kind::Accounts`], [`Kind::Budgets`] and
    /// [`Kind::Tags`].
    Ids(Vec<String>),
    /// For [`Kind::Columns`].
    Columns(Vec<ColumnSetting>),
    /// For [`Kind::Alarms`].
    Alarms(Vec<crate::AlarmRule>),
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
/// `params.accounts` (D-351): the accounts a component counts on its own,
/// whatever the filter bar says. Empty means it follows the page.
const ACCOUNTS: Property = Property {
    key: "accounts",
    place: Place::Params,
    kind: Kind::Accounts,
};
/// `params.tags` (US5.2, D-714): only the parts that carry one of these
/// tags count, so the component shows a sum by tag. Empty means every part.
/// The filter bar has no tags, so this is the component's own only.
const TAGS: Property = Property {
    key: TAGS_KEY,
    place: Place::Params,
    kind: Kind::Tags,
};
/// The key of a component's own tags.
pub(crate) const TAGS_KEY: &str = "tags";
/// `params.budgets` (D-642): the budgets the budget bars show, their *Which
/// budgets*. Empty means every budget.
const BUDGETS: Property = Property {
    key: "budgets",
    place: Place::Params,
    kind: Kind::Budgets,
};
const SERIES: Property = Property {
    key: "series",
    place: Place::Params,
    kind: Kind::Ids,
};
/// `params.income_accounts`, `params.income_categories`,
/// `params.expenses_accounts` and `params.expenses_categories` (US1.16,
/// D-646): a filter of its own for money in and for money out of
/// `income_expense_by_month`, the only dataset that takes them. Empty means
/// the series follows the component's own lists or the filter bar.
const INCOME_ACCOUNTS: Property = Property {
    key: "income_accounts",
    place: Place::Params,
    kind: Kind::Accounts,
};
const INCOME_CATEGORIES: Property = Property {
    key: "income_categories",
    place: Place::Params,
    kind: Kind::Ids,
};
const EXPENSES_ACCOUNTS: Property = Property {
    key: "expenses_accounts",
    place: Place::Params,
    kind: Kind::Accounts,
};
const EXPENSES_CATEGORIES: Property = Property {
    key: "expenses_categories",
    place: Place::Params,
    kind: Kind::Ids,
};
/// `params.columns` (D-339): the columns a chart or table is made of.
const COLUMNS: Property = Property {
    key: "columns",
    place: Place::Params,
    kind: Kind::Columns,
};
/// `props.spec` (D-504): the description of a custom chart, exactly as the
/// user wrote it.
const SPEC: Property = Property {
    key: "spec",
    place: Place::Props,
    kind: Kind::Spec,
};
/// `props.partial` (D-508): draw the supported part of a description that
/// has parts which are not supported here.
const PARTIAL: Property = Property {
    key: "partial",
    place: Place::Props,
    kind: Kind::Flag { default: false },
};
/// `props.hide_note` (US1.9, #523, D-636; M6-B202, #581, D-663): leave out
/// the grey marks under a tile that say what it is filtered to (*Own
/// filter*, *Filtered to*, *Not filtered by*). Every tile that can show them
/// has it; they are shown by default. The notes about a problem (a filter
/// that chooses nothing, one that names something gone) stay, and a title
/// drawn by the tile names the accounts it counts instead (D-636).
const HIDE_NOTE: Property = Property {
    key: "hide_note",
    place: Place::Props,
    kind: Kind::Flag { default: false },
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
/// `params.average` (US1.13, D-648): spending as its total or as an
/// average per month, quarter or year. Only `spending_by_category` takes an
/// average; other datasets offer `total` alone.
const AVERAGE: Property = Property {
    key: AVERAGE_KEY,
    place: Place::Params,
    kind: Kind::Choice(&crate::AVERAGE_WORDS),
};
pub(crate) const AVERAGE_KEY: &str = "average";
/// `params.compare` of a spending bar chart or table (US3.1a, #587, D-687):
/// each category next to its figure in the period before, the same period
/// last year, or the average of the 3, 6 or 12 periods before. Only
/// `spending_by_category` compares; other datasets offer `none` alone. A
/// key figure card's `compare` is [`COMPARE`], with its two words.
const COMPARE_SPENDING: Property = Property {
    key: COMPARE_KEY,
    place: Place::Params,
    kind: Kind::Choice(&crate::COMPARE_WORDS),
};
pub(crate) const COMPARE_KEY: &str = "compare";
/// `props.orientation` (US2, #563, D-679): which way the bars of a bar or
/// stacked bar chart go. `auto`, the default, keeps the rule the chart had
/// before: rows for one series whose names are too long to stand under
/// their bars (M6-B3), upright bars otherwise. `horizontal` always draws
/// rows, `vertical` always upright bars. Only the drawing changes, never a
/// figure.
const ORIENTATION: Property = Property {
    key: ORIENTATION_KEY,
    place: Place::Props,
    kind: Kind::Choice(&ORIENTATION_WORDS),
};
pub(crate) const ORIENTATION_KEY: &str = "orientation";
/// The words of `props.orientation`, the default first.
pub const ORIENTATION_WORDS: [&str; 3] = ["auto", "horizontal", "vertical"];
/// `params.candle` (US4.10, Q-498, D-719): how many days one candle of a
/// candle chart covers. `auto`, the default, keeps the rule of D-719 (a
/// candle per day up to a month, per week up to half a year, else per
/// month); `day`, `week` and `month` always use that length. It decides
/// which days each candle's figures come from, so it is a parameter.
const CANDLE: Property = Property {
    key: CANDLE_KEY,
    place: Place::Params,
    kind: Kind::Choice(&CANDLE_WORDS),
};
pub(crate) const CANDLE_KEY: &str = "candle";
/// The words of `params.candle`, the default first.
pub const CANDLE_WORDS: [&str; 4] = ["auto", "day", "week", "month"];

const CHART: [Property; 17] = [
    GROUPING,
    AVERAGE,
    GROUPS,
    CATEGORIES,
    ACCOUNTS,
    TAGS,
    SERIES,
    INCOME_CATEGORIES,
    INCOME_ACCOUNTS,
    EXPENSES_CATEGORIES,
    EXPENSES_ACCOUNTS,
    COLUMNS,
    TITLE,
    LEGEND,
    SHOW_HIDDEN,
    HIDE_NOTE,
    PERIOD,
];
/// Candles: which accounts, the title, the legend and the period; the
/// length of a candle follows the period (D-719).
const CANDLESTICK: [Property; 5] = [ACCOUNTS, TITLE, LEGEND, PERIOD, CANDLE];
/// Bars: a chart's properties, the comparison of spending (US3.1a) after
/// the average, and the way the bars go (D-679) after the legend.
const BAR_CHART: [Property; 19] = [
    GROUPING,
    AVERAGE,
    COMPARE_SPENDING,
    GROUPS,
    CATEGORIES,
    ACCOUNTS,
    TAGS,
    SERIES,
    INCOME_CATEGORIES,
    INCOME_ACCOUNTS,
    EXPENSES_CATEGORIES,
    EXPENSES_ACCOUNTS,
    COLUMNS,
    TITLE,
    LEGEND,
    ORIENTATION,
    SHOW_HIDDEN,
    HIDE_NOTE,
    PERIOD,
];
/// Stacked bars: the properties of bars without a comparison, since spending
/// stacked is one bar split into its categories (D-666) and has no place for
/// a second one.
const STACKED_BAR: [Property; 18] = [
    GROUPING,
    AVERAGE,
    GROUPS,
    CATEGORIES,
    ACCOUNTS,
    TAGS,
    SERIES,
    INCOME_CATEGORIES,
    INCOME_ACCOUNTS,
    EXPENSES_CATEGORIES,
    EXPENSES_ACCOUNTS,
    COLUMNS,
    TITLE,
    LEGEND,
    ORIENTATION,
    SHOW_HIDDEN,
    HIDE_NOTE,
    PERIOD,
];
const KPI_CARD: [Property; 8] = [
    MEASURE, ACCOUNTS, TAGS, HIDE_NOTE, COMPARE, FORMULA, TITLE, PERIOD,
];
const PROGRESS_BAR: [Property; 8] = [
    ACCOUNTS, TAGS, COMPARE, FORMULA, TARGET, TITLE, HIDE_NOTE, PERIOD,
];
const TABLE: [Property; 16] = [
    GROUPING,
    AVERAGE,
    COMPARE_SPENDING,
    GROUPS,
    CATEGORIES,
    ACCOUNTS,
    TAGS,
    INCOME_CATEGORIES,
    INCOME_ACCOUNTS,
    EXPENSES_CATEGORIES,
    EXPENSES_ACCOUNTS,
    COLUMNS,
    TITLE,
    SHOW_HIDDEN,
    HIDE_NOTE,
    PERIOD,
];
const HEADING: [Property; 1] = [TEXT];
/// The Update all accounts button (D-446): its title only; empty shows
/// *Update all accounts*.
const UPDATE_ALL: [Property; 1] = [TITLE];
const PERIOD_BOX: [Property; 2] = [TITLE, BOX_PERIOD];
/// The budget bars (D-357): which budgets (D-642) and which categories'
/// budgets (empty: every budget), a title and the period whose last day (or
/// today, if earlier) the bars are asked for (D-346).
const BUDGET_PROGRESS: [Property; 5] = [BUDGETS, CATEGORIES, TITLE, HIDE_NOTE, PERIOD];
/// What you are worth and where the money came from (D-329, D-336): the page
/// fills them, so the switch for their grey marks is all there is to set
/// (M6-B202, D-663).
const NOTES_ONLY: [Property; 1] = [HIDE_NOTE];

/// A custom chart (D-504): its description, whether to draw it without the
/// parts that are not supported, the series of a chart over time and the
/// categories of the budgets (Q-353), which count only while the description
/// names that dataset, and the period.
/// The warnings tile (D-721, D-722): its rules and its title.
const ALARM_RULES: Property = Property {
    key: ALARMS_KEY,
    place: Place::Props,
    kind: Kind::Alarms,
};
const ALARMS: [Property; 2] = [ALARM_RULES, TITLE];

/// The key of the warnings tile's rules.
pub const ALARMS_KEY: &str = "alarms";

const CUSTOM_CHART: [Property; 6] = [SPEC, PARTIAL, SERIES, CATEGORIES, HIDE_NOTE, PERIOD];

/// `params.rows` of the transactions tile (D-716): how many of the newest
/// transactions it lists.
const ROWS: Property = Property {
    key: "rows",
    place: Place::Params,
    kind: Kind::Choice(&crate::LISTED_ROWS),
};

/// The transactions tile (US4.7, D-716): its own accounts and categories,
/// how many rows, a title, the grey notes switch and the period.
const TRANSACTIONS: [Property; 6] = [ACCOUNTS, CATEGORIES, ROWS, TITLE, HIDE_NOTE, PERIOD];

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
            Kind::Choice(words) if self.key == AVERAGE_KEY || self.compares_spending() => {
                if dataset.id == crate::SPENDING_BY_CATEGORY {
                    words
                } else {
                    &words[..1]
                }
            }
            Kind::Choice(words) => words,
            Kind::Grouping => dataset.groupings,
            Kind::Text
            | Kind::Flag { .. }
            | Kind::Ids
            | Kind::Accounts
            | Kind::Budgets
            | Kind::Tags
            | Kind::Formula
            | Kind::Columns
            | Kind::Spec
            | Kind::Alarms => &[],
        }
    }

    /// Whether this is a spending chart's `params.compare` (US3.1a), whose
    /// words beyond `none` only spending by category takes.
    fn compares_spending(&self) -> bool {
        self.key == COMPARE_KEY
            && matches!(self.kind, Kind::Choice(words) if words.first() == Some(&"none"))
    }

    /// The value of a new component.
    pub fn default_setting(&self, dataset: &DatasetInfo) -> Setting {
        match self.kind {
            Kind::Text | Kind::Formula => Setting::Text(String::new()),
            Kind::Spec => Setting::Text(starter_spec(dataset.id)),
            Kind::Flag { default } => Setting::Flag(default),
            Kind::Ids | Kind::Accounts | Kind::Budgets | Kind::Tags => Setting::Ids(Vec::new()),
            Kind::Columns => Setting::Columns(Vec::new()),
            Kind::Alarms => Setting::Alarms(Vec::new()),
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
            (Kind::Spec, Setting::Text(text)) => {
                // The description is checked as a whole, the way a pasted
                // one is: a refused one is not kept (D-505).
                crate::spec::read(text)
                    .map(|_| ())
                    .map_err(|_| invalid("bad-spec", self.path()))
            }
            (Kind::Flag { .. }, Setting::Flag(_)) => Ok(()),
            // A series' own filter belongs to the dataset whose series it
            // names (D-646); another dataset has nothing it could narrow.
            (Kind::Ids | Kind::Accounts, Setting::Ids(ids))
                if !ids.is_empty()
                    && crate::is_series_filter(self.key)
                    && dataset.id != crate::INCOME_EXPENSE_BY_MONTH =>
            {
                Err(invalid("unknown-property", self.path()))
            }
            (Kind::Ids, Setting::Ids(ids)) => self.check_ids(ids, MAX_IDS),
            (Kind::Accounts | Kind::Budgets | Kind::Tags, Setting::Ids(ids)) => {
                self.check_ids(ids, MAX_FILTER_IDS)
            }
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
            (Kind::Alarms, Setting::Alarms(rules)) => {
                if rules.len() > crate::MAX_ALARMS {
                    return Err(invalid("too-many", self.path()));
                }
                for rule in rules {
                    rule.check()
                        .map_err(|reason| invalid(reason, self.path()))?;
                }
                Ok(())
            }
            (Kind::Choice(_) | Kind::Grouping, Setting::Word(word)) => {
                // A period can also be a rolling period (D-644) or a typed
                // range of days (D-332).
                if self.words(dataset).contains(&word.as_str())
                    || (self.key == "period"
                        && (crate::Rolling::read(word).is_some()
                            || crate::read_range_word(word).is_some()))
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
    if component.shows(dataset) {
        Ok(dataset)
    } else {
        Err(invalid("dataset-not-suited", "dataset".into()))
    }
}

/// The description a new custom chart starts with (D-504): bars of the
/// dataset's most telling figure, so the chart is something to change, not an
/// empty box. Written by hand for each dataset; every one is read by
/// [`crate::spec::read`] in the tests.
fn starter_spec(dataset: &str) -> String {
    let (mark, x, over_time, y, color) = match dataset {
        crate::SPENDING_BY_CATEGORY => ("bar", "category", false, "total", None),
        crate::INCOME_EXPENSE_BY_MONTH => ("bar", "month", true, "net", None),
        crate::ACCOUNT_BALANCES => ("bar", "account", false, "balance", None),
        crate::NET_WORTH_OVER_TIME => ("line", "month", true, "net_worth", None),
        crate::CASH_FLOW_BY_MONTH => ("bar", "month", true, "net_change", None),
        crate::CATEGORY_OVER_TIME => ("line", "month", true, "total", Some("series")),
        _ => ("bar", "category", false, "spent", None),
    };
    let x_type = if over_time {
        "\"type\": \"temporal\", \"timeUnit\": \"yearmonth\""
    } else {
        "\"type\": \"nominal\""
    };
    let color = color.map_or_else(String::new, |field| {
        format!(",\n    \"color\": {{\"field\": \"{field}\", \"type\": \"nominal\"}}")
    });
    format!(
        "{{\n  \"data\": {{\"name\": \"{dataset}\"}},\n  \"mark\": \"{mark}\",\n  \"encoding\": {{\n    \"x\": {{\"field\": \"{x}\", {x_type}}},\n    \"y\": {{\"field\": \"{y}\", \"type\": \"quantitative\"}}{color}\n  }}\n}}\n"
    )
}
