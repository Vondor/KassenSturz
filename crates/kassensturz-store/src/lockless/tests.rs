// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;

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

/// Holds the book's write lock from a second connection, as a folder that
/// refuses SQLite's locks looks to the first open (B-304).
fn book_with_busy_write_lock(folder: &TempFolder) -> (PathBuf, Connection) {
    let path = folder.join("Household.ksz");
    drop(Book::create(&path, &computer(folder)).expect("book made"));
    let holder = Connection::open(&path).expect("opened");
    holder.execute_batch("BEGIN IMMEDIATE").expect("locked");
    (path, holder)
}

fn files_in(folder: &TempFolder) -> Vec<String> {
    let mut files: Vec<String> = std::fs::read_dir(folder.path())
        .expect("listed")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    files.sort();
    files
}

#[test]
fn a_folder_that_cant_lock_is_told_at_open_and_nothing_stays_behind() {
    let folder = TempFolder::new();
    let (path, _holder) = book_with_busy_write_lock(&folder);
    let before = files_in(&folder);
    SQLITE_CANT_LOCK.with(|flag| flag.set(true));
    let result = crate::connect(&path);
    SQLITE_CANT_LOCK.with(|flag| flag.set(false));
    assert!(
        matches!(result, Err(StoreError::LocksUnsupported)),
        "{:?}",
        result.err()
    );
    assert_eq!(files_in(&folder), before);
}

#[test]
fn a_busy_write_lock_in_a_folder_that_locks_fine_keeps_sqlites_own_error() {
    let folder = TempFolder::new();
    let (path, _holder) = book_with_busy_write_lock(&folder);
    let before = files_in(&folder);
    let result = crate::connect(&path);
    assert!(
        matches!(result, Err(StoreError::Database(_))),
        "{:?}",
        result.err()
    );
    assert_eq!(files_in(&folder), before, "the scratch file is gone");
}

#[test]
fn the_error_has_its_own_code_and_no_parameters() {
    use kassensturz_core::Report;
    let report = StoreError::LocksUnsupported.report();
    assert_eq!(report.code, "store.locks-unsupported");
    assert!(report.params.is_empty());
}

#[test]
fn a_book_taken_over_with_open_anyway_refuses_the_first_ones_next_change() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let computer = computer(&folder);
    let mut first = Book::create(&path, &computer).expect("book made");
    add(&mut first, "Checking");
    let Err(StoreError::Locked(seen)) = Book::open(&path, &computer) else {
        panic!("expected the book to be locked");
    };
    // The user says "open anyway" on another computer: it takes the lock.
    let mut second = Book::open_anyway(&path, &computer, &seen).expect("taken over");
    assert!(
        first.lock_taken_over(),
        "the first one can ask before it writes"
    );
    assert!(!second.lock_taken_over());
    let before = std::fs::read(&path).expect("book read");
    let result = first.as_one_action(crate::Action::AddAccount, |_| Ok::<(), StoreError>(()));
    assert!(matches!(result, Err(StoreError::LockLost)), "{result:?}");
    assert!(matches!(
        first.dry_run(|_| Ok::<(), StoreError>(())),
        Err(StoreError::LockLost)
    ));
    assert_eq!(std::fs::read(&path).expect("book read"), before);
    // The one that took over goes on, and its lock outlives the first.
    add(&mut second, "Savings");
    drop(first);
    assert!(crate::lock::lock_path(&path).exists());
    assert_eq!(names(&second), ["Checking", "Savings"]);
}

// B-343: marking a Rule as seen writes to the book too, so it is refused
// like any other write once another computer took the book over.
#[test]
fn marking_a_rule_seen_is_refused_once_the_book_was_taken_over() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let computer = computer(&folder);
    let mut first = Book::create(&path, &computer).expect("book made");
    first
        .mark_rule_checked(kassensturz_core::RuleId::generate())
        .expect("ours while nobody took it over");
    let Err(StoreError::Locked(seen)) = Book::open(&path, &computer) else {
        panic!("expected the book to be locked");
    };
    let _second = Book::open_anyway(&path, &computer, &seen).expect("taken over");
    let before = std::fs::read(&path).expect("book read");
    let result = first.mark_rule_checked(kassensturz_core::RuleId::generate());
    assert!(matches!(result, Err(StoreError::LockLost)), "{result:?}");
    assert_eq!(std::fs::read(&path).expect("book read"), before);
}

// B-316: a pulled drive takes the book and its lock file away together and
// may leave an empty folder; that is the drive's error, not a take-over.
#[test]
fn a_lock_file_gone_with_the_book_is_not_a_take_over() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let computer = computer(&folder);
    let book = Book::create(&path, &computer).expect("book made");
    assert!(!book.lock_taken_over());
    let lock = crate::lock::lock_path(&path);
    // The lock file alone is gone while the book stays: someone removed it.
    std::fs::remove_file(&lock).expect("lock removed");
    assert!(book.lock_taken_over());
    // The book goes too, the folder stays: the drive is gone. (A book that
    // is open can't be moved on Windows, so a lock of a book that was never
    // made stands in for it.)
    let gone = folder.join("Gone.ksz");
    let held = crate::lock::HeldLock::acquire(&gone, "Laptop").expect("lock taken");
    std::fs::remove_file(crate::lock::lock_path(&gone)).expect("lock removed");
    assert!(!held.is_taken_over());
    std::fs::write(&gone, b"x").expect("book made");
    assert!(held.is_taken_over());
}

