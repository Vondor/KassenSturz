// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Importing a file from the user's bank into one account, as the screens
//! see it (M3.10, D-091, D-092, D-095, D-111, D-117, D-123).
//!
//! The steps: the file's format is recognized ([`recognize`]); a CSV file
//! first needs the user to say which column holds what ([`CsvForm`], kept
//! per bank in the app's settings); a file with statements for several
//! accounts needs the user to pick one ([`statement_choices`]); then the
//! import pipeline runs as a dry run ([`preview`]) and the user decides about
//! the entries that look like transactions the account already has, before
//! [`import`] does the same for real, as one user action that undo takes
//! back as a whole. Everything reaches QML as text (D-065).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use kassensturz_core::{AccountId, AccountKind, Currency, Date, Money, Report, TransactionId};
use kassensturz_import::{
    AmountColumns, Camt053, Column, Csv, CsvSettings, DateOrder, DecimalMark, ImportError,
    Imported, Importer, Kmy, KmySqlite, Limits, Mt940, Ofx, RECOGNITION_BYTES, Recognition,
    Statement, StatusColumn, TextEncoding, csv_preview, import as read_with,
};
use kassensturz_pipeline::{
    Choices, Duplicate, EntryOutcome, EntryRef, ImportReport, PipelineError, import_statements,
};
use kassensturz_store::{Action, Book, StatedBalance, StoreError};

use crate::errors::{ErrorText, report_text};
use crate::format::Locale;
use crate::register::{self, AmountWriter, Shown};
use crate::settings::Settings;
use crate::transfer_prompt::{self, SuggestionText, TransferPromptError};

/// The formats the app reads statements from, in the order they are tried.
/// The test format (D-092) isn't offered.
const FORMATS: &[&dyn Importer] = &[&Camt053, &Mt940, &Ofx];

/// The formats that hold a whole book, which M3.12 reads: recognized here
/// only to tell the user so.
const BOOKS: &[&dyn Importer] = &[&Kmy, &KmySqlite];

/// How many rows of a CSV file the column screen shows.
const CSV_PREVIEW_ROWS: usize = 6;

/// The settings group that keeps the banks' CSV settings (D-111): `Name1`
/// and `Settings1`, `Name2` and `Settings2`, and so on.
const CSV_BANKS: &str = "CsvBanks";

/// What can go wrong on the import screens.
#[derive(Debug)]
pub enum StatementImportError {
    /// The file holds a whole book, e.g. KMyMoney's, not statements.
    WholeBook,
    /// No format KassenSturz reads, and not a CSV file by its name.
    UnknownFormat,
    /// The file was read, but has no statement.
    NoStatements,
    /// The account, statement or entry meant isn't there (anymore).
    NoSuchEntry,
    /// The file couldn't be read, or the settings for it don't fit.
    Import(ImportError),
    /// The book refused.
    Store(StoreError),
}

impl From<ImportError> for StatementImportError {
    fn from(error: ImportError) -> Self {
        Self::Import(error)
    }
}

impl From<StoreError> for StatementImportError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<PipelineError> for StatementImportError {
    fn from(error: PipelineError) -> Self {
        match error {
            PipelineError::Import(error) => Self::Import(error),
            PipelineError::Store(error) => Self::Store(error),
        }
    }
}

impl From<TransferPromptError> for StatementImportError {
    fn from(error: TransferPromptError) -> Self {
        match error {
            TransferPromptError::NoSuchSuggestion => Self::NoSuchEntry,
            TransferPromptError::Store(error) => Self::Store(error),
        }
    }
}

/// The error as the UI gets it: the app's own codes start with `input.`,
/// the import crate's and the store's are those of `docs/ERROR_CODES.md`.
pub fn error_text(book: &Book, error: &StatementImportError, locale: &impl Locale) -> ErrorText {
    match error {
        StatementImportError::WholeBook => ErrorText::plain("input.whole-book"),
        StatementImportError::UnknownFormat => ErrorText::plain("input.unknown-format"),
        StatementImportError::NoStatements => ErrorText::plain("input.no-statements"),
        StatementImportError::NoSuchEntry => ErrorText::plain("input.no-such-entry"),
        StatementImportError::Import(error) => report_text(book, &error.report(), locale),
        StatementImportError::Store(error) => report_text(book, &error.report(), locale),
    }
}

/// What a file turned out to be.
pub enum FileKind {
    /// A format KassenSturz reads statements from.
    Statements(&'static dyn Importer),
    /// A CSV file, which needs the user's column settings first (D-095).
    Csv,
}

/// Judges a file from its start and its name (D-091): a format KassenSturz
/// reads for sure wins; a file named `.csv` or `.txt` is a CSV file; a
/// format that may fit is suggested; a whole book is refused with a hint,
/// and so is anything else.
pub fn recognize(input: &[u8], name: &str) -> Result<FileKind, StatementImportError> {
    let start = &input[..input.len().min(RECOGNITION_BYTES)];
    let best = |importers: &[&'static dyn Importer]| {
        importers
            .iter()
            .map(|importer| (importer.recognize(start), *importer))
            .filter(|(recognition, _)| *recognition != Recognition::No)
            .max_by_key(|(recognition, _)| *recognition)
    };
    let statements = best(FORMATS);
    let books = best(BOOKS);
    if let Some((Recognition::Yes, importer)) = statements {
        return Ok(FileKind::Statements(importer));
    }
    if let Some((Recognition::Yes, _)) = books {
        return Err(StatementImportError::WholeBook);
    }
    let extension = Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    if matches!(extension.as_deref(), Some("csv" | "txt")) {
        return Ok(FileKind::Csv);
    }
    match (statements, books) {
        (Some((_, importer)), _) => Ok(FileKind::Statements(importer)),
        (None, Some(_)) => Err(StatementImportError::WholeBook),
        (None, None) => Err(StatementImportError::UnknownFormat),
    }
}

/// Reads the statements of `input` with `importer`, within the limits for
/// untrusted files (D-091).
pub fn read_statements(
    importer: &dyn Importer,
    input: &[u8],
) -> Result<Vec<Statement>, StatementImportError> {
    match read_with(importer, input, &Limits::default())? {
        Imported::Statements(statements) if statements.is_empty() => {
            Err(StatementImportError::NoStatements)
        }
        Imported::Statements(statements) => Ok(statements),
        _ => Err(StatementImportError::WholeBook),
    }
}

/// Reads a CSV file with the user's column settings, in the account's
/// currency with the decimal places the book stores for it (D-027).
pub fn read_csv(
    book: &Book,
    account: AccountId,
    input: &[u8],
    settings: &CsvSettings,
) -> Result<Vec<Statement>, StatementImportError> {
    let account = book.account(account)?;
    let digits = book.currency_minor_digits(account.currency)?;
    let currency = Currency::new(account.currency, digits)
        .ok_or(StoreError::UnknownCurrency(account.currency))?;
    let importer = Csv::new(settings.clone(), currency)?;
    read_statements(&importer, input)
}

/// A file read into statements, with what the user chose so far.
#[derive(Debug)]
pub struct Loaded {
    /// The account the file is for.
    pub account: AccountId,
    /// The file, whose name the history keeps (D-109).
    pub path: PathBuf,
    /// The file's statements, in its order.
    pub statements: Vec<Statement>,
    /// Which of them go into the account; empty until the user chose, when
    /// the file has statements for several accounts.
    pub chosen: Vec<usize>,
    /// The entries the user wants imported although they look like
    /// transactions the account already has (D-092).
    pub choices: Choices,
    /// The day the statement was fetched from the account's bank, when it
    /// was (M4.7); `None` for a file. The import notes it on the account's
    /// bank link, in the same user action (D-153).
    pub fetched_on: Option<Date>,
}

impl Loaded {
    /// The statements of a file for `account`. A file with statements for
    /// one account only imports them all; otherwise the user picks one with
    /// [`Loaded::choose_statement`] first.
    pub fn new(account: AccountId, path: PathBuf, statements: Vec<Statement>) -> Self {
        let one_account = statements
            .windows(2)
            .all(|pair| pair[0].account == pair[1].account);
        let chosen = if one_account {
            (0..statements.len()).collect()
        } else {
            Vec::new()
        };
        Self {
            account,
            path,
            statements,
            chosen,
            choices: Choices::default(),
            fetched_on: None,
        }
    }

