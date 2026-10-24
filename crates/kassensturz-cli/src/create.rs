// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `create` command: a new, empty book, or one made from a complete
//! export (D-100) or from another program's book, such as KMyMoney's
//! (M2.9, M2.10).

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::str::FromStr;

use kassensturz_core::{CurrencyCode, ExchangeRate};
use kassensturz_export::read_book_file;
use kassensturz_import::{
    Imported, ImportedBook, Importer, Kmy, KmySqlite, Limits, RECOGNITION_BYTES, Recognition,
    Replacement, import,
};
use kassensturz_pipeline::{BookImportReport, RuleScope, fill_book, fill_book_from_with};
use kassensturz_store::{Book, Computer};

use crate::error::CliError;
use crate::input::{parse_currency, parse_date};
use crate::text::{counted, plain};

/// The ID of the complete export's format, as `export --format` names it.
const BOOK: &str = "book";

/// The currencies `--replaced` says replaced old ones that KassenSturz's
/// tables don't list, by the old one's code (D-106).
pub(crate) type Replaced = HashMap<CurrencyCode, Replacement>;

/// Reads each `--replaced` value, like `ROL=RON:10000:2005-07-01`: the old
/// currency, the one that replaced it, how many of the old one a unit of
/// the new one was worth, and the day.
pub(crate) fn parse_replaced(values: &[&str]) -> Result<Replaced, CliError> {
    let mut replaced = Replaced::new();
    for value in values {
        let shape = || {
            CliError::usage(format!(
                "`{}` isn't like --replaced ROL=RON:10000:2005-07-01: the old currency, \
                 the new one, how many old ones one new one was worth, and the day",
                plain(value)
            ))
        };
        let (old, rest) = value.split_once('=').ok_or_else(shape)?;
        let mut parts = rest.split(':');
        let (Some(new), Some(rate), Some(day), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(shape());
        };
        let old = parse_currency(old)?;
        let replacement = Replacement {
            successor: parse_currency(new)?,
            rate: ExchangeRate::from_str(rate.trim())?,
            date: parse_date(day.trim())?,
        };
        if replaced.insert(old, replacement).is_some() {
            return Err(CliError::usage(format!("--replaced names {old} twice")));
        }
    }
    Ok(replaced)
}

/// What a new book was made from.
pub(crate) enum Created {
    /// Nothing: it is empty.
    Empty,
    /// A complete export.
    Export,
    /// Another program's book, with what filling the new book did.
    Imported(Box<BookImportReport>),
}

/// Creates the book at `path`, filled from `from` if given: a complete
/// export, or another program's book in the format `format` names, or the
/// one `from` is recognized as, with the currencies `replaced` says
/// replaced old ones it uses (D-106) and `scope` saying which payees get
/// Rules (D-160). The file is read first, so a file
/// that isn't one leaves nothing behind; if the book refuses what it holds,
/// the new book is removed again.
pub(crate) fn create(
    path: &Path,
    computer: &Computer,
    from: Option<&Path>,
    format: Option<&str>,
    replaced: &Replaced,
    scope: RuleScope,
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
        check_replaced(&imported, replaced)?;
        fill_new(path, computer, |book| {
            Ok(fill_book_from_with(book, &imported, replaced, scope)?)
        })
        .map(|(book, report)| (book, Created::Imported(Box::new(report))))
    } else {
        if !replaced.is_empty() {
            return Err(CliError::usage(
                "--replaced is for another program's book; a complete export has none to replace",
            ));
        }
        if scope != RuleScope::default() {
            return Err(CliError::usage(
                "--rules-only-with-text is for another program's book; a complete export keeps its Rules as they are",
            ));
        }
        let data = read_book_file(from)?;
        fill_new(path, computer, |book| Ok(fill_book(book, &data)?))
            .map(|(book, ())| (book, Created::Export))
    }
}

/// Fails, before a book is made, if `imported` uses a currency KassenSturz
/// doesn't know that `replaced` doesn't name, saying how to name it and
/// what the file's own rates suggest; or if `replaced` names one the file
/// doesn't use that way.
fn check_replaced(imported: &ImportedBook, replaced: &Replaced) -> Result<(), CliError> {
    for old in replaced.keys() {
        if !imported
            .unlisted
            .iter()
            .any(|unlisted| unlisted.currency.code() == *old)
        {
            return Err(CliError::usage(format!(
                "the file has no amounts in {old} that KassenSturz needs told about: \
                 --replaced is only for currencies it doesn't know"
            )));
        }
    }
    for unlisted in &imported.unlisted {
        let code = unlisted.currency.code();
        if replaced.contains_key(&code) {
            continue;
        }
        let suggestion = unlisted.suggestion.map_or_else(String::new, |suggestion| {
            format!(
                " The file's own rates suggest --replaced {code}={}:{}:{}; check it first, \
                 since another program's rates can be off.",
                suggestion.successor, suggestion.rate, suggestion.date
            )
        });
        return Err(CliError::message(format!(
            "the file uses {code}, a currency KassenSturz doesn't know, most likely one \
             that another one replaced long ago. Say which with --replaced \
             {code}=NEW:RATE:DAY, where one NEW was worth RATE {code} from DAY on.{suggestion}"
        )));
    }
    Ok(())
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
            "Created the book from {}: {}, {}.",
            file(),
            counted(
                book.all_accounts()?.len(),
                "account or category",
                "accounts and categories"
            ),
            counted(book.transactions()?.len(), "transaction", "transactions")
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
        "Created the book from {file}: {}, {}, {}, {}.",
        counted(report.accounts, "account", "accounts"),
        counted(report.categories, "category", "categories"),
        counted(report.transactions, "transaction", "transactions"),
        counted(report.rates, "rate", "rates")
    )?;
    if report.uncategorized > 0 {
        writeln!(
            out,
            "{} no category yet, as in the file.",
            counted(report.uncategorized, "transaction has", "transactions have")
        )?;
    }
    if report.completed > 0 {
        writeln!(
            out,
            "{} didn't add up in the file and got a part without a category for the rest.",
            counted(report.completed, "transaction", "transactions")
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
    for converted in &report.converted {
        let withdrawn = converted.withdrawn;
        writeln!(
            out,
            "Converted {old} into {new}, which replaced it at 1 {new} = {rate} {old} on {date}: \
             {} accounts and categories, {} parts. Each part keeps its amount in {old}.",
            converted.accounts,
            converted.parts,
            old = withdrawn.currency().code(),
            new = withdrawn.successor(),
            rate = withdrawn.rate(),
            date = withdrawn.date(),
        )?;
    }
    if report.rules > 0 {
        writeln!(
            out,
            "{} made from the payees in the file, to sort the entries an import adds; see `rule list`.",
            counted(report.rules, "Rule", "Rules")
        )?;
    }
    for left_out in &report.patterns_left_out {
        writeln!(
            out,
            "Can't use the matching text {} of {}{}.",
            plain(&left_out.pattern),
            plain(&left_out.payee),
            if left_out.rule_made {
                ": the Rule keeps the others"
            } else {
                ": no Rule was made for it"
            }
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
            "{} of transactions {} like Transfers between your accounts; \
             see `transfer suggest`.",
            counted(report.transfers.len(), "pair", "pairs"),
            if report.transfers.len() == 1 {
                "looks"
            } else {
                "look"
            }
        )?;
    }
    Ok(())
}
