// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A drive that stops taking writes while the book is open (D-732). All data
//! is made up (AGENTS.md §3.1 rule 2).

use kassensturz_core::{Account, AccountId, AccountKind, CurrencyCode};

use crate::test_folder::TempFolder;
#[cfg(unix)]
use kassensturz_core::Report;

#[cfg(unix)]
use crate::StoreError;
use crate::{Book, Computer};

fn account(name: &str) -> Account {
    Account {
        id: AccountId::generate(),
        name: name.into(),
        kind: AccountKind::Asset,
        currency: CurrencyCode::new("EUR").expect("a currency code"),
        account_type: None,
        closed: false,
        parent: None,
        number: None,
        notes: None,
        institution: None,
    }
}

fn names(book: &Book) -> Vec<String> {
    let mut names: Vec<String> = book
        .accounts(true)
        .expect("accounts")
        .into_iter()
        .map(|account| account.name)
        .collect();
    names.sort();
    names
}

/// A book in a folder of its own, which a test can take away from under it
/// like a pulled stick.
fn book_on_a_stick(folder: &TempFolder) -> Book {
    let stick = folder.join("Stick");
    std::fs::create_dir(&stick).expect("stick");
    let computer = Computer {
        name: "Laptop".into(),
        backup_folder: folder.join("Backups"),
    };
    let mut book = Book::create(&stick.join("Household.ksz"), &computer).expect("book made");
    book.use_currency(CurrencyCode::new("EUR").expect("a currency code"))
        .expect("currency used");
    book.add_account(&account("Giro")).expect("account added");
    book
}

#[test]
fn a_healthy_book_is_not_marked_when_a_change_is_refused_for_its_content() {
    let folder = TempFolder::new();
    let mut book = book_on_a_stick(&folder);
    // A currency that isn't in the table: refused for what was asked.
    let mut bad = account("Odd");
    bad.currency = CurrencyCode::new("XAU").expect("a currency code");
    assert!(book.add_account(&bad).is_err());
    assert!(!book.write_failed());
    book.add_account(&account("Savings")).expect("still writes");
    assert_eq!(names(&book), ["Giro", "Savings"]);
}

// Unix lets a folder go while its file is open.
#[cfg(unix)]
#[test]
fn a_drive_that_drops_stops_all_writing_and_keeps_what_was_saved() {
    let folder = TempFolder::new();
    let mut book = book_on_a_stick(&folder);
    assert!(!book.write_failed());
    std::fs::remove_dir_all(folder.join("Stick")).expect("stick pulled");
    // The change is refused and taken back.
    let error = book.add_account(&account("Lost")).expect_err("not written");
    assert!(matches!(error, StoreError::Database(_)), "{error:?}");
    assert!(book.write_failed());
    assert_eq!(names(&book), ["Giro"]);
    // From then on every change is refused the same way, before it starts.
    let again = book.add_account(&account("Lost too")).expect_err("refused");
    assert!(matches!(again, StoreError::WriteFailed), "{again:?}");
    assert_eq!(again.report().code, "store.write-failed");
    assert_eq!(names(&book), ["Giro"]);
}

// A sync program (Nextcloud, Syncthing) puts the other computer's copy over
// the open book by renaming it into place (B-289). The open book is then the
// old, unlinked file: a change is not written to it, and the store says why.
#[cfg(unix)]
#[test]
fn a_book_replaced_by_another_file_refuses_writing_and_says_it_was_replaced() {
    let folder = TempFolder::new();
    let mut book = book_on_a_stick(&folder);
    let other = folder.join("Stick").join("Other.ksz");
    std::fs::copy(folder.join("Stick").join("Household.ksz"), &other).expect("copy");
    std::fs::rename(&other, folder.join("Stick").join("Household.ksz")).expect("replaced");
    let error = book.add_account(&account("Late")).expect_err("not written");
    assert!(matches!(error, StoreError::Database(_)), "{error:?}");
    assert!(book.write_failed(), "{error:?}");
    assert!(book.file_replaced(), "{error:?}");
    assert_eq!(names(&book), ["Giro"]);
}

#[cfg(unix)]
#[test]
fn a_book_whose_folder_went_is_not_called_replaced() {
    let folder = TempFolder::new();
    let mut book = book_on_a_stick(&folder);
    std::fs::remove_dir_all(folder.join("Stick")).expect("stick pulled");
    book.add_account(&account("Lost")).expect_err("not written");
    assert!(book.write_failed());
    assert!(!book.file_replaced());
}
