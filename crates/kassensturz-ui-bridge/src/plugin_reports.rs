// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reports from plugins (#791 step 5, D-926): which reports the added
//! plugins offer, working one out from the accounts and days the user
//! chose, and the table as QML gets it. Every amount and date is written
//! here for the user's locale (D-065); QML only lays the texts out.

use std::path::Path;

use cxx_qt_lib::QVariant;
use kassensturz_core::{AccountId, Date, Money, Report};
use kassensturz_export::ExportData;
use kassensturz_pipeline::{DateRange, export_transactions_in};
use kassensturz_plugin_api::ExtensionPoint;
use kassensturz_plugin_formats::{Cell, ColumnKind, PluginReport, ReportTable};
use kassensturz_store::Book;

use crate::errors::{ErrorText, report_text};
use crate::format::Locale;
use crate::plugins;
use crate::variant::{list, map, text};

/// A report plugin the user added.
pub struct ReportPlugin {
    /// The plugin's file name in the plugin folder.
    pub file: String,
    /// The plugin's name, shown as plain text.
    pub name: String,
    /// The loaded plugin, with the reports it offers.
    pub report: PluginReport,
}

/// The report plugins in the plugin folder, sorted by name.
pub fn report_plugins(ai_enabled: bool) -> Vec<ReportPlugin> {
    plugins::folder()
        .map(|folder| report_plugins_in(&folder, ai_enabled))
        .unwrap_or_default()
}

/// The report plugins in `folder`. One that doesn't ask to read the book
/// or doesn't name its reports well is left out; the Plugins page shows
/// it as it is.
pub fn report_plugins_in(folder: &Path, ai_enabled: bool) -> Vec<ReportPlugin> {
    plugins::loaded(folder, ExtensionPoint::Report, ai_enabled)
        .into_iter()
        .filter_map(|(file, plugin)| {
            let name = plugin.manifest().name.clone();
            PluginReport::new(plugin)
                .ok()
                .map(|report| ReportPlugin { file, name, report })
        })
        .collect()
}

/// Every report of `plugins`, for the "From plugins" list: each
/// `{ file, plugin, id, name }`.
pub fn list_object(plugins: &[ReportPlugin]) -> QVariant {
    list(plugins.iter().flat_map(|plugin| {
        plugin.report.reports().iter().map(|report| {
            map([
                ("file", text(&plugin.file)),
                ("plugin", text(&plugin.name)),
                ("id", text(&report.id)),
                ("name", text(&report.name)),
            ])
        })
    }))
}

/// Works out the report `id` of `plugin` from the transactions of the
/// accounts with `ids` (all accounts when empty) in `range`.
pub fn run(
    book: &Book,
    plugin: &ReportPlugin,
    id: &str,
    ids: &[String],
    range: DateRange,
    locale: &impl Locale,
) -> Result<ReportTable, ErrorText> {
    let store = |error: kassensturz_store::StoreError| report_text(book, &error.report(), locale);
    let known = book.accounts(true).map_err(store)?;
    let accounts: Vec<AccountId> = if ids.is_empty() {
        known.iter().map(|account| account.id).collect()
    } else {
        ids.iter()
            .map(|id| {
                known
                    .iter()
                    .find(|account| account.id.to_string() == *id)
                    .map(|account| account.id)
                    .ok_or_else(|| ErrorText::plain("input.plugin-gone"))
            })
            .collect::<Result<_, _>>()?
    };
    let data = export_transactions_in(book, &accounts, range).map_err(store)?;
    let ExportData::Transactions(accounts) = &data else {
        unreachable!("export_transactions_in gathers transactions")
    };
    plugin
        .report
        .run(id, accounts)
        .map_err(|error| report_text(book, &error.report(), locale))
}

/// The days of a period word of the report page, up to `today`: `month`
/// (this month), `3-months` and `12-months` (whole months back, this one
/// included), `year` (this calendar year) or `all`. Any other word is
/// `None`.
pub fn period_range(word: &str, today: Date) -> Option<DateRange> {
    let months_back = |back: i32| {
        let index = i32::from(today.year()) * 12 + i32::from(today.month()) - 1 - back;
        let year = i16::try_from(index.div_euclid(12)).ok()?;
        let month = i8::try_from(index.rem_euclid(12) + 1).ok()?;
        Date::new(year, month, 1).ok()
    };
    let from = match word {
        "month" => months_back(0)?,
        "3-months" => months_back(2)?,
        "12-months" => months_back(11)?,
        "year" => Date::new(today.year(), 1, 1).ok()?,
        "all" => {
            return Some(DateRange {
                from: None,
                until: Some(today),
            });
        }
        _ => return None,
    };
    Some(DateRange {
        from: Some(from),
        until: Some(today),
    })
}

/// The table as QML gets it: `{ title, columns, rows }`, each column
/// `{ label, right }` (`right` for figures, which line up on the right),
/// each row a list of texts, written for `locale`. An amount below zero
/// also says so (`negative` per row: the indices of its negative cells),
/// so QML can colour it without reading the number.
pub fn table_object(table: &ReportTable, locale: &impl Locale) -> QVariant {
    let columns = list(table.columns.iter().map(|column| {
        map([
            ("label", text(&column.label)),
            (
                "right",
                QVariant::from(&matches!(
                    column.kind,
                    ColumnKind::Amount | ColumnKind::Count
                )),
            ),
        ])
    }));
    let rows = list(table.rows.iter().map(|row| {
        let negative: Vec<QVariant> = row
            .iter()
            .enumerate()
            .filter(|(_, cell)| {
                matches!(cell, Cell::Amount { minor_units, .. } | Cell::Count(minor_units) if *minor_units < 0)
            })
            .map(|(index, _)| QVariant::from(&i32::try_from(index).unwrap_or(i32::MAX)))
            .collect();
        map([
            ("cells", list(row.iter().map(|cell| text(&cell_text(cell, locale))))),
            ("negative", list(negative)),
        ])
    }));
    map([
        ("title", text(&table.title)),
        ("columns", columns),
        ("rows", rows),
    ])
}

