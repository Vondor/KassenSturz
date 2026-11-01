// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Adding a dashboard from a file (M6d.4, D-441, D-442, T-51, T-53;
//! `docs/REPORT_DESIGNER_M6D.md` section 4.3). The file is read and its
//! names are looked up in the open book by `kassensturz-report`; this file
//! reads the bytes, offers a free name and, only after *Add dashboard*,
//! stores the dashboard as one entry of the change history.
//! `dashboard_file_model.rs` hands the results to QML.
//!
//! The file is never given to the AI and opening it starts no network
//! access.

use std::path::Path;

use kassensturz_core::{Report, ViewKind};
use kassensturz_import::{ImportError, Limits, read_file};
use kassensturz_report::{
    BookNames, Document, IncomingDashboardFile, MAX_DASHBOARD_FILE_BYTES, OVERVIEW_NAME,
    ReportError, Resolved, free_dashboard_name,
};
use kassensturz_store::{Action, Book, StoreError};

/// Why a dashboard file couldn't be read or added.
#[derive(Debug)]
pub enum DashboardFileError {
    /// The file couldn't be read from the disk.
    Import(ImportError),
    /// The file is refused, or its dashboard is.
    Report(ReportError),
    /// The book refused it, e.g. a name that is taken.
    Store(StoreError),
}

impl From<ReportError> for DashboardFileError {
    fn from(error: ReportError) -> Self {
        Self::Report(error)
    }
}

impl From<StoreError> for DashboardFileError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl DashboardFileError {
    /// The error's code and parameters.
    pub fn report(&self) -> kassensturz_core::ErrorReport {
        match self {
            Self::Import(error) => error.report(),
            Self::Report(error) => error.report(),
            Self::Store(error) => error.report(),
        }
    }
}

/// A file read into the open book, before anything is added.
#[derive(Clone, Debug, PartialEq)]
pub struct Opened {
    /// The document with the book's IDs and what the summary lists.
    pub resolved: Resolved,
    /// The dashboard's name, or *Name (2)* when a dashboard has it already.
    pub name: String,
}

/// Reads the file at `path`: no more than one byte past the largest
/// dashboard file is read.
pub fn read_bytes(path: &Path) -> Result<Vec<u8>, DashboardFileError> {
    let limits = Limits {
        max_file_bytes: u64::try_from(MAX_DASHBOARD_FILE_BYTES).unwrap_or(u64::MAX),
        ..Limits::default()
    };
    read_file(path, &limits).map_err(|error| match error {
        ImportError::Io(_) => DashboardFileError::Import(error),
        _ => DashboardFileError::Report(ReportError::DashboardFileTooBig {
            max_bytes: MAX_DASHBOARD_FILE_BYTES,
        }),
    })
}

/// The names the dashboards of the book have; the built-in Overview's while
/// none is stored, since adding stores it first.
fn taken_names(book: &Book) -> Result<Vec<String>, StoreError> {
    let names: Vec<String> = book
        .views(Some(ViewKind::Dashboard))?
        .into_iter()
        .map(|view| view.name)
        .collect();
    Ok(if names.is_empty() {
        vec![OVERVIEW_NAME.to_owned()]
    } else {
        names
    })
}

