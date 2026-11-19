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
/// What the names of the private folders begin with.
const COPY_PREFIX: &str = "kassensturz-copy-";

#[cfg(test)]
thread_local! {
    /// Makes the tests of this thread see every folder as one without locks.
    pub(crate) static NO_LOCKS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether the system refuses file locks in the folder `file` is in.
/// Nothing is learnt from a folder that can't be written to: it counts as
/// "locks work", and the ordinary errors follow. Only a snap's portal
/// folders refuse locks, so only those are tried, once each per program
/// run: nowhere else does a probe file ever appear in a user's folder.
pub(crate) fn locks_refused(file: &Path) -> bool {
    #[cfg(test)]
    if NO_LOCKS.with(std::cell::Cell::get) {
        return true;
    }
    if !cfg!(unix) || !in_snap() {
        return false;
    }
    let Some(folder) = file.parent() else {
        return false;
    };
    if !in_portal_mount(folder) {
        return false;
    }
    static KNOWN: std::sync::Mutex<Vec<(PathBuf, bool)>> = std::sync::Mutex::new(Vec::new());
    let mut known = KNOWN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((_, refused)) = known.iter().find(|(known, _)| known == folder) {
        return *refused;
    }
    let refused = check_locks(folder) == LockCheck::Refused;
    known.push((folder.to_owned(), refused));
    refused
}

/// Whether `folder` is in the document portal's mount, `doc` in the user's
/// runtime folder; a snap's own `$XDG_RUNTIME_DIR` is one below it.
fn in_portal_mount(folder: &Path) -> bool {
    let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) else {
        return false;
    };
    let own = runtime
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("snap."));
    let user = match runtime.parent() {
        Some(parent) if own => parent,
        _ => runtime.as_path(),
    };
    folder.starts_with(user.join("doc"))
}

/// Whether the program runs in a snap.
fn in_snap() -> bool {
    std::env::var_os("SNAP").is_some_and(|snap| !snap.is_empty())
}

/// Opens the book at `path` for writing, without OS locks if the folder
/// refuses them. Returns whether it did.
pub(crate) fn open_writer(path: &Path, flags: OpenFlags) -> Result<(Connection, bool), StoreError> {
    if locks_refused(path) {
        return Ok((open_without_locks(path, flags)?, true));
    }
    Ok((Connection::open_with_flags(path, flags)?, false))
}

/// How long the check below waits for a write lock. Nothing but a second
/// writer holds it, which the book's lock file keeps out, so a short wait
/// is enough.
const WRITE_LOCK_WAIT: Duration = if cfg!(test) {
    Duration::from_millis(100)
} else {
    Duration::from_secs(1)
};

