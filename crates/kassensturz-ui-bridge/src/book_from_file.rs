// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Starting a book from a file, as the screens see it (M3.12, D-100, D-101,
//! D-103, D-106, D-125): a complete export that KassenSturz wrote, or a
//! book that KMyMoney kept, as a `.kmy` file or as SQLite.
//!
//! The file is read first ([`read`]), as untrusted input, and the user sees
//! what it holds and answers what only they can answer: which current
//! currency replaced one that KassenSturz's tables don't list, at what rate
//! and from which day (D-106). Only then is a book filled ([`fill`]), either
//! the open book if nothing is in it yet, or a new one, so a file that is
//! refused leaves nothing behind. Everything reaches QML as text (D-065).

use std::collections::HashMap;
use std::path::Path;

use kassensturz_core::{CurrencyCode, Date, ExchangeRate, Report};
use kassensturz_export::{BookData, ExportError, read_book_file};
use kassensturz_import::{
    Camt053, ImportError, Imported, ImportedBook, Importer, Kmy, KmySqlite, Limits, Mt940, Ofx,
    RECOGNITION_BYTES, Recognition, Replacement, UnlistedCurrency, import, read_file,
};
use kassensturz_pipeline::{
    BookImportReport, PipelineError, RuleScope, fill_book, fill_book_from_with,
};
use kassensturz_store::{Book, StoreError};

use crate::errors::{ErrorText, report_text};
use crate::format::Locale;
use crate::parse::{RateInputError, read_rate};

/// What can go wrong on the screens that start a book from a file.
#[derive(Debug)]
pub enum FromFileError {
    /// The file is a KassenSturz book, which is opened, not started from.
    IsABook,
    /// The file is a bank's statement, which is imported into an account.
    IsAStatement,
    /// The step meant isn't there (anymore): no file read, or an answer for
    /// a currency the file doesn't need one for.
    NoSuchEntry,
    /// What the user typed for a currency that replaced another isn't usable.
    Replacement {
        /// The currency that was replaced.
        currency: String,
        /// Which field.
        field: Field,
        /// What is wrong with it.
        error: ReplacementError,
    },
    /// The file couldn't be read as another program's book.
    Import(ImportError),
    /// The file couldn't be read as a complete export.
    Export(ExportError),
    /// The book refused what the file holds.
    Store(StoreError),
}

/// The fields of the question about a currency that replaced another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    /// The currency that replaced it.
    Successor,
    /// How many of the old currency one of the new was worth.
    Rate,
    /// The day it took effect.
    Date,
}

impl Field {
    /// The word QML uses for the field.
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Successor => "successor",
            Self::Rate => "rate",
            Self::Date => "date",
        }
    }
}

/// What is wrong with an answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplacementError {
    /// Nothing was chosen or typed.
    Missing,
    /// Not a currency code, or a date or rate that isn't one.
    Malformed,
    /// A rate: why it isn't one.
    Rate(RateInputError),
}

impl From<ImportError> for FromFileError {
    fn from(error: ImportError) -> Self {
        Self::Import(error)
    }
}

impl From<ExportError> for FromFileError {
    fn from(error: ExportError) -> Self {
        Self::Export(error)
    }
}

impl From<StoreError> for FromFileError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<PipelineError> for FromFileError {
    fn from(error: PipelineError) -> Self {
        match error {
            PipelineError::Import(error) => Self::Import(error),
            PipelineError::Store(error) => Self::Store(error),
        }
    }
}

/// The error as the UI gets it: the app's own codes start with `input.`, the
/// import crate's, the export crate's and the store's are those of
/// `docs/ERROR_CODES.md`. An answer about a currency names it (`currency`)
/// and the field (`field`).
#[cfg(test)]
fn error_text(book: &Book, error: &FromFileError, locale: &impl Locale) -> ErrorText {
    error_text_in(Some(book), error, locale)
}