/// Reads a dashboard file's bytes and looks its names up in `book`.
/// Nothing in the book changes.
pub fn open(book: &Book, bytes: &[u8]) -> Result<Opened, DashboardFileError> {
    if bytes.len() > MAX_DASHBOARD_FILE_BYTES {
        return Err(ReportError::DashboardFileTooBig {
            max_bytes: MAX_DASHBOARD_FILE_BYTES,
        }
        .into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| ReportError::DashboardFileInvalid {
        reason: "not-json",
        path: String::new(),
    })?;
    let file = IncomingDashboardFile::read(text)?;
    let resolved = file.resolve(&BookNames::read(book)?)?;
    let name = free_dashboard_name(&resolved.document.name, &taken_names(book)?);
    Ok(Opened { resolved, name })
}

/// Adds the dashboard under `name` at the end of the book's list, as one
/// entry of the change history. While the book stores no dashboard, the
/// built-in Overview is stored first, so it keeps its place in front
/// (section 1.2). Returns the new dashboard's ID.
pub fn add(book: &mut Book, document: &Document, name: &str) -> Result<String, DashboardFileError> {
    let mut document = document.clone();
    name.trim().clone_into(&mut document.name);
    let text = document.to_json();
    // The name the user typed is checked like any document's.
    Document::read(&text)?;
    let overview = Document::default_overview()?.to_json();
    book.as_one_action(Action::AddView, |book| {
        if book.views(Some(ViewKind::Dashboard))?.is_empty() {
            book.add_view(&overview)?;
        }
        Ok(book.add_view(&text)?.to_string())
    })
}

#[cfg(test)]
mod tests {
    use kassensturz_core::{Account, AccountId, AccountKind, AccountType, CurrencyCode};

    use super::*;
    use crate::test_folder::TestFolder;

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a code"),
    };

    fn file(account: &str) -> String {
        serde_json::json!({
            "format": "kassensturz-dashboard-file",
            "version": 1,
            "references": {"ref:1": {"kind": "account", "name": account}},
            "dashboard": {
                "format": "kassensturz-dashboard", "version": 2, "kind": "dashboard",
                "name": "Overview",
                "filters": {"accounts": ["ref:1"]},
                "components": [{
                    "id": "c1", "type": "chart.line", "dataset": "income_expense_by_month",
                    "frame": {"col": 0, "row": 0, "cols": 12, "rows": 4},
                }],
            },
        })
        .to_string()
    }

    fn book_with_checking() -> (Book, String) {
        let mut book = Book::create_in_memory().expect("book");
        book.use_currency(EUR).expect("currency");
        let id = AccountId::generate();
        book.add_account(&Account {
            id,
            name: "Colony Checking".into(),
            kind: AccountKind::Asset,
            currency: EUR,
            account_type: Some(AccountType::Checking),
            parent: None,
            closed: false,
            number: None,
            notes: None,
            institution: None,
        })
        .expect("account");
        (book, id.to_string())
    }

    #[test]
    fn a_file_is_read_into_the_book_and_added_as_one_history_entry() {
        let (mut book, checking) = book_with_checking();
        let opened = open(&book, file("colony checking").as_bytes()).expect("opened");
        // The built-in Overview has the file's name: the next free one.
        assert_eq!(opened.name, "Overview (2)");
        assert_eq!(opened.resolved.document.accounts, [checking]);
        let id = add(&mut book, &opened.resolved.document, &opened.name).expect("added");
        let stored = book.views(Some(ViewKind::Dashboard)).expect("views");
        let names: Vec<&str> = stored.iter().map(|view| view.name.as_str()).collect();
        assert_eq!(names, [OVERVIEW_NAME, "Overview (2)"]);
        assert_eq!(stored[1].id.to_string(), id);
        // One Undo takes both back.
        book.undo().expect("undo");
        assert!(
            book.views(Some(ViewKind::Dashboard))
                .expect("views")
                .is_empty()
        );
    }

    #[test]
    fn a_taken_name_typed_by_the_user_is_refused_and_nothing_is_added() {
        let (mut book, _) = book_with_checking();
        let opened = open(&book, file("Colony Checking").as_bytes()).expect("opened");
        add(&mut book, &opened.resolved.document, "Mine").expect("added");
        let again = add(&mut book, &opened.resolved.document, "MINE");
        assert!(matches!(
            again,
            Err(DashboardFileError::Store(StoreError::ViewNameTaken { .. }))
        ));
        assert_eq!(
            book.views(Some(ViewKind::Dashboard)).expect("views").len(),
            2
        );
    }

    #[test]
    fn a_hostile_file_is_refused_before_the_book_is_touched() {
        let (book, _) = book_with_checking();
        let uuid = file("x").replace("ref:1\"]", "0190d2a4-7a8c-7cc3-9d2e-6f1a2b3c4d5e\"]");
        let refused = open(&book, uuid.as_bytes()).expect_err("refused");
        assert_eq!(refused.report().code, "report.dashboard-file-invalid");
        let not_text = open(&book, &[0xff, 0xfe, b'{']).expect_err("refused");
        assert_eq!(not_text.report().code, "report.dashboard-file-invalid");
        let big = vec![b' '; MAX_DASHBOARD_FILE_BYTES + 1];
        assert_eq!(
            open(&book, &big).expect_err("refused").report().code,
            "report.dashboard-file-too-big"
        );
    }

    #[test]
    fn reading_a_file_from_the_disk_stops_after_the_limit() {
        let folder = TestFolder::new();
        let small = folder.join("small.dashboard.json");
        std::fs::write(&small, file("Colony Checking")).expect("written");
        assert_eq!(
            read_bytes(&small).expect("read"),
            file("Colony Checking").as_bytes()
        );
        let big = folder.join("big.dashboard.json");
        std::fs::write(&big, vec![b' '; MAX_DASHBOARD_FILE_BYTES + 10]).expect("written");
        assert_eq!(
            read_bytes(&big).expect_err("refused").report().code,
            "report.dashboard-file-too-big"
        );
        assert_eq!(
            read_bytes(&folder.join("gone.json"))
                .expect_err("refused")
                .report()
                .code,
            "import.io"
        );
    }
}
