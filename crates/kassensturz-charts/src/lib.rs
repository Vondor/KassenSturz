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

mod compared;
mod custom;
mod datasets;
mod error;
mod financial;
mod flow;
mod model;
mod palette;
mod provider;
mod ticks;

pub use compared::{compare_key, spending_by_category_compared};
pub use custom::{CustomChart, CustomError, MAX_ROWS, MAX_SERIES, custom_chart};
pub use datasets::{
    category_over_time, income_expense_by_month, spending_by_category, spending_by_category_as,
};
pub use error::ChartError;
pub use financial::{
    BalancesBy, account_balances, balance_candles, cash_flow_by_month, net_worth_over_time,
};
pub use flow::{FlowModel, LaidBand, LaidNode, flow_model};
pub use model::{
    ANSWER_NO, ANSWER_YES, ChangeMark, ChartKind, ChartModel, ComparedTable, EXTRA_COLOR_BASE,
    HiddenNote, MAX_DOT_SIZE, MAX_SLICES, NAMED_FROM_TENTHS, Overlay, OverlayItem, OverlayMark,
    OverlaySeries, Point, Series, Slices, Tick,
};
pub use palette::{
    DEFAULT_COLORS, MAX_COLORS, MIN_COLORS, Palette, PaletteWarning, STANDARD_COLORS,
    contrast_is_enough, too_alike,
};
pub use provider::{ChartProvider, Prepared, PreparedSeries, drawing_value};

#[cfg(test)]
mod custom_tests;
#[cfg(test)]
mod palette_tests;
#[cfg(test)]
mod tests;
