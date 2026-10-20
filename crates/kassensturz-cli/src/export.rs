// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `export` command: the transactions of accounts, or the whole book,
//! into a new file (D-029, D-099, D-100).

use std::io::Write;
use std::path::Path;

use kassensturz_export::{
    BookExport, CsvExport, CsvExportSettings, DecimalMark, ExportData, Exporter, Separator,
    export_file,
};
use kassensturz_pipeline::{export_book, export_transactions};
use kassensturz_store::Book;

use crate::args::Arguments;
use crate::error::CliError;
use crate::lookup::find_account;
use crate::text::plain;

/// The IDs of the formats: transactions as CSV, the default, and the
/// complete export of the whole book.
const CSV: &str = "csv";
const BOOK: &str = "book";

/// The options only the CSV format takes.
const CSV_OPTIONS: [&str; 3] = ["account", "separator", "decimal-mark"];
const CSV_FLAGS: [&str; 2] = ["no-header", "byte-order-mark"];

/// `export FILE [--format csv|book] [--account NAME ...] ...`.
pub(crate) fn export(book: &Book, args: &[String], out: &mut dyn Write) -> Result<(), CliError> {
    let mut options = vec!["format"];
    options.extend(CSV_OPTIONS);
    let args = Arguments::parse(args, &options, &CSV_FLAGS)?;
    let [file] = args.exactly(["file"])?;
    match args.value("format")? {
        None | Some(CSV) => export_csv(book, &args, file, out),
        Some(BOOK) => export_whole_book(book, &args, file, out),
        Some(other) => Err(CliError::usage(format!(
            "`{}` isn't a format to export: use csv or book",
            plain(other)
        ))),
    }
}

/// The complete export: everything in the book, in the open format of
/// `docs/EXPORT_FORMAT.md`, which `create --from` reads back.
fn export_whole_book(
    book: &Book,
    args: &Arguments,
    file: &str,
    out: &mut dyn Write,
) -> Result<(), CliError> {
    // The whole book goes into the file, so nothing chooses parts of it.
    for option in CSV_OPTIONS {
        if !args.all(option).is_empty() {
            return Err(CliError::usage(format!(
                "--{option} is only for the csv format"
            )));
        }
    }
    if let Some(flag) = CSV_FLAGS.into_iter().find(|flag| args.flag(flag)) {
        return Err(CliError::usage(format!(
            "--{flag} is only for the csv format"
        )));
    }
    let data = export_book(book)?;
    let ExportData::Book(exported) = &data else {
        unreachable!("export_book gathers the book")
    };
    export_file(&BookExport, &data, Path::new(file))?;
    writeln!(
        out,
        "Exported the book to {} as {BOOK}: {} accounts and categories, {} transactions.",
        plain(file),
        exported.accounts.len(),
        exported.transactions.len()
    )?;
    Ok(())
}

/// Transactions of accounts, one row each, as CSV.
fn export_csv(
    book: &Book,
    args: &Arguments,
    file: &str,
    out: &mut dyn Write,
) -> Result<(), CliError> {
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