/// The error as the UI gets it (see the tests' `error_text`), where there may
/// be no open book: a book that was refused at the start leaves none, yet a
/// file can still be started from, and its errors are told the same (a
/// throwaway book in memory only reads the currencies' decimals for them).
pub fn error_text_in(
    book: Option<&Book>,
    error: &FromFileError,
    locale: &impl Locale,
) -> ErrorText {
    let Some(book) = book else {
        return match Book::create_in_memory() {
            Ok(scratch) => error_text_in(Some(&scratch), error, locale),
            Err(_) => ErrorText::plain("input.no-such-entry"),
        };
    };
    match error {
        FromFileError::IsABook => ErrorText::plain("input.is-a-book"),
        FromFileError::IsAStatement => ErrorText::plain("input.is-a-statement"),
        FromFileError::NoSuchEntry => ErrorText::plain("input.no-such-entry"),
        FromFileError::Replacement {
            currency,
            field,
            error,
        } => {
            let (code, mut params) = match error {
                ReplacementError::Missing => match field {
                    Field::Successor => ("input.replacement-missing", Vec::new()),
                    Field::Rate => ("input.empty-rate", Vec::new()),
                    Field::Date => ("input.empty-date", Vec::new()),
                },
                ReplacementError::Malformed => match field {
                    Field::Date => (
                        "input.malformed-date",
                        vec![("example", locale.short_date(locale.today()))],
                    ),
                    _ => ("input.replacement-missing", Vec::new()),
                },
                ReplacementError::Rate(RateInputError::Empty) => ("input.empty-rate", Vec::new()),
                ReplacementError::Rate(RateInputError::Malformed) => (
                    "input.malformed-rate",
                    vec![("example", rate_example(locale))],
                ),
                ReplacementError::Rate(RateInputError::TooManyDecimalPlaces { max }) => (
                    "input.rate-too-many-decimal-places",
                    vec![("max", max.to_string())],
                ),
                ReplacementError::Rate(RateInputError::TooLarge) => {
                    ("input.rate-too-large", Vec::new())
                }
                ReplacementError::Rate(RateInputError::NotPositive) => {
                    ("input.rate-not-positive", Vec::new())
                }
            };
            params.push(("currency", currency.clone()));
            params.push(("field", field.keyword().to_owned()));
            ErrorText { code, params }
        }
        FromFileError::Import(error) => report_text(book, &error.report(), locale),
        FromFileError::Export(error) => report_text(book, &error.report(), locale),
        FromFileError::Store(error) => report_text(book, &error.report(), locale),
    }
}

/// A rate written the way the user should type it, e.g. "1.234,5678" in
/// German.
fn rate_example(locale: &impl Locale) -> String {
    format!(
        "{}{}{}{}5678",
        1,
        locale.group_separator(),
        234,
        locale.decimal_point()
    )
}

/// Whether the file is a complete export or another program's book.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// KassenSturz's own complete export (D-100).
    Export,
    /// KMyMoney's book, as a `.kmy` file or SQLite (D-101, D-103).
    Other,
}

impl Kind {
    /// The word QML uses.
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Export => "export",
            Self::Other => "kmymoney",
        }
    }
}

/// What a file holds, read and checked, waiting for the user's word.
#[derive(Debug)]
pub struct Prepared {
    /// The file's name, without its folder.
    pub file_name: String,
    /// The name for a book made from it: the file's name without its
    /// extension.
    pub stem: String,
    source: Source,
}

#[derive(Debug)]
enum Source {
    Export(Box<BookData>),
    Other(Box<ImportedBook>),
}

/// What the screen shows before anything is made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedView {
    /// The file's name.
    pub file_name: String,
    /// What kind of file it is.
    pub kind: Kind,
    /// How many accounts, categories and transactions it holds.
    pub accounts: usize,
    /// See `accounts`.
    pub categories: usize,
    /// See `accounts`.
    pub transactions: usize,
    /// The currencies KassenSturz's tables don't list that the file uses,
    /// each of which needs the user's word (D-106).
    pub unlisted: Vec<UnlistedView>,
    /// How many payees the file has a default category for but no matching
    /// text: by default they get a Rule that recognizes them by name, and
    /// the user can choose to leave them out (D-160).
    pub rules_by_name_only: usize,
}

/// A currency the file uses that KassenSturz's tables don't list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnlistedView {
    /// Its code.
    pub currency: String,
    /// What the file's own rates suggest replaced it, to check first, never
    /// to use by itself (D-106).
    pub suggestion: Option<SuggestionView>,
}