    /// The statement fetched from the bank of `account` on `day` (M4.7). It
    /// is the account's own, so nothing is left for the user to choose, and
    /// there is no file.
    pub fn from_bank(account: AccountId, statement: Statement, day: Date) -> Self {
        Self {
            fetched_on: Some(day),
            ..Self::new(account, PathBuf::new(), vec![statement])
        }
    }

    /// Whether the user still has to say which statement is the account's.
    pub fn needs_statement_choice(&self) -> bool {
        self.chosen.is_empty()
    }

    /// The statement at `index` in the file is the one to import.
    pub fn choose_statement(&mut self, index: usize) -> Result<(), StatementImportError> {
        if index >= self.statements.len() {
            return Err(StatementImportError::NoSuchEntry);
        }
        self.chosen = vec![index];
        self.choices = Choices::default();
        Ok(())
    }

    /// The statements to import, as the pipeline gets them.
    pub fn chosen_statements(&self) -> Vec<Statement> {
        self.chosen
            .iter()
            .map(|&index| self.statements[index].clone())
            .collect()
    }

    /// Imports the entry `key` names ("statement.entry", as the preview
    /// shows it) although it looks like a transaction the account has, or
    /// leaves it out again.
    pub fn set_import_anyway(&mut self, key: &str, on: bool) -> Result<(), StatementImportError> {
        let (statement, entry) = key
            .split_once('.')
            .ok_or(StatementImportError::NoSuchEntry)?;
        let at = EntryRef {
            statement: statement
                .parse()
                .map_err(|_| StatementImportError::NoSuchEntry)?,
            entry: entry
                .parse()
                .map_err(|_| StatementImportError::NoSuchEntry)?,
        };
        let exists = self
            .chosen
            .get(at.statement)
            .map(|&index| &self.statements[index])
            .is_some_and(|statement| at.entry < statement.entries.len());
        if !exists {
            return Err(StatementImportError::NoSuchEntry);
        }
        if on {
            self.choices.import_similar.insert(at);
        } else {
            self.choices.import_similar.remove(&at);
        }
        Ok(())
    }
}

/// One statement of a file with several, for the user to pick from.
#[derive(Debug, PartialEq, Eq)]
pub struct StatementChoice {
    /// Its place in the file, which [`Loaded::choose_statement`] takes.
    pub index: usize,
    /// The account as the file names it: the IBAN, else the number.
    pub account: String,
    /// The first and last booking date, if the statement has entries.
    pub first_date: String,
    pub last_date: String,
    pub entries: usize,
    /// The closing balance the bank reports, if any.
    pub closing: Option<Shown>,
    pub closing_date: String,
}

/// The statements of a file as the user sees them, in the file's order.
pub fn statement_choices(
    book: &Book,
    loaded: &Loaded,
    locale: &impl Locale,
) -> Vec<StatementChoice> {
    loaded
        .statements
        .iter()
        .enumerate()
        .map(|(index, statement)| {
            let dates = statement.entries.iter().map(|entry| entry.booking_date);
            let writer = AmountWriter::new(book, statement.currency.code(), locale).ok();
            let closing = statement.closing_balance;
            StatementChoice {
                index,
                account: statement
                    .account
                    .iban
                    .clone()
                    .or_else(|| statement.account.number.clone())
                    .unwrap_or_default(),
                first_date: dates
                    .clone()
                    .min()
                    .map_or_else(String::new, |date| locale.short_date(date)),
                last_date: dates
                    .max()
                    .map_or_else(String::new, |date| locale.short_date(date)),
                entries: statement.entries.len(),
                closing: closing
                    .and_then(|balance| writer.as_ref().map(|writer| writer.text(balance.amount))),
                closing_date: closing
                    .map_or_else(String::new, |balance| locale.short_date(balance.date)),
            }
        })
        .collect()
}

/// One entry of a statement, as the preview and the summary show it.
#[derive(Debug, PartialEq, Eq)]
pub struct EntryText {
    /// "statement.entry", counted from zero within the statements chosen,
    /// which [`Loaded::set_import_anyway`] takes.
    pub key: String,
    pub date: String,
    pub other_party: String,
    /// The remittance information, as the bank delivered it.
    pub purpose: String,
    pub amount: Shown,
    /// Whether money came in.
    pub incoming: bool,
    /// `imported`, `similar` (left out until the user decides, D-092),
    /// `already-in-book`, `twice-in-file` or `pending`.
    pub outcome: &'static str,
    /// The category the Rules gave an imported entry (D-088), or empty.
    pub category: String,
    /// The transactions the entry looks like.
    pub similar: Vec<SimilarText>,
    /// Whether the user chose to import it anyway.
    pub import_anyway: bool,
}

/// A transaction of the account that an entry looks like.
#[derive(Debug, PartialEq, Eq)]
pub struct SimilarText {
    pub id: String,
    pub date: String,
    pub other_party: String,
    pub category: String,
    pub amount: Shown,
}

/// A balance the bank reports against the account's own figure (D-029,
/// D-111, D-123): each at the end of its day, the closing balance after the
/// import. Banks date an opening balance with the day the statement before
/// ended, so that day's end is what it is held against (Q-053).
#[derive(Debug, PartialEq, Eq)]
pub struct BalanceText {
    /// `opening` or `closing`.
    pub which: &'static str,
    /// The day the bank gives.
    pub date: String,
    /// What the bank says; for a debt, what is owed.
    pub bank: Shown,
    pub bank_owes: bool,
    /// What KassenSturz has for that day.
    pub book: Shown,
    pub book_owes: bool,
    pub matches: bool,
    /// How far apart the two are, always positive.
    pub difference: Shown,
    /// Whether KassenSturz has more than the bank.
    pub book_higher: bool,
    /// What to type into "Check against my bank" to look into it: the
    /// bank's figure and its day.
    pub check_amount: String,
    pub check_date: String,
    /// Whether this balance is, or would be, kept as what the bank said
    /// (D-117, D-123).
    pub stated: bool,
}

/// What an import does or did, for the preview and the summary.
#[derive(Debug, PartialEq, Eq)]
pub struct ImportView {
    /// The file's name; empty for a statement fetched from the bank.
    pub file_name: String,
    /// Whether the statement was fetched from the bank (M4.7).
    pub from_bank: bool,
    pub account_name: String,
    /// Whether the account is a debt, e.g. a credit card.
    pub debt: bool,
    pub entries: Vec<EntryText>,
    pub imported: usize,
    pub to_decide: usize,
    pub duplicates: usize,
    pub pending: usize,
    /// How many of the new transactions a Rule booked.
    pub rules: usize,
    /// How many Transfers the new transactions suggest (D-087).
    pub transfers: usize,
    pub balances: Vec<BalanceText>,
    /// The new transactions, for the Transfer prompt.
    pub new: Vec<TransactionId>,
}

/// What [`import`] would do with `loaded`, without changing the book: the
/// pipeline runs, Rules and Transfers included, and is taken back
/// (`Book::dry_run`), so even the categories the Rules would give are shown.
pub fn preview(
    book: &mut Book,
    loaded: &Loaded,
    locale: &impl Locale,
) -> Result<ImportView, StatementImportError> {
    let statements = loaded.chosen_statements();
    book.dry_run(|book| perform(book, loaded, &statements, false, locale))
}

/// Imports the statements chosen into the account as one user action
/// (D-086), and keeps the bank's closing balance as what the bank said
/// when KassenSturz agrees with it (D-117, D-123).
pub fn import(
    book: &mut Book,
    loaded: &Loaded,
    locale: &impl Locale,
) -> Result<ImportView, StatementImportError> {
    let statements = loaded.chosen_statements();
    book.as_one_action(Action::ImportStatement, |book| {
        perform(book, loaded, &statements, true, locale)
    })
}

fn perform(
    book: &mut Book,
    loaded: &Loaded,
    statements: &[Statement],
    for_real: bool,
    locale: &impl Locale,
) -> Result<ImportView, StatementImportError> {
    let report = import_statements(
        book,
        loaded.account,
        statements,
        &loaded.choices,
        Some(&loaded.path),
    )?;
    let stated = closing_to_keep(book, loaded.account, &report)?;
    if for_real && let Some(stated) = &stated {
        book.set_stated_balance(loaded.account, Some(stated))?;
    }
    // The fetch day is part of the import's action: undo takes back both,
    // and a fetch the user didn't import is not noted (D-153).
    if for_real && let Some(day) = loaded.fetched_on {
        book.note_bank_fetch(loaded.account, day)?;
    }
    describe(book, loaded, statements, &report, stated.as_ref(), locale)
}

/// The closing balance to keep as what the bank said (D-123): the latest
/// one the import reports, when KassenSturz's figure for that day agrees,
/// unless the account already holds what the bank said on a later day. A
/// difference is never kept: the user looks into it first (D-111).
fn closing_to_keep(
    book: &Book,
    account: AccountId,
    report: &ImportReport,
) -> Result<Option<StatedBalance>, StoreError> {
    let Some(latest) = report.balances.iter().max_by_key(|check| check.date) else {
        return Ok(None);
    };
    if !latest.matches() {
        return Ok(None);
    }
    let later = book
        .stated_balance(account)?
        .is_some_and(|stated| stated.on > latest.date);
    Ok((!later).then_some(StatedBalance {
        amount: latest.bank,
        on: latest.date,
    }))
}

fn describe(
    book: &Book,
    loaded: &Loaded,
    statements: &[Statement],
    report: &ImportReport,
    stated: Option<&StatedBalance>,
    locale: &impl Locale,
) -> Result<ImportView, StatementImportError> {
    let account = book.account(loaded.account)?;
    let writer = AmountWriter::new(book, account.currency, locale)?;
    let register: HashMap<TransactionId, register::Entry> = register::entries(book, account.id)?
        .into_iter()
        .map(|entry| (entry.transaction, entry))
        .collect();
    let similar_text = |id: &TransactionId| {
        register.get(id).map(|entry| SimilarText {
            id: id.to_string(),
            date: locale.short_date(entry.date),
            other_party: entry.other_party.clone(),
            category: entry.category.clone(),
            amount: writer.text(entry.amount),
        })
    };
    let writers: Vec<Option<AmountWriter<'_, _>>> = statements
        .iter()
        .map(|statement| AmountWriter::new(book, statement.currency.code(), locale).ok())
        .collect();
    let mut entries = Vec::with_capacity(report.entries.len());
    for result in &report.entries {
        let at = result.entry;
        let entry = &statements[at.statement].entries[at.entry];
        let amount = writers[at.statement]
            .as_ref()
            .unwrap_or(&writer)
            .text(entry.amount);
        let (outcome, category, similar) = match &result.outcome {
            EntryOutcome::Imported {
                transaction,
                similar,
            } => (
                "imported",
                register
                    .get(transaction)
                    .map(|entry| entry.category.clone())
                    .unwrap_or_default(),
                similar.iter().filter_map(similar_text).collect(),
            ),
            EntryOutcome::Similar(similar) => (
                "similar",
                String::new(),
                similar.iter().filter_map(similar_text).collect(),
            ),
            EntryOutcome::Duplicate(Duplicate::InBook(_)) => {
                ("already-in-book", String::new(), Vec::new())
            }
            EntryOutcome::Duplicate(Duplicate::InStatements(_)) => {
                ("twice-in-file", String::new(), Vec::new())
            }
            EntryOutcome::Pending => ("pending", String::new(), Vec::new()),
        };
        entries.push(EntryText {
            key: format!("{}.{}", at.statement, at.entry),
            date: locale.short_date(entry.booking_date),
            other_party: entry.other_party_name.clone().unwrap_or_default(),
            purpose: entry.remittance_information.clone().unwrap_or_default(),
            amount,
            incoming: entry.amount.minor_units() > 0,
            outcome,
            category,
            similar,
            import_anyway: loaded.choices.import_similar.contains(&at),
        });
    }

    let mut balances = Vec::new();
    for (index, statement) in statements.iter().enumerate() {
        // An opening balance in a currency the account replaced isn't
        // compared; the closing one is, converted by the pipeline (D-105).
        if let Some(opening) = statement.opening_balance
            && statement.currency.code() == account.currency
        {
            let held = book.balance_on(account.id, opening.date)?;
            balances.push(balance_text(
                "opening",
                opening.date,
                opening.amount,
                held,
                false,
                account.kind,
                &writer,
                locale,
            )?);
        }
        if let Some(check) = report
            .balances
            .iter()
            .find(|check| check.statement == index)
        {
            let kept = stated.is_some_and(|stated| stated.on == check.date);
            balances.push(balance_text(
                "closing",
                check.date,
                check.bank,
                check.book,
                kept,
                account.kind,
                &writer,
                locale,
            )?);
        }
    }

    let summary = report.summary();
    Ok(ImportView {
        from_bank: loaded.fetched_on.is_some(),
        file_name: loaded
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        account_name: account.name,
        debt: account.kind == AccountKind::Liability,
        entries,
        imported: summary.imported,
        to_decide: summary.to_decide,
        duplicates: summary.duplicates,
        pending: summary.pending,
        rules: report.rules.changed.len(),
        transfers: report.transfers.len(),
        balances,
        new: report.imported(),
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "one balance line has this many parts"
)]
fn balance_text(
    which: &'static str,
    date: Date,
    bank: Money,
    held: Money,
    stated: bool,
    kind: AccountKind,
    writer: &AmountWriter<'_, impl Locale>,
    locale: &impl Locale,
) -> Result<BalanceText, StatementImportError> {
    let (bank_shown, bank_owes) = writer.balance(bank, kind);
    let (book_shown, book_owes) = writer.balance(held, kind);
    let difference = held
        .checked_sub(bank)
        .map_err(|_| StoreError::AmountOverflow)?;
    let book_higher = !difference.is_negative() && !difference.is_zero();
    let apart = if difference.is_negative() {
        difference
            .checked_neg()
            .map_err(|_| StoreError::AmountOverflow)?
    } else {
        difference
    };
    Ok(BalanceText {
        which,
        date: locale.short_date(date),
        check_amount: bank_shown.exact.clone(),
        bank: bank_shown,
        bank_owes,
        book: book_shown,
        book_owes,
        matches: bank == held,
        difference: writer.text(apart),
        book_higher,
        check_date: locale.short_date(date),
        stated,
    })
}

/// The Transfers the book suggests that involve one of the transactions
/// `new`, as the Transfer prompt shows them (D-120).
pub fn transfer_suggestions(
    book: &Book,
    new: &[TransactionId],
    locale: &impl Locale,
) -> Result<Vec<SuggestionText>, StatementImportError> {
    let accounts = book.all_accounts()?;
    book.transfer_suggestions()?
        .iter()
        .filter(|found| {
            new.contains(&found.outgoing.transaction) || new.contains(&found.incoming.transaction)
        })
        .map(|found| {
            Ok(transfer_prompt::suggestion_text(
                book, &accounts, found, locale,
            )?)
        })
        .collect()
}

/// The column screen's form (D-095), every field as text so QML holds no
/// numbers. Columns are a name from the header row or a number counted
/// from 1; an empty one means the file has no such column.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CsvForm {
    /// `utf-8`, `windows-1252`, `iso-8859-15`, or empty to tell from the file.
    pub encoding: String,
    /// The separator, or `tab`.
    pub delimiter: String,
    pub skip_lines: String,
    pub skip_last_lines: String,
    pub header: bool,
    /// `comma` or `point`.
    pub decimal_mark: String,
    /// `day-month-year`, `month-day-year` or `year-month-day`.
    pub date_order: String,
    pub booking_date: String,
    pub value_date: String,
    /// `signed`, `debit-credit` or `direction`.
    pub amount_mode: String,
    pub amount: String,
    pub debit: String,
    pub credit: String,
    pub direction: String,
    pub money_out: String,
    pub money_in: String,
    pub other_party_name: String,
    pub other_party_iban: String,
    pub purpose: String,
    pub purpose_2: String,
    pub status: String,
    /// The values meaning booked, separated by commas.
    pub booked: String,
    pub currency: String,
    pub account: String,
}

impl CsvForm {
    /// A form with the most common choices in German files: a semicolon, a
    /// decimal comma, day before month, a header row.
    pub fn usual() -> Self {
        Self {
            delimiter: ";".into(),
            skip_lines: "0".into(),
            skip_last_lines: "0".into(),
            header: true,
            decimal_mark: "comma".into(),
            date_order: "day-month-year".into(),
            amount_mode: "signed".into(),
            ..Self::default()
        }
    }