/// A cell written for `locale`.
pub fn cell_text(cell: &Cell, locale: &impl Locale) -> String {
    match cell {
        Cell::Empty => String::new(),
        Cell::Text(text) => text.clone(),
        Cell::Date(date) => locale.short_date(*date),
        Cell::Count(count) => {
            let digits = locale.group_digits(count.unsigned_abs());
            if *count < 0 {
                format!("−{digits}")
            } else {
                digits
            }
        }
        Cell::Amount {
            minor_units,
            currency,
            minor_digits,
        } => locale.amount_format(*currency).format(
            Money::from_minor_units(*minor_units, *currency),
            *minor_digits,
            |value| locale.group_digits(value),
        ),
    }
}

#[cfg(test)]
mod tests {
    use kassensturz_core::CurrencyCode;
    use kassensturz_plugin_formats::Column;

    use super::*;
    use crate::register::tests::TestLocale;
    use crate::variant::read_back::{flag_of, list_of, text_of};

    fn table() -> ReportTable {
        let eur = CurrencyCode::new("EUR").unwrap();
        ReportTable {
            title: "Monthly summary".into(),
            columns: vec![
                Column {
                    label: "Month".into(),
                    kind: ColumnKind::Text,
                },
                Column {
                    label: "Left over".into(),
                    kind: ColumnKind::Amount,
                },
                Column {
                    label: "Payments".into(),
                    kind: ColumnKind::Count,
                },
                Column {
                    label: "Day".into(),
                    kind: ColumnKind::Date,
                },
            ],
            rows: vec![
                vec![
                    Cell::Text("<b>November</b>".into()),
                    Cell::Amount {
                        minor_units: -123_456,
                        currency: eur,
                        minor_digits: 2,
                    },
                    Cell::Count(1200),
                    Cell::Date("2026-11-30".parse().unwrap()),
                ],
                vec![Cell::Empty, Cell::Empty, Cell::Count(-3), Cell::Empty],
            ],
        }
    }

    #[test]
    fn the_table_reads_back_the_way_qml_reads_it() {
        let object = table_object(&table(), &TestLocale);
        assert_eq!(text_of(&object, "title"), "Monthly summary");
        let columns = list_of(&object, "columns");
        assert_eq!(columns.len(), 4);
        assert_eq!(text_of(&columns[1], "label"), "Left over");
        assert!(!flag_of(&columns[0], "right"));
        assert!(flag_of(&columns[1], "right"));
        assert!(flag_of(&columns[2], "right"));
        assert!(!flag_of(&columns[3], "right"));
        let rows = list_of(&object, "rows");
        assert_eq!(rows.len(), 2);
        let cells: Vec<String> = list_of(&rows[0], "cells")
            .iter()
            .map(|cell| cell.value::<cxx_qt_lib::QString>().unwrap().to_string())
            .collect();
        let expected = [
            "<b>November</b>".to_owned(),
            cell_text(&table().rows[0][1], &TestLocale),
            cell_text(&Cell::Count(1200), &TestLocale),
            TestLocale.short_date("2026-11-30".parse().unwrap()),
        ];
        assert_eq!(cells, expected);
        assert!(cells[1].contains("1") && cells[1].contains("234") && cells[1].contains("56"));
        let negative: Vec<i32> = list_of(&rows[0], "negative")
            .iter()
            .map(|index| index.value::<i32>().unwrap())
            .collect();
        assert_eq!(negative, [1]);
        let negative: Vec<i32> = list_of(&rows[1], "negative")
            .iter()
            .map(|index| index.value::<i32>().unwrap())
            .collect();
        assert_eq!(negative, [2]);
    }

    #[test]
    fn periods_count_whole_months_back_to_today() {
        let day = |text: &str| -> Date { text.parse().unwrap() };
        let range = |word: &str, today: &str| {
            period_range(word, day(today)).map(|range| (range.from, range.until))
        };
        let today = "2026-02-15";
        assert_eq!(
            range("month", today),
            Some((Some(day("2026-02-01")), Some(day(today))))
        );
        assert_eq!(
            range("3-months", today),
            Some((Some(day("2025-12-01")), Some(day(today))))
        );
        assert_eq!(
            range("12-months", today),
            Some((Some(day("2025-03-01")), Some(day(today))))
        );
        assert_eq!(
            range("year", today),
            Some((Some(day("2026-01-01")), Some(day(today))))
        );
        assert_eq!(range("all", today), Some((None, Some(day(today)))));
        assert_eq!(range("week", today), None);
    }

    #[test]
    fn a_count_below_zero_has_a_minus() {
        assert!(cell_text(&Cell::Count(-3), &TestLocale).starts_with('−'));
        assert_eq!(cell_text(&Cell::Empty, &TestLocale), "");
    }
}
