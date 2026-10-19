// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! KassenSturz's command-line tool: a front end for trying out the core and
//! the store without the UI (M1.8). It creates and opens books, adds
//! accounts, categories, transactions and exchange rates, shows registers
//! and balances, suggests and joins Transfers (M1.10), and undoes and
//! redoes changes (M1.9).
//!
//! The tool is for developers and testers. It writes English text and plain
//! amounts (`-12.50 EUR`), shows errors as their code and parameters, and is
//! not translated or installed with the app (D-085). Everything it does goes
//! through the public functions of `kassensturz-core` and `kassensturz-store`.

#![forbid(unsafe_code)]

mod accounts;
mod amount;
mod args;
mod error;
mod history;
mod input;
mod lookup;
mod rate;
mod text;
mod transaction;
mod transfer;

use std::io::Write;
use std::path::PathBuf;

use kassensturz_store::{Book, Computer, StoreError};

use args::Arguments;
use error::{CliError, print_error};

/// The app ID, which names KassenSturz's folders (D-049).
const APP_ID: &str = "io.github.vondor.KassenSturz";

/// What the tool takes from the computer it runs on, besides its arguments.
///
/// Finding these differs between Plasma, GNOME, Windows and macOS, so it
/// belongs in `kassensturz-platform` once that exists (`docs/ARCHITECTURE.md`);
/// until then the tool makes its best guess and the options override it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Environment {
    /// The book to use when `--book` isn't given (`KASSENSTURZ_BOOK`).
    pub book: Option<PathBuf>,
    /// The computer's name, which a book's lock file tells others (D-038).
    pub computer_name: String,
    /// Where automatic backups go (D-033): KassenSturz's data folder, if the
    /// tool found it.
    pub backup_folder: Option<PathBuf>,
}

impl Environment {
    /// Reads the environment variables and the computer's name.
    pub fn from_system() -> Self {
        let var = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
        let computer_name = std::fs::read_to_string("/etc/hostname")
            .ok()
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty())
            .or_else(|| {
                var("COMPUTERNAME")
                    .or_else(|| var("HOSTNAME"))?
                    .into_string()
                    .ok()
            })
            .unwrap_or_else(|| "this computer".to_owned());
        let data_folder = if cfg!(windows) {
            var("APPDATA").map(PathBuf::from)
        } else if cfg!(target_os = "macos") {
            var("HOME").map(|home| PathBuf::from(home).join("Library/Application Support"))
        } else {
            var("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| var("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        };
        Self {
            book: var("KASSENSTURZ_BOOK").map(PathBuf::from),
            computer_name,
            backup_folder: data_folder.map(|folder| folder.join(APP_ID).join("backups")),
        }
    }
}

const USAGE: &str = "\
kassensturz-cli: try out KassenSturz's core without the app

Usage: kassensturz-cli [--book FILE] [--backup-folder DIR] [--computer NAME]
                       [--open-anyway] <command> [arguments]

The book is the file given with --book, or the one in KASSENSTURZ_BOOK.
Amounts are typed and shown plainly, like -12.50. A negative amount takes
money out of the account. Dates are written 2026-10-05. Names ignore capitals.
Categories can be named by their path, like Food/Groceries.

Commands:
  create                         make a new, empty book
  account add NAME --type TYPE --currency CODE
                                 TYPE is checking, savings, cash or credit-card
  account list [--all]           --all includes closed accounts
  account rename NAME NEW-NAME
  account close NAME             only when its balance is zero
  account reopen NAME
  category add NAME --kind income|expense --currency CODE [--inside PARENT]
  category list
  category move NAME [--inside PARENT]
  transaction add --date DATE --account NAME --amount AMOUNT [--payee TEXT]
                  [--category NAME | --to ACCOUNT | --part NAME=AMOUNT ...]
                                 --category books the whole amount to a
                                 category, --to makes it a transfer to
                                 another account, --part splits it (leave one
                                 amount empty, NAME=, for the remainder).
                                 Without any of them, it has no category yet.
  transaction show ID            ID is the end of the ID the register shows
  transaction remove ID
  transfer suggest               pairs of transactions that look like the two
                                 sides of one Transfer between your accounts:
                                 the same amount out of one and into another,
                                 at most 3 business days apart, the other side
                                 without a category yet. Nothing is changed
  transfer join ID ID            join two such transactions into one Transfer
  register NAME                  an account's or category's transactions
  balances [--all]               every account's balance, and the totals
  rate add --date DATE --from CODE --to CODE --rate RATE
  rate list --from CODE --to CODE
  rate remove --date DATE --from CODE --to CODE
  undo                           undo the last change, also one made by an
                                 earlier command
  redo                           make the change undone last again
  history list                   every change, newest first
  history keep DAYS|forever      how long the book keeps its history (30 days
                                 for a new book); with 0, a change can be
                                 undone only until the book is closed
  history clean-up               delete the whole history for good
  help                           this text

A book that is open somewhere else is refused; --open-anyway opens it after
you checked that it isn't.
Errors are shown as their code and parameters (docs/ERROR_CODES.md) and end
the program with status 1; a mistake in the command line ends it with 2.
";

