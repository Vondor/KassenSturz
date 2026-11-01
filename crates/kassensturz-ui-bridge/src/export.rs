// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Saving a copy of the user's data in a file, as the screens see it
//! (M3.12, D-099, D-100, D-125): the transactions of chosen accounts as CSV
//! for a spreadsheet, or the whole book as a complete export that a new book
//! can be made from; and one dashboard as a dashboard file (M6d.3, D-441,
//! D-443), which holds no money.
//!
//! The work is in `kassensturz-export` and `kassensturz-pipeline`; this file
//! turns what the user chose into their settings, and what they report into
//! counts and codes for the screen (D-034, D-065).

use std::path::Path;

use kassensturz_core::{AccountId, Report, ViewKind};
use kassensturz_export::{
    BookExport, CsvExport, CsvExportSettings, DecimalMark, ExportData, ExportError, Separator,
    export_file, replace_file,
};
use kassensturz_pipeline::{export_book, export_transactions};
use kassensturz_report::{Document, ReportError, dashboard_file_name, write_dashboard_file};
use kassensturz_store::{Book, StoreError};

use crate::errors::{ErrorText, report_text};
use crate::format::Locale;

/// What can go wrong on the export screens.
#[derive(Debug)]
pub enum SaveError {
    /// No account was chosen, so there is nothing to write.
    NoAccounts,
    /// An account meant isn't there (anymore).
    NoSuchEntry,
    /// The settings contradict each other: a comma can't be both the
    /// separator and the decimal mark.
    BadSettings,
    /// The book refused to give its data.
    Store(StoreError),
    /// The file couldn't be written.
    Export(ExportError),
    /// The dashboard couldn't be read or written as a file.
    Report(ReportError),
}

impl From<ReportError> for SaveError {
    fn from(error: ReportError) -> Self {
        Self::Report(error)
    }
}

impl From<StoreError> for SaveError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<ExportError> for SaveError {
    fn from(error: ExportError) -> Self {
        match error {
            ExportError::BadSettings => Self::BadSettings,
            error => Self::Export(error),
        }
    }
}

/// The error as the UI gets it: the app's own codes start with `input.`, the
/// export crate's and the store's are those of `docs/ERROR_CODES.md`.
pub fn error_text(book: &Book, error: &SaveError, locale: &impl Locale) -> ErrorText {
    match error {
        SaveError::NoAccounts => ErrorText::plain("input.no-accounts-chosen"),
        SaveError::NoSuchEntry => ErrorText::plain("input.no-such-entry"),
        SaveError::BadSettings => ErrorText::plain("export.bad-settings"),
        SaveError::Store(error) => report_text(book, &error.report(), locale),
        SaveError::Export(error) => report_text(book, &error.report(), locale),
        SaveError::Report(error) => report_text(book, &error.report(), locale),
    }
}

/// The choices for the CSV file, as words QML keeps and sends back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CsvForm {
    /// `comma`, `semicolon` or `tab`.
    pub separator: String,
    /// `point` or `comma`.
    pub decimal_mark: String,
    /// Whether the first row names the columns.
    pub header: bool,
    /// Whether the file starts with a UTF-8 byte order mark, which Excel
    /// needs to show umlauts.
    pub byte_order_mark: bool,
}

impl CsvForm {
    /// What suits the user's region: where a comma is the decimal mark, a
    /// spreadsheet expects a semicolon between the columns (D-099). A
    /// header row, no byte order mark.
    pub fn usual(locale: &impl Locale) -> Self {
        let decimal_comma = locale.decimal_point() == ",";
        Self {
            separator: if decimal_comma { "semicolon" } else { "comma" }.to_owned(),
            decimal_mark: if decimal_comma { "comma" } else { "point" }.to_owned(),
            header: true,
            byte_order_mark: false,
        }
    }

    fn settings(&self) -> Result<CsvExportSettings, SaveError> {
        Ok(CsvExportSettings {
            separator: match self.separator.as_str() {
                "comma" => Separator::Comma,
                "semicolon" => Separator::Semicolon,
                "tab" => Separator::Tab,
                _ => return Err(SaveError::BadSettings),
            },
            decimal_mark: match self.decimal_mark.as_str() {
                "point" => DecimalMark::Point,
                "comma" => DecimalMark::Comma,
                _ => return Err(SaveError::BadSettings),
            },
            header: self.header,
            byte_order_mark: self.byte_order_mark,
        })
    }
}

