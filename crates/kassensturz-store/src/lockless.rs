// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Books in folders where the system refuses file locks (Q-570, D-776).
//!
//! A snap's AppArmor profile denies `flock` and `fcntl` locks in the folders
//! the desktop portal hands out, so SQLite can't open a book there with its
//! usual locks. Where that is the case, and only there:
//!
//! - the app opens the book with SQLite's `unix-none` VFS, which takes no
//!   OS locks. Single-writer is then kept by the book's lock file (D-038),
//!   which is plain file creation and works there. The rollback journal
//!   needs files and `fsync`, not locks, so a crash mid-write is still
//!   rolled back at the next open;
//! - every read-only open (the MCP server, backups, the checks of a move)
//!   reads a verified private copy instead of the live file, since without
//!   locks a reader can't tell that a write is under way.

use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::time::Duration;

use kassensturz_core::book_folder::{LockCheck, check_locks};
use rusqlite::{Connection, OpenFlags};

use crate::error::StoreError;

/// How often a private copy is tried when the book changes while it is
/// copied, and how long to wait between tries.
const COPY_TRIES: u32 = 12;
const COPY_WAIT: Duration = Duration::from_millis(150);

#[cfg(test)]
thread_local! {
    /// Makes the tests of this thread see every folder as one without locks.
    pub(crate) static NO_LOCKS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether the system refuses file locks in the folder `file` is in.
/// Nothing is learnt from a folder that can't be written to: it counts as
/// "locks work", and the ordinary errors follow.
pub(crate) fn locks_refused(file: &Path) -> bool {
    #[cfg(test)]
    if NO_LOCKS.with(std::cell::Cell::get) {
        return true;
    }
    // Only the sandboxes (a snap, a Flatpak) refuse locks; elsewhere nothing
    // is tried, so no stray file shows up in a sync folder.
    if !cfg!(unix) || !in_sandbox() {
        return false;
    }
    let Some(folder) = file.parent() else {
        return false;
    };
    check_locks(folder) == LockCheck::Refused
}

/// Whether the program runs in a snap or a Flatpak.
fn in_sandbox() -> bool {
    std::env::var_os("SNAP").is_some_and(|snap| !snap.is_empty())
        || Path::new("/.flatpak-info").exists()
}

/// Opens the book at `path` for writing, without OS locks if the folder
/// refuses them. Returns whether it did.
pub(crate) fn open_writer(path: &Path, flags: OpenFlags) -> Result<(Connection, bool), StoreError> {
    if locks_refused(path) {
        return Ok((open_without_locks(path, flags)?, true));
    }
    Ok((Connection::open_with_flags(path, flags)?, false))
}

#[cfg(unix)]
fn open_without_locks(path: &Path, flags: OpenFlags) -> Result<Connection, StoreError> {
    Ok(Connection::open_with_flags_and_vfs(
        path,
        flags,
        "unix-none",
    )?)
}

#[cfg(not(unix))]
fn open_without_locks(path: &Path, flags: OpenFlags) -> Result<Connection, StoreError> {
    Ok(Connection::open_with_flags(path, flags)?)
}

/// A folder only this user can read, in the computer's temporary folder,
/// that goes away when dropped. A copy of a book is in it for a moment.
pub(crate) struct PrivateFolder(PathBuf);

impl PrivateFolder {
    pub(crate) fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "kassensturz-copy-{}",
            uuid::Uuid::now_v7().hyphenated()
        ));
        #[cfg(unix)]
        {
            let mut builder = fs::DirBuilder::new();
            std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
            builder.create(&path)?;
        }
        // The user's temporary folder on Windows is theirs alone.
        #[cfg(not(unix))]
        fs::DirBuilder::new().create(&path)?;
        Ok(Self(path))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for PrivateFolder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A connection that only reads: the book's own file, or a private copy of
/// it where the folder refuses locks. The copy goes away with this value.
pub(crate) struct ReadOnly {
    conn: Connection,
    _copy: Option<PrivateFolder>,
}

impl Deref for ReadOnly {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        &self.conn
    }
}

impl ReadOnly {
    /// Splits it into the connection and what keeps its copy alive.
    pub(crate) fn into_parts(self) -> (Connection, Option<PrivateFolder>) {
        (self.conn, self._copy)
    }
}

/// Opens the file at `path` read-only. `path` must be absolute. Where the
/// folder refuses locks, a private copy is read, made only while no write
/// is under way: no `-journal` file, the same size and time before and
/// after copying, and a clean quick check of the copy.
pub(crate) fn open_read_only(path: &Path, flags: OpenFlags) -> Result<ReadOnly, StoreError> {
    if !locks_refused(path) {
        return Ok(ReadOnly {
            conn: Connection::open_with_flags(path, flags)?,
            _copy: None,
        });
    }
    let mut last = StoreError::Io(std::io::ErrorKind::WouldBlock);
    for attempt in 0..COPY_TRIES {
        if attempt > 0 {
            std::thread::sleep(COPY_WAIT);
        }
        match copy_once(path, flags) {
            Ok(read) => return Ok(read),
            // A book that isn't one, or can't be read, won't get better.
            Err(Retry::Never(error)) => return Err(error),
            Err(Retry::Maybe(error)) => last = error,
        }
    }
    Err(last)
}

enum Retry {
    Maybe(StoreError),
    Never(StoreError),
}

fn copy_once(path: &Path, flags: OpenFlags) -> Result<ReadOnly, Retry> {
    let busy = || Retry::Maybe(StoreError::Io(std::io::ErrorKind::WouldBlock));
    let never = |error: std::io::Error| Retry::Never(StoreError::Io(error.kind()));
    let journal = journal_of(path);
    if journal.exists() {
        return Err(busy());
    }
    let before = fs::metadata(path).map_err(never)?;
    let folder = PrivateFolder::new().map_err(never)?;
    let copy = folder.path().join("copy.ksz");
    fs::copy(path, &copy).map_err(never)?;
    let after = fs::metadata(path).map_err(never)?;
    if journal.exists()
        || before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
    {
        return Err(busy());
    }
    let conn =
        Connection::open_with_flags(&copy, flags).map_err(|error| Retry::Never(error.into()))?;
    // A copy taken between two writes is whole; one that isn't is tried again.
    let check: String = conn
        .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
        .map_err(|_| busy())?;
    if check != "ok" {
        return Err(busy());
    }
    Ok(ReadOnly {
        conn,
        _copy: Some(folder),
    })
}

/// SQLite's rollback journal for the book at `path`.
fn journal_of(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push("-journal");
    PathBuf::from(name)
}

#[cfg(test)]
mod tests;
