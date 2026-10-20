// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What goes wrong in a command, and how it is told to the user.
//!
//! Errors of the core and the store reach this tool as error reports: a
//! stable code with typed parameters (D-084). The tool prints the code and
//! the parameters, not a sentence, since translated messages belong to the UI
//! (D-034); `docs/ERROR_CODES.md` says what each code means (D-085).

use std::io::{self, Write};

use kassensturz_core::{
    CurrencyCodeError, DecimalError, ErrorParam, ErrorReport, MoneyError, Report, RuleError,
    TransactionError,
};
use kassensturz_export::ExportError;
use kassensturz_import::ImportError;
use kassensturz_pipeline::PipelineError;
use kassensturz_store::{Book, StoreError};

use crate::amount::money_text;
use crate::text::plain;

/// Why a command failed.
#[derive(Debug)]
pub(crate) enum CliError {
    /// The command line itself is wrong. Exit status 2.
    Usage(String),
    /// Something the tool itself found wrong, e.g. a name that no account
    /// has, in plain English.
    Message(String),
    /// An error of the core, the store, the importers, exporters or the import
    /// pipeline.
    Report(ErrorReport),
    /// Writing the result failed, e.g. because the reader closed the pipe.
    Output(io::Error),
}

impl CliError {
    pub(crate) fn usage(message: impl Into<String>) -> Self {
        Self::Usage(message.into())
    }

    pub(crate) fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}

macro_rules! from_report {
    ($($error:ty),*) => {$(
        impl From<$error> for CliError {
            fn from(error: $error) -> Self {
                Self::Report(error.report())
            }
        }
    )*};
}

from_report!(
    StoreError,
    ImportError,
    ExportError,
    PipelineError,
    TransactionError,
    MoneyError,
    DecimalError,
    CurrencyCodeError,
    RuleError
);

impl From<io::Error> for CliError {
    fn from(error: io::Error) -> Self {
        Self::Output(error)
    }
}

/// Writes `error` to `err`. `book`, if open, lets the report show account
/// names and amounts with the book's decimal places.
pub(crate) fn print_error(
    error: &CliError,
    book: Option<&Book>,
    err: &mut dyn Write,
) -> io::Result<()> {
    match error {
        CliError::Usage(message) => {
            writeln!(err, "error: {message}")?;
            writeln!(err, "Run `kassensturz-cli help` to see the commands.")
        }
        CliError::Message(message) => writeln!(err, "error: {message}"),
        CliError::Report(report) => print_report(report, book, err),
        CliError::Output(error) => writeln!(err, "error: could not write the result: {error}"),
    }
}

fn print_report(report: &ErrorReport, book: Option<&Book>, err: &mut dyn Write) -> io::Result<()> {
    writeln!(err, "error: {}", report.code)?;
    let mut indent = String::from("  ");
    let mut current = report;
    loop {
        for (name, value) in &current.params {
            writeln!(err, "{indent}{name}: {}", param_text(value, book))?;
        }
        let Some(cause) = current.cause.as_deref() else {
            return Ok(());
        };
        writeln!(err, "{indent}caused by: {}", cause.code)?;
        indent.push_str("  ");
        current = cause;
    }
}

/// A parameter as text. Text from a book or file is shown quoted with its
/// special characters escaped, since it may contain anything (AGENTS.md §8).
fn param_text(param: &ErrorParam, book: Option<&Book>) -> String {
    match param {
        ErrorParam::Money(money) => money_text(book, *money),
        ErrorParam::Currency(currency) => currency.to_string(),
        ErrorParam::Date(date) => date.to_string(),
        ErrorParam::Timestamp(timestamp) => timestamp.to_string(),
        ErrorParam::Number(number) => number.to_string(),
        ErrorParam::Account(id) => book
            .and_then(|book| book.account(*id).ok())
            .map_or_else(|| id.to_string(), |account| plain(&account.name)),
        ErrorParam::AccountKind(kind) => format!("{kind:?}").to_lowercase(),
        ErrorParam::AccountType(account_type) => format!("{account_type:?}").to_lowercase(),
        ErrorParam::Transaction(id) => id.to_string(),
        ErrorParam::Split(id) => id.to_string(),
        ErrorParam::Rate(id) => id.to_string(),
        ErrorParam::Rule(id) => book
            .and_then(|book| book.rule(*id).ok())
            .map_or_else(|| id.to_string(), |rule| plain(&rule.name)),
        ErrorParam::Keyword(word) => (*word).to_owned(),
        ErrorParam::Text(text) => format!("{text:?}"),
    }
}
