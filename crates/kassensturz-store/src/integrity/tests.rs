// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the checks made when a book is opened (B-291). All data is made
//! up (AGENTS.md §3.1 rule 2).

use std::path::Path;

use kassensturz_core::AccountKind;

use crate::test_folder::TempFolder;
use crate::tests::{EUR, account, date, eur, transaction};
use crate::{Book, Computer, Corruption, StoreError};

fn computer(folder: &TempFolder) -> Computer {
    Computer {
        name: "Laptop".to_owned(),
        backup_folder: folder.join("Backups"),
    }
}

/// A book in a file with a few transactions, closed again.
fn book_file(folder: &TempFolder) -> std::path::PathBuf {
    let path = folder.join("Household.ksz");
    let mut book = Book::create(&path, &computer(folder)).expect("book made");
    book.use_currency(EUR).expect("currency used");
    let checking = account(&mut book, "Checking account", AccountKind::Asset, EUR);
    let food = account(&mut book, "Groceries", AccountKind::Expense, EUR);
    for day in 1..=20 {
        let parts = [(checking, eur(-1250)), (food, eur(1250))];
        book.add_transaction(&transaction(date(2026, 3, day), "Corner shop", &parts))
            .expect("added");
    }
    path
}

/// Another program changes a number in the file, as a raw SQL statement
/// does: past every check of KassenSturz.
fn change_an_amount(path: &Path) {
    let conn = rusqlite::Connection::open(path).expect("file opened");
    let changed = conn
        .execute(
            "UPDATE splits SET amount = amount + 1 WHERE rowid = (SELECT min(rowid) FROM splits)",
            [],
        )
        .expect("changed");
    assert_eq!(changed, 1);
}

#[test]
fn a_book_that_is_all_right_opens() {
    let folder = TempFolder::new();
    let path = book_file(&folder);
    Book::open(&path, &computer(&folder)).expect("opens");
    Book::open_to_read(&path).expect("reads");
}

/// SQLite's `integrity_check` is content with the book after one amount
/// changed; the balance check is what stops it.
#[test]
fn a_changed_amount_is_found_and_the_book_stays_as_it_was() {
    let folder = TempFolder::new();
    let path = book_file(&folder);
    change_an_amount(&path);
    let conn = rusqlite::Connection::open(&path).expect("file opened");
    let check: String = conn
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .expect("checked");
    assert_eq!(check, "ok", "the premise: SQLite sees nothing wrong");
    drop(conn);
    let before = std::fs::read(&path).expect("read");

    let refused = Book::open(&path, &computer(&folder));
    assert!(
        matches!(
            refused,
            Err(StoreError::Corrupt(Corruption::UnbalancedTransaction))
        ),
        "{:?}",
        refused.err()
    );
    let refused = Book::open_to_read(&path);
    assert!(matches!(
        refused,
        Err(StoreError::Corrupt(Corruption::UnbalancedTransaction))
    ));
    assert_eq!(std::fs::read(&path).expect("read"), before, "changed");
    assert!(
        !crate::lock::lock_path(&path).exists(),
        "the lock stayed after a refusal"
    );
}

/// A page SQLite can't parse: the book is refused with a clear code, not
/// opened to fail at the next save.
#[test]
fn a_damaged_page_is_found() {
    let folder = TempFolder::new();
    let path = book_file(&folder);
    let mut bytes = std::fs::read(&path).expect("read");
    // Overwrite the start of a table page in the middle of the file.
    let page = 4096;
    assert!(bytes.len() > 6 * page, "{}", bytes.len());
    for byte in &mut bytes[3 * page..3 * page + 64] {
        *byte = 0xFF;
    }
    std::fs::write(&path, &bytes).expect("written");

    let refused = Book::open(&path, &computer(&folder));
    let code = refused.as_ref().err().map(|error| {
        use kassensturz_core::Report;
        error.report().code
    });
    assert!(
        matches!(refused, Err(StoreError::Corrupt(Corruption::DamagedFile))),
        "{code:?}"
    );
    assert!(!crate::lock::lock_path(&path).exists());
}
