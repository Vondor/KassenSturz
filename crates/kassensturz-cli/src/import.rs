// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `import` command: a statement file into one of the user's accounts,
//! through the import pipeline (D-030, D-092).

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::Path;

use kassensturz_core::{CurrencyCode, Report, TransactionId};
use kassensturz_import::{
    Camt053, Csv, CsvSettings, ImportError, Imported, Importer, Limits, Mt940, Ofx,
    RECOGNITION_BYTES, Recognition, Statement, TestFormat, currency, import, read_file,
};
use kassensturz_pipeline::{
    Choices, EntryOutcome, EntryRef, ImportReport, import_statements, preview_import,
};
use kassensturz_store::Book;

use crate::amount::money_text;
use crate::args::Arguments;
use crate::error::CliError;
use crate::history::change_text;
use crate::lookup::{find_account, short_id};
use crate::text::{counted, plain, table};

/// The formats the tool recognizes by themselves: CAMT.053, MT940, OFX and
/// the made-up test format so far. CSV files need settings
/// (`--csv-settings`); KMyMoney's books come in M2.9 and M2.10.
const IMPORTERS: &[&dyn Importer] = &[&Camt053, &Mt940, &Ofx, &TestFormat];

/// The ID of the CSV importer, which `--format` takes only together with
/// `--csv-settings`.
const CSV: &str = "csv";

/// `import FILE --account NAME`.
pub(crate) fn import_file(
    book: &mut Book,
    args: &[String],
    out: &mut dyn Write,
) -> Result<(), CliError> {
    let args = Arguments::parse(
        args,
        &["account", "format", "csv-settings", "statement", "also"],
        &["dry-run"],
    )?;
    let [file] = args.exactly(["file"])?;
    let account = find_account(book, args.required("account")?)?;
    let limits = Limits::default();
    let input = read_file(Path::new(file), &limits)?;
    let format = args.value("format")?;
    let csv = match args.value("csv-settings")? {
        Some(path) => {
            if format.is_some_and(|format| format != CSV) {
                return Err(CliError::usage(
                    "--csv-settings is for CSV files; leave out --format or give --format csv",
                ));
            }
            Some(csv_importer(Path::new(path), account.currency)?)
        }
        None if format == Some(CSV) => {
            return Err(CliError::usage(
                "a CSV file needs --csv-settings FILE, which says how to read your bank's \
                 files: which column holds what, and how dates and amounts are written",
            ));
        }
        None => None,
    };
    let importer: &dyn Importer = match &csv {
        Some(csv) => csv,
        None => choose_importer(format, &input)?,
    };
    let statements = match import(importer, &input, &limits)? {
        Imported::Statements(statements) => statements,
        _ => {
            return Err(CliError::message(
                "this file holds a whole book, which `import` can't put into an account",
            ));
        }
    };
    // The chosen statements' numbers in the file, counted from 1.
    let numbers: Vec<usize> = match args.value("statement")? {
        Some(text) => vec![statement_number(text, statements.len())?],
        None => {
            let accounts: HashSet<_> = statements
                .iter()
                .map(|statement| &statement.account)
                .map(|account| (&account.iban, &account.number))
                .collect();
            if accounts.len() > 1 {
                return Err(CliError::message(format!(
                    "the file has statements for {}; choose one with --statement N, \
                     N from 1 to {}",
                    counted(accounts.len(), "account", "accounts"),
                    statements.len()
                )));
            }
            (1..=statements.len()).collect()
        }
    };
    let chosen: Vec<Statement> = numbers
        .iter()
        .map(|number| statements[number - 1].clone())
        .collect();
    let mut choices = Choices::default();
    for text in args.all("also") {
        choices
            .import_similar
            .insert(entry_ref(text, &numbers, &chosen)?);
    }
    let dry_run = args.flag("dry-run");
    let last_change = book.change_to_undo()?.map(|change| change.id);
    let report = if dry_run {
        preview_import(book, account.id, &chosen, &choices)?
    } else {
        import_statements(book, account.id, &chosen, &choices)?
    };
    let shown = Shown {
        book,
        numbers: &numbers,
        statements: &chosen,
        report: &report,
        new: report
            .entries
            .iter()
            .filter_map(|result| match result.outcome {
                EntryOutcome::Imported { transaction, .. } => Some((transaction, result.entry)),
                _ => None,
            })
            .collect(),
        dry_run,
    };
    shown.write(&plain(&account.name), out)?;
    // An import that changes nothing leaves no entry in the history (D-086),
    // so `undo` would take back an earlier change, e.g. the import before
    // (#51, D-109).
    if !dry_run {
        let undo = book.change_to_undo()?;
        if undo.as_ref().map(|change| change.id) == last_change {
            write!(
                out,
                "Nothing in the book changed, so this import isn't in the history. "
            )?;
            match undo {
                Some(change) => writeln!(
                    out,
                    "`undo` would take back the change before it: {}.",
                    change_text(book, &change)?
                )?,
                None => writeln!(out, "There is nothing to undo.")?,
            }
        }
    }
    Ok(())
}