/// An account the user can include in the CSV file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountChoice {
    /// Its ID, as text.
    pub id: String,
    /// Its name.
    pub name: String,
    /// The currency its amounts are in.
    pub currency: String,
    /// Whether the user closed it.
    pub closed: bool,
}

/// The user's accounts, closed ones too: their history is the user's.
pub fn account_choices(book: &Book) -> Result<Vec<AccountChoice>, StoreError> {
    Ok(book
        .accounts(true)?
        .into_iter()
        .map(|account| AccountChoice {
            id: account.id.to_string(),
            name: account.name,
            currency: account.currency.to_string(),
            closed: account.closed,
        })
        .collect())
}

/// What a file now holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Saved {
    /// The file's name, without its folder.
    pub file_name: String,
    /// How many transactions are in it.
    pub transactions: usize,
    /// How many accounts they are of; for the whole book, accounts and
    /// categories.
    pub accounts: usize,
}

/// Writes the transactions of the accounts `ids` name into a new file at
/// `path` as CSV (D-099). The file is new: an existing one is never
/// overwritten.
pub fn save_csv(
    book: &Book,
    ids: &[String],
    form: &CsvForm,
    path: &Path,
) -> Result<Saved, SaveError> {
    if ids.is_empty() {
        return Err(SaveError::NoAccounts);
    }
    let exporter = CsvExport::new(form.settings()?)?;
    let known = book.accounts(true)?;
    let accounts: Vec<AccountId> = ids
        .iter()
        .map(|id| {
            known
                .iter()
                .find(|account| account.id.to_string() == *id)
                .map(|account| account.id)
                .ok_or(SaveError::NoSuchEntry)
        })
        .collect::<Result<_, _>>()?;
    let data = export_transactions(book, &accounts)?;
    let ExportData::Transactions(exported) = &data else {
        unreachable!("export_transactions gathers transactions")
    };
    let transactions = exported.iter().map(|account| account.rows.len()).sum();
    let accounts = exported.len();
    export_file(&exporter, &data, path)?;
    Ok(Saved {
        file_name: file_name(path),
        transactions,
        accounts,
    })
}

/// Writes the whole book into a new file at `path`: the complete export of
/// `docs/EXPORT_FORMAT.md` (D-100), which a new book can be made from. The
/// change history stays behind (Q-063).
pub fn save_book(book: &Book, path: &Path) -> Result<Saved, SaveError> {
    let data = export_book(book)?;
    let ExportData::Book(exported) = &data else {
        unreachable!("export_book gathers the book")
    };
    let (transactions, accounts) = (exported.transactions.len(), exported.accounts.len());
    export_file(&BookExport, &data, path)?;
    Ok(Saved {
        file_name: file_name(path),
        transactions,
        accounts,
    })
}

/// What saving a dashboard file did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedDashboard {
    /// The name of the file written, without its folder.
    pub file_name: String,
    /// How many accounts, categories and groups the file names.
    pub references: usize,
    /// How many of the dashboard's accounts, categories and groups were left
    /// out, because the book no longer has them (D-458).
    pub left_out: usize,
}

/// The dashboard `id` names (the book's ID as text), or the built-in
/// Overview for an empty `id`.
fn dashboard(book: &Book, id: &str) -> Result<Document, SaveError> {
    if id.is_empty() {
        return Ok(Document::default_overview()?);
    }
    let stored = book
        .views(Some(ViewKind::Dashboard))?
        .into_iter()
        .find(|view| view.id.to_string() == id)
        .ok_or(SaveError::NoSuchEntry)?;
    Ok(Document::read(&stored.document)?)
}

/// The file name suggested for the dashboard `id` names (empty: the
/// built-in Overview): its name with `.dashboard.json`.
pub fn dashboard_file_suggestion(book: &Book, id: &str) -> Result<String, SaveError> {
    Ok(dashboard_file_name(&dashboard(book, id)?.name))
}

