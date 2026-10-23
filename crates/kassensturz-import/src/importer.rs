// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Importer interface every format implements (D-007, D-069).

use std::path::Path;

use crate::book::ImportedBook;
use crate::check::check;
use crate::error::ImportError;
use crate::limits::{Limits, read_file};
use crate::statement::Statement;

/// How many bytes from the start of a file [`Importer::recognize`] gets:
/// enough for an XML declaration and its first elements, a CSV header or an
/// OFX header.
pub const RECOGNITION_BYTES: usize = 4096;

/// Reads one file format, e.g. CAMT.053 or KMyMoney's `.kmy` file.
///
/// Built-in importers implement it directly; the WASM plugin API mirrors it
/// after 1.0, so a plugin importer works the same way (D-069, D-071). An
/// importer only reads: it gets the file's bytes and returns what they say,
/// and never sees a book. Settings a format needs, such as which CSV column
/// holds the amount (M2.4), are the importer value's own, given when it is
/// created, so the interface stays the same for every format.
///
/// Front ends call [`import`] or [`import_file`] rather than
/// [`Importer::read`], so the [`Limits`] are kept and the result is checked
/// whatever the importer does.
pub trait Importer {
    /// What the importer reads.
    fn info(&self) -> ImporterInfo;

    /// Whether a file looks like this format, judged from its first bytes
    /// (at most [`RECOGNITION_BYTES`]), so KassenSturz can suggest the right
    /// importer. It must not fail or panic on any input, and never decides
    /// on its own: the user can always pick another importer.
    fn recognize(&self, start: &[u8]) -> Recognition;

    /// Reads a whole file.
    ///
    /// `input` is the file's content, at most `limits.max_file_bytes` long.
    /// The importer keeps to the other limits while it reads, e.g. stops at
    /// `limits.max_depth` levels of nesting before going deeper, so a
    /// malicious file can't exhaust memory or the stack. It returns an error
    /// rather than guess: a value it can't read exactly, such as an amount
    /// with more decimal places than its currency, fails the import, since a
    /// wrong number is worse than none (AGENTS.md §2).
    fn read(&self, input: &[u8], limits: &Limits) -> Result<Imported, ImportError>;
}

/// What an importer reads, for choosing it: in the file dialog, on the
/// command line and in settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImporterInfo {
    /// A stable ID: lowercase ASCII letters, digits and hyphens, e.g.
    /// `camt053`. It names the importer on the command line, in saved
    /// settings and in the golden-file tests' folders, and never changes.
    /// The UI shows the format's name, which it keeps itself (D-034).
    pub id: &'static str,
    /// What reading a file gives.
    pub kind: ImportKind,
    /// The file name extensions the format usually has, lowercase and
    /// without the dot, e.g. `["xml"]`, for the file dialog. Files with
    /// other names can still be read.
    pub extensions: &'static [&'static str],
}

/// What reading a file gives, known before the file is read, so the UI can
/// ask the right question first: which account the entries go into, or,
/// for a whole book, where the new book goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ImportKind {
    /// Bank statements, which go into accounts of an open book.
    Statements,
    /// A whole book another program kept, such as KMyMoney's, which goes
    /// into a new book (M2.9).
    Book,
}

/// How sure an importer is that a file is in its format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Recognition {
    /// It isn't.
    No,
    /// It could be: nothing speaks against it, but nothing proves it either,
    /// as for most CSV files.
    Maybe,
    /// The file says so itself, e.g. with CAMT.053's XML namespace.
    Yes,
}

/// What an importer read from a file.
///
/// Code that handles one variant has to say what it does with the others,
/// so a new one is never ignored silently.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Imported {
    /// Bank statements, in the order of the file. A file can hold several,
    /// e.g. one per account or per day.
    Statements(Vec<Statement>),
    /// A whole book another program kept, such as KMyMoney's: its accounts,
    /// categories, transactions and rates, and what was left out (D-029).
    Book(ImportedBook),
}

/// Reads `input` with `importer`, within `limits`, and checks what the
/// importer returns before anyone uses it: that it keeps the limits on
/// entries and text, that each statement's amounts are in its currency,
/// and that a book's parts are booked to its accounts in their currencies.
pub fn import(
    importer: &dyn Importer,
    input: &[u8],
    limits: &Limits,
) -> Result<Imported, ImportError> {
    limits.check_file_size(input.len())?;
    let imported = importer.read(input, limits)?;
    check(&imported, limits)?;
    Ok(imported)
}

/// Checks statements that didn't come through an importer, such as those a
/// bank connection delivers (`kassensturz-bank`, M4.3), exactly as [`import`]
/// checks an importer's: the limits on entries and text, and that every
/// amount is in its statement's currency. The pipeline relies on these
/// checks whichever way a statement arrives.
pub fn check_statements(statements: &[Statement], limits: &Limits) -> Result<(), ImportError> {
    check(&Imported::Statements(statements.to_vec()), limits)
}

/// Reads the file at `path` with `importer`, as [`import`] does. A file
/// larger than `limits.max_file_bytes` is refused without reading more than
/// that.
pub fn import_file(
    importer: &dyn Importer,
    path: &Path,
    limits: &Limits,
) -> Result<Imported, ImportError> {
    let input = read_file(path, limits)?;
    import(importer, &input, limits)
}