/// See [`UnlistedView::suggestion`], as text in the user's format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuggestionView {
    /// The current currency.
    pub successor: String,
    /// How many of the old currency one of the new was worth.
    pub rate: String,
    /// The day, as the user's format writes dates.
    pub date: String,
}

impl Prepared {
    /// What the screen shows.
    pub fn view(&self, locale: &impl Locale) -> PreparedView {
        match &self.source {
            Source::Export(data) => PreparedView {
                file_name: self.file_name.clone(),
                kind: Kind::Export,
                accounts: data
                    .accounts
                    .iter()
                    .filter(|account| account.kind.is_account())
                    .count(),
                categories: data
                    .accounts
                    .iter()
                    .filter(|account| account.kind.is_category())
                    .count(),
                transactions: data.transactions.len(),
                unlisted: Vec::new(),
                rules_by_name_only: 0,
            },
            Source::Other(book) => PreparedView {
                file_name: self.file_name.clone(),
                kind: Kind::Other,
                accounts: book
                    .accounts
                    .iter()
                    .filter(|account| account.kind.is_account())
                    .count(),
                categories: book
                    .accounts
                    .iter()
                    .filter(|account| account.kind.is_category())
                    .count(),
                transactions: book.transactions.len(),
                unlisted: book
                    .unlisted
                    .iter()
                    .map(|unlisted| unlisted_view(unlisted, locale))
                    .collect(),
                rules_by_name_only: book
                    .rules
                    .iter()
                    .filter(|rule| rule.conditions.is_empty())
                    .count(),
            },
        }
    }

    /// The currencies the user must say something about.
    pub fn unlisted(&self) -> Vec<CurrencyCode> {
        match &self.source {
            Source::Export(_) => Vec::new(),
            Source::Other(book) => book
                .unlisted
                .iter()
                .map(|unlisted| unlisted.currency.code())
                .collect(),
        }
    }
}

fn unlisted_view(unlisted: &UnlistedCurrency, locale: &impl Locale) -> UnlistedView {
    UnlistedView {
        currency: unlisted.currency.code().to_string(),
        suggestion: unlisted.suggestion.map(|suggestion| SuggestionView {
            successor: suggestion.successor.to_string(),
            rate: rate_text(&suggestion.rate, locale),
            date: locale.short_date(suggestion.date),
        }),
    }
}

/// `rate` in the user's format: the plain form `1.95583` with the locale's
/// decimal point and digit grouping.
fn rate_text(rate: &ExchangeRate, locale: &impl Locale) -> String {
    let plain = rate.to_string();
    let grouped = |whole: &str| {
        whole
            .parse::<u64>()
            .map_or_else(|_| whole.to_owned(), |value| locale.group_digits(value))
    };
    match plain.split_once('.') {
        Some((whole, fraction)) => {
            format!("{}{}{fraction}", grouped(whole), locale.decimal_point())
        }
        None => grouped(&plain),
    }
}

/// The formats a bank's statements come in: recognized only to say that
/// such a file is imported into an account, not started a book from.
const STATEMENTS: &[&dyn Importer] = &[&Camt053, &Mt940, &Ofx];

/// Reads the file at `path`, judging it from its start (D-091): a
/// KMyMoney book for sure, a SQLite file that may be one, or else a complete
/// export. A KassenSturz book and a bank's statement are told apart, so the
/// message can say where they belong.
pub fn read(path: &Path) -> Result<Prepared, FromFileError> {
    let name = path.file_name().map(|name| name.to_string_lossy());
    let file_name = name.as_deref().unwrap_or_default().to_owned();
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase());
    if extension.as_deref() == Some(crate::books::EXTENSION) {
        return Err(FromFileError::IsABook);
    }
    let limits = Limits::default();
    let start = start_of(path);
    let importer: Option<&dyn Importer> = if Kmy.recognize(&start) == Recognition::Yes {
        Some(&Kmy)
    } else if KmySqlite.recognize(&start) != Recognition::No {
        // Any SQLite file could be KMyMoney's; neither a complete export nor
        // a KassenSturz book is one.
        Some(&KmySqlite)
    } else {
        None
    };
    let source = if let Some(importer) = importer {
        let input = read_file(path, &limits)?;
        let Imported::Book(book) = import(importer, &input, &limits)? else {
            unreachable!("the KMyMoney importers read a book")
        };
        Source::Other(Box::new(book))
    } else if STATEMENTS
        .iter()
        .any(|importer| importer.recognize(&start) == Recognition::Yes)
    {
        return Err(FromFileError::IsAStatement);
    } else {
        Source::Export(Box::new(read_book_file(path)?))
    };
    Ok(Prepared {
        file_name,
        stem,
        source,
    })
}

