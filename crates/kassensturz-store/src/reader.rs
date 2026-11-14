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

use std::path::Path;
use std::time::Duration;

use rusqlite::OpenFlags;

use crate::{Book, StoreError, cached, is_book_file, layout};

/// How long a connection waits for another one's lock before it gives up:
/// the app's write for a reader's answer, or a reader for the app's write.
pub(crate) const BUSY_WAIT: Duration = Duration::from_secs(5);

impl Book {
    /// Opens the book at `path` to read it only, beside the app that may
    /// have it open (D-195).
    ///
    /// Takes no lock and changes nothing: SQLite opens the file read-only.
    /// Fails with [`StoreError::OlderLayout`] for a book whose layout the app
    /// must update first, and like [`Book::open`] for a file that isn't a
    /// book or a layout this version can't read.
    pub fn open_to_read(path: &Path) -> Result<Self, StoreError> {
        Self::open_to_read_at(path, layout::VERSION)
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
        self.conn
            .execute_batch("BEGIN DEFERRED")
            .map_err(StoreError::from)?;
        let result = read(self);
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