/// Runs one command line (the arguments after the program's name), writing
/// the result to `out` and errors to `err`. Returns the exit status: 0 if it
/// worked, 1 if the command failed, 2 if the command line is wrong.
pub fn run(
    args: &[String],
    environment: &Environment,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let command_line = match CommandLine::parse(args) {
        Ok(command_line) => command_line,
        Err(error) => return fail(&error, None, err),
    };
    if matches!(command_line.command, None | Some("help" | "--help" | "-h")) {
        return match out.write_all(USAGE.as_bytes()) {
            Ok(()) => 0,
            Err(error) => fail(&error.into(), None, err),
        };
    }
    // Mistakes in the command are found before the book is touched: opening
    // it takes its lock and may update it.
    if let Err(error) = command_line.check_command() {
        return fail(&error, None, err);
    }
    let mut book = match open(&command_line, environment, err) {
        Ok(book) => book,
        Err(error) => return fail(&error, None, err),
    };
    match command_line.execute(&mut book, out) {
        Ok(()) => 0,
        Err(error) => fail(&error, Some(&book), err),
    }
}

/// Writes the error and returns the exit status for it.
fn fail(error: &CliError, book: Option<&Book>, err: &mut dyn Write) -> u8 {
    // If even the error can't be written, there's nowhere left to tell.
    let _ = print_error(error, book, err);
    match error {
        CliError::Usage(_) => 2,
        CliError::Message(_) | CliError::Report(_) | CliError::Output(_) => 1,
    }
}

/// The options before the command, the command and what follows it.
struct CommandLine<'a> {
    book: Option<PathBuf>,
    backup_folder: Option<PathBuf>,
    computer: Option<String>,
    open_anyway: bool,
    command: Option<&'a str>,
    rest: &'a [String],
}

impl<'a> CommandLine<'a> {
    const VALUE_OPTIONS: [&'static str; 3] = ["book", "backup-folder", "computer"];
    const FLAG_OPTIONS: [&'static str; 1] = ["open-anyway"];

    fn parse(args: &'a [String]) -> Result<Self, CliError> {
        // The options come first, up to the command's name.
        let mut end = 0;
        while let Some(option) = args.get(end).and_then(|arg| arg.strip_prefix("--")) {
            if option.is_empty() || option == "help" {
                break;
            }
            let takes_value = !option.contains('=') && Self::VALUE_OPTIONS.contains(&option);
            end += if takes_value { 2 } else { 1 };
        }
        let end = end.min(args.len());
        let options = Arguments::parse(&args[..end], &Self::VALUE_OPTIONS, &Self::FLAG_OPTIONS)?;
        options.exactly([])?;
        Ok(Self {
            book: options.value("book")?.map(PathBuf::from),
            backup_folder: options.value("backup-folder")?.map(PathBuf::from),
            computer: options.value("computer")?.map(str::to_owned),
            open_anyway: options.flag("open-anyway"),
            command: args.get(end).map(String::as_str),
            rest: args.get(end + 1..).unwrap_or_default(),
        })
    }

    /// Fails if the command is unknown or, for `create`, `undo` and `redo`,
    /// has arguments.
    fn check_command(&self) -> Result<(), CliError> {
        match self.command {
            Some("create" | "undo" | "redo") => {
                Arguments::parse(self.rest, &[], &[])?.exactly([])?;
                Ok(())
            }
            Some(
                "account" | "category" | "transaction" | "transfer" | "rate" | "balances"
                | "register" | "history",
            ) => Ok(()),
            Some(other) => Err(CliError::usage(format!("`{other}` isn't a command"))),
            None => Ok(()),
        }
    }

    fn execute(&self, book: &mut Book, out: &mut dyn Write) -> Result<(), CliError> {
        let rest = self.rest;
        match self.command {
            Some("create") => {
                // `open` made the book; saying so is all that is left.
                writeln!(out, "Created the book.")?;
                Ok(())
            }
            Some("account") => accounts::account(book, rest, out),
            Some("category") => accounts::category(book, rest, out),
            Some("transaction") => transaction::transaction(book, rest, out),
            Some("transfer") => transfer::transfer(book, rest, out),
            Some("rate") => rate::rate(book, rest, out),
            Some("balances") => accounts::balances(book, rest, out),
            Some("register") => accounts::register(book, rest, out),
            Some("undo") => history::undo(book, out),
            Some("redo") => history::redo(book, out),
            Some("history") => history::history(book, rest, out),
            Some(other) => unreachable!("`{other}` was checked before"),
            None => unreachable!("without a command the help text is shown"),
        }
    }
}

/// Creates or opens the book the command line asks for.
fn open(
    command_line: &CommandLine<'_>,
    environment: &Environment,
    err: &mut dyn Write,
) -> Result<Book, CliError> {
    let path = command_line
        .book
        .clone()
        .or_else(|| environment.book.clone())
        .ok_or_else(|| CliError::usage("no book: give --book FILE or set KASSENSTURZ_BOOK"))?;
    let backup_folder = command_line
        .backup_folder
        .clone()
        .or_else(|| environment.backup_folder.clone())
        .ok_or_else(|| CliError::usage("no backup folder found: give --backup-folder DIR"))?;
    let computer = Computer {
        name: command_line
            .computer
            .clone()
            .unwrap_or_else(|| environment.computer_name.clone()),
        backup_folder,
    };
    if command_line.command == Some("create") {
        return Ok(Book::create(&path, &computer)?);
    }
    match Book::open(&path, &computer) {
        Err(StoreError::Locked(seen)) if command_line.open_anyway => {
            // The user asked for this on the command line, which is the
            // warning's answer in a tool without dialogs (D-038, D-085).
            writeln!(
                err,
                "warning: opening the book anyway, although it says it is open on {} since {}",
                seen.computer
                    .as_deref()
                    .map_or_else(|| "an unknown computer".to_owned(), text::plain),
                seen.since
                    .map_or_else(|| "an unknown time".to_owned(), |since| since.to_string()),
            )?;
            Ok(Book::open_anyway(&path, &computer, &seen)?)
        }
        other => Ok(other?),
    }
}
