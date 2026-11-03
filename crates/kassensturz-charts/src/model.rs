// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The types of a chart (D-276). Every amount is kept exact, in minor units,
//! with its label already written as text.

use kassensturz_report::TextTable;
use kassensturz_report::spec::Rgb;

/// Where a chart's free colors start in the color index space (M7a.10,
/// D-588). A series or slice has the index of its place in the chart, from 0
/// up; the app's palette (2 to 16 colors, set by the user) wraps it, so a
/// chart does not need to know how many colors there are. An index from here
/// on is not a place but one of [`ChartModel::extra_colors`].
pub const EXTRA_COLOR_BASE: u8 = 128;

/// The mark a custom chart writes for the answer "yes" of a yes/no field,
/// wherever a name of the chart holds one: a category, a series, a slice, a
/// table cell, a tooltip (D-525). Rust cannot translate (D-034), so the
/// screen's word for it goes in its place with [`ChartModel::write_answers`].
pub const ANSWER_YES: char = '\u{E000}';

/// The mark for the answer "no" (see [`ANSWER_YES`]).
pub const ANSWER_NO: char = '\u{E001}';

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
    /// Areas of the series on top of each other (a custom chart, M6e.2).
    StackedArea,
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
            Self::StackedArea => "stacked-area",
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
            Self::StackedArea,
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
    /// The color index: the series' place in the chart (the palette wraps
    /// it), or [`EXTRA_COLOR_BASE`] and up for a free color. The first four
    /// also get a second mark in the UI, so color is never the only
    /// difference.
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

/// How many slices a pie, donut or ring has at most (D-291): the seven
/// largest and "Other", so every slice has its own color of the palette.
pub const MAX_SLICES: usize = 8;

/// A slice of at least this share of what is shown has its name on the
/// chart, in tenths of a percent (5 %).
pub const NAMED_FROM_TENTHS: i64 = 50;

/// What a round chart says about its slices (M6b.5b, D-307). The slices are
/// the chart's categories: the largest ones, then "Other" if there were more
/// than [`MAX_SLICES`], then the rows that cannot be drawn.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Slices {
    /// The place of the slice "Other" in [`ChartModel::categories`]; its
    /// text there is empty and the screen writes the translated name.
    pub other: Option<usize>,
    /// What "Other" holds, largest first: each category's name (empty for
    /// money not sorted yet) and its amount written for the user's locale.
    pub holds: Vec<(String, String)>,
    /// Each slice's share of what is drawn, written for the user's locale;
    /// empty for a row that cannot be drawn.
    pub shares: Vec<String>,
    /// Whether the slice is big enough (5 %) to carry its name on the chart.
    pub named: Vec<bool>,
    /// Whether the row cannot be drawn: money that came back (a negative
    /// total) or nothing. It is listed in the legend and the table with a
    /// note, never as a slice.
    pub undrawn: Vec<bool>,
}

/// What a layer of a custom chart draws on top of the chart's own marks
/// (M6e.2, D-503). The first line, bar or area layer of a description is the
/// chart itself; every other layer is an overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OverlayMark {
    /// A line through the items of a series.
    Line,
    /// The area below such a line.
    Area,
    /// A bar from zero to each item.
    Bar,
    /// A dot at each item.
    Point,
    /// A text at each item.
    Text,
    /// A level line across the chart at each item's amount.
    LevelRule,
    /// A line from bottom to top at each item's category.
    CategoryRule,
}

impl OverlayMark {
    /// The mark's fixed name, as QML spells it.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::Area => "area",
            Self::Bar => "bar",
            Self::Point => "point",
            Self::Text => "text",
            Self::LevelRule => "rule-level",
            Self::CategoryRule => "rule-category",
        }
    }
}