/// The first bytes of the file, for recognizing its format. If it can't be
/// read, none: reading it then says why.
fn start_of(path: &Path) -> Vec<u8> {
    use std::io::Read;
    let mut start = Vec::with_capacity(RECOGNITION_BYTES);
    let read = std::fs::File::open(path)
        .and_then(|file| file.take(RECOGNITION_BYTES as u64).read_to_end(&mut start));
    if read.is_err() {
        start.clear();
    }
    start
}

/// What the user chose for one currency: the code of the one that replaced
/// it, how many of the old one a unit of it was worth, and the day, all as
/// typed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Answer {
    /// The currency that was replaced.
    pub currency: String,
    /// The one that replaced it.
    pub successor: String,
    /// The rate, in the user's format.
    pub rate: String,
    /// The day it took effect, in the user's format.
    pub date: String,
}

/// Turns the answers into what the pipeline takes: one replacement for each
/// currency the file needs one for, and none for any other.
pub fn replacements(
    prepared: &Prepared,
    answers: &[Answer],
    locale: &impl Locale,
) -> Result<HashMap<CurrencyCode, Replacement>, FromFileError> {
    let mut replaced = HashMap::new();
    for code in prepared.unlisted() {
        let answer = answers
            .iter()
            .find(|answer| answer.currency == code.as_str())
            .ok_or_else(|| FromFileError::Replacement {
                currency: code.to_string(),
                field: Field::Successor,
                error: ReplacementError::Missing,
            })?;
        let wrong = |field, error| FromFileError::Replacement {
            currency: code.to_string(),
            field,
            error,
        };
        let successor = answer.successor.trim();
        if successor.is_empty() {
            return Err(wrong(Field::Successor, ReplacementError::Missing));
        }
        let successor = CurrencyCode::new(successor)
            .map_err(|_| wrong(Field::Successor, ReplacementError::Malformed))?;
        let rate = read_rate(
            &answer.rate,
            &locale.decimal_point(),
            &locale.group_separator(),
        )
        .map_err(|error| wrong(Field::Rate, ReplacementError::Rate(error)))?;
        let date: Date = if answer.date.trim().is_empty() {
            return Err(wrong(Field::Date, ReplacementError::Missing));
        } else {
            locale
                .read_date(answer.date.trim())
                .ok_or_else(|| wrong(Field::Date, ReplacementError::Malformed))?
        };
        replaced.insert(
            code,
            Replacement {
                successor,
                rate,
                date,
            },
        );
    }
    Ok(replaced)
}

/// What filling a book did, as text and counts for the last screen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Created {
    /// How many accounts and categories the book has now.
    pub accounts: usize,
    /// See `accounts`.
    pub categories: usize,
    /// How many transactions.
    pub transactions: usize,
    /// How many exchange rates came with it.
    pub rates: usize,
    /// How many transactions have no category yet, as in the file (D-101).
    pub uncategorized: usize,
    /// How many didn't add up in the file and got a part without a category
    /// for the rest.
    pub completed: usize,
    /// Accounts and categories that got a number after their name, because
    /// it was taken: the new name.
    pub renamed: Vec<String>,
    /// Categories now at the top level, because they sat inside one in
    /// another currency.
    pub moved_to_top: Vec<String>,
    /// Accounts kept open although the file had them closed, as money is
    /// still in them.
    pub left_open: Vec<String>,
    /// Currencies that were converted into the one that replaced them.
    pub converted: Vec<ConvertedText>,
    /// What the file held that a book can't yet: the keyword of each kind
    /// and how many (D-029).
    pub skipped: Vec<(&'static str, usize)>,
    /// How many pairs of transactions look like Transfers between the
    /// accounts (D-087).
    pub transfers: usize,
    /// How many Rules the file's payee recognition became (D-160).
    pub rules: usize,
    /// Matching texts KassenSturz can't use (D-160).
    pub patterns_left_out: Vec<PatternText>,
}