    /// The form for settings kept for a bank.
    pub fn from_settings(settings: &CsvSettings) -> Self {
        let column = |column: &Option<Column>| column.as_ref().map(column_text).unwrap_or_default();
        let mut form = Self {
            encoding: settings
                .encoding
                .map(|encoding| encoding.keyword().to_owned())
                .unwrap_or_default(),
            delimiter: if settings.delimiter == '\t' {
                "tab".into()
            } else {
                settings.delimiter.to_string()
            },
            skip_lines: settings.skip_lines.to_string(),
            skip_last_lines: settings.skip_last_lines.to_string(),
            header: settings.header,
            decimal_mark: match settings.decimal_mark {
                DecimalMark::Comma => "comma".into(),
                DecimalMark::Point => "point".into(),
            },
            date_order: match settings.date_order {
                DateOrder::DayMonthYear => "day-month-year".into(),
                DateOrder::MonthDayYear => "month-day-year".into(),
                DateOrder::YearMonthDay => "year-month-day".into(),
            },
            booking_date: column_text(&settings.booking_date),
            value_date: column(&settings.value_date),
            other_party_name: column(&settings.other_party_name),
            other_party_iban: column(&settings.other_party_iban),
            purpose: settings
                .remittance_information
                .first()
                .map(column_text)
                .unwrap_or_default(),
            purpose_2: settings
                .remittance_information
                .get(1)
                .map(column_text)
                .unwrap_or_default(),
            status: settings
                .status
                .as_ref()
                .map(|status| column_text(&status.column))
                .unwrap_or_default(),
            booked: settings
                .status
                .as_ref()
                .map(|status| status.booked.join(", "))
                .unwrap_or_default(),
            currency: column(&settings.currency),
            account: column(&settings.account),
            ..Self::default()
        };
        match &settings.amount {
            AmountColumns::Signed(amount) => {
                form.amount_mode = "signed".into();
                form.amount = column_text(amount);
            }
            AmountColumns::DebitCredit { debit, credit } => {
                form.amount_mode = "debit-credit".into();
                form.debit = column_text(debit);
                form.credit = column_text(credit);
            }
            AmountColumns::WithDirection {
                amount,
                direction,
                money_out,
                money_in,
            } => {
                form.amount_mode = "direction".into();
                form.amount = column_text(amount);
                form.direction = column_text(direction);
                form.money_out = money_out.clone();
                form.money_in = money_in.clone();
            }
        }
        form
    }

