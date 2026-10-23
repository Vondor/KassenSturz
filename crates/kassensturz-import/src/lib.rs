// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! KassenSturz's importers: reading files from banks and other programs
//! into a model that doesn't depend on the file's format.
//!
//! Every format implements one interface, the [`Importer`] trait (D-069).
//! The WASM plugin API mirrors it once the plugin host comes, after 1.0
//! (D-007, D-071), so built-in importers and plugins work the same way. An
//! importer only reads: it turns a file into [`Imported`] data, here bank
//! [`Statement`]s, and never touches a book. What happens to a statement
//! afterwards (only booked entries, the duplicate check, Transfers, Rules,
//! D-030) is the job of the import pipeline, `kassensturz-pipeline`
//! (D-092).
//!
//! Files are untrusted (AGENTS.md §8, D-091): [`import`] and [`read_file`]
//! enforce [`Limits`] on size, nesting, entries and text, and check what an
//! importer returns before anyone else sees it. Text from a file is kept
//! exactly as it was delivered (D-067) and is only ever data, never part of
//! a message or an instruction. Errors are codes with parameters
//! ([`ImportError`], D-034, D-084), like everywhere else in KassenSturz.
//!
//! [`TestFormat`] is a made-up format for tests and the command-line tool;
//! the real formats come in M2.3 to M2.10: CAMT.053 is [`Camt053`], CSV
//! files from online banking [`Csv`], MT940 [`Mt940`], OFX [`Ofx`].
//!
//! This crate depends only on `kassensturz-core` (`docs/ARCHITECTURE.md`).

#![forbid(unsafe_code)]

mod amount;
mod book;
mod camt053;
mod check;
mod csv;
mod error;
mod importer;
mod kmy;
mod limits;
mod mt940;
mod ofx;
mod statement;
mod test_format;

pub use amount::{DecimalMark, currency, parse_amount};
pub use book::{
    BookAccount, BookInstitution, BookPart, BookPayee, BookRate, BookTag, BookTransaction,
    ImportedBook, Replacement, Skipped, SkippedData, UnlistedCurrency,
};
pub use camt053::Camt053;
pub use csv::{
    AmountColumns, Column, Csv, CsvSettings, DateOrder, MAX_COLUMNS, StatusColumn, TextEncoding,
    csv_preview,
};
pub use error::ImportError;
pub use importer::{
    ImportKind, Imported, Importer, ImporterInfo, RECOGNITION_BYTES, Recognition, check_statements,
    import, import_file,
};
pub use kmy::{Kmy, KmySqlite};
pub use limits::{Limits, read_file};
pub use mt940::Mt940;
pub use ofx::Ofx;
pub use statement::{Balance, Entry, EntryStatus, Statement, StatementAccount};
pub use test_format::TestFormat;
