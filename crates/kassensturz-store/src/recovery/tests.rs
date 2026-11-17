// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of taking back a save that was cut short (B-364). All data is made
//! up (AGENTS.md §3.1 rule 2).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use kassensturz_core::AccountKind;

use super::*;
use crate::test_folder::TempFolder;
use crate::tests::{EUR, account, date, eur, transaction};
use crate::{Book, Computer};

/// Where the helper below writes, and the file it makes once its save is
/// under way.
const BOOK_VARIABLE: &str = "KASSENSTURZ_B364_BOOK";

fn computer(folder: &TempFolder) -> Computer {
    Computer {
        name: "Laptop".into(),
        backup_folder: folder.join("backups"),
    }
}

/// A closed book with one account and one transaction.
fn book_on_disk(folder: &TempFolder) -> PathBuf {
    let path = folder.join("Household.ksz");
    let mut book = Book::create(&path, &computer(folder)).expect("book created");
    book.use_currency(EUR).expect("currency used");
    let checking = account(&mut book, "Checking", AccountKind::Asset, EUR);
    let opening = account(&mut book, "Opening", AccountKind::Equity, EUR);
    book.add_transaction(&transaction(
        date(2026, 10, 1),
        "Start",
        &[(checking, eur(10_000)), (opening, eur(-10_000))],
    ))
    .expect("added");
    path
}

fn ready_marker(book: &Path) -> PathBuf {
    let mut name = book.as_os_str().to_owned();
    name.push(".ready");
    PathBuf::from(name)
}

/// Starts a save to `book` too big for SQLite's page cache, so changed
/// pages are already in the file, and keeps it open.
fn start_long_save(book: &Path) -> rusqlite::Connection {
    let conn = rusqlite::Connection::open(book).expect("opened");
    conn.execute_batch(
        "PRAGMA cache_size = 5;
         BEGIN;
         CREATE TABLE half_done (x BLOB);
         WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 2000)
         INSERT INTO half_done SELECT randomblob(4000) FROM n;
         UPDATE accounts SET name = 'Half saved';",
    )
    .expect("save under way");
    conn
}

/// Not a test of its own: the program the crash test kills in the middle
/// of a save. It does nothing unless that test started it.
#[test]
#[ignore = "run by a_save_cut_short_by_a_killed_program_is_taken_back"]
fn helper_saving_until_killed() {
    let Some(book) = std::env::var_os(BOOK_VARIABLE).map(PathBuf::from) else {
        return;
    };
    let _conn = start_long_save(&book);
    std::fs::write(ready_marker(&book), "").expect("marker written");
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn account_names(book: &Book) -> Vec<String> {
    let mut names: Vec<String> = book
        .accounts(false)
        .expect("accounts read")
        .into_iter()
        .map(|account| account.name)
        .collect();
    names.sort();
    names
}

/// Copies `book` and its journal to `to`, as they are while a save is under
/// way: what a crash at this moment leaves behind.
fn crashed_copy(book: &Path, to: &Path) {
    let conn = start_long_save(book);
    std::fs::copy(book, to).expect("book copied");
    std::fs::copy(journal_of(book), journal_of(to)).expect("journal copied");
    drop(conn);
}

#[test]
fn a_save_cut_short_by_a_killed_program_is_taken_back() {
    let folder = TempFolder::new();
    let path = book_on_disk(&folder);
    let mut child = Command::new(std::env::current_exe().expect("test program"))
        .args([
            "--exact",
            "recovery::tests::helper_saving_until_killed",
            "--ignored",
        ])
        .env(BOOK_VARIABLE, &path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("helper started");
    let until = Instant::now() + Duration::from_secs(60);
    while !ready_marker(&path).exists() {
        assert!(Instant::now() < until, "the helper never started its save");
        std::thread::sleep(Duration::from_millis(50));
    }
    // SIGKILL on Unix, TerminateProcess on Windows: nothing is cleaned up.
    child.kill().expect("helper killed");
    child.wait().expect("helper ended");
    assert!(journal_is_hot(&journal_of(&path)), "the save was cut short");

    let book = Book::open(&path, &computer(&folder)).expect("the book opens");
    assert_eq!(account_names(&book), ["Checking"]);
    assert!(!journal_of(&path).exists());
    // The app is told, so it can say the last changes may be missing.
    assert_eq!(book.cut_short_save(), Some(&crate::CutShortSave::TakenBack));
    drop(book);
    // Opened again, there is nothing to tell.
    let again = Book::open(&path, &computer(&folder)).expect("the book opens");
    assert_eq!(again.cut_short_save(), None);
    drop(again);
    // Taken back in place: nothing was put beside the book.
    let mut names: Vec<String> = std::fs::read_dir(folder.path())
        .expect("folder read")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.contains("cut-short"))
        .collect();
    names.sort();
    assert_eq!(names, Vec::<String>::new());
}

#[test]
fn where_sqlite_cant_take_a_save_back_in_place_a_private_copy_does() {
    let folder = TempFolder::new();
    let original = book_on_disk(&folder);
    let path = folder.join("Crashed.ksz");
    crashed_copy(&original, &path);
    let cut_short = std::fs::read(&path).expect("read");
    let journal = std::fs::read(journal_of(&path)).expect("read");

    IN_PLACE_FAILS.set(true);
    let outcome = finish_cut_short_save(&path);
    IN_PLACE_FAILS.set(false);
    let Ok(Outcome::ThroughCopy(kept)) = outcome else {
        panic!("{outcome:?}");
    };
    // The cut-short pair is kept unchanged under a name of its own.
    assert_eq!(std::fs::read(&kept).expect("kept"), cut_short);
    assert_eq!(std::fs::read(journal_of(&kept)).expect("kept"), journal);
    assert!(!journal_of(&path).exists());
    // The book in its place is the one from before the save, and only
    // that size: the pages the save added weren't copied.
    let book = Book::open(&path, &computer(&folder)).expect("the book opens");
    assert_eq!(account_names(&book), ["Checking"]);
    drop(book);
    assert!(std::fs::metadata(&path).expect("size").len() < cut_short.len() as u64);
    // No file of the step is left over.
    let left: Vec<String> = std::fs::read_dir(folder.path())
        .expect("folder read")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.contains("taken-back"))
        .collect();
    assert_eq!(left, Vec::<String>::new());
}