/// One mark of an overlay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OverlayItem {
    /// The place in [`ChartModel::categories`], or `None` for a mark that
    /// runs across the chart.
    pub category: Option<usize>,
    /// The exact amount (or count, or tenths of a percent) on the value
    /// axis, or `None` for a mark that runs from bottom to top.
    pub minor_units: Option<i64>,
    /// The value written for the user's locale; what a screen reader says.
    pub label: String,
    /// The text of a text mark, from the description's data; empty for the
    /// other marks.
    pub text: String,
    /// How big a dot is, 1 (smallest) to [`MAX_DOT_SIZE`].
    pub size: u8,
}

/// The biggest size of a dot.
pub const MAX_DOT_SIZE: u8 = 4;

/// The items of one series of an overlay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OverlaySeries {
    /// The series' name, from the description (a value of its `color`
    /// field or a title): the user's own text, shown as plain text.
    pub name: String,
    /// The palette index, as for [`Series::color`].
    pub color: u8,
    /// The marks, in the order of the categories.
    pub items: Vec<OverlayItem>,
}

/// A layer drawn on top of the chart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Overlay {
    /// What it draws.
    pub mark: OverlayMark,
    /// Its series.
    pub series: Vec<OverlaySeries>,
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
    /// The slices of a round chart; empty for the other kinds.
    pub slices: Slices,
    /// Hidden categories this chart leaves out, if any. The note belongs to
    /// the chart and to its table alike.
    pub hidden: Option<HiddenNote>,
    /// Layers drawn on top of the chart on the same axes; empty for every
    /// built-in chart. Only a custom chart has them (M6e.2).
    pub overlays: Vec<Overlay>,
    /// Free colors that a custom chart names as `#rrggbb` (M7a.10, Q-410).
    /// A color index from [`EXTRA_COLOR_BASE`] on is one of these: that index
    /// is the first, the next the second. Empty for every built-in chart.
    pub extra_colors: Vec<Rgb>,
    /// A round chart's color index for each slice, in the order of
    /// [`categories`](Self::categories). Empty when the slices take the
    /// palette's colors by place, as the built-in round charts do.
    pub slice_colors: Vec<u8>,
}

impl ChartModel {
    /// Writes `yes` and `no`, the screen's words in the user's language, in
    /// place of the marks [`ANSWER_YES`] and [`ANSWER_NO`] in every text of
    /// the chart that can hold a name: the categories, the series, the table,
    /// the slices, the hidden note and the layers (D-525). A chart without
    /// marks is unchanged, and so is every number.
    pub fn write_answers(&mut self, yes: &str, no: &str) {
        let write = |text: &mut String| {
            if text.contains([ANSWER_YES, ANSWER_NO]) {
                *text = text.replace(ANSWER_YES, yes).replace(ANSWER_NO, no);
            }
        };
        self.categories.iter_mut().for_each(write);
        for series in &mut self.series {
            series.name.iter_mut().for_each(write);
            series.points.iter_mut().for_each(|p| write(&mut p.label));
        }
        self.table.rows.iter_mut().flatten().for_each(write);
        for (name, label) in &mut self.slices.holds {
            write(name);
            write(label);
        }
        if let Some(hidden) = &mut self.hidden {
            write(&mut hidden.first);
        }
        for series in self.overlays.iter_mut().flat_map(|o| &mut o.series) {
            write(&mut series.name);
            for item in &mut series.items {
                write(&mut item.label);
                write(&mut item.text);
            }
        }
    }

    /// The color index of slice `index` of a round chart.
    #[must_use]
    pub fn slice_color(&self, index: usize) -> u8 {
        self.slice_colors
            .get(index)
            .copied()
            .unwrap_or_else(|| Self::color_of(index))
    }

    /// The color index of slice or series `index`: its place in the chart,
    /// which the app's palette wraps to its own number of colors. A place
    /// from [`EXTRA_COLOR_BASE`] on starts again at 0 (no chart has so
    /// many).
    pub const fn color_of(index: usize) -> u8 {
        // The remainder is below 128, so it fits.
        (index % EXTRA_COLOR_BASE as usize) as u8
    }
}
