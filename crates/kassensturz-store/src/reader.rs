// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading a book that the app may have open at the same time (D-050,
//! Q-036, D-195): for `kassensturz-mcp`, which answers the user's own AI
//! tool from the book while KassenSturz runs.
//!
//! Such a reader never writes: SQLite opens the file read-only, so it takes
//! no lock file (D-038), never updates the layout and never touches the
//! change history. It keeps its reads short: each answer is read in one
//! read transaction ([`Book::reading`]), which holds SQLite's shared lock
//! only for that answer, and the app's write waits for it to end
//! ([`BUSY_WAIT`]) instead of failing, since the book keeps the classic
//! rollback journal (D-033).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use rusqlite::OpenFlags;

use crate::{Book, StoreError, cached, is_book_file, layout};

/// How long a connection waits for another one's lock before it gives up:
/// the app's write for a reader's answer, or a reader for the app's write.
pub(crate) const BUSY_WAIT: Duration = Duration::from_secs(5);

/// How often a reader looks whether a save it waits for is over.
const SAVE_POLL: Duration = Duration::from_millis(50);

/// Waits up to [`BUSY_WAIT`] while the book at `path` has a rollback
/// journal with its header written beside it, that is while a save is under way or one was cut
/// short, and then fails with [`StoreError::Locked`]: the book is in use.
/// SQLite's own locks say the same where they work; the journal says it
/// also where they don't reach the program that saves (B-368).
fn wait_for_save(path: &Path) -> Result<(), StoreError> {
    let journal = crate::recovery::journal_of(path);
    let started = Instant::now();
    // A journal that never got its header changed nothing (see
    // `journal_is_hot`), so it doesn't count.
    while crate::lockless::journal_is_hot(&journal) {
        if started.elapsed() >= BUSY_WAIT {
            return Err(StoreError::Locked(crate::lock::Lock::unreadable()));
        }
        std::thread::sleep(SAVE_POLL);
    }
    Ok(())
}

/// What tells whether the book's file was written to: its size, when it
/// was last changed, and SQLite's change counter in its header, which
/// every save raises. `None` where the file can't be read.
#[derive(PartialEq, Eq)]
struct FileState(Option<(u64, Option<SystemTime>, [u8; 4])>);

impl FileState {
    fn of(path: &Path) -> Self {
        use std::io::{Read, Seek, SeekFrom};
        let state = (|| {
            let mut file = std::fs::File::open(path).ok()?;
            let metadata = file.metadata().ok()?;
            let mut counter = [0u8; 4];
            file.seek(SeekFrom::Start(24)).ok()?;
            file.read_exact(&mut counter).ok()?;
            Some((metadata.len(), metadata.modified().ok(), counter))
        })();
        Self(state)
    }
}

impl Book {
    /// Opens the book at `path` to read it only, beside the app that may
    /// have it open (D-195).
    ///
    /// Takes no lock and changes nothing: SQLite opens the file read-only.
    /// Fails with [`StoreError::OlderLayout`] for a book whose layout the app
    /// must update first, and like [`Book::open`] for a file that isn't a
    /// book or a layout this version can't read.
    pub fn open_to_read(path: &Path) -> Result<Self, StoreError> {
        // A book another program is saving to for longer than a reader
        // waits is "in use", not a database error (B-363), also where
        // SQLite's locks don't reach the program that saves (B-368).
        wait_for_save(path)?;
        Self::open_to_read_at(path, layout::VERSION).map_err(|error| {
            if crate::lockless::journal_is_hot(&crate::recovery::journal_of(path)) {
                StoreError::Locked(crate::lock::Lock::unreadable())
            } else {
                crate::lock::in_use_instead(path, error)
            }
        })
    }