#[test]
fn a_save_that_cant_be_taken_back_at_all_leaves_the_book_as_it_was() {
    let folder = TempFolder::new();
    let original = book_on_disk(&folder);
    let path = folder.join("Crashed.ksz");
    crashed_copy(&original, &path);
    let cut_short = std::fs::read(&path).expect("read");
    let journal = std::fs::read(journal_of(&path)).expect("read");

    IN_PLACE_FAILS.set(true);
    COPY_FAILS.set(true);
    let opened = Book::open(&path, &computer(&folder));
    IN_PLACE_FAILS.set(false);
    COPY_FAILS.set(false);
    assert!(matches!(opened, Err(StoreError::UnfinishedSave)));
    assert_eq!(std::fs::read(&path).expect("read"), cut_short);
    assert_eq!(std::fs::read(journal_of(&path)).expect("read"), journal);
    // A plain retry once the folder allows it takes the save back.
    let book = Book::open(&path, &computer(&folder)).expect("the book opens");
    assert_eq!(account_names(&book), ["Checking"]);
}

#[test]
fn a_reader_meeting_a_save_cut_short_is_told_the_book_is_in_use() {
    let folder = TempFolder::new();
    let original = book_on_disk(&folder);
    let path = folder.join("Crashed.ksz");
    crashed_copy(&original, &path);
    let before = std::fs::read(&path).expect("read");
    // SQLite can't take the save back on a read-only connection, which
    // was a general database error (B-364).
    assert!(matches!(
        Book::open_to_read(&path),
        Err(StoreError::Locked(_))
    ));
    assert_eq!(std::fs::read(&path).expect("read"), before);
}

#[test]
fn the_size_before_a_save_is_read_from_the_journal() {
    let folder = TempFolder::new();
    let original = book_on_disk(&folder);
    let size = std::fs::metadata(&original).expect("size").len();
    let path = folder.join("Crashed.ksz");
    crashed_copy(&original, &path);
    assert_eq!(
        size_before_the_save(&journal_of(&path)).expect("read"),
        Some(size)
    );
    // A journal without SQLite's header: everything is copied.
    let other = folder.join("other-journal");
    std::fs::write(&other, [1u8; 40]).expect("written");
    assert_eq!(size_before_the_save(&other).expect("read"), None);
}

#[test]
fn a_book_opened_through_a_private_copy_says_where_the_cut_short_file_is() {
    let folder = TempFolder::new();
    let original = book_on_disk(&folder);
    let path = folder.join("Crashed.ksz");
    crashed_copy(&original, &path);
    IN_PLACE_FAILS.set(true);
    let opened = Book::open(&path, &computer(&folder));
    IN_PLACE_FAILS.set(false);
    let book = opened.expect("the book opens");
    let Some(crate::CutShortSave::KeptBeside(kept)) = book.cut_short_save() else {
        panic!("{:?}", book.cut_short_save());
    };
    assert!(kept.exists());
    assert!(journal_of(kept).exists());
    assert_eq!(kept.parent(), path.parent());
}