/// Writes the dashboard `id` names (empty: the built-in Overview) as a
/// dashboard file at `path` (D-441), replacing a file there, since the file
/// chooser has asked already (D-443). `application` is the program and
/// build, which the file names for information only. Nothing in the book
/// changes.
pub fn save_dashboard(
    book: &Book,
    id: &str,
    application: &str,
    path: &Path,
) -> Result<SavedDashboard, SaveError> {
    let file = write_dashboard_file(book, &dashboard(book, id)?, application)?;
    replace_file(path, file.text.as_bytes())?;
    Ok(SavedDashboard {
        file_name: file_name(path),
        references: file.references,
        left_out: file.left_out,
    })
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use kassensturz_core::{AccountType, CurrencyCode, Date};
    use kassensturz_export::read_book_file;
    use kassensturz_import::{Camt053, Imported, Limits, import};
    use kassensturz_pipeline::{Choices, fill_book, import_statements};

    use super::*;
    use crate::accounts::{NewAccount, add_account};
    use crate::first_start::tests::started;
    use crate::register::tests::TestLocale;
    use crate::register::{Sort, register_view};
    use crate::test_folder::TestFolder;
    use crate::transaction_entry::{self, NewEntry};

    fn account(book: &mut Book, name: &str, account_type: AccountType, balance: &str) -> AccountId {
        add_account(
            book,
            &NewAccount {
                name,
                account_type,
                currency: "EUR",
                balance,
                date: "2026-09-30",
            },
            &TestLocale,
        )
        .expect("account added")
    }

    fn typed(book: &mut Book, account: AccountId, amount: &str, other_party: &str, date: &str) {
        transaction_entry::add(
            book,
            &NewEntry {
                account: &account.to_string(),
                money_in: false,
                amount,
                other_party,
                category: "",
                date,
            },
            &TestLocale,
        )
        .expect("transaction added");
    }

    /// A book with a credit card that a CAMT.053 statement was imported into
    /// through the real pipeline, so the export is of what a bank delivered
    /// (with its details and bank ID), not of values built by hand; a
    /// checking account with an entry typed in; and a savings account.
    fn book_with_imported_statement() -> (Book, AccountId, AccountId, AccountId) {
        let mut book = started();
        let checking = account(
            &mut book,
            "Checking account",
            AccountType::Checking,
            "1000,00",
        );
        let card = account(&mut book, "Credit card", AccountType::CreditCard, "310,40");
        let savings = account(&mut book, "Savings", AccountType::Savings, "0");
        typed(&mut book, checking, "12,50", "Corner Shop", "2026-10-03");
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../kassensturz-import/tests/golden/camt053/statement-v08.xml");
        let input = std::fs::read(&path).expect("golden file");
        let Imported::Statements(statements) =
            import(&Camt053, &input, &Limits::default()).expect("statements")
        else {
            panic!("a statement file holds statements");
        };
        import_statements(
            &mut book,
            card,
            &statements,
            &Choices::default(),
            Some(&path),
        )
        .expect("statement imported");
        (book, checking, card, savings)
    }

    fn csv(book: &Book, ids: &[String], form: &CsvForm, folder: &TestFolder) -> (Saved, String) {
        let path = folder.join("transactions.csv");
        let saved = save_csv(book, ids, form, &path).expect("saved");
        (saved, std::fs::read_to_string(path).expect("file written"))
    }

    /// A started book with a stored dashboard whose filter bar names an
    /// account the book has and one it doesn't.
    fn book_with_dashboard() -> (Book, String, AccountId) {
        let mut book = started();
        let checking = account(&mut book, "Colony Checking", AccountType::Checking, "0");
        let text = format!(
            r#"{{"format": "kassensturz-dashboard", "version": 2, "kind": "dashboard",
                "name": "Rent / Food", "components": [],
                "filters": {{"accounts": ["{checking}", "01890000-0000-7000-8000-000000000000"]}}}}"#
        );
        let id = book.add_view(&text).expect("stored").to_string();
        (book, id, checking)
    }

    #[test]
    fn a_dashboard_file_names_the_account_and_replaces_the_chosen_file() {
        let (book, id, checking) = book_with_dashboard();
        assert_eq!(
            dashboard_file_suggestion(&book, &id).expect("named"),
            "Rent _ Food.dashboard.json"
        );
        let folder = TestFolder::new();
        let path = folder.join("Rent _ Food.dashboard.json");
        // The chooser asked already, so an old file is replaced.
        std::fs::write(&path, "old").expect("written");
        let saved = save_dashboard(&book, &id, "KassenSturz test", &path).expect("saved");
        assert_eq!(
            saved,
            SavedDashboard {
                file_name: "Rent _ Food.dashboard.json".into(),
                references: 1,
                left_out: 1,
            }
        );
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.contains(r#""name": "Colony Checking""#), "{text}");
        assert!(!text.contains(&checking.to_string()), "{text}");
        // The same file again, byte for byte.
        save_dashboard(&book, &id, "KassenSturz test", &path).expect("saved");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), text);
    }

    #[test]
    fn the_built_in_overview_is_saved_for_an_empty_id_and_a_wrong_id_is_refused() {
        let book = started();
        assert_eq!(
            dashboard_file_suggestion(&book, "").expect("named"),
            "Overview.dashboard.json"
        );
        let folder = TestFolder::new();
        let path = folder.join("Overview.dashboard.json");
        let saved = save_dashboard(&book, "", "KassenSturz test", &path).expect("saved");
        assert_eq!((saved.references, saved.left_out), (0, 0));
        let wrong = save_dashboard(&book, "not-an-id", "KassenSturz test", &path);
        assert!(matches!(wrong, Err(SaveError::NoSuchEntry)), "{wrong:?}");
        let gone = folder.join("missing").join("a.dashboard.json");
        let error = save_dashboard(&book, "", "KassenSturz test", &gone).expect_err("no folder");
        assert_eq!(
            error_text(&book, &error, &TestLocale).code,
            "export.io",
            "{error:?}"
        );
    }

    #[test]
    fn the_usual_choices_follow_the_regional_format() {
        // The tests' locale writes a decimal comma, like German.
        let usual = CsvForm::usual(&TestLocale);
        assert_eq!(
            (usual.separator.as_str(), usual.decimal_mark.as_str()),
            ("semicolon", "comma")
        );
        assert!(usual.header && !usual.byte_order_mark);
        assert_eq!(CsvForm::usual(&Point).separator, "comma");
        assert_eq!(CsvForm::usual(&Point).decimal_mark, "point");
    }

    /// The tests' locale with a decimal point, like American English.
    struct Point;

    impl Locale for Point {
        fn amount_format(&self, currency: CurrencyCode) -> crate::format::AmountFormat {
            TestLocale.amount_format(currency)
        }
        fn decimal_point(&self) -> String {
            ".".into()
        }
        fn group_digits(&self, value: u64) -> String {
            TestLocale.group_digits(value)
        }
        fn group_separator(&self) -> String {
            ",".into()
        }
        fn short_date(&self, date: Date) -> String {
            TestLocale.short_date(date)
        }
        fn month_and_year(&self, date: Date) -> String {
            TestLocale.month_and_year(date)
        }
        fn read_date(&self, text: &str) -> Option<Date> {
            TestLocale.read_date(text)
        }
        fn today(&self) -> Date {
            TestLocale.today()
        }
    }

    fn point_form() -> CsvForm {
        CsvForm::usual(&Point)
    }

    #[test]
    fn the_csv_has_the_rows_the_register_shows() {
        let (book, checking, card, savings) = book_with_imported_statement();
        let ids = vec![checking.to_string(), card.to_string()];
        let (saved, text) = csv(&book, &ids, &point_form(), &TestFolder::new());
        let rows = |account| {
            register_view(&book, account, Sort::default(), &TestLocale)
                .expect("register")
                .rows
                .len()
        };
        assert_eq!(saved.accounts, 2);
        assert_eq!(saved.transactions, rows(checking) + rows(card));
        assert_eq!(saved.file_name, "transactions.csv");
        // A header and one line per row; the savings account was not chosen.
        assert_eq!(text.lines().count(), saved.transactions + 1);
        assert!(
            text.starts_with("date,account,payee,category,transfer,memo,amount,currency,parts")
        );
        assert!(text.contains("Corner Shop"));
        assert!(text.contains(",-12.50,EUR,"));
        // What the bank delivered: the statement's entry, with the bank's
        // other party as the payee.
        assert!(text.contains("Example Bookshop"));
        assert!(text.contains(",-35.25,EUR,"));
        assert!(!text.contains("Savings"));
        let _ = savings;
    }

    #[test]
    fn the_choices_change_how_the_file_is_written() {
        let (book, checking, ..) = book_with_imported_statement();
        let ids = vec![checking.to_string()];
        let form = CsvForm {
            separator: "semicolon".into(),
            decimal_mark: "comma".into(),
            header: false,
            byte_order_mark: true,
        };
        let (_, text) = csv(&book, &ids, &form, &TestFolder::new());
        assert!(text.starts_with('\u{feff}'));
        assert!(!text.contains("payee"));
        assert!(text.contains(";-12,50;EUR;"));
    }

    #[test]
    fn nothing_is_written_for_choices_that_contradict_or_name_nothing() {
        let (book, checking, ..) = book_with_imported_statement();
        let folder = TestFolder::new();
        let path = folder.join("transactions.csv");
        let ids = vec![checking.to_string()];
        let code = |error: &SaveError| error_text(&book, error, &TestLocale).code;
        let contradicting = CsvForm {
            separator: "comma".into(),
            decimal_mark: "comma".into(),
            header: true,
            byte_order_mark: false,
        };
        let error = save_csv(&book, &ids, &contradicting, &path).expect_err("refused");
        assert_eq!(code(&error), "export.bad-settings");
        let error = save_csv(&book, &[], &point_form(), &path).expect_err("refused");
        assert_eq!(code(&error), "input.no-accounts-chosen");
        let missing = vec!["no-such-account".to_owned()];
        let error = save_csv(&book, &missing, &point_form(), &path).expect_err("refused");
        assert_eq!(code(&error), "input.no-such-entry");
        let unknown = CsvForm {
            separator: "colon".into(),
            ..point_form()
        };
        let error = save_csv(&book, &ids, &unknown, &path).expect_err("refused");
        assert_eq!(code(&error), "export.bad-settings");
        assert!(!path.exists());
    }

    #[test]
    fn a_file_is_never_overwritten_but_an_empty_one_is_filled() {
        let (book, checking, ..) = book_with_imported_statement();
        let folder = TestFolder::new();
        let path = folder.join("transactions.csv");
        let ids = vec![checking.to_string()];
        std::fs::write(&path, "mine").expect("a file");
        let error = save_csv(&book, &ids, &point_form(), &path).expect_err("refused");
        assert_eq!(
            error_text(&book, &error, &TestLocale).code,
            "export.already-exists"
        );
        assert_eq!(std::fs::read_to_string(&path).expect("kept"), "mine");
        // What a sandbox's file chooser leaves.
        std::fs::write(&path, "").expect("an empty file");
        save_csv(&book, &ids, &point_form(), &path).expect("filled");
        assert!(!std::fs::read_to_string(&path).expect("written").is_empty());
    }

    #[test]
    fn the_whole_book_comes_back_exactly_from_its_complete_export() {
        let (book, checking, card, _) = book_with_imported_statement();
        let folder = TestFolder::new();
        let path = folder.join("book.json");
        let saved = save_book(&book, &path).expect("saved");
        assert!(saved.transactions >= 3);
        let data = read_book_file(&path).expect("read back");
        let mut copy = Book::create_in_memory().expect("book");
        fill_book(&mut copy, &data).expect("filled");
        // The same book again gives the same bytes (D-100), and the registers
        // show the same rows.
        let again = folder.join("again.json");
        save_book(&copy, &again).expect("saved again");
        assert_eq!(
            std::fs::read(&path).expect("first"),
            std::fs::read(&again).expect("second")
        );
        let rows = |book: &Book, account| {
            register_view(book, account, Sort::default(), &TestLocale)
                .expect("register")
                .rows
                .len()
        };
        assert_eq!(rows(&book, checking), rows(&copy, checking));
        assert_eq!(rows(&book, card), rows(&copy, card));
    }

    #[test]
    fn the_accounts_to_choose_from_include_closed_ones() {
        let (mut book, checking, card, savings) = book_with_imported_statement();
        let choices = account_choices(&book).expect("choices");
        assert_eq!(
            choices.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            ["Checking account", "Credit card", "Savings"]
        );
        book.close_account(savings).expect("closed");
        let choices = account_choices(&book).expect("choices");
        assert!(
            choices
                .iter()
                .any(|c| c.closed && c.id == savings.to_string())
        );
        assert!(
            choices
                .iter()
                .any(|c| !c.closed && c.id == checking.to_string())
        );
        assert!(
            choices
                .iter()
                .any(|c| !c.closed && c.id == card.to_string())
        );
    }
}