    /// The settings the form says, checked as the import crate checks them;
    /// `import.bad-settings` when a required field is empty or no number.
    pub fn settings(&self) -> Result<CsvSettings, StatementImportError> {
        let bad = || StatementImportError::Import(ImportError::BadSettings { line: None });
        let column = |text: &str| -> Result<Column, StatementImportError> {
            let text = text.trim();
            if text.is_empty() {
                return Err(bad());
            }
            match text.parse::<usize>() {
                Ok(number) => Ok(Column::Number(number)),
                Err(_) if self.header => Ok(Column::Name(text.to_owned())),
                Err(_) => Err(bad()),
            }
        };
        let optional = |text: &str| -> Result<Option<Column>, StatementImportError> {
            if text.trim().is_empty() {
                Ok(None)
            } else {
                column(text).map(Some)
            }
        };
        let count = |text: &str| -> Result<usize, StatementImportError> {
            let text = text.trim();
            if text.is_empty() {
                Ok(0)
            } else {
                text.parse().map_err(|_| bad())
            }
        };
        let delimiter = match self.delimiter.as_str() {
            "tab" => '\t',
            other => {
                let mut chars = other.chars();
                match (chars.next(), chars.next()) {
                    (Some(delimiter), None) => delimiter,
                    _ => return Err(bad()),
                }
            }
        };
        let decimal_mark = match self.decimal_mark.as_str() {
            "comma" => DecimalMark::Comma,
            "point" => DecimalMark::Point,
            _ => return Err(bad()),
        };
        let date_order = match self.date_order.as_str() {
            "day-month-year" => DateOrder::DayMonthYear,
            "month-day-year" => DateOrder::MonthDayYear,
            "year-month-day" => DateOrder::YearMonthDay,
            _ => return Err(bad()),
        };
        let amount = match self.amount_mode.as_str() {
            "signed" => AmountColumns::Signed(column(&self.amount)?),
            "debit-credit" => AmountColumns::DebitCredit {
                debit: column(&self.debit)?,
                credit: column(&self.credit)?,
            },
            "direction" => AmountColumns::WithDirection {
                amount: column(&self.amount)?,
                direction: column(&self.direction)?,
                money_out: self.money_out.trim().to_owned(),
                money_in: self.money_in.trim().to_owned(),
            },
            _ => return Err(bad()),
        };
        let mut settings = CsvSettings::new(
            delimiter,
            decimal_mark,
            date_order,
            column(&self.booking_date)?,
            amount,
        );
        settings.encoding = encoding(&self.encoding)?;
        settings.skip_lines = count(&self.skip_lines)?;
        settings.skip_last_lines = count(&self.skip_last_lines)?;
        settings.header = self.header;
        settings.value_date = optional(&self.value_date)?;
        settings.other_party_name = optional(&self.other_party_name)?;
        settings.other_party_iban = optional(&self.other_party_iban)?;
        settings.remittance_information = [&self.purpose, &self.purpose_2]
            .into_iter()
            .map(|text| optional(text))
            .filter_map(Result::transpose)
            .collect::<Result<_, _>>()?;
        settings.status = optional(&self.status)?.map(|column| StatusColumn {
            column,
            booked: self
                .booked
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect(),
        });
        settings.currency = optional(&self.currency)?;
        settings.account = optional(&self.account)?;
        settings.check()?;
        Ok(settings)
    }