/// A matching text of a payee that couldn't become part of a Rule (D-160).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatternText {
    /// The payee.
    pub payee: String,
    /// The pattern, as the file has it.
    pub pattern: String,
    /// Whether the payee still got a Rule with its other patterns.
    pub rule_made: bool,
}

/// One currency converted into the one that replaced it (D-104, D-106).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConvertedText {
    /// The old currency.
    pub old: String,
    /// The one that replaced it.
    pub new: String,
    /// How many of the old one a unit of the new was worth, in the user's
    /// format.
    pub rate: String,
    /// The day it took effect.
    pub date: String,
    /// How many accounts and categories were converted.
    pub accounts: usize,
    /// How many parts.
    pub parts: usize,
}

/// Fills the empty `book` with what `prepared` holds, with `replaced` as
/// the user's word about currencies the tables don't list and `scope` as
/// their choice of which payees get Rules. If anything is refused, the book
/// stays empty.
pub fn fill(
    book: &mut Book,
    prepared: &Prepared,
    replaced: &HashMap<CurrencyCode, Replacement>,
    scope: RuleScope,
    locale: &impl Locale,
) -> Result<Created, FromFileError> {
    match &prepared.source {
        Source::Export(data) => {
            fill_book(book, data)?;
            Ok(Created {
                accounts: book.accounts(true)?.len(),
                categories: book.categories()?.len(),
                transactions: book.transactions()?.len(),
                rates: book.all_rates()?.len(),
                ..Created::default()
            })
        }
        Source::Other(imported) => {
            let report = fill_book_from_with(book, imported, replaced, scope)?;
            created_text(book, &report, locale)
        }
    }
}

