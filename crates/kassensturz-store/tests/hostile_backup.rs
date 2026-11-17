// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Regression test for a restore from a damaged backup (D-787, US7 Phase 3
//! round 1, section C). Putting a backup in place of the book replaces the
//! user's book; a backup that opens only to an error, or that disagrees with
//! its own balances, must be refused *before* anything is replaced, and the
//! book must be as it was. All data is made up (AGENTS.md §3.1 rule 2).

use std::fs;
use std::path::{Path, PathBuf};

use kassensturz_core::{
    Account, AccountId, AccountKind, CurrencyCode, Date, Money, Split, Transaction, TransactionId,
};
use kassensturz_store::{Book, Computer, PreparedRestore, book_id_of_file};

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

fn account(book: &mut Book, name: &str, kind: AccountKind) -> AccountId {
    let id = AccountId::generate();
    book.add_account(&Account {
        id,
        name: name.into(),
        kind,
        currency: CurrencyCode::new("EUR").expect("valid code"),
        account_type: None,
        closed: false,
        parent: None,
        number: None,
        notes: None,
        institution: None,
    })
    .expect("account added");
    id
}

/// A book with a few entries, closed again; its backup is returned too.
fn book_and_backup(folder: &TempFolder) -> (PathBuf, PathBuf) {
    let path = folder.0.join("Household.ksz");
    let mut book = Book::create(&path, &folder.computer()).expect("book made");
    let eur = CurrencyCode::new("EUR").expect("valid code");
    book.use_currency(eur).expect("currency used");
    let checking = account(&mut book, "Checking account", AccountKind::Asset);
    let food = account(&mut book, "Groceries", AccountKind::Expense);
    for (day, cents) in [(3, 1250), (9, 4710), (17, 899)] {
        let money = |units: i64| Money::from_minor_units(units, eur);
        let parts = vec![
            Split::new(checking, money(-cents)),
            Split::new(food, money(cents)),
        ];
        book.add_transaction(
            &Transaction::new(
                TransactionId::generate(),
                Date::new(2026, 3, day).expect("valid date"),
                Some("Corner shop".into()),
                parts,
            )
            .expect("balanced"),
        )
        .expect("added");
    }
    let backup = book
        .back_up_now(&path, &folder.computer().backup_folder)
        .expect("backed up");
    drop(book);
    (path, backup)
}

/// The restore is refused and the book is byte for byte as it was, with no
/// part file left next to it.
fn assert_refused(backup: &Path, book: &Path) {
    let before = fs::read(book).expect("book read");
    let id = book_id_of_file(book).expect("the book has an identity");
    assert!(
        PreparedRestore::new(backup, book, &id).is_err(),
        "a damaged backup was accepted for a restore"
    );
    assert_eq!(fs::read(book).expect("book read"), before);
    let mut part = book.as_os_str().to_owned();
    part.push(".restore.part");
    assert!(!PathBuf::from(part).exists(), "a part file was left");
}

#[test]
fn a_cut_short_backup_is_refused() {
    let folder = TempFolder::new();
    let (book, backup) = book_and_backup(&folder);
    let bytes = fs::read(&backup).expect("backup read");
    for keep in [0, 1, 100, 4096, bytes.len() / 2, bytes.len() - 4096] {
        let cut = folder.0.join(format!("Household cut {keep}.ksz"));
        fs::write(&cut, &bytes[..keep]).expect("written");
        assert_refused(&cut, &book);
    }
}

#[test]
fn a_backup_with_a_changed_amount_is_refused() {
    let folder = TempFolder::new();
    let (book, backup) = book_and_backup(&folder);
    let changed = folder.0.join("Household changed.ksz");
    fs::copy(&backup, &changed).expect("copied");
    rusqlite::Connection::open(&changed)
        .and_then(|conn| {
            conn.execute(
                "UPDATE splits SET amount = amount + 1 WHERE rowid = (SELECT min(rowid) FROM splits)",
                [],
            )
        })
        .expect("amount changed");
    assert_refused(&changed, &book);
}

#[test]
fn a_backup_with_a_part_of_a_missing_account_is_refused() {
    let folder = TempFolder::new();
    let (book, backup) = book_and_backup(&folder);
    let changed = folder.0.join("Household broken.ksz");
    fs::copy(&backup, &changed).expect("copied");
    rusqlite::Connection::open(&changed)
        .and_then(|conn| {
            conn.pragma_update(None, "foreign_keys", false)?;
            conn.execute(
                "UPDATE splits SET account_id = zeroblob(16) WHERE rowid = (SELECT min(rowid) FROM splits)",
                [],
            )
        })
        .expect("account changed");
    assert_refused(&changed, &book);
}

#[test]
fn a_backup_that_is_a_folder_or_missing_is_refused() {
    let folder = TempFolder::new();
    let (book, _backup) = book_and_backup(&folder);
    assert_refused(&folder.0, &book);
    assert_refused(&folder.0.join("nothing here.ksz"), &book);
}

#[cfg(unix)]
#[test]
fn a_restore_into_a_read_only_folder_changes_nothing() {
    use std::os::unix::fs::PermissionsExt;

    let folder = TempFolder::new();
    let (book, backup) = book_and_backup(&folder);
    let before = fs::read(&book).expect("book read");
    let locked = folder.0.clone();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).expect("locked");
    // Running as the superuser, a read-only folder doesn't stop anyone.
    let writable = fs::write(locked.join("probe"), b"x").is_ok();
    let id = book_id_of_file(&book).expect("the book has an identity");
    let result = PreparedRestore::new(&backup, &book, &id);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("unlocked");
    let _ = fs::remove_file(locked.join("probe"));
    if !writable {
        assert!(
            result.is_err(),
            "nothing can be written, yet it was made ready"
        );
    }
    drop(result);
    assert_eq!(fs::read(&book).expect("book read"), before);
}
