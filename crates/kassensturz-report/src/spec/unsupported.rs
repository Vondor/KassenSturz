// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The "not supported here" list (D-507, D-508): every part of a chart
//! description that KassenSturz does not read, with where it is. It is not an
//! error; nothing in it is dropped silently.

/// Data from somewhere other than the book: `data.url`, `data.values`,
/// `data.sequence`, `data.format`, top-level `datasets`.
pub const DATA_SOURCE: &str = "data-source";
/// An expression: `filter` as a string, `calculate`, `{"expr": …}`, a
/// `condition`.
pub const EXPRESSION: &str = "expression";
/// Another transform (`lookup`, `window`, `aggregate`, `bin`, …).
pub const TRANSFORM: &str = "transform";
/// Another mark (`boxplot`, `geoshape`, `image`, `trail`, …).
pub const MARK: &str = "mark";
/// `facet`, `repeat`, `concat`, `hconcat`, `vconcat`, `params`,
/// `selection`, a layer inside a layer.
pub const COMPOSITION: &str = "composition";
/// Another channel, or a channel the mark does not take.
pub const CHANNEL: &str = "channel";
/// A quantitative `x` with a nominal, ordinal or temporal `y`.
pub const HORIZONTAL: &str = "horizontal";
/// A key the subset does not have inside an allowed part (`scale`, `axis`,
/// `legend`, `stack`, `bin`, `datum`, other `config`, `view`, `padding`, …).
pub const OPTION: &str = "option";
/// An allowed key with a value outside the subset (`aggregate: median`,
/// `timeUnit: quarter`, a date-time object, …).
pub const VALUE: &str = "value";

/// Every reason word, in the order of the design's table.
pub const REASONS: [&str; 9] = [
    DATA_SOURCE,
    EXPRESSION,
    TRANSFORM,
    MARK,
    COMPOSITION,
    CHANNEL,
    HORIZONTAL,
    OPTION,
    VALUE,
];

/// One part of a description that is not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported {
    path: String,
    reason: &'static str,
}

impl Unsupported {
    pub(super) fn new(path: String, reason: &'static str) -> Self {
        debug_assert!(REASONS.contains(&reason), "{reason}");
        Self { path, reason }
    }

    /// Where it is, as JSON-path text such as `transform[1].calculate`. It
    /// is data from the user's text, shown as plain text and never part of a
    /// message (D-034).
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Why, as one of [`REASONS`].
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        self.reason
    }
}
