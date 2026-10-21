// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `create` command: a new, empty book, or one made from a complete
//! export (D-100) or from another program's book, such as KMyMoney's
//! (M2.9, M2.10).

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use kassensturz_export::read_book_file;
use kassensturz_import::{
    Imported, Importer, Kmy, KmySqlite, Limits, RECOGNITION_BYTES, Recognition, import,
};
use kassensturz_pipeline::{BookImportReport, fill_book, fill_book_from};
use kassensturz_store::{Book, Computer};

use crate::error::CliError;
use crate::text::plain;

/// The ID of the complete export's format, as `export --format` names it.
const BOOK: &str = "book";

/// What a new book was made from.
pub(crate) enum Created {
    /// Nothing: it is empty.
    Empty,
    /// A complete export.
    Export,
    /// Another program's book, with what filling the new book did.
    Imported(BookImportReport),
}

/// Creates the book at `path`, filled from `from` if given: a complete
/// export, or another program's book in the format `format` names, or the
/// one `from` is recognized as. The file is read first, so a file that
/// isn't one leaves nothing behind; if the book refuses what it holds, the
/// new book is removed again.
pub(crate) fn create(
    path: &Path,
    computer: &Computer,
    from: Option<&Path>,
    format: Option<&str>,
) -> Result<(Book, Created), CliError> {
    let Some(from) = from else {
        if format.is_some() {
            return Err(CliError::usage("--format needs --from FILE"));
        }
        return Ok((Book::create(path, computer)?, Created::Empty));
    };
    let importer: Option<&dyn Importer> = match format {
        Some(BOOK) => None,
        Some(id) if id == Kmy.info().id => Some(&Kmy),
        Some(id) if id == KmySqlite.info().id => Some(&KmySqlite),
        Some(other) => {
            return Err(CliError::usage(format!(
                "`{}` isn't a format to make a book from: use book, kmy or kmy-sqlite",
                plain(other)
            )));
        }
        None => {
            let start = start_of(from);
            if Kmy.recognize(&start) == Recognition::Yes {
                Some(&Kmy)
            } else if KmySqlite.recognize(&start) != Recognition::No {
                // Any SQLite file could be KMyMoney's, and neither a
                // complete export nor a KassenSturz book is one.
                Some(&KmySqlite)
            } else {
                None
            }
        }
    };
    if let Some(importer) = importer {
        let limits = Limits::default();
        let input = kassensturz_import::read_file(from, &limits)?;
        let Imported::Book(imported) = import(importer, &input, &limits)? else {
            unreachable!("the KMyMoney importers read a book")
        };
        fill_new(path, computer, |book| Ok(fill_book_from(book, &imported)?))
            .map(|(book, report)| (book, Created::Imported(report)))
    } else {
        let data = read_book_file(from)?;
        fill_new(path, computer, |book| Ok(fill_book(book, &data)?))
            .map(|(book, ())| (book, Created::Export))
    }
}

/// The first bytes of the file at `path`, for recognizing its format. If
/// the file can't be read, none: reading it as a complete export then says
/// why.
fn start_of(path: &Path) -> Vec<u8> {
    let mut start = Vec::with_capacity(RECOGNITION_BYTES);
    let read = File::open(path)
        .and_then(|file| file.take(RECOGNITION_BYTES as u64).read_to_end(&mut start));
    if read.is_err() {
        start.clear();
    }
    start
}

/// Creates the book at `path` and fills it with `fill`; if that fails, the
/// book is removed again.
fn fill_new<T>(
    path: &Path,
    computer: &Computer,
    fill: impl FnOnce(&mut Book) -> Result<T, CliError>,
) -> Result<(Book, T), CliError> {
    let mut book = Book::create(path, computer)?;
    match fill(&mut book) {
        Ok(value) => Ok((book, value)),
        Err(error) => {
            // Closing it first lets go of its lock. The book is empty and was
            // only just made, so nothing is lost; if removing it fails, the
            // refusal is still the error worth reporting.
            drop(book);
            let _ = std::fs::remove_file(path);
            Err(error)
        }
    }
}

/// Says what `create` did.
pub(crate) fn write_created(
    book: &Book,
    from: Option<&Path>,
    created: &Created,
    out: &mut dyn Write,
) -> Result<(), CliError> {
    let file = || from.map_or_else(String::new, |from| plain(&from.display().to_string()));
    match created {
        Created::Empty => writeln!(out, "Created the book.")?,
        Created::Export => writeln!(
            out,
            "Created the book from {}: {} accounts and categories, {} transactions.",
            file(),
            book.all_accounts()?.len(),
            book.transactions()?.len()
        )?,
        Created::Imported(report) => write_report(book, &file(), report, out)?,
    }
    Ok(())
}

/// What filling a book from another program's did, and what it left out.
fn write_report(
    book: &Book,
    file: &str,
    report: &BookImportReport,
    out: &mut dyn Write,
) -> Result<(), CliError> {
    writeln!(
        out,
        "Created the book from {file}: {} accounts, {} categories, {} transactions, {} rates.",
        report.accounts, report.categories, report.transactions, report.rates
    )?;
    if report.completed > 0 {
        writeln!(
            out,
            "{} transactions didn't add up and got a part without a category for the rest.",
            report.completed
        )?;
    }
    for (id, name) in &report.renamed {
        writeln!(
            out,
            "Renamed {}, whose name was taken: now {}.",
            plain(name),
            plain(&book.account(*id)?.name)
        )?;
    }
    for id in &report.moved_to_top {
        writeln!(
            out,
            "Moved {} to the top level: it is in another currency than the category it was in.",
            plain(&book.account(*id)?.name)
        )?;
    }
    for id in &report.left_open {
        writeln!(
            out,
            "Kept {} open: it was closed, but money is still in it.",
            plain(&book.account(*id)?.name)
        )?;
    }
    if !report.skipped.is_empty() {
        writeln!(out, "Not imported, because KassenSturz can't hold it yet:")?;
        for skipped in &report.skipped {
            writeln!(out, "  {}: {}", skipped.what.keyword(), skipped.count)?;
        }
    }
    if !report.transfers.is_empty() {
        writeln!(
            out,
            "{} pairs of transactions look like Transfers between your accounts; \
             see `transfer suggest`.",
            report.transfers.len()
        )?;
    }
    Ok(())
}