    /// The first rows of `input` as the form's encoding, separator and
    /// skipped lines read them, so the user sees whether they fit and can
    /// pick columns from the header row.
    pub fn rows(&self, input: &[u8]) -> Result<Vec<Vec<String>>, StatementImportError> {
        let delimiter = match self.delimiter.as_str() {
            "tab" => '\t',
            other => other.chars().next().unwrap_or(';'),
        };
        let skip = self.skip_lines.trim().parse().unwrap_or(0);
        Ok(csv_preview(
            input,
            encoding(&self.encoding)?,
            delimiter,
            skip,
            CSV_PREVIEW_ROWS,
        )?)
    }
}

fn encoding(keyword: &str) -> Result<Option<TextEncoding>, StatementImportError> {
    Ok(match keyword.trim() {
        "" => None,
        "utf-8" => Some(TextEncoding::Utf8),
        "windows-1252" => Some(TextEncoding::Windows1252),
        "iso-8859-15" => Some(TextEncoding::Iso8859_15),
        _ => {
            return Err(StatementImportError::Import(ImportError::BadSettings {
                line: None,
            }));
        }
    })
}

fn column_text(column: &Column) -> String {
    match column {
        Column::Number(number) => number.to_string(),
        Column::Name(name) => name.clone(),
    }
}

/// The banks whose CSV settings the app keeps (D-111), in the order they
/// were saved.
pub fn csv_banks(settings: &Settings) -> Vec<String> {
    (1..)
        .map(|number| settings.text(CSV_BANKS, &format!("Name{number}")))
        .take_while(|name| !name.is_empty())
        .collect()
}

/// The CSV settings kept for the bank `name`, if any and still readable.
pub fn csv_bank(settings: &Settings, name: &str) -> Option<CsvSettings> {
    let number = csv_banks(settings).iter().position(|bank| bank == name)? + 1;
    CsvSettings::from_text(&settings.text(CSV_BANKS, &format!("Settings{number}"))).ok()
}

/// Keeps `csv` as the bank `name`'s settings, replacing those kept under
/// that name. An empty name keeps nothing.
pub fn save_csv_bank(settings: &Settings, name: &str, csv: &CsvSettings) -> bool {
    let name = name.trim();
    if name.is_empty() {
        return true;
    }
    let banks = csv_banks(settings);
    let number = banks
        .iter()
        .position(|bank| bank == name)
        .map_or(banks.len() + 1, |index| index + 1);
    settings.set_text(CSV_BANKS, &format!("Name{number}"), name)
        && settings.set_text(CSV_BANKS, &format!("Settings{number}"), &csv.to_text())
}

#[cfg(test)]
mod tests {
    use kassensturz_core::{
        AccountType, Actions, BookTo, Condition, CurrencyCode, MatchMode, Rule, RuleId, TextField,
        TextOperator,
    };
    use kassensturz_import::{Entry, StatementAccount};

    use super::*;
    use crate::accounts::{NewAccount, add_account};
    use crate::first_start::tests::started;
    use crate::register::tests::TestLocale;
    use crate::register::{Sort, register_view};
    use crate::test_folder::TestFolder;
    use crate::transaction_entry::{self, NewEntry};

    fn golden(format: &str, name: &str) -> (PathBuf, Vec<u8>) {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../kassensturz-import/tests/golden")
            .join(format)
            .join(name);
        let input = std::fs::read(&path).expect("golden file");
        (path, input)
    }

    fn account(
        book: &mut Book,
        name: &str,
        account_type: AccountType,
        balance: &str,
        date: &str,
    ) -> AccountId {
        add_account(
            book,
            &NewAccount {
                name,
                account_type,
                currency: "EUR",
                balance,
                date,
            },
            &TestLocale,
        )
        .expect("account added")
    }

    fn typed(
        book: &mut Book,
        account: AccountId,
        money_in: bool,
        amount: &str,
        other_party: &str,
        date: &str,
    ) -> TransactionId {
        transaction_entry::add(
            book,
            &NewEntry {
                account: &account.to_string(),
                money_in,
                amount,
                other_party,
                category: "",
                date,
            },
            &TestLocale,
        )
        .expect("transaction added")
    }

    fn rows(book: &Book, account: AccountId) -> usize {
        register_view(book, account, Sort::default(), &TestLocale)
            .expect("register")
            .rows
            .len()
    }

    fn code(error: &StatementImportError) -> &'static str {
        error_text(&started(), error, &TestLocale).code
    }

    fn eur() -> Currency {
        Currency::new(CurrencyCode::new("EUR").expect("code"), 2).expect("places")
    }

    fn entry(day: i8, cents: i64, other_party: &str) -> Entry {
        let mut entry = Entry::booked(
            Date::new(2026, 10, day).expect("a date"),
            Money::from_minor_units(cents, eur().code()),
        );
        entry.other_party_name = Some(other_party.into());
        entry
    }

