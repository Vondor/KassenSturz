// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use kassensturz_core::AccountKind;
#[cfg(target_os = "linux")]
use rusqlite::{Connection, OpenFlags};

use super::*;
use crate::test_folder::TempFolder;
use crate::tests::{EUR, account};
use crate::{Book, Computer, StoreError};
#[cfg(target_os = "linux")]
use kassensturz_core::Timestamp;

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

// Lock-less open exists only in the Linux snap (D-776).
#[cfg(target_os = "linux")]
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

// The program's process number and start time come from Linux's `/proc`;
// lock-less books exist only in the snap (D-776).
#[cfg(target_os = "linux")]
#[test]
fn a_lock_of_a_program_that_still_runs_here_is_not_taken_over_without_file_locks() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let computer = computer(&folder);
    let _no_locks = NoLocks::here();
    let _first = Book::create(&path, &computer).expect("book made");
    let Err(StoreError::Locked(seen)) = Book::open(&path, &computer) else {
        panic!("expected the book to be locked");
    };
    // The lock names this very program, which is running: "open anyway"
    // would put two writers on a file nothing locks.
    match Book::open_anyway(&path, &computer, &seen) {
        Err(StoreError::Locked(_)) => {}
        other => panic!("expected the lock to stay, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn a_lock_a_crash_left_behind_can_be_taken_over_without_file_locks() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let computer = computer(&folder);
    let _no_locks = NoLocks::here();
    drop(Book::create(&path, &computer).expect("book made"));
    // A lock file of a program that is gone: a process number that no
    // running program has with that start time.
    std::fs::write(
        crate::lock::lock_path(&path),
        "computer=Laptop\nsince=2026-10-19T11:04:02Z\ntoken=old\npid=4194000\nstart=1\n",
    )
    .expect("lock file written");
    let Err(StoreError::Locked(seen)) = Book::open(&path, &computer) else {
        panic!("expected the book to be locked");
    };
    assert!(Book::open_anyway(&path, &computer, &seen).is_ok());
}

// The program's process number and start time come from Linux's `/proc`;
// lock-less books exist only in the snap (D-776).
#[cfg(target_os = "linux")]
#[test]
fn a_lock_of_a_book_open_without_file_locks_is_not_taken_over_by_a_program_with_locks() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let computer = computer(&folder);
    let first = {
        let _no_locks = NoLocks::here();
        Book::create(&path, &computer).expect("book made")
    };
    // This side's locks work (the guard is gone), as for the command line.
    let Err(StoreError::Locked(seen)) = Book::open(&path, &computer) else {
        panic!("expected the book to be locked");
    };
    match Book::open_anyway(&path, &computer, &seen) {
        Err(StoreError::Locked(_)) => {}
        other => panic!("expected the lock to stay, got {:?}", other.map(|_| ())),
    }
    // The page is told, so it doesn't offer "open anyway" again.
    assert!(seen.stays_locked_here(&path, &computer.name));
    drop(first);
}

/// Rewrites the lock file's `key=` line, or adds it.
#[cfg(target_os = "linux")]
fn set_lock_line(path: &Path, key: &str, value: &str) {
    let lock = crate::lock::lock_path(path);
    let text = std::fs::read_to_string(&lock).expect("lock file read");
    let mut lines: Vec<String> = text
        .lines()
        .filter(|line| !line.starts_with(&format!("{key}=")))
        .map(str::to_owned)
        .collect();
    lines.push(format!("{key}={value}"));
    std::fs::write(lock, lines.join("\n") + "\n").expect("lock file written");
}

#[cfg(target_os = "linux")]
#[test]
fn a_lock_from_another_process_space_is_not_taken_over_beside_a_book_without_file_locks() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let computer = computer(&folder);
    drop(Book::create(&path, &computer).expect("book made"));
    // A running program of another PID namespace (a Flatpak seen from a
    // snap): its process number can't be looked up, so it may still run.
    let first = Book::open(&path, &computer).expect("opened");
    set_lock_line(&path, "namespace", "pid:[4026539999]");
    let _no_locks = NoLocks::here();
    let Err(StoreError::Locked(seen)) = Book::open(&path, &computer) else {
        panic!("expected the book to be locked");
    };
    match Book::open_anyway(&path, &computer, &seen) {
        Err(StoreError::Locked(_)) => {}
        other => panic!("expected the lock to stay, got {:?}", other.map(|_| ())),
    }
    drop(first);
}

#[cfg(target_os = "linux")]
#[test]
fn a_lock_from_before_a_restart_is_taken_over_without_file_locks() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let computer = computer(&folder);
    drop(Book::create(&path, &computer).expect("book made"));
    let first = Book::open(&path, &computer).expect("opened");
    // The same process number, but another start of the computer.
    set_lock_line(&path, "boot", "00000000-0000-0000-0000-000000000000");
    let _no_locks = NoLocks::here();
    let Err(StoreError::Locked(seen)) = Book::open(&path, &computer) else {
        panic!("expected the book to be locked");
    };
    assert!(Book::open_anyway(&path, &computer, &seen).is_ok());
    drop(first);
}