#[cfg(test)]
thread_local! {
    /// Makes the tests of this thread see a folder where SQLite can't lock
    /// even a file of its own (B-304).
    pub(crate) static SQLITE_CANT_LOCK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
thread_local! {
    /// Makes the tests of this thread see every folder as one on a network
    /// share (B-304).
    pub(crate) static PRETEND_SHARE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Checks at open that SQLite can take the write lock in the book's folder
/// (B-304). Some network shares (a cifs mount with byte-range locks, say)
/// refuse it, and the book would open and then fail at its first save with
/// nothing the user could act on. If the lock is busy, a scratch database in
/// the same folder is tried: if that can't be locked either, the folder is
/// the cause and [`StoreError::LocksUnsupported`] says so; if it can, the
/// busy lock is real and SQLite's own error stands. The book is not opened
/// without locks here: two writers could then damage it (D-038, D-776).
pub(crate) fn check_write_lock(conn: &Connection, book: &Path) -> Result<(), StoreError> {
    conn.busy_timeout(WRITE_LOCK_WAIT)?;
    let taken = conn.execute_batch("BEGIN IMMEDIATE; ROLLBACK");
    let Err(error) = taken else {
        // Taking the write lock works, but a save also needs the exclusive
        // lock: [`check_save`] tries that.
        return Ok(());
    };
    let _ = conn.execute_batch("ROLLBACK");
    if !is_busy(&error) {
        // Not a question of locks (a read-only book, say): the ordinary
        // errors follow where the book is used.
        return Ok(());
    }
    if book.parent().is_some_and(scratch_cant_lock) {
        return Err(StoreError::LocksUnsupported);
    }
    Err(error.into())
}

/// Checks, on a network share, that the book can really be saved there
/// (B-304): the whole of a save is done once on the book itself, with the
/// journal and sync settings it is used with. A cifs mount with byte-range
/// locks takes the write lock at open and refuses the exclusive lock a save
/// needs, so the book would open and fail at the user's first save. Nothing
/// logical is written: the layout number is set to the value it has, which
/// SQLite still writes as a save. A failure that isn't about the file's
/// permissions, space or damage is [`StoreError::LocksUnsupported`]. Local
/// folders are left alone, and where a save works the book opens as ever.
pub(crate) fn check_save(conn: &Connection, book: &Path) -> Result<(), StoreError> {
    if !may_be_on_a_share(book) {
        return Ok(());
    }
    conn.busy_timeout(WRITE_LOCK_WAIT)?;
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let tried = conn.execute_batch(&format!(
        "BEGIN IMMEDIATE; PRAGMA user_version = {version}; COMMIT"
    ));
    let Err(error) = tried else {
        return Ok(());
    };
    let _ = conn.execute_batch("ROLLBACK");
    use rusqlite::ErrorCode::{
        DatabaseCorrupt, DiskFull, NotADatabase, PermissionDenied, ReadOnly,
    };
    match error.sqlite_error_code() {
        Some(ReadOnly | PermissionDenied | DiskFull | NotADatabase | DatabaseCorrupt) | None => {
            Err(error.into())
        }
        Some(_) => Err(StoreError::LocksUnsupported),
    }
}

fn is_busy(error: &rusqlite::Error) -> bool {
    matches!(
        error.sqlite_error_code(),
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)
    )
}

/// Whether the save check must run for `file`: unless the mount table says
/// the file is on a local disk, it may be a share, and a snap or another
/// sandbox can hide the mount table or its file system type (B-304 round 5).
/// A save that works costs one small write; a wrongly skipped check costs
/// the user's first save.
pub(crate) fn may_be_on_a_share(file: &Path) -> bool {
    #[cfg(test)]
    if PRETEND_SHARE.with(std::cell::Cell::get) {
        return true;
    }
    #[cfg(test)]
    if SQLITE_CANT_LOCK.with(std::cell::Cell::get) {
        return false;
    }
    // Only Linux has a mount table to ask; elsewhere the check stays as it was.
    cfg!(target_os = "linux") && !is_local_type(mount_type(file).as_deref())
}

/// The file system type that holds `file`, or `None` if the mount table
/// can't be read or doesn't name one.
fn mount_type(file: &Path) -> Option<String> {
    let table = fs::read_to_string("/proc/self/mountinfo").ok()?;
    let folder = fs::canonicalize(file.parent()?).ok()?;
    mount_type_of(&table, &folder)
}

fn is_local_type(kind: Option<&str>) -> bool {
    matches!(
        kind,
        Some(
            "ext2"
                | "ext3"
                | "ext4"
                | "btrfs"
                | "xfs"
                | "f2fs"
                | "zfs"
                | "tmpfs"
                | "overlay"
                | "vfat"
                | "exfat"
                | "ntfs"
                | "ntfs3"
                | "fuseblk"
                | "apfs"
                | "hfsplus"
        )
    )
}

/// The file system type of the mount that holds `folder`: of the longest
/// mount point in `table` (the format of `/proc/self/mountinfo`) that
/// `folder` is in.
fn mount_type_of(table: &str, folder: &Path) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for line in table.lines() {
        let Some((before, after)) = line.split_once(" - ") else {
            continue;
        };
        let (Some(point), Some(kind)) = (before.split(' ').nth(4), after.split(' ').next()) else {
            continue;
        };
        let point = point.replace("\\040", " ");
        if folder.starts_with(Path::new(&point))
            && best.as_ref().is_none_or(|(len, _)| point.len() >= *len)
        {
            best = Some((point.len(), kind.to_owned()));
        }
    }
    best.map(|(_, kind)| kind)
}

/// Whether SQLite can't lock a database of its own made in `folder`. Nothing
/// of the user's is touched: the file has a name of its own and goes away.
fn scratch_cant_lock(folder: &Path) -> bool {
    #[cfg(test)]
    if SQLITE_CANT_LOCK.with(std::cell::Cell::get) {
        return true;
    }
    scratch_fails(folder, "BEGIN IMMEDIATE; ROLLBACK")
}

