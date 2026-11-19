// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A plugin with the `report` extension point (#791 step 5, D-926): it
//! gets the transactions of the accounts and days the user chose and
//! answers with a table, which KassenSturz draws with its own components.
//! The plugin describes, KassenSturz draws (D-756): no markup, no charts,
//! and every text is shown as plain text.
//!
//! The figures are the plugin's, not KassenSturz's (AGENTS.md §8: numbers
//! come from code, and this is the plugin's code), so the page says which
//! plugin worked them out.

use std::sync::{Mutex, PoisonError};

use kassensturz_core::{CurrencyCode, Date};
use kassensturz_export::AccountTransactions;
use kassensturz_plugin_api::{ExtensionPoint, Permission};
use kassensturz_plugin_host::{HostError, Plugin};
use serde_json::{Value, json};

use crate::read::{self, Fields, Invalid};

/// The call that names the reports a plugin offers: empty input,
/// `{"reports": [{"id": "monthly", "name": "Monthly summary"}]}` out.
pub const REPORT_INFO: &str = "ks_report_info";
/// The call that works out one report: `{"report": id, "accounts": ...}`
/// in, the table out.
pub const REPORT_RUN: &str = "ks_report_run";

/// The most reports one plugin offers.
pub const MAX_REPORTS: usize = 20;
/// The most columns of a table.
pub const MAX_COLUMNS: usize = 20;
/// The most rows of a table.
pub const MAX_ROWS: usize = 10_000;
/// The most characters of a report's name, a title or a column's label.
pub const MAX_NAME_CHARS: usize = 80;
/// The most characters of a text cell.
pub const MAX_CELL_CHARS: usize = 500;
/// The most decimal places of an amount.
const MAX_MINOR_DIGITS: u64 = 4;

/// One report a plugin offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportInfo {
    /// Its ID within the plugin: 1 to 64 lowercase letters, digits, `-`
    /// and `_`.
    pub id: String,
    /// Its name, as the plugin gives it.
    pub name: String,
}

/// What a column holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    /// Text.
    Text,
    /// Calendar dates.
    Date,
    /// Amounts of money.
    Amount,
    /// Whole numbers, such as how many transactions.
    Count,
}

/// A column of a report's table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    /// Its heading, as the plugin gives it.
    pub label: String,
    /// What its cells hold.
    pub kind: ColumnKind,
}

/// A cell of a report's table. Its kind is its column's, or it is empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cell {
    /// Nothing.
    Empty,
    /// Text, shown as plain text.
    Text(String),
    /// A calendar date.
    Date(Date),
    /// An amount in minor units of a currency with that many decimal
    /// places.
    Amount {
        /// The amount in minor units.
        minor_units: i64,
        /// Its currency.
        currency: CurrencyCode,
        /// The decimal places the minor units count, 0 to 4.
        minor_digits: u8,
    },
    /// A whole number.
    Count(i64),
}

/// A report's table, read and checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportTable {
    /// The title the plugin gives it.
    pub title: String,
    /// Its columns, at least one.
    pub columns: Vec<Column>,
    /// Its rows, each with one cell per column.
    pub rows: Vec<Vec<Cell>>,
}

/// A plugin report. The plugin must ask for `read_book` (D-848): the user
/// sees on the Plugins tab that it reads the book.
pub struct PluginReport {
    plugin: Mutex<Plugin>,
    reports: Vec<ReportInfo>,
}

impl std::fmt::Debug for PluginReport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PluginReport")
            .field("reports", &self.reports)
            .finish_non_exhaustive()
    }
}

impl PluginReport {
    /// Makes a report of a loaded plugin and asks it which reports it
    /// offers.
    pub fn new(mut plugin: Plugin) -> Result<Self, HostError> {
        let manifest = plugin.manifest();
        if !manifest.extension_points.contains(&ExtensionPoint::Report) {
            return Err(HostError::ExportMissing {
                name: REPORT_INFO.to_owned(),
            });
        }
        if !manifest.permissions.contains(&Permission::ReadBook) {
            return Err(HostError::PermissionMissing {
                permission: Permission::ReadBook,
            });
        }
        let answer = plugin.call(REPORT_INFO, &[])?;
        let reports = reports(&answer).map_err(|Invalid| HostError::OutputInvalid)?;
        Ok(Self {
            plugin: Mutex::new(plugin),
            reports,
        })
    }

    /// The reports the plugin offers, in its order.
    pub fn reports(&self) -> &[ReportInfo] {
        &self.reports
    }

    /// Works out the report `id` from the transactions of `accounts`.
    pub fn run(
        &self,
        id: &str,
        accounts: &[AccountTransactions],
    ) -> Result<ReportTable, HostError> {
        if !self.reports.iter().any(|report| report.id == id) {
            return Err(HostError::ExportMissing {
                name: id.to_owned(),
            });
        }
        let input = input(id, accounts);
        let answer = self
            .plugin
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .call(REPORT_RUN, input.to_string().as_bytes())?;
        table(&answer).map_err(|Invalid| HostError::OutputInvalid)
    }
}

