// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `export` command: the transactions of accounts into a new file
//! (D-029, D-099).

use std::io::Write;
use std::path::Path;

use kassensturz_export::{
    CsvExport, CsvExportSettings, DecimalMark, ExportData, Exporter, Separator, export_file,
};
use kassensturz_pipeline::export_transactions;
use kassensturz_store::Book;

use crate::args::Arguments;
use crate::error::CliError;
use crate::lookup::find_account;
use crate::text::plain;

/// The ID of the only format so far; the complete export (M2.8) joins it.
const CSV: &str = "csv";

/// `export FILE [--account NAME ...]`.
pub(crate) fn export(book: &Book, args: &[String], out: &mut dyn Write) -> Result<(), CliError> {
    let args = Arguments::parse(
        args,
        &["format", "account", "separator", "decimal-mark"],
        &["no-header", "byte-order-mark"],
    )?;
    let [file] = args.exactly(["file"])?;
    if let Some(format) = args.value("format")?
        && format != CSV
    {
        return Err(CliError::usage(format!(
            "`{}` isn't a format to export. The format yet is csv",
            plain(format)
        )));
    }
    let settings = CsvExportSettings {
        separator: match args.value("separator")? {
            None | Some("comma") => Separator::Comma,
            Some("semicolon") => Separator::Semicolon,
            Some("tab") => Separator::Tab,
            Some(other) => {
                return Err(CliError::usage(format!(
                    "`{}` isn't a separator: use comma, semicolon or tab",
                    plain(other)
                )));
            }
        },
        decimal_mark: match args.value("decimal-mark")? {
            None | Some("point") => DecimalMark::Point,
            Some("comma") => DecimalMark::Comma,
            Some(other) => {
                return Err(CliError::usage(format!(
                    "`{}` isn't a decimal mark: use point or comma",
                    plain(other)
                )));
            }
        },
        header: !args.flag("no-header"),
        byte_order_mark: args.flag("byte-order-mark"),
    };
    let exporter = CsvExport::new(settings)?;
    let names = args.all("account");
    let accounts = if names.is_empty() {
        book.accounts(true)?
    } else {
        names
            .iter()
            .map(|name| find_account(book, name))
            .collect::<Result<_, _>>()?
    };
    let ids: Vec<_> = accounts.iter().map(|account| account.id).collect();
    let data = export_transactions(book, &ids)?;
    let ExportData::Transactions(exported) = &data else {
        unreachable!("export_transactions gathers transactions")
    };
    let rows: usize = exported.iter().map(|account| account.rows.len()).sum();
    export_file(&exporter, &data, Path::new(file))?;
    writeln!(
        out,
        "Exported {rows} transactions of {} accounts to {} as {}.",
        exported.len(),
        plain(file),
        exporter.info().id
    )?;
    Ok(())
}