/// The importer `--format` names, or else the one that recognizes the file
/// best.
fn choose_importer(format: Option<&str>, input: &[u8]) -> Result<&'static dyn Importer, CliError> {
    let ids = || {
        IMPORTERS
            .iter()
            .map(|importer| importer.info().id)
            .collect::<Vec<_>>()
            .join(", ")
    };
    if let Some(id) = format {
        return IMPORTERS
            .iter()
            .copied()
            .find(|importer| importer.info().id == id)
            .ok_or_else(|| CliError::usage(format!("`{id}` isn't a format; known: {}", ids())));
    }
    let start = &input[..input.len().min(RECOGNITION_BYTES)];
    let mut best: Option<(Recognition, &'static dyn Importer)> = None;
    for &importer in IMPORTERS {
        let recognition = importer.recognize(start);
        if recognition > best.map_or(Recognition::No, |(found, _)| found) {
            best = Some((recognition, importer));
        }
    }
    best.map(|(_, importer)| importer).ok_or_else(|| {
        CliError::message(format!(
            "KassenSturz doesn't recognize the file's format; give it with --format ({}), \
             or for a CSV file give --csv-settings FILE",
            ids()
        ))
    })
}

/// The CSV importer for an account in `account_currency`, reading files as
/// the settings file at `path` says (D-095).
fn csv_importer(path: &Path, account_currency: CurrencyCode) -> Result<Csv, CliError> {
    let text = read_file(path, &Limits::default())?;
    let text = String::from_utf8(text).map_err(|_| ImportError::BadSettings { line: None })?;
    let settings = CsvSettings::from_text(&text)?;
    Ok(Csv::new(settings, currency(account_currency)?)?)
}

/// The statement `--statement` names, counted from 1.
fn statement_number(text: &str, statements: usize) -> Result<usize, CliError> {
    text.parse::<usize>()
        .ok()
        .filter(|number| (1..=statements).contains(number))
        .ok_or_else(|| {
            CliError::usage(format!(
                "--statement takes a number from 1 to {statements}, the statement's place in \
                 the file"
            ))
        })
}

/// The entry an `--also` names, by its number in the list, like `1.4`: the
/// statement's number in the file, then the entry's in the statement.
fn entry_ref(text: &str, numbers: &[usize], chosen: &[Statement]) -> Result<EntryRef, CliError> {
    let wrong = || {
        CliError::usage(format!(
            "--also takes an entry's number as the list shows it, like 1.4, not `{}`",
            plain(text)
        ))
    };
    let (statement, entry) = text.split_once('.').ok_or_else(wrong)?;
    let statement: usize = statement.parse().map_err(|_| wrong())?;
    let entry: usize = entry.parse().map_err(|_| wrong())?;
    let index = numbers
        .iter()
        .position(|&number| number == statement)
        .ok_or_else(wrong)?;
    if !(1..=chosen[index].entries.len()).contains(&entry) {
        return Err(wrong());
    }
    Ok(EntryRef {
        statement: index,
        entry: entry - 1,
    })
}

/// What an import did, or would do, as the tool writes it.
struct Shown<'a> {
    book: &'a Book,
    numbers: &'a [usize],
    statements: &'a [Statement],
    report: &'a ImportReport,
    /// The new transactions, each with its entry.
    new: HashMap<TransactionId, EntryRef>,
    dry_run: bool,
}

