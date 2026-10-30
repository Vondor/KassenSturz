// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The built-in default dashboard (M6b.2, D-289, D-313, Q-215 = A): today's
//! Overview as a document on the grid. It is not stored in the book. The
//! Overview is made from it until the user changes the layout, and then the
//! changed copy is saved as a dashboard (`Book::add_view`, later
//! `Book::edit_view`) and the built-in one stays here to go back to.

use crate::{Document, ReportError};

/// The name of the dashboard the Overview shows.
pub const OVERVIEW_NAME: &str = "Overview";

/// The document, as text. The warnings about accounts not updated from the
/// bank, what the accounts are worth, four key figures in a row, the spending donut and
/// the table of what it shows side by side, then the line of the last 12
/// months, where the money came from and the note about transfers. A component without a `props.period` takes the page's period (the
/// month the Overview shows); the line asks for its own.
pub const DEFAULT_OVERVIEW: &str = r#"{
  "format": "kassensturz-dashboard", "version": 1, "kind": "dashboard",
  "name": "Overview",
  "components": [
    {"id": "fetch-gaps", "type": "notice.fetch_gap",
     "frame": {"col": 0, "row": 0, "cols": 12, "rows": 1}},
    {"id": "worth", "type": "card.net_worth",
     "frame": {"col": 0, "row": 1, "cols": 12, "rows": 2}},
    {"id": "income", "type": "kpi.card", "dataset": "income_expense_by_month",
     "params": {"measure": "income", "compare": "previous_period"},
     "frame": {"col": 0, "row": 3, "cols": 3, "rows": 2}},
    {"id": "expenses", "type": "kpi.card", "dataset": "income_expense_by_month",
     "params": {"measure": "expenses", "compare": "previous_period"},
     "frame": {"col": 3, "row": 3, "cols": 3, "rows": 2}},
    {"id": "left", "type": "kpi.card", "dataset": "income_expense_by_month",
     "params": {"measure": "net", "compare": "previous_period"},
     "frame": {"col": 6, "row": 3, "cols": 3, "rows": 2}},
    {"id": "kept", "type": "kpi.card", "dataset": "income_expense_by_month",
     "params": {"measure": "savings-rate", "compare": "previous_period"},
     "frame": {"col": 9, "row": 3, "cols": 3, "rows": 2}},
    {"id": "spending", "type": "chart.donut", "dataset": "spending_by_category",
     "params": {"grouping": "top_level_category"},
     "frame": {"col": 0, "row": 5, "cols": 6, "rows": 5}},
    {"id": "biggest", "type": "table.rows", "dataset": "spending_by_category",
     "params": {"grouping": "top_level_category"},
     "frame": {"col": 6, "row": 5, "cols": 6, "rows": 5}},
    {"id": "trend", "type": "chart.line", "dataset": "income_expense_by_month",
     "props": {"period": "last-12-months"},
     "frame": {"col": 0, "row": 10, "cols": 12, "rows": 4}},
    {"id": "sources", "type": "table.income_sources",
     "frame": {"col": 0, "row": 14, "cols": 12, "rows": 4}},
    {"id": "transfers", "type": "notice.transfers",
     "frame": {"col": 0, "row": 18, "cols": 12, "rows": 1}}
  ]
}"#;

impl Document {
    /// The built-in default dashboard: [`DEFAULT_OVERVIEW`], read and checked
    /// like every other document.
    pub fn default_overview() -> Result<Self, ReportError> {
        Self::read(DEFAULT_OVERVIEW)
    }
}