/// What the plugin gets: the exporter's transactions and the report's ID.
pub(crate) fn input(id: &str, accounts: &[AccountTransactions]) -> Value {
    let mut input = crate::exporter::input(accounts);
    input["report"] = json!(id);
    input
}

fn reports(answer: &[u8]) -> Result<Vec<ReportInfo>, Invalid> {
    let object = read::object(answer)?;
    let fields = Fields::of_map(&object, &["reports"])?;
    let items = fields.array("reports")?;
    if items.is_empty() || items.len() > MAX_REPORTS {
        return Err(Invalid);
    }
    let mut reports: Vec<ReportInfo> = Vec::with_capacity(items.len());
    for item in items {
        let fields = Fields::of(item, &["id", "name"])?;
        let id = fields.text("id")?;
        let fine = (1..=64).contains(&id.len())
            && id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
        if !fine || reports.iter().any(|report| report.id == id) {
            return Err(Invalid);
        }
        reports.push(ReportInfo {
            id: id.to_owned(),
            name: shown_text(fields.text("name")?, MAX_NAME_CHARS)?,
        });
    }
    Ok(reports)
}

fn table(answer: &[u8]) -> Result<ReportTable, Invalid> {
    let object = read::object(answer)?;
    let fields = Fields::of_map(&object, &["title", "columns", "rows"])?;
    let title = shown_text(fields.text("title")?, MAX_NAME_CHARS)?;
    let columns = fields.array("columns")?;
    if columns.is_empty() || columns.len() > MAX_COLUMNS {
        return Err(Invalid);
    }
    let columns = columns
        .iter()
        .map(|column| {
            let fields = Fields::of(column, &["label", "kind"])?;
            let kind = match fields.text("kind")? {
                "text" => ColumnKind::Text,
                "date" => ColumnKind::Date,
                "amount" => ColumnKind::Amount,
                "count" => ColumnKind::Count,
                _ => return Err(Invalid),
            };
            Ok(Column {
                label: shown_text(fields.text("label")?, MAX_NAME_CHARS)?,
                kind,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let rows = fields.array("rows")?;
    if rows.len() > MAX_ROWS {
        return Err(Invalid);
    }
    let rows = rows
        .iter()
        .map(|row| {
            let cells = row.as_array().ok_or(Invalid)?;
            if cells.len() != columns.len() {
                return Err(Invalid);
            }
            cells
                .iter()
                .zip(&columns)
                .map(|(cell, column)| self::cell(cell, column.kind))
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ReportTable {
        title,
        columns,
        rows,
    })
}

fn cell(value: &Value, kind: ColumnKind) -> Result<Cell, Invalid> {
    if value.is_null() {
        return Ok(Cell::Empty);
    }
    match kind {
        ColumnKind::Text => Ok(Cell::Text(shown_text(
            value.as_str().ok_or(Invalid)?,
            MAX_CELL_CHARS,
        )?)),
        ColumnKind::Date => Ok(Cell::Date(read::date(value.as_str().ok_or(Invalid)?)?)),
        ColumnKind::Count => Ok(Cell::Count(read::minor_units(
            value.as_str().ok_or(Invalid)?,
        )?)),
        ColumnKind::Amount => {
            let fields = Fields::of(value, &["amount", "currency", "minor_digits"])?;
            let minor_digits = fields
                .required("minor_digits")?
                .as_u64()
                .filter(|digits| *digits <= MAX_MINOR_DIGITS)
                .ok_or(Invalid)?;
            Ok(Cell::Amount {
                minor_units: read::minor_units(fields.text("amount")?)?,
                currency: read::currency_code(fields.text("currency")?)?,
                minor_digits: u8::try_from(minor_digits).map_err(|_| Invalid)?,
            })
        }
    }
}

/// A text KassenSturz shows: at most `max` characters, no control
/// characters or characters that reorder text (AGENTS.md §8). A name,
/// title or label also needs a visible character; a cell may be empty.
fn shown_text(text: &str, max: usize) -> Result<String, Invalid> {
    if max != MAX_CELL_CHARS && text.trim().is_empty() {
        return Err(Invalid);
    }
    hidden_free(text, max)
}

/// `text`, if it has at most `max` characters and none that are hidden.
pub(crate) fn hidden_free(text: &str, max: usize) -> Result<String, Invalid> {
    let hidden = |c: char| {
        c.is_control()
            || matches!(
                c,
                '\u{00AD}'
                    | '\u{061C}'
                    | '\u{180E}'
                    | '\u{200B}'..='\u{200F}'
                    | '\u{202A}'..='\u{202E}'
                    | '\u{2060}'..='\u{2064}'
                    | '\u{2066}'..='\u{2069}'
                    | '\u{FEFF}'
            )
    };
    if text.chars().count() > max || text.chars().any(hidden) {
        return Err(Invalid);
    }
    Ok(text.to_owned())
}
