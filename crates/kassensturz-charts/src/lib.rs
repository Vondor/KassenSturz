// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The chart model of the `ChartProvider` boundary (M6a.3, D-276, D-283).
//!
//! A [`ChartModel`] is made from a dataset of `kassensturz-report` and holds
//! everything a chart needs, as text or as small integers: the label of every
//! point as text that Rust formatted for the user's locale, the exact amount
//! in minor units, tick labels, a palette index per series, and the same
//! values as a [`TextTable`](kassensturz_report::TextTable) for the screen
//! reader. The crate knows nothing about Qt or any chart library.
//!
//! Drawing values (`f64`) exist in one place only, [`ChartProvider::prepare`]:
//! they place marks and are never read back. The provider draws; it formats
//! no number, and it answers pointer events with indices, never values.

mod datasets;
mod error;
mod model;
mod provider;
mod ticks;

pub use datasets::{category_over_time, income_expense_by_month, spending_by_category};
pub use error::ChartError;
pub use model::{
    ChartKind, ChartModel, HiddenNote, MAX_SLICES, NAMED_FROM_TENTHS, PALETTE_SIZE, Point, Series,
    Slices, Tick,
};
pub use provider::{ChartProvider, Prepared, PreparedSeries, drawing_value};

#[cfg(test)]
mod tests;