    #[test]
    fn recognizes_formats_by_content_and_csv_files_by_name() {
        let id = |input: &[u8], name: &str| match recognize(input, name) {
            Ok(FileKind::Statements(importer)) => importer.info().id,
            Ok(FileKind::Csv) => "csv",
            Err(error) => code(&error),
        };
        let (_, camt) = golden("camt053", "statement-v08.xml");
        assert_eq!(id(&camt, "statement.xml"), "camt053");
        let (_, mt940) = golden("mt940", "german-bank.sta");
        assert_eq!(id(&mt940, "umsaetze.sta"), "mt940");
        let (_, ofx) = golden("ofx", "amounts.ofx");
        assert_eq!(id(&ofx, "download.ofx"), "ofx");
        let (_, csv) = golden("csv", "direction.csv");
        assert_eq!(id(&csv, "umsaetze.CSV"), "csv");
        assert_eq!(id(b"some notes", "notes.txt"), "csv");
        assert_eq!(id(b"%PDF-1.7 ...", "statement.pdf"), "input.unknown-format");
        let kmy = std::fs::read_dir(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../kassensturz-import/tests/golden/kmy"),
        )
        .expect("kmy samples")
        .filter_map(Result::ok)
        .find(|entry| entry.path().extension().is_some_and(|ext| ext == "kmy"))
        .expect("a .kmy sample");
        let book = std::fs::read(kmy.path()).expect("read");
        assert_eq!(id(&book, "household.kmy"), "input.whole-book");
        // The content wins over the name when it is recognized for sure.
        assert_eq!(id(&book, "household.csv"), "input.whole-book");
        assert_eq!(
            code(&StatementImportError::NoStatements),
            "input.no-statements"
        );
    }

    #[test]
    fn imports_a_credit_card_statement_and_keeps_the_banks_closing_balance() {
        let mut book = started();
        let card = account(
            &mut book,
            "Credit card",
            AccountType::CreditCard,
            "310,40",
            "2026-09-30",
        );
        let (path, input) = golden("camt053", "statement-v08.xml");
        let statements = read_statements(&Camt053, &input).expect("statements");
        let loaded = Loaded::new(card, path, statements);
        assert!(!loaded.needs_statement_choice());
        let last_change = book
            .change_to_undo()
            .expect("history")
            .map(|change| change.id);

        let shown = preview(&mut book, &loaded, &TestLocale).expect("preview");
        assert_eq!(shown.file_name, "statement-v08.xml");
        assert_eq!(shown.account_name, "Credit card");
        assert!(shown.debt);
        assert_eq!(
            (
                shown.imported,
                shown.to_decide,
                shown.duplicates,
                shown.pending
            ),
            (1, 0, 0, 1)
        );
        assert_eq!(shown.entries.len(), 2);
        let bought = &shown.entries[0];
        assert_eq!(bought.key, "0.0");
        assert_eq!(bought.date, "2026-10-12");
        assert_eq!(bought.other_party, "Example Bookshop");
        assert_eq!(bought.purpose, "RF18539007547034");
        assert_eq!(bought.amount.exact, "-35,25 €");
        assert!(!bought.incoming);
        assert_eq!(bought.outcome, "imported");
        assert_eq!(bought.category, "");
        assert_eq!(shown.entries[1].outcome, "pending");
        assert_eq!(shown.balances.len(), 2);
        let opening = &shown.balances[0];
        assert_eq!(opening.which, "opening");
        assert_eq!(opening.date, "2026-09-30");
        assert_eq!(opening.bank.exact, "310,40 €");
        assert!(opening.bank_owes);
        assert_eq!(opening.book.exact, "310,40 €");
        assert!(opening.matches);
        assert!(!opening.stated);
        let closing = &shown.balances[1];
        assert_eq!(closing.which, "closing");
        assert_eq!(closing.date, "2026-10-31");
        assert_eq!(closing.bank.exact, "345,65 €");
        assert_eq!(closing.book.exact, "345,65 €");
        assert!(closing.matches);
        assert!(closing.stated, "a matching closing balance is kept");
        assert_eq!(closing.check_amount, "345,65 €");
        assert_eq!(closing.check_date, "2026-10-31");
        // The preview changed nothing.
        assert_eq!(rows(&book, card), 1);
        let stated = book.stated_balance(card).expect("stated").expect("set");
        assert_eq!(stated.on, Date::new(2026, 9, 30).expect("a date"));
        assert_eq!(
            book.change_to_undo()
                .expect("history")
                .map(|change| change.id),
            last_change,
            "a preview isn't in the history"
        );

        let done = import(&mut book, &loaded, &TestLocale).expect("imported");
        assert_eq!(done.imported, 1);
        assert_eq!(done.new.len(), 1);
        assert_eq!(rows(&book, card), 2);
        let stated = book.stated_balance(card).expect("stated").expect("set");
        assert_eq!(stated.on, Date::new(2026, 10, 31).expect("a date"));
        assert_eq!(stated.amount.minor_units(), -34_565);
        let change = book.change_to_undo().expect("history").expect("one change");
        assert_eq!(change.action, Action::ImportStatement);
        // Undo takes the import and the bank's balance back together.
        book.undo().expect("undone");
        assert_eq!(rows(&book, card), 1);
        let stated = book.stated_balance(card).expect("stated").expect("set");
        assert_eq!(stated.on, Date::new(2026, 9, 30).expect("a date"));
    }

    /// Never asked for a PIN: the session already has it.
    struct NoQuestions;

    impl kassensturz_bank::PinAsker for NoQuestions {
        fn pin(
            &mut self,
            _login: &kassensturz_bank::Login,
            _rejected: bool,
        ) -> Option<(kassensturz_bank::Pin, kassensturz_bank::PinMode)> {
            panic!("the PIN was remembered, so nothing should be asked");
        }
    }

    /// What the fetch screen does for a linked account, with the fake bank:
    /// the statement the bank crate builds from its fetch, as `Loaded`.
    fn fetched_for(book: &mut Book, account: AccountId, day: Date) -> Loaded {
        use kassensturz_bank::{
            Bank, FakeBank, LoginId, MemoryPinStore, Pin, PinMode, PinSession, fetch_linked,
        };
        let mut bank = FakeBank::new();
        let mut pins = PinSession::new(Box::new(MemoryPinStore::new()));
        pins.provide(
            &LoginId::pending(),
            Pin::new(FakeBank::PIN.to_owned()),
            PinMode::RememberUntilClose,
        )
        .expect("provided");
        bank.add_login(&FakeBank::setup(), &mut pins)
            .expect("login added");
        book.link_bank_account(&kassensturz_core::BankLink {
            account,
            iban: Some("DE89370400440532013000".to_owned()),
            bank_code: None,
            number: None,
            last_fetched_on: None,
        })
        .expect("linked");
        let link = book.bank_link(account).expect("read").expect("linked");
        let statement = fetch_linked(
            &mut bank,
            &mut pins,
            &mut NoQuestions,
            &link,
            &Limits::default(),
        )
        .expect("a statement");
        Loaded::from_bank(account, statement, day)
    }