    /// [`Book::open_to_read`] for a KassenSturz whose current layout is
    /// `current`. That is [`layout::VERSION`] in every case but the test of
    /// an older layout, which passes a later one, since every book has the
    /// current layout until the first Beta has a successor (D-135, D-536).
    fn open_to_read_at(path: &Path, current: i32) -> Result<Self, StoreError> {
        let path = std::path::absolute(path)?;
        let metadata = std::fs::metadata(&path).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => StoreError::NotFound,
            kind => StoreError::Io(kind),
        })?;
        if !metadata.is_file() || !is_book_file(&path)? {
            return Err(StoreError::NotABook);
        }
        // `path` is absolute, so SQLite never takes it for a URI, and
        // without SQLITE_OPEN_CREATE it never creates a file.
        let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let (conn, copy) = crate::lockless::open_read_only(&path, flags)?.into_parts();
        crate::harden(&conn)?;
        conn.busy_timeout(BUSY_WAIT)?;
        // A second guard, should a statement ever try to write anyway.
        conn.pragma_update(None, "query_only", true)?;
        crate::integrity::check_file(&conn)?;
        let version: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        match version {
            same if same == current => {}
            older if (layout::FIRST_VERSION..current).contains(&older) => {
                return Err(StoreError::OlderLayout { found: older });
            }
            found
                if (layout::FIRST_DEVELOPMENT_VERSION..layout::FIRST_VERSION).contains(&found) =>
            {
                return Err(StoreError::DevelopmentLayout { found });
            }
            found => return Err(StoreError::UnsupportedLayout { found }),
        }
        // A book with SQL another program planted is refused here too, so
        // the MCP server never reads through it (B-245).
        layout::check_schema(&conn, &layout::MIGRATIONS, layout::FIRST_VERSION, current)?;
        // The same checks as the app makes (B-291): the MCP server and the
        // command line never read figures from a damaged book either.
        crate::integrity::check_references(&conn)?;
        crate::integrity::check_balances(&conn)?;
        crate::integrity::check_conversions(&conn)?;
        conn.set_prepared_statement_cache_capacity(cached::CAPACITY);
        Ok(Self {
            conn,
            recording: false,
            filling: false,
            write_failed: false,
            file_replaced: false,
            _lock: None,
            without_file_locks: copy.is_some(),
            _copy: copy,
            cut_short: None,
            parked_ai_marks: std::collections::HashSet::new(),
        })
    }

    /// Runs `read` in one read transaction, so that everything it reads
    /// comes from the same state of the book, and ends the transaction
    /// right after, so the app can write again (Q-036).
    ///
    /// Inside a transaction that is already open, such as the read the MCP
    /// server wraps around a whole call, `read` runs in that one: SQLite
    /// can't start a transaction within a transaction, and the outer one
    /// already gives `read` one state of the book (M6-B198).
    pub fn reading<T, E>(&self, read: impl FnOnce(&Self) -> Result<T, E>) -> Result<T, E>
    where
        E: From<StoreError>,
    {
        if !self.conn.is_autocommit() {
            return read(self);
        }
        let path = PathBuf::from(self.conn.path().unwrap_or_default());
        wait_for_save(&path)?;
        let before = FileState::of(&path);
        self.conn
            .execute_batch("BEGIN DEFERRED")
            .map_err(StoreError::from)?;
        // SQLite's shared lock is taken here, before `read` runs: a book the
        // app is in the middle of a long save to is "in use", not a database
        // error from whichever read met it first (B-368). Once held, the
        // app's save waits for this read to end ([`BUSY_WAIT`]).
        if let Err(error) = self
            .conn
            .query_row("SELECT count(*) FROM sqlite_schema", [], |row| {
                row.get::<_, i64>(0)
            })
        {
            let _ = self.conn.execute_batch("ROLLBACK");
            let error = if crate::lockless::journal_is_hot(&crate::recovery::journal_of(&path)) {
                StoreError::Locked(crate::lock::Lock::unreadable())
            } else {
                crate::lock::in_use_instead(&path, StoreError::from(error))
            };
            return Err(error.into());
        }
        let result = read(self);
        // While this read holds SQLite's shared lock, nothing writes to the
        // book's file. Where the locks don't reach the program that saves,
        // such as a book on a network share opened through the desktop
        // portal, the file can change under the read: then what was read
        // may be half old and half new, and the answer is "in use" instead
        // (B-368). Looked at before the read ends, while a save that does
        // see the lock still waits for it.
        if FileState::of(&path) != before {
            let _ = self.conn.execute_batch("ROLLBACK");
            return Err(StoreError::Locked(crate::lock::Lock::unreadable()).into());
        }
        // A read transaction has nothing to keep, so it ends the same way
        // whether `read` succeeded or not.
        match self.conn.execute_batch("COMMIT") {
            Ok(()) => result,
            Err(error) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                result.and(Err(StoreError::from(error).into()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use std::sync::mpsc;
    use std::thread;

    use kassensturz_core::{AccountKind, RoundingMode};

    use super::*;
    use crate::Computer;
    use crate::test_folder::TempFolder;
    use crate::tests::{EUR, account, date, eur, transaction};

    fn computer(folder: &TempFolder) -> Computer {
        Computer {
            name: "Laptop".into(),
            backup_folder: folder.join("backups"),
        }
    }

    /// A book on disk with one account and one transaction, closed again.
    fn book_on_disk(folder: &TempFolder) -> std::path::PathBuf {
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

    fn files_in(folder: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(folder)
            .expect("folder read")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_reader_reads_the_book_the_app_has_open_and_leaves_no_file() {
        let folder = TempFolder::new();
        let path = book_on_disk(&folder);
        let app = Book::open(&path, &computer(&folder)).expect("the app opens it");
        let before = files_in(folder.path());

        let reader = Book::open_to_read(&path).expect("read beside the app");
        let accounts = reader
            .reading(|book| book.accounts(false))
            .expect("accounts read");
        assert_eq!(accounts.len(), 1);
        drop(reader);

        // No lock file, journal or anything else was left by the reader.
        assert_eq!(files_in(folder.path()), before);
        drop(app);
    }

    /// B-363: a reader that opens the book while the app is in the middle
    /// of a long save is told the book is in use, not a database error.
    #[test]
    fn a_reader_meeting_a_long_save_is_told_the_book_is_in_use() {
        let folder = TempFolder::new();
        let path = book_on_disk(&folder);
        let writer = Connection::open(&path).expect("opened");
        writer
            .execute_batch("BEGIN EXCLUSIVE")
            .expect("a save under way");
        let read = Book::open_to_read(&path);
        writer.execute_batch("ROLLBACK").expect("rolled back");
        assert!(
            matches!(read, Err(StoreError::Locked(_))),
            "{:?}",
            read.err()
        );
    }

    /// B-368: a reader that opened the book before the app began a long
    /// save is told the book is in use when it reads, not a database error.
    #[test]
    fn a_read_during_a_long_save_is_told_the_book_is_in_use() {
        let folder = TempFolder::new();
        let path = book_on_disk(&folder);
        let reader = Book::open_to_read(&path).expect("opened to read");
        let writer = Connection::open(&path).expect("opened");
        writer
            .execute_batch("BEGIN EXCLUSIVE")
            .expect("a save under way");
        let read = reader.reading(|book| book.accounts(false));
        writer.execute_batch("ROLLBACK").expect("rolled back");
        assert!(
            matches!(read, Err(StoreError::Locked(_))),
            "{:?}",
            read.err()
        );
        // Once the save is over, the same reader reads again.
        assert!(reader.reading(|book| book.accounts(false)).is_ok());
    }

    /// A connection to the book whose locks no reader sees, like the app
    /// saving to a book on a network share through the desktop portal,
    /// while the MCP server reads the share (B-368).
    #[cfg(unix)]
    fn saving_unseen(path: &Path) -> Connection {
        Connection::open_with_flags_and_vfs(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
            "unix-none",
        )
        .expect("opened without locks")
    }

    /// B-368 on a share: a save whose locks the reader can't see is under
    /// way. Reading and opening say "in use", not a database error, and
    /// both work again once the save is over.
    #[cfg(unix)]
    #[test]
    fn a_save_the_reader_cant_see_the_locks_of_makes_the_book_in_use() {
        let folder = TempFolder::new();
        let path = book_on_disk(&folder);
        let reader = Book::open_to_read(&path).expect("opened to read");
        let writer = saving_unseen(&path);
        // A save large enough that SQLite writes pages of the book before
        // it ends, as a large import does.
        writer
            .execute_batch(
                "PRAGMA cache_size = 1; BEGIN;
                 CREATE TABLE filler (x);
                 WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 500)
                 INSERT INTO filler SELECT randomblob(1000) FROM n;",
            )
            .expect("a save under way");
        assert!(crate::lockless::journal_is_hot(
            &crate::recovery::journal_of(&path)
        ));
        let read = reader.reading(|book| book.accounts(false));
        assert!(
            matches!(read, Err(StoreError::Locked(_))),
            "{:?}",
            read.err()
        );
        let opened = Book::open_to_read(&path);
        assert!(
            matches!(opened, Err(StoreError::Locked(_))),
            "{:?}",
            opened.err()
        );
        // The save is taken back, and the book can be read again.
        writer.execute_batch("ROLLBACK").expect("taken back");
        assert!(reader.reading(|book| book.accounts(false)).is_ok());
        assert!(Book::open_to_read(&path).is_ok());
    }

    /// B-368 on a share: a save the reader can't see the locks of ends in
    /// the middle of a read. What was read may be half old and half new,
    /// so the answer is "in use"; the next read reads the saved book.
    #[cfg(unix)]
    #[test]
    fn a_save_during_a_read_that_cant_see_its_locks_makes_the_read_in_use() {
        let folder = TempFolder::new();
        let path = book_on_disk(&folder);
        let reader = Book::open_to_read(&path).expect("opened to read");
        let writer = saving_unseen(&path);
        let read = reader.reading(|book| {
            let accounts = book.accounts(false)?;
            writer
                .execute_batch("UPDATE accounts SET name = name || ' saved'")
                .expect("saved under the read");
            Ok::<_, StoreError>(accounts)
        });
        assert!(
            matches!(read, Err(StoreError::Locked(_))),
            "{:?}",
            read.err()
        );
        let names: Vec<String> = reader
            .reading(|book| book.accounts(false))
            .expect("read again")
            .into_iter()
            .map(|account| account.name)
            .collect();
        assert!(
            names.iter().all(|name| name.ends_with(" saved")),
            "{names:?}"
        );
    }

    #[test]
    fn a_read_inside_a_read_runs_in_the_outer_one_and_the_book_can_be_written_after() {
        // The MCP server reads every call in one read, and a report the
        // call makes reads again (M6-B198).
        let folder = TempFolder::new();
        let path = book_on_disk(&folder);
        let mut app = Book::open(&path, &computer(&folder)).expect("the app opens it");
        let checking = app.accounts(false).expect("read")[0].id;
        let reader = Book::open_to_read(&path).expect("opened to read");
        let count = reader
            .reading(|book| {
                let inner = book.reading(|book| book.accounts(false))?;
                // Still inside the outer read after the inner one ended.
                assert!(!book.conn.is_autocommit());
                Ok::<_, StoreError>(inner.len() + book.accounts(false)?.len())
            })
            .expect("read twice");
        assert_eq!(count, 2);
        // The outer read ended, so nothing holds the app's write back.
        assert!(reader.conn.is_autocommit());
        app.rename_account(checking, "Everyday account")
            .expect("written");
        // An error inside the inner read still ends the outer one.
        let failed = reader.reading(|book| {
            book.reading(|_| Err::<(), _>(StoreError::from(rusqlite::Error::InvalidQuery)))
        });
        assert!(failed.is_err());
        assert!(reader.conn.is_autocommit());
    }

    #[test]
    fn a_reader_never_writes() {
        let folder = TempFolder::new();
        let path = book_on_disk(&folder);
        let mut reader = Book::open_to_read(&path).expect("opened to read");
        let checking = reader.accounts(false).expect("read")[0].id;
        assert!(reader.rename_account(checking, "New name").is_err());
        assert!(reader.set_rounding_mode(RoundingMode::HalfEven).is_err());
        assert_eq!(reader.account(checking).expect("read").name, "Checking");
        assert_eq!(
            reader.rounding_mode().expect("read"),
            RoundingMode::default()
        );
    }

    #[test]
    fn the_apps_write_waits_for_a_readers_answer() {
        let folder = TempFolder::new();
        let path = book_on_disk(&folder);
        let mut app = Book::open(&path, &computer(&folder)).expect("the app opens it");
        let checking = app.accounts(false).expect("read")[0].id;

        let (reading, read_started) = mpsc::channel();
        let (finish, may_finish) = mpsc::channel::<()>();
        let reader_path = path.clone();
        let reader = thread::spawn(move || {
            let reader = Book::open_to_read(&reader_path).expect("opened to read");
            reader
                .reading(|book| {
                    let accounts = book.accounts(false)?;
                    reading.send(()).expect("told");
                    // The app tries to write while this answer is read.
                    may_finish.recv().expect("told");
                    thread::sleep(Duration::from_millis(200));
                    Ok::<_, StoreError>(accounts.len())
                })
                .expect("read")
        });
        read_started.recv().expect("reading");
        finish.send(()).expect("told");
        app.rename_account(checking, "Everyday account")
            .expect("the write waited for the reader instead of failing");
        assert_eq!(reader.join().expect("reader ended"), 1);

        let reader = Book::open_to_read(&path).expect("opened to read");
        assert_eq!(
            reader.account(checking).expect("read").name,
            "Everyday account"
        );
    }

    #[test]
    fn a_book_with_an_older_layout_is_left_for_the_app_to_update() {
        let folder = TempFolder::new();
        let path = book_on_disk(&folder);
        let conn = Connection::open(&path).expect("opened");
        conn.pragma_update(None, "user_version", layout::FIRST_VERSION)
            .expect("set");
        drop(conn);
        // As the KassenSturz with the next layout will see it.
        assert!(matches!(
            Book::open_to_read_at(&path, layout::VERSION + 1),
            Err(StoreError::OlderLayout { found }) if found == layout::FIRST_VERSION
        ));
        // Nothing was backed up or updated.
        assert!(!folder.join("backups").exists());
        let conn = Connection::open(&path).expect("opened");
        let version: i32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read");
        assert_eq!(version, layout::FIRST_VERSION);
    }

    #[test]
    fn other_files_and_layouts_are_refused_like_the_app_does() {
        let folder = TempFolder::new();
        assert!(matches!(
            Book::open_to_read(&folder.join("missing.ksz")),
            Err(StoreError::NotFound)
        ));
        let text = folder.join("notes.txt");
        std::fs::write(&text, "not a book").expect("written");
        assert!(matches!(
            Book::open_to_read(&text),
            Err(StoreError::NotABook)
        ));
        let path = book_on_disk(&folder);
        let conn = Connection::open(&path).expect("opened");
        conn.pragma_update(None, "user_version", layout::VERSION + 1)
            .expect("set");
        drop(conn);
        assert!(matches!(
            Book::open_to_read(&path),
            Err(StoreError::UnsupportedLayout { found }) if found == layout::VERSION + 1
        ));
    }
}