/// Runs `sql` on a scratch database in `folder`; whether it fails because
/// of the folder's locks or files (busy, locked, or an I/O error).
fn scratch_fails(folder: &Path, sql: &str) -> bool {
    let path = folder.join(format!(
        ".kassensturz-lock-check-{}.db",
        uuid::Uuid::now_v7().simple()
    ));
    let refused = (|| {
        let conn = Connection::open(&path)?;
        conn.busy_timeout(WRITE_LOCK_WAIT)?;
        conn.execute_batch(sql)
    })()
    .is_err_and(|error| {
        is_busy(&error)
            || matches!(
                error.sqlite_error_code(),
                Some(rusqlite::ErrorCode::SystemIoFailure)
            )
    });
    let _ = fs::remove_file(&path);
    let mut journal = path.into_os_string();
    journal.push("-journal");
    let _ = fs::remove_file(journal);
    refused
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
        remove_stale();
        let path = std::env::temp_dir().join(format!(
            "{COPY_PREFIX}{}",
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

/// Removes the private folders that an earlier run left behind because it
/// was killed, such as the MCP server when its AI tool ends it: those more
/// than an hour old, so the folder of a program that is running is never
/// touched. A copy of a book must not outlive its use.
fn remove_stale() {
    let Ok(entries) = fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|made| made.elapsed().ok())
            .is_some_and(|age| age > Duration::from_secs(3600));
        if old
            && entry.file_name().to_string_lossy().starts_with(COPY_PREFIX)
            && entry.file_type().is_ok_and(|kind| kind.is_dir())
        {
            let _ = fs::remove_dir_all(entry.path());
        }
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
    // Copies that were still whole at both ends but failed the check: the
    // book itself is damaged, not busy.
    let mut damaged = 0;
    for attempt in 0..COPY_TRIES {
        if attempt > 0 {
            std::thread::sleep(COPY_WAIT);
        }
        match copy_once(path, flags) {
            Ok(read) => return Ok(read),
            // A book that isn't one, or can't be read, won't get better.
            Err(Retry::Never(error)) => return Err(error),
            Err(Retry::Maybe(error)) => last = error,
            Err(Retry::Damaged) => {
                damaged += 1;
                if damaged >= 3 {
                    return Err(StoreError::NotABook);
                }
            }
        }
    }
    Err(last)
}

enum Retry {
    /// A write was under way; try again.
    Maybe(StoreError),
    /// The copy is whole, no write was under way, and it fails the check.
    Damaged,
    Never(StoreError),
}

/// The file change counter in the header of the book at `path` (bytes 24 to
/// 27, sqlite.org/fileformat.html): every transaction that changes the book
/// raises it, so equal counters before and after a copy mean no commit
/// happened in between, however coarse the file system's times are.
fn change_counter(path: &Path) -> std::io::Result<[u8; 4]> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = fs::File::open(path)?;
    file.seek(SeekFrom::Start(24))?;
    let mut counter = [0; 4];
    file.read_exact(&mut counter)?;
    Ok(counter)
}

fn copy_once(path: &Path, flags: OpenFlags) -> Result<ReadOnly, Retry> {
    let busy = || Retry::Maybe(StoreError::Io(std::io::ErrorKind::WouldBlock));
    let never = |error: std::io::Error| Retry::Never(StoreError::Io(error.kind()));
    let journal = journal_of(path);
    if journal_is_hot(&journal) {
        return Err(busy());
    }
    let counter = change_counter(path).map_err(never)?;
    let before = fs::metadata(path).map_err(never)?;
    let folder = PrivateFolder::new().map_err(never)?;
    let copy = folder.path().join("copy.ksz");
    fs::copy(path, &copy).map_err(never)?;
    let after = fs::metadata(path).map_err(never)?;
    // No write began or ended while the file was copied: no journal, the
    // same counter at both ends and in the copy, and the same size and time.
    if journal_is_hot(&journal)
        || change_counter(path).map_err(never)? != counter
        || change_counter(&copy).map_err(never)? != counter
        || before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
    {
        return Err(busy());
    }
    let conn =
        Connection::open_with_flags(&copy, flags).map_err(|error| Retry::Never(error.into()))?;
    let check: String = conn
        .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
        .map_err(|_| Retry::Damaged)?;
    if check != "ok" {
        return Err(Retry::Damaged);
    }
    Ok(ReadOnly {
        conn,
        _copy: Some(folder),
    })
}

/// Whether the rollback journal at `journal` means a write is under way, as
/// SQLite itself decides: it has content and its header is not zero. SQLite
/// writes the header only after the journal is on disk and before it changes
/// any page of the book, so a journal with a zero header (a program was
/// killed before that) leaves the book untouched and is ignored by SQLite
/// until the next save removes it. Counting it as a write would leave readers
/// "busy" for good with nothing running. A journal that can't be read counts
/// as hot, the safe side.
pub(crate) fn journal_is_hot(journal: &Path) -> bool {
    use std::io::Read;
    let mut file = match fs::File::open(journal) {
        Ok(file) => file,
        Err(error) => return error.kind() != std::io::ErrorKind::NotFound,
    };
    let mut first = [0u8; 1];
    match file.read(&mut first) {
        Ok(0) => false,
        Ok(_) => first[0] != 0,
        Err(_) => true,
    }
}

/// Which file is at a path, as the system numbers files (the device and
/// the file's number on it), so a file put in its place under the same name
/// can be told from it (#1011).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FileId {
    device: u64,
    number: u64,
}

/// The [`FileId`] of the file at `path`, if a file is there. SQLite notices
/// a book replaced under the running app through the same numbers, but its
/// `unix-none` VFS keeps none, so a book opened without file locks would
/// save into the replaced file unnoticed. Only Unix has such numbers; the
/// lock-less mode exists only there.
#[cfg(unix)]
pub(crate) fn file_id(path: &Path) -> Option<FileId> {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::metadata(path).ok().filter(fs::Metadata::is_file)?;
    Some(FileId {
        device: metadata.dev(),
        number: metadata.ino(),
    })
}

/// See above: no such numbers here.
#[cfg(not(unix))]
pub(crate) fn file_id(_path: &Path) -> Option<FileId> {
    None
}

/// SQLite's rollback journal for the book at `path`.
fn journal_of(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push("-journal");
    PathBuf::from(name)
}

#[cfg(test)]
mod tests;