    #[test]
    fn a_statement_fetched_from_the_bank_is_imported_like_a_file_and_notes_its_day() {
        let mut book = started();
        let checking = account(
            &mut book,
            "Checking",
            AccountType::Checking,
            "1000,00",
            "2026-09-30",
        );
        let day = Date::new(2026, 10, 9).expect("a date");
        let loaded = fetched_for(&mut book, checking, day);
        assert!(!loaded.needs_statement_choice());

        let shown = preview(&mut book, &loaded, &TestLocale).expect("preview");
        assert!(shown.from_bank);
        assert_eq!(shown.file_name, "");
        assert_eq!(
            (
                shown.imported,
                shown.to_decide,
                shown.duplicates,
                shown.pending
            ),
            (3, 0, 0, 1)
        );
        // The bank's balance is checked against the book's (D-029): they
        // agree, since the account started with what the bank had before
        // October, and it is kept as what the bank said (D-123).
        let closing = shown.balances.last().expect("a closing balance");
        assert_eq!(closing.which, "closing");
        assert_eq!(closing.bank.exact, "3254,77 €");
        assert!(closing.matches && closing.stated, "{closing:?}");
        assert_eq!(rows(&book, checking), 1);
        let link = book.bank_link(checking).expect("read").expect("linked");
        assert_eq!(link.last_fetched_on, None, "a preview notes nothing");

        let done = import(&mut book, &loaded, &TestLocale).expect("imported");
        assert_eq!(done.imported, 3);
        assert_eq!(rows(&book, checking), 4);
        let link = book.bank_link(checking).expect("read").expect("linked");
        assert_eq!(link.last_fetched_on, Some(day));
        // One undo takes back the transactions, the bank's balance and the
        // day together.
        book.undo().expect("undone");
        assert_eq!(rows(&book, checking), 1);
        let link = book.bank_link(checking).expect("read").expect("linked");
        assert_eq!(link.last_fetched_on, None);
    }

    #[test]
    fn a_fetched_statement_for_an_account_that_lost_its_link_imports_nothing() {
        let mut book = started();
        let checking = account(
            &mut book,
            "Checking",
            AccountType::Checking,
            "1000,00",
            "2026-09-30",
        );
        let day = Date::new(2026, 10, 9).expect("a date");
        let loaded = fetched_for(&mut book, checking, day);
        book.unlink_bank_account(checking).expect("unlinked");
        let before = rows(&book, checking);
        let error = import(&mut book, &loaded, &TestLocale).expect_err("refused");
        assert_eq!(code(&error), "store.not-linked");
        assert_eq!(rows(&book, checking), before, "all or nothing");
    }

    #[test]
    fn a_statement_is_chosen_and_a_similar_entry_waits_for_the_user() {
        let mut book = started();
        let checking = account(
            &mut book,
            "Checking account",
            AccountType::Checking,
            "1000",
            "2026-10-01",
        );
        typed(&mut book, checking, true, "5", "Refund", "2026-10-02");
        let (path, input) = golden("mt940", "two-accounts.sta");
        let statements = read_statements(&Mt940, &input).expect("statements");
        let mut loaded = Loaded::new(checking, path, statements);
        assert!(loaded.needs_statement_choice());
        let choices = statement_choices(&book, &loaded, &TestLocale);
        assert_eq!(choices.len(), 2);
        assert_eq!(choices[0].account, "DE89370400440532013000");
        assert_eq!(choices[0].entries, 1);
        assert_eq!(choices[0].first_date, "2026-10-02");
        assert_eq!(
            choices[0].closing.as_ref().map(|c| c.exact.as_str()),
            Some("15,00 €")
        );
        assert_eq!(choices[0].closing_date, "2026-10-02");
        assert_eq!(choices[1].account, "GB82WEST12345698765432");
        assert_eq!(
            choices[1].closing.as_ref().map(|c| c.exact.as_str()),
            Some("-2,50 €")
        );
        assert_eq!(
            code(&loaded.choose_statement(2).expect_err("no third statement")),
            "input.no-such-entry"
        );
        loaded.choose_statement(0).expect("chosen");

        let shown = preview(&mut book, &loaded, &TestLocale).expect("preview");
        assert_eq!((shown.imported, shown.to_decide), (0, 1));
        let waiting = &shown.entries[0];
        assert_eq!(waiting.outcome, "similar");
        assert!(!waiting.import_anyway);
        assert_eq!(waiting.similar.len(), 1);
        assert_eq!(waiting.similar[0].other_party, "Refund");
        assert_eq!(waiting.similar[0].date, "2026-10-02");
        assert_eq!(waiting.similar[0].amount.exact, "5,00 €");
        let opening = &shown.balances[0];
        assert_eq!(
            (opening.bank.exact.as_str(), opening.book.exact.as_str()),
            ("10,00 €", "1000,00 €")
        );
        assert!(!opening.matches);
        assert!(opening.book_higher);
        assert_eq!(opening.difference.exact, "990,00 €");
        assert_eq!(opening.check_amount, "10,00 €");
        assert_eq!(opening.check_date, "2026-10-01");
        let closing = &shown.balances[1];
        assert_eq!(
            (closing.bank.exact.as_str(), closing.book.exact.as_str()),
            ("15,00 €", "1005,00 €")
        );
        assert!(!closing.stated);

        assert_eq!(
            code(
                &loaded
                    .set_import_anyway("1.0", true)
                    .expect_err("not chosen")
            ),
            "input.no-such-entry"
        );
        assert_eq!(
            code(&loaded.set_import_anyway("x", true).expect_err("no key")),
            "input.no-such-entry"
        );
        loaded.set_import_anyway("0.0", true).expect("chosen");
        let shown = preview(&mut book, &loaded, &TestLocale).expect("preview");
        assert_eq!((shown.imported, shown.to_decide), (1, 0));
        assert_eq!(shown.entries[0].outcome, "imported");
        assert!(shown.entries[0].import_anyway);
        assert_eq!(
            shown.entries[0].similar.len(),
            1,
            "the lookalike is still shown"
        );
        assert_eq!(shown.balances[1].book.exact, "1010,00 €");
        loaded
            .set_import_anyway("0.0", false)
            .expect("left out again");
        let shown = preview(&mut book, &loaded, &TestLocale).expect("preview");
        assert_eq!(shown.entries[0].outcome, "similar");
        loaded.set_import_anyway("0.0", true).expect("chosen");

        let done = import(&mut book, &loaded, &TestLocale).expect("imported");
        assert_eq!(done.imported, 1);
        assert_eq!(rows(&book, checking), 3);
        // A closing balance that differs is never kept (D-111).
        let stated = book.stated_balance(checking).expect("stated").expect("set");
        assert_eq!(stated.on, Date::new(2026, 10, 1).expect("a date"));
        assert_eq!(stated.amount.minor_units(), 100_000);
    }

