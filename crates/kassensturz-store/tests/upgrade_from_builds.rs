// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A newer build opens a book an older build made (D-787, US7 Phase 3 round
//! 1, section C). The two books are made-up households that the builds
//! 0.261112.51 (layout 30) and 0.261113.45 (layout 32) wrote with their own
//! code, kept as SQL text (`tests/fixtures/`) so a reviewer can read them.
//! The rule: the new build either converts the book and every balance stays
//! as it was, with a backup in the old layout, or it refuses it with a clear
//! error and the file is left byte for byte as it was. It never damages it.

use std::fs;
use std::path::{Path, PathBuf};

use kassensturz_core::AccountId;
use kassensturz_store::{Book, Computer, StoreError, backups_of};

struct TempFolder(PathBuf);

impl TempFolder {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("kassensturz-test-{}", AccountId::generate()));
        fs::create_dir(&path).expect("temporary folder created");
        Self(path)
    }

    fn computer(&self) -> Computer {
        Computer {
            name: "Laptop".into(),
            backup_folder: self.0.join("Backups"),
        }
    }
}

impl Drop for TempFolder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// `KSZB`, the application id of a book (D-037).
const APPLICATION_ID: i32 = i32::from_be_bytes(*b"KSZB");

/// The book the build wrote, as the file that build wrote.
fn old_book(folder: &TempFolder, fixture: &str, layout: i32) -> PathBuf {
    let sql = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(fixture),
    )
    .expect("fixture read")
    // A checkout on Windows may turn line ends into CR LF; the layout is
    // compared as text, so the file must hold the line ends the build wrote.
    .replace("\r\n", "\n");
    let path = folder.0.join("Household.ksz");
    let conn = rusqlite::Connection::open(&path).expect("file made");
    // The tables come in alphabetical order, so a part may come before its account.
    conn.pragma_update(None, "foreign_keys", false)
        .expect("keys off");
    conn.execute_batch(&sql).expect("fixture loaded");
    conn.pragma_update(None, "application_id", APPLICATION_ID)
        .expect("application id");
    conn.pragma_update(None, "user_version", layout)
        .expect("layout");
    drop(conn);
    path
}

/// What the made-up household holds, in the figures of its builder.
fn assert_the_household(book: &Book) {
    let accounts = book.all_accounts().expect("accounts");
    let balance = |name: &str| {
        let account = accounts
            .iter()
            .find(|account| account.name == name)
            .unwrap_or_else(|| panic!("no account {name}"));
        book.balance(account.id).expect("balance").minor_units()
    };
    assert_eq!(balance("Checking account"), 143_141);
    assert_eq!(balance("Groceries"), 6_859);
    assert_eq!(balance("Salary"), -250_000);
    assert_eq!(balance("Dollar account"), 108_560);
    assert_eq!(book.transactions().expect("transactions").len(), 5);
}

fn user_version(path: &Path) -> i32 {
    rusqlite::Connection::open(path)
        .and_then(|conn| conn.pragma_query_value(None, "user_version", |row| row.get(0)))
        .expect("layout read")
}

fn check_one(fixture: &str, layout: i32) {
    let folder = TempFolder::new();
    let path = old_book(&folder, fixture, layout);
    let before = fs::read(&path).expect("book read");
    match Book::open(&path, &folder.computer()) {
        Ok(book) => {
            assert_the_household(&book);
            drop(book);
            // The book is in the layout of this build, and a copy in the
            // layout it was is kept.
            assert_eq!(user_version(&path), kassensturz_store::LAYOUT);
            let backups = backups_of(&path, &folder.computer().backup_folder).expect("listed");
            assert!(
                backups
                    .iter()
                    .any(|backup| user_version(&backup.path) == layout),
                "no backup in layout {layout}"
            );
            // And it opens again, with the same figures.
            let again = Book::open(&path, &folder.computer()).expect("opens again");
            assert_the_household(&again);
        }
        Err(StoreError::DevelopmentLayout { .. } | StoreError::UnsupportedLayout { .. }) => {
            assert_eq!(
                fs::read(&path).expect("book read"),
                before,
                "a refused book changed"
            );
        }
        Err(other) => panic!("{fixture}: neither converted nor refused clearly: {other:?}"),
    }
}

#[test]
fn a_book_of_build_0_261112_51_is_converted_or_refused_never_damaged() {
    check_one("alpha-0.261112.51.sql", 30);
}

#[test]
fn a_book_of_build_0_261113_45_is_converted_or_refused_never_damaged() {
    check_one("alpha-0.261113.45.sql", 32);
}

#[test]
fn a_book_of_an_older_layout_than_this_build_knows_is_refused_untouched() {
    for fixture in ["alpha-0.261112.51.sql", "alpha-0.261113.45.sql"] {
        let folder = TempFolder::new();
        let path = old_book(&folder, fixture, kassensturz_store::FIRST_LAYOUT - 1);
        let before = fs::read(&path).expect("book read");
        let Err(error) = Book::open(&path, &folder.computer()) else {
            panic!("a book of another layout was opened");
        };
        assert!(
            matches!(error, StoreError::DevelopmentLayout { .. }),
            "{error:?}"
        );
        assert_eq!(fs::read(&path).expect("book read"), before);
    }
}

#[test]
fn a_book_of_a_newer_layout_is_refused_untouched() {
    let folder = TempFolder::new();
    let path = old_book(
        &folder,
        "alpha-0.261113.45.sql",
        kassensturz_store::LAYOUT + 1,
    );
    let before = fs::read(&path).expect("book read");
    let Err(error) = Book::open(&path, &folder.computer()) else {
        panic!("a book of another layout was opened");
    };
    assert!(
        matches!(error, StoreError::UnsupportedLayout { .. }),
        "{error:?}"
    );
    assert_eq!(fs::read(&path).expect("book read"), before);
}
