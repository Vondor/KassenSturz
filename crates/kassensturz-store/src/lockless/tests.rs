// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use kassensturz_core::AccountKind;
use rusqlite::{Connection, OpenFlags};

use super::*;
use crate::test_folder::TempFolder;
use crate::tests::{EUR, account};
use crate::{Book, Computer, StoreError};

/// Makes this thread see every folder as one without file locks, as in a
/// snap's portal folder, until dropped.
struct NoLocks;

impl NoLocks {
    fn here() -> Self {
        NO_LOCKS.with(|flag| flag.set(true));
        Self
    }
}

impl Drop for NoLocks {
    fn drop(&mut self) {
        NO_LOCKS.with(|flag| flag.set(false));
    }
}

fn computer(folder: &TempFolder) -> Computer {
    Computer {
        name: "Laptop".into(),
        backup_folder: folder.join("Backups"),
    }
}

/// Adds an account called `name` to `book`.
fn add(book: &mut Book, name: &str) {
    book.use_currency(EUR).expect("currency used");
    account(book, name, AccountKind::Asset, EUR);
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

#[test]
fn a_book_without_file_locks_keeps_what_is_saved() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let computer = computer(&folder);
    {
        let _no_locks = NoLocks::here();
        let mut book = Book::create(&path, &computer).expect("book made");
        assert!(book.without_file_locks());
        add(&mut book, "Checking");
    }
    {
        let _no_locks = NoLocks::here();
        let book = Book::open(&path, &computer).expect("opened again");
        assert!(book.without_file_locks());
        assert_eq!(names(&book), ["Checking"]);
    }
    // Where locks work, the same file opens the ordinary way.
    let book = Book::open(&path, &computer).expect("opened with locks");
    assert!(!book.without_file_locks());
    assert_eq!(names(&book), ["Checking"]);
}

#[test]
fn a_second_writer_is_stopped_by_the_lock_file_without_file_locks() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let computer = computer(&folder);
    let _no_locks = NoLocks::here();
    let _first = Book::create(&path, &computer).expect("book made");
    match Book::open(&path, &computer) {
        Err(StoreError::Locked(_)) => {}
        other => panic!(
            "expected the book to be locked, got {:?}",
            other.map(|_| ())
        ),
    }
}

#[test]
fn a_write_cut_short_is_rolled_back_at_the_next_open() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let computer = computer(&folder);
    let _no_locks = NoLocks::here();
    {
        let mut book = Book::create(&path, &computer).expect("book made");
        add(&mut book, "Checking");
    }
    // A write that has begun to reach the file, then the program dies: the
    // book's file and its journal are what is left.
    let crashed = folder.join("Crashed");
    std::fs::create_dir(&crashed).expect("folder made");
    {
        let conn = Connection::open_with_flags_and_vfs(
            &path,
            OpenFlags::SQLITE_OPEN_READ_WRITE,
            "unix-none",
        )
        .expect("opened");
        conn.pragma_update(None, "cache_size", 5).expect("cache");
        conn.execute_batch(
            "BEGIN;
             CREATE TABLE junk(x BLOB);
             WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 400)
             INSERT INTO junk SELECT zeroblob(4000) FROM n;",
        )
        .expect("a big write begun");
        let journal = journal_of(&path);
        assert!(journal.exists(), "the write has a journal");
        // The state of the folder at this moment is what a crash leaves.
        std::fs::copy(&path, crashed.join("Household.ksz")).expect("book copied");
        std::fs::copy(&journal, journal_of(&crashed.join("Household.ksz")))
            .expect("journal copied");
        conn.execute_batch("ROLLBACK").expect("rolled back");
    }
    let book = Book::open(&crashed.join("Household.ksz"), &computer).expect("opened");
    assert_eq!(names(&book), ["Checking"], "the last good state is back");
    let tables: i64 = book
        .conn
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name = 'junk'",
            [],
            |row| row.get(0),
        )
        .expect("count");
    assert_eq!(tables, 0, "the cut-short write left nothing");
}

#[test]
fn a_book_is_read_through_a_whole_private_copy_without_file_locks() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let computer = computer(&folder);
    let _no_locks = NoLocks::here();
    let mut writer = Book::create(&path, &computer).expect("book made");
    add(&mut writer, "Checking");
    let reader = Book::open_to_read(&path).expect("read");
    assert!(reader.without_file_locks());
    assert_eq!(names(&reader), ["Checking"]);
    // The reader sees one state; a later change shows in the next read.
    add(&mut writer, "Savings");
    assert_eq!(names(&reader), ["Checking"]);
    drop(reader);
    assert_eq!(
        names(&Book::open_to_read(&path).expect("read")),
        ["Checking", "Savings"]
    );
}

#[test]
fn a_book_that_is_being_written_is_not_read_half_way() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let computer = computer(&folder);
    let _no_locks = NoLocks::here();
    drop(Book::create(&path, &computer).expect("book made"));
    // A journal beside the book means a write is under way.
    std::fs::write(journal_of(&path), b"journal").expect("journal made");
    let started = std::time::Instant::now();
    match Book::open_to_read(&path) {
        Err(StoreError::Io(std::io::ErrorKind::WouldBlock)) => {}
        other => panic!("expected a busy book, got {:?}", other.map(|_| ())),
    }
    assert!(started.elapsed() >= COPY_WAIT, "it tried more than once");
    std::fs::remove_file(journal_of(&path)).expect("journal removed");
    assert!(Book::open_to_read(&path).is_ok());
}

#[test]
fn a_file_that_is_not_a_book_is_never_read_as_one_without_file_locks() {
    let folder = TempFolder::new();
    let path = folder.join("Other.ksz");
    std::fs::write(&path, b"not a book at all").expect("file made");
    let _no_locks = NoLocks::here();
    assert!(matches!(
        Book::open_to_read(&path),
        Err(StoreError::NotABook)
    ));
    // Nothing is left of the copy.
    assert_eq!(std::fs::read(&path).expect("read"), b"not a book at all");
}