impl Shown<'_> {
    fn write(&self, account: &str, out: &mut dyn Write) -> Result<(), CliError> {
        let report = self.report;
        let summary = report.summary();
        let verb = if self.dry_run {
            "Would import"
        } else {
            "Imported"
        };
        writeln!(
            out,
            "{verb} {} of {} into {account}.",
            summary.imported,
            counted(report.entries.len(), "entry", "entries")
        )?;
        if summary.imported > 0 {
            // A preview's transactions aren't kept, so they have no ID.
            let mut names = vec!["Entry", "Date", "Payee", "Amount"];
            if !self.dry_run {
                names.push("ID");
            }
            let mut rows = vec![header(&names)];
            for result in &report.entries {
                let EntryOutcome::Imported { transaction, .. } = result.outcome else {
                    continue;
                };
                let mut row = self.entry_row(result.entry);
                if !self.dry_run {
                    row.push(short_id(transaction));
                }
                rows.push(row);
            }
            write!(out, "{}", indented(&table(&rows, &[3])))?;
        }
        for withdrawn in &report.converted_from {
            writeln!(
                out,
                "Converted the entries in {old} into {new}, which replaced it at 1 {new} = \
                 {rate} {old} on {date}. Each transaction keeps its amount in {old}.",
                old = withdrawn.currency().code(),
                new = withdrawn.successor(),
                rate = withdrawn.rate(),
                date = withdrawn.date(),
            )?;
        }
        let pending = self.with(|outcome| matches!(outcome, EntryOutcome::Pending));
        if !pending.is_empty() {
            writeln!(
                out,
                "Left out {} the bank hasn't booked yet: {}.",
                counted(pending.len(), "entry", "entries"),
                self.labels(&pending)
            )?;
        }
        let duplicates = self.with(|outcome| matches!(outcome, EntryOutcome::Duplicate(_)));
        if !duplicates.is_empty() {
            writeln!(
                out,
                "Skipped {} already in the book: {}.",
                counted(duplicates.len(), "entry", "entries"),
                self.labels(&duplicates)
            )?;
        }
        self.write_similar(out)?;
        self.write_transfers(out)?;
        self.write_rules(out)?;
        for check in &report.balances {
            let on = if self.numbers.len() > 1 {
                format!(
                    "Statement {}: the balance on {}",
                    self.numbers[check.statement], check.date
                )
            } else {
                format!("The balance on {}", check.date)
            };
            let book = money_text(Some(self.book), check.book);
            // The bank's balance in the currency the statement was in.
            let was = check.bank_original.map_or_else(String::new, |original| {
                format!(" ({})", money_text(Some(self.book), original))
            });
            if check.matches() {
                writeln!(out, "{on} is {book}, as the bank says{was}.")?;
            } else {
                writeln!(
                    out,
                    "{on} is {book}, but the bank says {}{was}. Look for a missing or extra \
                     transaction.",
                    money_text(Some(self.book), check.bank)
                )?;
            }
        }
        if self.dry_run {
            writeln!(out, "Nothing was changed.")?;
        }
        Ok(())
    }

    /// The entries left out because they look like what the account has.
    fn write_similar(&self, out: &mut dyn Write) -> Result<(), CliError> {
        let similar: Vec<(EntryRef, &Vec<TransactionId>)> = self
            .report
            .entries
            .iter()
            .filter_map(|result| match &result.outcome {
                EntryOutcome::Similar(ids) => Some((result.entry, ids)),
                _ => None,
            })
            .collect();
        if similar.is_empty() {
            return Ok(());
        }
        writeln!(
            out,
            "Left out {} that may be in the book already. To import one anyway, add --also \
             with its number:",
            counted(similar.len(), "entry", "entries")
        )?;
        let mut rows = vec![header(&["Entry", "Date", "Payee", "Amount", "Looks like"])];
        for (at, ids) in similar {
            let mut row = self.entry_row(at);
            row.push(
                ids.iter()
                    .map(|&id| self.describe(id))
                    .collect::<Vec<_>>()
                    .join("; "),
            );
            rows.push(row);
        }
        write!(out, "{}", indented(&table(&rows, &[3])))?;
        Ok(())
    }

    /// The Transfers to suggest with a new transaction as one side.
    fn write_transfers(&self, out: &mut dyn Write) -> Result<(), CliError> {
        if self.report.transfers.is_empty() {
            return Ok(());
        }
        writeln!(
            out,
            "{} like Transfers between your accounts:",
            if self.report.transfers.len() == 1 {
                "One pair looks"
            } else {
                "These pairs look"
            }
        )?;
        let mut rows = vec![header(&["From", "Out", "To", "In", "Amount"])];
        for found in &self.report.transfers {
            let name = |id| {
                self.book
                    .account(id)
                    .map_or_else(|_| short_id(id), |account| plain(&account.name))
            };
            rows.push(vec![
                name(found.outgoing.account),
                self.describe(found.outgoing.transaction),
                name(found.incoming.account),
                self.describe(found.incoming.transaction),
                money_text(Some(self.book), found.incoming.amount),
            ]);
        }
        write!(out, "{}", indented(&table(&rows, &[4])))?;
        if !self.dry_run {
            writeln!(
                out,
                "Check each one: `transfer join ID ID` joins the two into one Transfer."
            )?;
        }
        Ok(())
    }

    /// What the Rules did to the new transactions.
    fn write_rules(&self, out: &mut dyn Write) -> Result<(), CliError> {
        let rules = &self.report.rules;
        let rule_name = |id| {
            self.book
                .rule(id)
                .map_or_else(|_| short_id(id), |rule| plain(&rule.name))
        };
        if !rules.changed.is_empty() {
            writeln!(
                out,
                "The Rules {} {} of the new transactions:",
                if self.dry_run {
                    "would change"
                } else {
                    "changed"
                },
                rules.changed.len()
            )?;
            let mut rows = vec![header(&["Transaction", "Rule"])];
            for change in &rules.changed {
                rows.push(vec![
                    self.describe(change.transaction),
                    rule_name(change.rule),
                ]);
            }
            write!(out, "{}", indented(&table(&rows, &[])))?;
        }
        if !rules.skipped.is_empty() {
            writeln!(
                out,
                "The Rules left {} alone that a Rule matched:",
                rules.skipped.len()
            )?;
            let mut rows = vec![header(&["Transaction", "Rule", "Why"])];
            for skipped in &rules.skipped {
                rows.push(vec![
                    self.describe(skipped.transaction),
                    rule_name(skipped.rule),
                    skipped.reason.report().code.to_owned(),
                ]);
            }
            write!(out, "{}", indented(&table(&rows, &[])))?;
        }
        Ok(())
    }

    /// The entries whose outcome `wanted` picks, in their order.
    fn with(&self, wanted: impl Fn(&EntryOutcome) -> bool) -> Vec<EntryRef> {
        self.report
            .entries
            .iter()
            .filter(|result| wanted(&result.outcome))
            .map(|result| result.entry)
            .collect()
    }

    /// An entry's number in the list, like `1.4`.
    fn label(&self, at: EntryRef) -> String {
        format!("{}.{}", self.numbers[at.statement], at.entry + 1)
    }

    fn labels(&self, entries: &[EntryRef]) -> String {
        entries
            .iter()
            .map(|&at| self.label(at))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// An entry's number, date, other party and amount, as the file says.
    fn entry_row(&self, at: EntryRef) -> Vec<String> {
        let entry = &self.statements[at.statement].entries[at.entry];
        vec![
            self.label(at),
            entry.booking_date.to_string(),
            entry
                .other_party_name
                .as_deref()
                .map(plain)
                .unwrap_or_default(),
            money_text(Some(self.book), entry.amount),
        ]
    }

    /// A transaction as the tool names it: a new one by its entry, with its
    /// ID unless it was only a preview, whose transactions aren't kept; one
    /// the book had already by its date, payee and ID.
    fn describe(&self, id: TransactionId) -> String {
        if let Some(&at) = self.new.get(&id) {
            return if self.dry_run {
                format!("entry {}", self.label(at))
            } else {
                format!("entry {} ({})", self.label(at), short_id(id))
            };
        }
        match self.book.transaction(id) {
            Ok(transaction) => {
                let mut words = vec![transaction.booking_date().to_string()];
                words.extend(transaction.payee().map(plain));
                words.push(format!("({})", short_id(id)));
                words.join(" ")
            }
            Err(_) => short_id(id),
        }
    }
}

fn header(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

fn indented(text: &str) -> String {
    text.lines().map(|line| format!("  {line}\n")).collect()
}