#[test]
fn a_book_without_file_locks_refuses_a_change_when_its_lock_file_was_taken_away() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let computer = computer(&folder);
    let _no_locks = NoLocks::here();
    let mut book = Book::create(&path, &computer).expect("book made");
    add(&mut book, "Checking");
    // A program that doesn't know about file locks being off (an older
    // build) opened it anyway and removed the lock file.
    std::fs::remove_file(crate::lock::lock_path(&path)).expect("lock file removed");
    let result = book.as_one_action(crate::Action::AddAccount, |_| Ok::<(), StoreError>(()));
    assert!(matches!(result, Err(StoreError::LockLost)), "{result:?}");
    assert_eq!(names(&book), ["Checking"]);
    // A lock file of another program is lost too.
    std::fs::write(
        crate::lock::lock_path(&path),
        "computer=Laptop\nsince=2026-10-19T11:04:02Z\ntoken=other\n",
    )
    .expect("lock file written");
    assert!(matches!(
        book.as_one_action(crate::Action::AddAccount, |_| Ok::<(), StoreError>(())),
        Err(StoreError::LockLost)
    ));
    // A dry run writes into the book too, and rolls back over what another
    // writer committed: it is refused the same way.
    assert!(matches!(
        book.dry_run(|_| Ok::<(), StoreError>(())),
        Err(StoreError::LockLost)
    ));
}

/// Writes the lock file an older build (without process, place or locks
/// note) wrote, as `since` says.
#[cfg(target_os = "linux")]
fn old_format_lock(path: &Path, since: &str) {
    std::fs::write(
        crate::lock::lock_path(path),
        format!("computer=Laptop\nsince={since}\ntoken=old\n"),
    )
    .expect("lock file written");
}

#[cfg(target_os = "linux")]
#[test]
fn a_lock_without_a_process_made_since_the_computer_started_is_not_taken_over_without_file_locks() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let computer = computer(&folder);
    drop(Book::create(&path, &computer).expect("book made"));
    // The Flatpak of before D-776 holds the book: computer, since, token.
    let now = Timestamp::from_second(Timestamp::now().as_second())
        .expect("a time")
        .to_string();
    old_format_lock(&path, &now);
    let _no_locks = NoLocks::here();
    let Err(StoreError::Locked(seen)) = Book::open(&path, &computer) else {
        panic!("expected the book to be locked");
    };
    assert!(seen.stays_locked_here(&path, &computer.name));
    match Book::open_anyway(&path, &computer, &seen) {
        Err(StoreError::Locked(_)) => {}
        other => panic!("expected the lock to stay, got {:?}", other.map(|_| ())),
    }
    // Where this side has file locks too, the old lock is taken over as
    // before: SQLite's locks keep two writers apart.
    drop(_no_locks);
    let Err(StoreError::Locked(seen)) = Book::open(&path, &computer) else {
        panic!("expected the book to be locked");
    };
    assert!(Book::open_anyway(&path, &computer, &seen).is_ok());
}

#[cfg(target_os = "linux")]
#[test]
fn a_lock_without_a_process_from_before_the_computer_started_is_taken_over_without_file_locks() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let computer = computer(&folder);
    drop(Book::create(&path, &computer).expect("book made"));
    old_format_lock(&path, "2020-01-01T00:00:00Z");
    let _no_locks = NoLocks::here();
    let Err(StoreError::Locked(seen)) = Book::open(&path, &computer) else {
        panic!("expected the book to be locked");
    };
    assert!(!seen.stays_locked_here(&path, &computer.name));
    assert!(Book::open_anyway(&path, &computer, &seen).is_ok());
}

#[test]
fn a_journal_with_a_zero_header_does_not_keep_readers_waiting() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let computer = computer(&folder);
    let _no_locks = NoLocks::here();
    drop(Book::create(&path, &computer).expect("book made"));
    // A program killed before SQLite synced the journal leaves one whose
    // header is still zero: the book was not touched, and SQLite ignores it.
    std::fs::write(journal_of(&path), [0u8; 512]).expect("journal made");
    assert!(Book::open_to_read(&path).is_ok());
    std::fs::write(journal_of(&path), []).expect("journal emptied");
    assert!(Book::open_to_read(&path).is_ok());
    // A journal with its header written is a write under way.
    let mut hot = vec![0u8; 512];
    hot[..8].copy_from_slice(&[0xd9, 0xd5, 0x05, 0xf9, 0x20, 0xa1, 0x63, 0xd7]);
    std::fs::write(journal_of(&path), hot).expect("journal made");
    assert!(matches!(
        Book::open_to_read(&path),
        Err(StoreError::Io(std::io::ErrorKind::WouldBlock))
    ));
}

#[test]
fn every_commit_changes_the_counter_a_copy_is_checked_against() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let computer = computer(&folder);
    let mut book = Book::create(&path, &computer).expect("book made");
    let before = change_counter(&path).expect("counter");
    add(&mut book, "Checking");
    assert_ne!(change_counter(&path).expect("counter"), before);
}

// Lock-less open exists only in the Linux snap (D-776).
#[cfg(target_os = "linux")]
#[test]
fn private_copies_a_killed_program_left_are_removed_when_old_enough() {
    let old = std::env::temp_dir().join(format!("{COPY_PREFIX}test-old-{}", uuid::Uuid::now_v7()));
    let young =
        std::env::temp_dir().join(format!("{COPY_PREFIX}test-young-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir(&old).expect("folder made");
    std::fs::create_dir(&young).expect("folder made");
    let two_hours_ago = std::time::SystemTime::now() - Duration::from_secs(7200);
    std::fs::File::open(&old)
        .expect("opened")
        .set_modified(two_hours_ago)
        .expect("time set");
    remove_stale();
    assert!(!old.exists(), "the old one is removed");
    assert!(young.exists(), "one that may still be in use stays");
    std::fs::remove_dir(&young).expect("removed");
}