// B-315: a book that another program holds shut at the database, with its
// lock file not visible here, gets the in-use page, and leaves no lock file
// of ours behind.
#[test]
fn a_busy_book_without_a_visible_lock_file_shows_the_in_use_page() {
    let folder = TempFolder::new();
    let (path, _holder) = book_with_busy_write_lock(&folder);
    let result = Book::open(&path, &computer(&folder));
    assert!(
        matches!(result, Err(StoreError::Locked(_))),
        "{:?}",
        result.err()
    );
    assert!(
        !crate::lock::lock_path(&path).exists(),
        "no lock file of ours is left"
    );
}

#[test]
fn only_busy_and_held_shut_errors_become_the_in_use_page() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let busy = StoreError::from(rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
        None,
    ));
    assert!(matches!(
        crate::lock::in_use_instead(&path, busy),
        StoreError::Locked(_)
    ));
    let other = StoreError::from(rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CORRUPT),
        None,
    ));
    assert!(matches!(
        crate::lock::in_use_instead(&path, other),
        StoreError::Database(_)
    ));
    assert!(matches!(
        crate::lock::in_use_instead(&path, StoreError::Io(std::io::ErrorKind::NotFound)),
        StoreError::Io(std::io::ErrorKind::NotFound)
    ));
}

// B-304: a cifs mount takes the write lock but fails the save, so the open
// has to look at the mount's type and try a whole save on a scratch file.
#[test]
fn the_mount_table_tells_a_share_from_a_local_folder() {
    let table = "\
25 1 8:2 / / rw - ext4 /dev/sda2 rw
40 25 0:35 / /media/tester/netz rw,relatime - cifs //server/share rw,vers=3.1.1
41 25 0:36 / /media/tester/my\\040disk rw - ext4 /dev/sdb1 rw
garbage line
";
    let kind = |folder: &str| mount_type_of(table, Path::new(folder));
    assert_eq!(kind("/media/tester/netz/b304").as_deref(), Some("cifs"));
    assert_eq!(kind("/media/tester/my disk/x").as_deref(), Some("ext4"));
    assert_eq!(kind("/home/tester").as_deref(), Some("ext4"));
    // Only a mount known to be a local disk skips the save check: a share,
    // an unknown type and an unreadable table (a snap, B-304) are checked.
    assert!(!is_local_type(kind("/media/tester/netz").as_deref()));
    assert!(is_local_type(kind("/home/tester").as_deref()));
    assert!(!is_local_type(None));
    assert!(!is_local_type(Some("fuse.gvfsd-fuse")));
}

/// A book whose folder takes the write lock but refuses the exclusive lock a
/// save needs: a second connection holds a read lock, as the mount does for
/// every save (B-304).
fn book_with_a_reader(folder: &TempFolder) -> (PathBuf, Connection) {
    let path = folder.join("Household.ksz");
    drop(Book::create(&path, &computer(folder)).expect("book made"));
    let reader = Connection::open(&path).expect("opened");
    reader.execute_batch("BEGIN").expect("began");
    reader
        .query_row("SELECT count(*) FROM sqlite_master", [], |row| {
            row.get::<_, i64>(0)
        })
        .expect("read, so the read lock is held");
    (path, reader)
}

#[test]
fn a_share_that_takes_the_write_lock_but_fails_the_save_is_told_at_open() {
    let folder = TempFolder::new();
    let (path, _reader) = book_with_a_reader(&folder);
    let before = files_in(&folder);
    let bytes = std::fs::read(&path).expect("book read");
    PRETEND_SHARE.with(|flag| flag.set(true));
    let result = crate::connect(&path);
    PRETEND_SHARE.with(|flag| flag.set(false));
    assert!(
        matches!(result, Err(StoreError::LocksUnsupported)),
        "{:?}",
        result.err()
    );
    assert_eq!(
        files_in(&folder),
        before,
        "no journal or scratch file stays"
    );
    assert_eq!(std::fs::read(&path).expect("book read"), bytes);
}

#[test]
fn a_share_where_a_save_works_opens_the_book_and_changes_nothing_in_it() {
    let folder = TempFolder::new();
    let path = folder.join("Household.ksz");
    let book = Book::create(&path, &computer(&folder)).expect("book made");
    let id = book.book_id();
    drop(book);
    let before = files_in(&folder);
    PRETEND_SHARE.with(|flag| flag.set(true));
    let result = crate::connect(&path);
    PRETEND_SHARE.with(|flag| flag.set(false));
    let (conn, _) = result.expect("a share where saving works opens");
    let version: i32 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("layout read");
    assert_eq!(version, crate::layout::VERSION);
    drop(conn);
    assert_eq!(files_in(&folder), before, "no journal file stays");
    let reopened = Book::open(&path, &computer(&folder)).expect("opens");
    assert_eq!(reopened.book_id(), id);
}

#[test]
fn a_local_folder_is_not_probed_with_a_save() {
    // The same reader that fails a share's open is no matter in a local
    // folder: nothing is written there at open.
    let folder = TempFolder::new();
    let (path, _reader) = book_with_a_reader(&folder);
    let result = crate::connect(&path);
    assert!(result.is_ok(), "{:?}", result.err());
}
