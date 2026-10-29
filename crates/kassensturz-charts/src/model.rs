// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The types of a chart (D-276). Every amount is kept exact, in minor units,
//! with its label already written as text.

use kassensturz_report::TextTable;

/// How many colors the palette has (D-283). A series or slice beyond the
/// eighth starts again at the first.
pub const PALETTE_SIZE: u8 = 8;

/// The kinds of chart M6a draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChartKind {
    /// Lines through the points of each series.
    Line,
    /// Bars side by side.
    Bar,
    /// Bars of the series on top of each other.
    StackedBar,
    /// Lines with the area below them filled.
    Area,
    /// Slices of a circle.
    Pie,
    /// A pie with a hole.
    Donut,
}

impl ChartKind {
    /// The kind's fixed name, as dashboards and QML spell it.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::Bar => "bar",
            Self::StackedBar => "stacked-bar",
            Self::Area => "area",
            Self::Pie => "pie",
            Self::Donut => "donut",
        }
    }

    /// The kind with this name.
    pub fn from_key(key: &str) -> Option<Self> {
        [
            Self::Line,
            Self::Bar,
            Self::StackedBar,
            Self::Area,
            Self::Pie,
            Self::Donut,
        ]
        .into_iter()
        .find(|kind| kind.key() == key)
    }

    /// Whether the chart is made of slices rather than of an axis.
    pub const fn is_round(self) -> bool {
        matches!(self, Self::Pie | Self::Donut)
    }
}

/// One value of a series.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Point {
    /// The exact amount in minor units, negative for money that came back.
    pub minor_units: i64,
    /// The amount written for the user's locale, with its currency. This is
    /// what a chart shows and a screen reader speaks.
    pub label: String,
}

/// A line, a set of bars or a pie of points.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Series {
    /// The series' fixed name, which the UI translates: `income`,
    /// `expenses` or `spending`.
    pub key: &'static str,
    /// The name of a series the user chose (a category or a group of theirs),
    /// from the book, so the UI shows it as plain text. `None` for the series
    /// that have a fixed name, which the UI translates by `key`.
    pub name: Option<String>,
    /// The palette index, 0 to [`PALETTE_SIZE`] - 1. The first four also get
    /// a second mark in the UI, so color is never the only difference.
    pub color: u8,
    /// One point per category, in the order of [`ChartModel::categories`].
    pub points: Vec<Point>,
}

/// A mark on the value axis.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tick {
    /// Where the mark is, in minor units.
    pub minor_units: i64,
    /// The amount written for the user's locale.
    pub label: String,
}

/// What a chart or its table leaves out because the categories are hidden
/// (D-291). The words are the screen's, so the translated sentence is
/// written there (D-034): "Hidden: Rent and 1 more (1,250.00 € not shown)".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HiddenNote {
    /// The name of the largest hidden category, written like the names in
    /// [`ChartModel::categories`]. It comes from the book.
    pub first: String,
    /// How many other hidden categories there are.
    pub more: usize,
    /// What they add up to, written for the user's locale with the currency.
    pub total: String,
}

/// A chart: what to draw and what to say about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChartModel {
    /// The kind of chart.
    pub kind: ChartKind,
    /// The ID of the dataset it shows.
    pub dataset: &'static str,
    /// The currency of every amount.
    pub currency: String,
    /// The names along the horizontal axis, or the slices: categories or
    /// months, already written for the user's locale. They come from the
    /// book, so the UI shows them as plain text.
    pub categories: Vec<String>,
    /// The series, each with a point for every category.
    pub series: Vec<Series>,
    /// The place in [`categories`](Self::categories), and in the first
    /// column of the table, of money not sorted into a category yet. The
    /// text there is empty: the UI writes its translated name (D-034).
    pub unsorted: Option<usize>,
    /// Marks on the value axis, lowest first. Empty for a round chart.
    pub ticks: Vec<Tick>,
    /// The total of the dataset written for the user's locale, for the
    /// chart's description.
    pub total: String,
    /// The same values as a table: the screen-reader path and the "Show as
    /// table" control (D-283).
    pub table: TextTable,
    /// Hidden categories this chart leaves out, if any. The note belongs to
    /// the chart and to its table alike.
    pub hidden: Option<HiddenNote>,
}

impl ChartModel {
    /// The palette index of slice or series `index`.
    pub const fn color_of(index: usize) -> u8 {
        // The remainder is below 8, so it fits.
        (index % PALETTE_SIZE as usize) as u8
    }
}