fn created_text(
    book: &Book,
    report: &BookImportReport,
    locale: &impl Locale,
) -> Result<Created, FromFileError> {
    let name = |id| book.account(id).map(|account| account.name);
    Ok(Created {
        accounts: report.accounts,
        categories: report.categories,
        transactions: report.transactions,
        rates: report.rates,
        uncategorized: report.uncategorized,
        completed: report.completed,
        renamed: report
            .renamed
            .iter()
            .map(|(id, _)| name(*id))
            .collect::<Result<_, _>>()?,
        moved_to_top: report
            .moved_to_top
            .iter()
            .map(|id| name(*id))
            .collect::<Result<_, _>>()?,
        left_open: report
            .left_open
            .iter()
            .map(|id| name(*id))
            .collect::<Result<_, _>>()?,
        converted: report
            .converted
            .iter()
            .map(|converted| ConvertedText {
                old: converted.withdrawn.currency().code().to_string(),
                new: converted.withdrawn.successor().to_string(),
                rate: rate_text(&converted.withdrawn.rate(), locale),
                date: locale.short_date(converted.withdrawn.date()),
                accounts: converted.accounts,
                parts: converted.parts,
            })
            .collect(),
        skipped: report
            .skipped
            .iter()
            .map(|skipped| (skipped.what.keyword(), skipped.count))
            .collect(),
        transfers: report.transfers.len(),
        rules: report.rules,
        patterns_left_out: report
            .patterns_left_out
            .iter()
            .map(|left_out| PatternText {
                payee: left_out.payee.clone(),
                pattern: left_out.pattern.clone(),
                rule_made: left_out.rule_made,
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::export::{CsvForm, save_book, save_csv};
    use crate::first_start::tests::started;
    use crate::register::tests::TestLocale;
    use crate::register::{Sort, register_view};
    use crate::test_folder::TestFolder;

    fn golden(folder: &str, name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../kassensturz-import/tests/golden")
            .join(folder)
            .join(name)
    }

    fn empty() -> Book {
        Book::create_in_memory().expect("book in memory")
    }

    fn code(book: &Book, error: &FromFileError) -> String {
        error_text(book, error, &TestLocale).code.to_owned()
    }

    fn answer(currency: &str, successor: &str, rate: &str, date: &str) -> Answer {
        Answer {
            currency: currency.into(),
            successor: successor.into(),
            rate: rate.into(),
            date: date.into(),
        }
    }

    #[test]
    fn a_complete_export_of_the_app_comes_back_as_the_same_book() {
        let mut book = started();
        let folder = TestFolder::new();
        let path = folder.join("household.json");
        save_book(&book, &path).expect("saved");
        let prepared = read(&path).expect("read");
        let shown = prepared.view(&TestLocale);
        assert_eq!(shown.kind, Kind::Export);
        assert_eq!(shown.file_name, "household.json");
        assert_eq!(prepared.stem, "household");
        assert_eq!(
            shown.categories,
            book.categories().expect("categories").len()
        );
        assert!(shown.unlisted.is_empty());

        let mut copy = empty();
        assert!(copy.is_empty().expect("empty"));
        let created = fill(
            &mut copy,
            &prepared,
            &HashMap::new(),
            RuleScope::default(),
            &TestLocale,
        )
        .expect("filled");
        assert_eq!(created.categories, shown.categories);
        assert_eq!(created.transactions, shown.transactions);
        assert!(created.skipped.is_empty());
        assert!(!copy.is_empty().expect("filled now"));
        // The copy saves to the same bytes (D-100).
        let again = folder.join("again.json");
        save_book(&copy, &again).expect("saved again");
        assert_eq!(
            std::fs::read(&path).expect("first"),
            std::fs::read(&again).expect("second")
        );
        // A book with something in it refuses, and is left as it was.
        let error = fill(
            &mut book,
            &prepared,
            &HashMap::new(),
            RuleScope::default(),
            &TestLocale,
        )
        .expect_err("refused");
        assert_eq!(code(&book, &error), "store.book-not-empty");
    }

    #[test]
    fn a_kmymoney_book_shows_what_it_holds_and_what_it_leaves_out() {
        let path = golden("kmy", "kassensturz-sample.kmy");
        let prepared = read(&path).expect("read");
        let shown = prepared.view(&TestLocale);
        assert_eq!(shown.kind, Kind::Other);
        assert!(shown.accounts > 0 && shown.categories > 0 && shown.transactions > 0);
        assert!(shown.unlisted.is_empty());

        let mut book = empty();
        let created = fill(
            &mut book,
            &prepared,
            &HashMap::new(),
            RuleScope::default(),
            &TestLocale,
        )
        .expect("filled");
        assert_eq!(created.accounts, shown.accounts);
        assert_eq!(created.categories, shown.categories);
        assert_eq!(created.transactions, shown.transactions);
        // The sample has what a book can't hold yet, said by keyword (D-029).
        assert!(!created.skipped.is_empty());
        assert!(
            created
                .skipped
                .iter()
                .all(|(what, count)| !what.is_empty() && *count > 0)
        );
        // The bank-imported entries have no category yet, as in the file.
        assert!(created.uncategorized > 0);
        // What came in is a book the screens read: every account has a register.
        for account in book.accounts(true).expect("accounts") {
            register_view(&book, account.id, Sort::default(), &TestLocale).expect("register");
        }
    }

    #[test]
    fn a_kmymoney_books_payee_recognition_comes_in_as_rules_and_says_what_it_couldnt() {
        let prepared = read(&golden("kmy", "payee-rules.kmy")).expect("read");
        let mut book = empty();
        let created = fill(
            &mut book,
            &prepared,
            &HashMap::new(),
            RuleScope::default(),
            &TestLocale,
        )
        .expect("filled");
        assert_eq!(created.rules, 5);
        assert_eq!(book.rules().expect("Rules").len(), 5);
        assert_eq!(
            created.patterns_left_out,
            vec![
                PatternText {
                    payee: "Corner Grocery".into(),
                    pattern: "^(?!x)Corner$".into(),
                    rule_made: true,
                },
                PatternText {
                    payee: "Only Unusable".into(),
                    pattern: "^(?=a).*$".into(),
                    rule_made: true,
                },
            ]
        );
        assert!(created.skipped.contains(&("payee-rules", 2)));
    }

    #[test]
    fn the_advanced_choice_makes_rules_only_for_payees_with_a_matching_text() {
        let prepared = read(&golden("kmy", "payee-rules.kmy")).expect("read");
        // The screen offers the choice and says how many it concerns.
        assert_eq!(prepared.view(&TestLocale).rules_by_name_only, 1);
        let mut book = empty();
        let created = fill(
            &mut book,
            &prepared,
            &HashMap::new(),
            RuleScope::WithMatchingText,
            &TestLocale,
        )
        .expect("filled");
        assert_eq!(created.rules, 3);
        assert_eq!(book.rules().expect("Rules").len(), 3);
        assert!(created.skipped.contains(&("payee-rules", 3)));
    }

    #[test]
    fn a_file_without_rules_to_choose_about_offers_no_choice() {
        let prepared = read(&golden("kmy", "kassensturz-sample.kmy")).expect("read");
        assert_eq!(prepared.view(&TestLocale).rules_by_name_only, 0);
    }

    #[test]
    fn kmymoneys_two_file_kinds_give_the_same_book() {
        let kmy = read(&golden("kmy", "kassensturz-sample.kmy")).expect("kmy");
        let sqlite = read(&golden("kmy-sqlite", "kassensturz-sample.sqlite")).expect("sqlite");
        assert_eq!(
            kmy.view(&TestLocale).accounts,
            sqlite.view(&TestLocale).accounts
        );
        assert_eq!(
            kmy.view(&TestLocale).transactions,
            sqlite.view(&TestLocale).transactions
        );
        assert_eq!(kmy.file_name, "kassensturz-sample.kmy");
        assert_eq!(sqlite.stem, "kassensturz-sample");
    }

    #[test]
    fn currencies_no_table_lists_need_the_users_word_and_are_converted_with_it() {
        let path = golden("kmy", "unlisted-currency.kmy");
        let prepared = read(&path).expect("read");
        let shown = prepared.view(&TestLocale);
        let codes: Vec<&str> = shown.unlisted.iter().map(|u| u.currency.as_str()).collect();
        assert_eq!(codes, ["BGL", "ROL", "TRL"]);
        // The file's own rates are a suggestion, written the user's way.
        assert_eq!(shown.unlisted[0].suggestion, None);
        assert_eq!(
            shown.unlisted[1].suggestion,
            Some(SuggestionView {
                successor: "RON".into(),
                // The tests' locale writes digits without grouping.
                rate: "10000".into(),
                date: "2005-06-30".into(),
            })
        );

        // Without the user's word nothing is made, and the question is named.
        let book = empty();
        let error = replacements(&prepared, &[], &TestLocale).expect_err("asked");
        assert_eq!(code(&book, &error), "input.replacement-missing");
        let unanswered = [
            answer("BGL", "EUR", "1955,83", "2026-01-01"),
            answer("ROL", "RON", "10.000", "2005-06-30"),
        ];
        let error = replacements(&prepared, &unanswered, &TestLocale).expect_err("TRL asked");
        let text = error_text(&book, &error, &TestLocale);
        assert_eq!(text.code, "input.replacement-missing");
        assert!(text.params.contains(&("currency", "TRL".to_owned())));

        // Each wrong answer names its field and currency.
        let wrong = |rate: &str, date: &str| {
            let answers = [
                answer("BGL", "EUR", "1955,83", "2026-01-01"),
                answer("ROL", "RON", rate, date),
                answer("TRL", "TRY", "1.000.000", "2005-01-01"),
            ];
            let error = replacements(&prepared, &answers, &TestLocale).expect_err("refused");
            let text = error_text(&book, &error, &TestLocale);
            (text.code, text.params)
        };
        assert_eq!(wrong("10.5", "2005-06-30").0, "input.malformed-rate");
        assert_eq!(wrong("", "2005-06-30").0, "input.empty-rate");
        assert_eq!(wrong("0", "2005-06-30").0, "input.rate-not-positive");
        let (code, params) = wrong("10.000", "soon");
        assert_eq!(code, "input.malformed-date");
        assert!(params.contains(&("currency", "ROL".to_owned())));
        assert_eq!(wrong("10.000", " ").0, "input.empty-date");

        // With it, the book is made, and each converted amount keeps its
        // original in the old currency.
        let answers = [
            answer("BGL", "EUR", "1955,83", "2026-01-01"),
            answer("ROL", "RON", "10.000", "2005-06-30"),
            answer("TRL", "TRY", "1.000.000", "2005-01-01"),
        ];
        let replaced = replacements(&prepared, &answers, &TestLocale).expect("answered");
        let mut book = empty();
        let created = fill(
            &mut book,
            &prepared,
            &replaced,
            RuleScope::default(),
            &TestLocale,
        )
        .expect("filled");
        assert_eq!(created.converted.len(), 3);
        let rol = created
            .converted
            .iter()
            .find(|converted| converted.old == "ROL")
            .expect("ROL was converted");
        assert_eq!((rol.new.as_str(), rol.rate.as_str()), ("RON", "10000"));
        assert!(rol.parts > 0);
        let transactions = book.transactions().expect("transactions");
        assert!(transactions.iter().any(|transaction| {
            transaction
                .splits()
                .iter()
                .any(|split| split.original.is_some())
        }));
        // A currency the file doesn't need an answer for is passed over.
        let extra = [
            answer("BGL", "EUR", "1955,83", "2026-01-01"),
            answer("ROL", "RON", "10.000", "2005-06-30"),
            answer("TRL", "TRY", "1.000.000", "2005-01-01"),
            answer("XYZ", "EUR", "2", "2005-01-01"),
        ];
        assert_eq!(
            replacements(&prepared, &extra, &TestLocale)
                .expect("answered")
                .len(),
            3
        );
    }

    #[test]
    fn files_that_are_not_books_are_told_where_they_belong() {
        let folder = TestFolder::new();
        let book = empty();
        let read_code = |path: &Path| code(&book, &read(path).expect_err("refused"));

        // A KassenSturz book is opened, not started from.
        let ksz = folder.join("Household.ksz");
        std::fs::write(&ksz, "not even read").expect("file");
        assert_eq!(read_code(&ksz), "input.is-a-book");

        // A bank's statement is imported into an account.
        let statement = golden("camt053", "statement-v08.xml");
        assert_eq!(read_code(&statement), "input.is-a-statement");

        // Anything else is read as a complete export and says why not.
        let notes = folder.join("notes.txt");
        std::fs::write(&notes, "Just some notes").expect("file");
        assert_eq!(read_code(&notes), "export.not-json");
        let json = folder.join("other.json");
        std::fs::write(&json, "{\"hello\": 1}").expect("file");
        assert_eq!(read_code(&json), "export.not-a-book");
        assert_eq!(read_code(&folder.join("missing.kmy")), "export.io");

        // A damaged KMyMoney book is refused whole, with its place.
        let damaged = golden("kmy", "cut-off.kmy");
        assert_eq!(read_code(&damaged), "import.malformed");

        // With no book open (the last one was refused at the start) a file
        // is told the same, not as "no such entry".
        for path in [&ksz, &notes, &folder.join("missing.kmy"), &damaged] {
            let refused = read(path).expect_err("refused");
            assert_eq!(
                error_text_in(None, &refused, &TestLocale).code,
                error_text(&book, &refused, &TestLocale).code
            );
        }
    }

    #[test]
    fn a_csv_of_a_book_that_came_from_a_file_has_its_rows() {
        // Steps meeting: what a KMyMoney book became is exported again.
        let prepared = read(&golden("kmy", "kassensturz-sample.kmy")).expect("read");
        let mut book = empty();
        fill(
            &mut book,
            &prepared,
            &HashMap::new(),
            RuleScope::default(),
            &TestLocale,
        )
        .expect("filled");
        let ids: Vec<String> = book
            .accounts(true)
            .expect("accounts")
            .iter()
            .map(|account| account.id.to_string())
            .collect();
        let folder = TestFolder::new();
        let saved = save_csv(
            &book,
            &ids,
            &CsvForm::usual(&TestLocale),
            &folder.join("all.csv"),
        )
        .expect("saved");
        assert!(saved.transactions > 0);
    }
}