    #[test]
    fn rules_book_the_new_transactions_and_their_transfers_are_suggested() {
        let mut book = started();
        let checking = account(
            &mut book,
            "Checking account",
            AccountType::Checking,
            "1000",
            "2026-10-01",
        );
        let savings = account(
            &mut book,
            "Savings",
            AccountType::Savings,
            "500",
            "2026-10-01",
        );
        let groceries = book
            .categories()
            .expect("categories")
            .into_iter()
            .find(|entry| entry.path.join(" › ") == "Food › Groceries")
            .expect("Groceries")
            .category
            .id;
        book.add_rule(
            &Rule {
                id: RuleId::generate(),
                name: "Shops".into(),
                match_mode: MatchMode::All,
                conditions: vec![Condition::Text {
                    field: TextField::OtherPartyName,
                    operator: TextOperator::Contains,
                    value: "bookshop".into(),
                    case_sensitive: false,
                }],
                actions: Actions {
                    book_to: Some(BookTo::Category(groceries)),
                    payee: None,
                },
            },
            None,
        )
        .expect("rule added");
        let arrived = typed(
            &mut book,
            savings,
            true,
            "100",
            "From checking",
            "2026-10-07",
        );
        let statement = Statement {
            account: StatementAccount::default(),
            currency: eur(),
            opening_balance: None,
            closing_balance: None,
            entries: vec![
                entry(5, -3525, "Example Bookshop"),
                entry(6, -10_000, "Own transfer"),
            ],
        };
        let loaded = Loaded::new(checking, PathBuf::from("october.sta"), vec![statement]);

        let shown = preview(&mut book, &loaded, &TestLocale).expect("preview");
        assert_eq!(shown.imported, 2);
        assert_eq!(shown.entries[0].category, "Groceries");
        assert_eq!(shown.entries[1].category, "");
        assert_eq!((shown.rules, shown.transfers), (1, 1));
        assert!(shown.balances.is_empty());
        assert_eq!(rows(&book, checking), 1, "a preview changes nothing");

        let done = import(&mut book, &loaded, &TestLocale).expect("imported");
        assert_eq!(rows(&book, checking), 3);
        let found = transfer_suggestions(&book, &done.new, &TestLocale).expect("suggestions");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].outgoing.id, done.new[1].to_string());
        assert_eq!(found[0].outgoing.account, "Checking account");
        assert_eq!(found[0].incoming.id, arrived.to_string());
        assert_eq!(found[0].incoming.account, "Savings");
        assert_eq!(found[0].amount.exact, "100,00 €");
        assert!(
            transfer_suggestions(&book, &[], &TestLocale)
                .expect("none")
                .is_empty()
        );
        // The Rule's mark survives: the register shows the category.
        let register =
            register_view(&book, checking, Sort::default(), &TestLocale).expect("register");
        assert!(register.rows.iter().any(|row| row.category == "Groceries"));
    }

    #[test]
    fn the_csv_form_round_trips_the_golden_settings_and_reads_a_file() {
        let folder =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../kassensturz-import/tests/golden/csv");
        let mut round_trips = 0;
        for entry in std::fs::read_dir(&folder)
            .expect("golden csv files")
            .filter_map(Result::ok)
        {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "settings") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("settings file");
            let Ok(settings) = CsvSettings::from_text(&text) else {
                continue;
            };
            if settings.remittance_information.len() > 2 {
                continue;
            }
            let form = CsvForm::from_settings(&settings);
            assert_eq!(
                form.settings().expect("settings from the form"),
                settings,
                "{}",
                path.display()
            );
            round_trips += 1;
        }
        assert!(
            round_trips >= 5,
            "{round_trips} settings files round-tripped"
        );

        let mut book = started();
        // Not 1000: the file's refund of 1000 two business days after the
        // starting balance would otherwise look like it (D-092).
        let checking = account(
            &mut book,
            "Checking account",
            AccountType::Checking,
            "1234",
            "2026-09-30",
        );
        let (path, input) = golden("csv", "direction.csv");
        let text = std::fs::read_to_string(path.with_extension("csv.settings")).expect("settings");
        let settings = CsvSettings::from_text(&text).expect("golden settings");
        let form = CsvForm::from_settings(&settings);
        assert_eq!(form.delimiter, "tab");
        assert_eq!(form.encoding, "iso-8859-15");
        assert_eq!(form.amount_mode, "direction");
        assert_eq!(
            (form.money_out.as_str(), form.money_in.as_str()),
            ("S", "H")
        );
        let rows = form.rows(&input).expect("rows");
        assert_eq!(rows[0], ["Datum", "Name", "Text", "Umsatz", "S/H"]);
        assert_eq!(rows[1][1], "Škoda Händler");
        let statements = read_csv(&book, checking, &input, &settings).expect("read");
        let expected = read_with(
            &Csv::new(settings.clone(), eur()).expect("importer"),
            &input,
            &Limits::default(),
        )
        .expect("the importer's own reading");
        assert_eq!(Imported::Statements(statements.clone()), expected);
        let loaded = Loaded::new(checking, path, statements);
        let shown = preview(&mut book, &loaded, &TestLocale).expect("preview");
        assert_eq!(shown.imported, 3);
        assert_eq!(shown.entries[0].amount.exact, "-199,00 €");
        assert_eq!(shown.entries[1].amount.exact, "1000,00 €");
        assert!(shown.balances.is_empty(), "a CSV file reports no balances");

        // Wrong forms are refused before the file is read.
        let mut wrong = form.clone();
        wrong.booking_date = String::new();
        assert_eq!(
            code(&wrong.settings().expect_err("no date column")),
            "import.bad-settings"
        );
        let mut wrong = form.clone();
        wrong.header = false;
        assert_eq!(
            code(&wrong.settings().expect_err("names need a header")),
            "import.bad-settings"
        );
        let mut wrong = form.clone();
        wrong.delimiter = "ab".into();
        assert_eq!(
            code(&wrong.settings().expect_err("one character")),
            "import.bad-settings"
        );
        let mut wrong = form.clone();
        wrong.encoding = "latin-9".into();
        assert_eq!(
            code(&wrong.settings().expect_err("unknown encoding")),
            "import.bad-settings"
        );
        let mut numbers = CsvForm::usual();
        numbers.header = false;
        numbers.booking_date = "1".into();
        numbers.amount = "4".into();
        let by_number = numbers.settings().expect("columns by number");
        assert_eq!(by_number.booking_date, Column::Number(1));
        assert_eq!(by_number.amount, AmountColumns::Signed(Column::Number(4)));
    }

    #[test]
    fn the_banks_csv_settings_are_kept_by_name_in_the_app_settings() {
        let folder = TestFolder::new();
        let settings = Settings::at(&folder.join("kassensturzrc"));
        assert!(csv_banks(&settings).is_empty());
        let text = std::fs::read_to_string(
            golden("csv", "direction.csv")
                .0
                .with_extension("csv.settings"),
        )
        .expect("settings");
        let direction = CsvSettings::from_text(&text).expect("golden settings");
        assert!(
            save_csv_bank(&settings, "", &direction),
            "an empty name keeps nothing"
        );
        assert!(csv_banks(&settings).is_empty());
        assert!(save_csv_bank(&settings, " Example Bank ", &direction));
        assert_eq!(csv_banks(&settings), ["Example Bank"]);
        assert_eq!(csv_bank(&settings, "Example Bank"), Some(direction.clone()));
        assert_eq!(csv_bank(&settings, "Other Bank"), None);

        let mut changed = direction.clone();
        changed.skip_lines = 3;
        assert!(save_csv_bank(&settings, "Example Bank", &changed));
        assert_eq!(csv_banks(&settings), ["Example Bank"]);
        assert_eq!(csv_bank(&settings, "Example Bank"), Some(changed));
        let simple = CsvForm::usual();
        let mut simple = simple;
        simple.booking_date = "Buchungstag".into();
        simple.amount = "Betrag".into();
        let simple = simple.settings().expect("settings");
        assert!(save_csv_bank(&settings, "Second Bank", &simple));
        assert_eq!(csv_banks(&settings), ["Example Bank", "Second Bank"]);
        // Read back by another Settings on the same file, as the next start does.
        let again = Settings::at(&folder.join("kassensturzrc"));
        assert_eq!(csv_bank(&again, "Second Bank"), Some(simple));
    }
}
