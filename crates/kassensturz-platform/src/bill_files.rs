// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Moving a bill between the receipt inbox and its archive without ever
//! losing it (US7, #333, D-752, `docs/RECEIPT_INBOX.md` §6 and §7).
//!
//! A move is copy, check, put in place, and only after the book has saved
//! the link, remove the original: the two folders may be on different
//! drives, and in a sandbox they are two separate portal grants, where a
//! rename from one to the other may not exist. Every step goes through a
//! [`Disk`], so the tests can make any step fail.
//!
//! The [`Journal`] (`receipt-moves.jsonl`, per computer, next to the
//! settings file) writes a line before the copy and after each step that
//! counts. After a crash, [`Journal::settle`] finishes or takes back every
//! move left open **from the facts on disk and in the book, never by
//! guessing, and never by deleting the last copy** of a bill.
//!
//! Nothing here reads what a bill says: only its size and time, and its
//! bytes to compare a copy with the original.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::{Map, Value, json};

/// The journal's file name, next to the settings file.
pub const JOURNAL_FILE: &str = "receipt-moves.jsonl";

/// What a copy is called until it is checked and put in place: a name
/// sync tools leave out by default.
pub const PART_ENDING: &str = ".part";

/// A bill's size, time of last change and checksum, read when the AI
/// proposed it and again before it is moved: a difference means the file
/// changed after the AI read it (`bill.changed`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fingerprint {
    /// The size in bytes.
    pub size: u64,
    /// The time of the last change, in nanoseconds since 1970, if the
    /// system tells it.
    pub modified: Option<u64>,
    /// [`kassensturz_core::receipt_inbox::content_digest`] of its bytes, so
    /// that another file of the same size and time is noticed too (B-284).
    /// `None` where it wasn't read, e.g. in an older journal line.
    pub digest: Option<u64>,
}

impl Fingerprint {
    /// Whether `now` is still the file this was read from: the same size
    /// and time, and the same checksum where both have one.
    pub fn matches(&self, now: &Self) -> bool {
        self.size == now.size
            && self.modified == now.modified
            && match (self.digest, now.digest) {
                (Some(then), Some(now)) => then == now,
                _ => true,
            }
    }
}

/// Why a bill wasn't moved. The bill stays where it was in every case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveFailure {
    /// It changed after the AI read it, or is no longer a plain file.
    Changed,
    /// The original isn't there.
    NotFound,
    /// The copy couldn't be written.
    Copy,
    /// The copy differs from the original.
    Verify,
    /// The name it should get is taken, also by a leftover copy.
    Exists,
    /// The drive is full.
    NoSpace,
    /// The system doesn't let KassenSturz write or remove there.
    NoPermission,
    /// The folder it should go into is gone.
    ArchiveGone,
    /// The folder it comes from is gone.
    InboxGone,
}

impl MoveFailure {
    /// The reason of `bill.move-failed` (`docs/ERROR_CODES.md`), or the
    /// code of its own for [`MoveFailure::Changed`].
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Changed => "changed",
            Self::NotFound => "not-found",
            Self::Copy => "copy",
            Self::Verify => "verify",
            Self::Exists => "exists",
            Self::NoSpace => "no-space",
            Self::NoPermission => "no-permission",
            Self::ArchiveGone => "archive-gone",
            Self::InboxGone => "inbox-gone",
        }
    }

    /// The failure for an error of the system while writing; `otherwise`
    /// for one that tells nothing more.
    fn of(error: &io::Error, otherwise: Self) -> Self {
        match error.kind() {
            io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded => Self::NoSpace,
            io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem => {
                Self::NoPermission
            }
            io::ErrorKind::AlreadyExists => Self::Exists,
            _ => otherwise,
        }
    }
}

/// The file operations a move needs. [`RealDisk`] does them; tests use one
/// that fails at a chosen step.
pub trait Disk {
    /// The size and time of a plain file, not following a link.
    fn fingerprint(&self, path: &Path) -> io::Result<Fingerprint>;
    /// Copies `from` into a new file `to`, which must not exist yet, and
    /// writes it to the drive.
    fn copy_new(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// Whether the two files hold the same bytes.
    fn same(&self, a: &Path, b: &Path) -> io::Result<bool>;
    /// Renames `from` to `to`, failing with `AlreadyExists` instead of
    /// replacing a file there.
    fn rename_no_replace(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// Writes the folder's list of names to the drive.
    fn sync_folder(&self, folder: &Path) -> io::Result<()>;
    /// Removes a file.
    fn remove(&self, path: &Path) -> io::Result<()>;
    /// Whether anything has this name, without following a link.
    fn exists(&self, path: &Path) -> bool;
    /// Whether this is a folder.
    fn is_folder(&self, path: &Path) -> bool;
    /// Makes the folder and the ones above it.
    fn make_folders(&self, path: &Path) -> io::Result<()>;
    /// The names in a folder.
    fn names_in(&self, folder: &Path) -> io::Result<Vec<String>>;
    /// Removes a folder if it is empty; anything else is left.
    fn remove_empty_folder(&self, path: &Path);
}

/// The real files, with the platform's rename that never replaces a file
/// (`renameat2` with `RENAME_NOREPLACE`, `renamex_np`, `MoveFileEx`; D-731),
/// which the app hands in.
pub struct RealDisk<R> {
    rename: R,
}

impl<R: Fn(&Path, &Path) -> io::Result<()>> RealDisk<R> {
    /// The real files, renaming with `rename`.
    pub const fn new(rename: R) -> Self {
        Self { rename }
    }
}

impl<R: Fn(&Path, &Path) -> io::Result<()>> Disk for RealDisk<R> {
    fn fingerprint(&self, path: &Path) -> io::Result<Fingerprint> {
        let meta = std::fs::symlink_metadata(path)?;
        if !meta.is_file() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a file"));
        }
        let modified = meta
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .and_then(|since| u64::try_from(since.as_nanos()).ok());
        let digest = kassensturz_core::receipt_inbox::content_digest(File::open(path)?)?;
        Ok(Fingerprint {
            size: meta.len(),
            modified,
            digest: Some(digest),
        })
    }

    fn copy_new(&self, from: &Path, to: &Path) -> io::Result<()> {
        let mut source = File::open(from)?;
        let mut target = OpenOptions::new().write(true).create_new(true).open(to)?;
        io::copy(&mut source, &mut target)?;
        target.sync_all()
    }

    fn same(&self, a: &Path, b: &Path) -> io::Result<bool> {
        let (mut a, mut b) = (File::open(a)?, File::open(b)?);
        if a.metadata()?.len() != b.metadata()?.len() {
            return Ok(false);
        }
        let (mut left, mut right) = (vec![0u8; 64 * 1024], vec![0u8; 64 * 1024]);
        loop {
            let read = a.read(&mut left)?;
            if read == 0 {
                return Ok(b.read(&mut right[..1])? == 0);
            }
            b.read_exact(&mut right[..read])?;
            if left[..read] != right[..read] {
                return Ok(false);
            }
        }
    }

    fn rename_no_replace(&self, from: &Path, to: &Path) -> io::Result<()> {
        (self.rename)(from, to)
    }

    fn sync_folder(&self, folder: &Path) -> io::Result<()> {
        // Windows can't open a folder as a file; its rename is written
        // through already.
        if cfg!(windows) {
            return Ok(());
        }
        File::open(folder)?.sync_all()
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }

    fn exists(&self, path: &Path) -> bool {
        std::fs::symlink_metadata(path).is_ok()
    }

    fn is_folder(&self, path: &Path) -> bool {
        std::fs::metadata(path).is_ok_and(|meta| meta.is_dir())
    }

    fn make_folders(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }

    fn names_in(&self, folder: &Path) -> io::Result<Vec<String>> {
        std::fs::read_dir(folder)?
            .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
            .collect()
    }

    fn remove_empty_folder(&self, path: &Path) {
        // Fails, and leaves it, unless it is empty.
        let _ = std::fs::remove_dir(path);
    }
}

/// The copy's name while it is written: `<to>.part`.
pub fn part_of(to: &Path) -> PathBuf {
    let mut name = to.as_os_str().to_owned();
    name.push(PART_ENDING);
    PathBuf::from(name)
}

/// A bill copied, checked and put in place at `to`; the original at `from`
/// is still there until [`finish`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    /// Where it was.
    pub from: PathBuf,
    /// Where it is now as well.
    pub to: PathBuf,
}

/// Steps 1 to 4 of a move: checks that `from` is the file the AI read
/// (`expected`, if any), copies it to `<to>.part` in `to`'s folder (made if
/// missing), compares the copy byte by byte, and renames it to `to`
/// without replacing anything there. On any failure the copy is removed
/// and the original is untouched.
pub fn place(
    disk: &dyn Disk,
    from: &Path,
    to: &Path,
    expected: Option<Fingerprint>,
) -> Result<Placed, MoveFailure> {
    let from_folder = from.parent().unwrap_or(from);
    if !disk.is_folder(from_folder) {
        return Err(MoveFailure::InboxGone);
    }
    let now = match disk.fingerprint(from) {
        Ok(now) => now,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(MoveFailure::NotFound);
        }
        Err(_) => return Err(MoveFailure::Changed),
    };
    if expected.is_some_and(|expected| !expected.matches(&now)) {
        return Err(MoveFailure::Changed);
    }
    let folder = to.parent().ok_or(MoveFailure::ArchiveGone)?;
    disk.make_folders(folder)
        .map_err(|error| MoveFailure::of(&error, MoveFailure::ArchiveGone))?;
    let part = part_of(to);
    if disk.exists(to) || disk.exists(&part) {
        return Err(MoveFailure::Exists);
    }
    if let Err(error) = disk.copy_new(from, &part) {
        // `create_new` refused: a file of that name appeared, not ours.
        if error.kind() != io::ErrorKind::AlreadyExists {
            let _ = disk.remove(&part);
        }
        return Err(MoveFailure::of(&error, MoveFailure::Copy));
    }
    match disk.same(from, &part) {
        Ok(true) => {}
        _ => {
            let _ = disk.remove(&part);
            return Err(MoveFailure::Verify);
        }
    }
    if let Err(error) = disk.rename_no_replace(&part, to) {
        let _ = disk.remove(&part);
        return Err(MoveFailure::of(&error, MoveFailure::Copy));
    }
    // The name is in place; a folder that can't be written to the drive
    // now is written by the system later, so this doesn't undo the move.
    let _ = disk.sync_folder(folder);
    Ok(Placed {
        from: from.to_path_buf(),
        to: to.to_path_buf(),
    })
}

/// Takes a placed copy back when the book couldn't save the link: the copy
/// is removed only while the original is still there and the same, so the
/// last copy is never the one removed.
pub fn take_back(disk: &dyn Disk, placed: &Placed) {
    if disk.same(&placed.from, &placed.to).unwrap_or(false) {
        let _ = disk.remove(&placed.to);
    }
}

/// Step 6, once the book has saved the link: removes the original, after
/// checking once more that the placed copy is still there and the same.
/// A failure leaves both copies; the bill is filed.
pub fn finish(disk: &dyn Disk, placed: &Placed) -> Result<(), MoveFailure> {
    if !disk.same(&placed.from, &placed.to).unwrap_or(false) {
        return Err(MoveFailure::Verify);
    }
    disk.remove(&placed.from)
        .map_err(|error| MoveFailure::of(&error, MoveFailure::NoPermission))?;
    if let Some(folder) = placed.from.parent() {
        let _ = disk.sync_folder(folder);
    }
    Ok(())
}

/// Which way a journal line moves a bill.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// From the inbox into the archive: the book links the archive path
    /// once it is done.
    File,
    /// Back from the archive into the inbox, for an Undo: the book no
    /// longer links the archive path once it is done.
    Back,
}

impl Direction {
    const fn word(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Back => "back",
        }
    }
}

/// One move as the journal holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Its ID in the journal.
    pub id: String,
    /// Which way it goes.
    pub direction: Direction,
    /// Where the bill was.
    pub from: PathBuf,
    /// Where it goes.
    pub to: PathBuf,
    /// The bill's size and time before the move.
    pub fingerprint: Fingerprint,
    /// The ID of the book's history action it belongs to, once the book
    /// saved it (hex).
    pub change: Option<String>,
    /// Whether it is over: done, taken back or settled.
    pub closed: bool,
}

impl Entry {
    /// The path the book links to while the bill is filed: the archive
    /// path.
    pub fn archive_path(&self) -> &Path {
        match self.direction {
            Direction::File => &self.to,
            Direction::Back => &self.from,
        }
    }

    /// The path in the inbox.
    pub fn inbox_path(&self) -> &Path {
        match self.direction {
            Direction::File => &self.from,
            Direction::Back => &self.to,
        }
    }
}

/// What [`Journal::settle`] found for a move left open after a crash, for
/// the user (`docs/RECEIPT_INBOX.md` §6, the table).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Settled {
    /// Nothing was lost: a copy was removed, the move was finished, or
    /// nothing had happened yet. Nothing to tell.
    Quiet,
    /// The bill is in the archive without a link in the book.
    UnlinkedInArchive(PathBuf),
    /// The bill is in the inbox, while the book still links its archive
    /// path, which is empty.
    NotWhereLinked(PathBuf),
    /// Both copies are there and differ, so neither was removed.
    BothKept(PathBuf, PathBuf),
    /// Only the unchecked copy is left; it was kept as the last copy.
    OnlyPartLeft(PathBuf),
    /// Neither the original nor the copy is there any more.
    Gone(PathBuf),
}

/// The journal of moves, one JSON object per line, appended to and never
/// rewritten while moves are open.
pub struct Journal {
    path: PathBuf,
}

impl Journal {
    /// The journal in the file `path`.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Notes a move before its copy is written; its new ID.
    pub fn open_move(
        &self,
        direction: Direction,
        from: &Path,
        to: &Path,
        fingerprint: Fingerprint,
    ) -> io::Result<String> {
        let id = new_id();
        let mut line = Map::new();
        line.insert("move".into(), json!(id));
        line.insert("direction".into(), json!(direction.word()));
        line.insert("from".into(), path_value(from)?);
        line.insert("to".into(), path_value(to)?);
        line.insert("size".into(), json!(fingerprint.size));
        if let Some(modified) = fingerprint.modified {
            line.insert("modified".into(), json!(modified));
        }
        if let Some(digest) = fingerprint.digest {
            line.insert("digest".into(), json!(digest));
        }
        self.append(&Value::Object(line))?;
        Ok(id)
    }

    /// Notes that the book saved the move as the history action `change`.
    pub fn saved(&self, id: &str, change: &str) -> io::Result<()> {
        self.append(&json!({"move": id, "change": change}))
    }

    /// Notes that the move is over: done, taken back or settled.
    pub fn close(&self, id: &str) -> io::Result<()> {
        self.append(&json!({"move": id, "closed": true}))
    }

    fn append(&self, line: &Value) -> io::Result<()> {
        if let Some(folder) = self.path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let mut text = line.to_string();
        text.push('\n');
        file.write_all(text.as_bytes())?;
        file.sync_all()
    }

    /// Every move the journal holds, oldest first. A line that can't be
    /// read is passed over: it never leads to removing anything.
    pub fn entries(&self) -> Vec<Entry> {
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        let mut order = Vec::new();
        let mut entries: BTreeMap<String, Entry> = BTreeMap::new();
        for line in text.lines() {
            let Ok(Value::Object(line)) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let Some(id) = line.get("move").and_then(Value::as_str) else {
                continue;
            };
            if let Some(entry) = entries.get_mut(id) {
                if let Some(change) = line.get("change").and_then(Value::as_str) {
                    entry.change = Some(change.to_owned());
                }
                if line.get("closed").and_then(Value::as_bool) == Some(true) {
                    entry.closed = true;
                }
                continue;
            }
            let direction = match line.get("direction").and_then(Value::as_str) {
                Some("file") => Direction::File,
                Some("back") => Direction::Back,
                _ => continue,
            };
            let (Some(from), Some(to), Some(size)) = (
                line.get("from").and_then(Value::as_str),
                line.get("to").and_then(Value::as_str),
                line.get("size").and_then(Value::as_u64),
            ) else {
                continue;
            };
            order.push(id.to_owned());
            entries.insert(
                id.to_owned(),
                Entry {
                    id: id.to_owned(),
                    direction,
                    from: PathBuf::from(from),
                    to: PathBuf::from(to),
                    fingerprint: Fingerprint {
                        size,
                        modified: line.get("modified").and_then(Value::as_u64),
                        digest: line.get("digest").and_then(Value::as_u64),
                    },
                    change: None,
                    closed: false,
                },
            );
        }
        order
            .into_iter()
            .filter_map(|id| entries.remove(&id))
            .collect()
    }

    /// The archive paths the history action `change` filed and that are
    /// filed still: the moves an Undo of it takes back, newest last.
    pub fn filed_by(&self, change: &str) -> Vec<Entry> {
        self.latest_closed(change, Direction::File)
    }

    /// The moves an Undo of `change` took back and a Redo makes again.
    pub fn taken_back_by(&self, change: &str) -> Vec<Entry> {
        self.latest_closed(change, Direction::Back)
    }

    /// The finished moves of `change` whose newest move per archive path
    /// goes `direction`.
    fn latest_closed(&self, change: &str, direction: Direction) -> Vec<Entry> {
        let mut latest: BTreeMap<PathBuf, Entry> = BTreeMap::new();
        for entry in self.entries() {
            if entry.closed && entry.change.as_deref() == Some(change) {
                latest.insert(entry.archive_path().to_path_buf(), entry);
            }
        }
        latest
            .into_values()
            .filter(|entry| entry.direction == direction)
            .collect()
    }

    /// Settles every move left open, by the facts: which files are there,
    /// and whether the book links the archive path (`linked`). Removes a
    /// file only when another, identical copy stays, and only a copy the
    /// book doesn't rely on. Returns what the user should hear.
    pub fn settle(&self, disk: &dyn Disk, linked: &dyn Fn(&Path) -> bool) -> Vec<Settled> {
        let mut told = Vec::new();
        for entry in self.entries().into_iter().filter(|entry| !entry.closed) {
            let settled = settle_one(disk, &entry, linked(entry.archive_path()));
            if self.close(&entry.id).is_ok() && settled != Settled::Quiet {
                told.push(settled);
            }
        }
        told
    }
}

/// One open move settled by the table of `docs/RECEIPT_INBOX.md` §6, for
/// both directions: the move counts as saved when the book's link agrees
/// with where it was going.
fn settle_one(disk: &dyn Disk, entry: &Entry, linked: bool) -> Settled {
    let saved = match entry.direction {
        Direction::File => linked,
        Direction::Back => !linked,
    };
    let (from, to) = (entry.from.as_path(), entry.to.as_path());
    let part = part_of(to);
    let (has_from, has_to) = (disk.exists(from), disk.exists(to));
    let same = |a: &Path, b: &Path| disk.same(a, b).unwrap_or(false);
    // A leftover unchecked copy goes whenever the original is there.
    if has_from && disk.exists(&part) && same(from, &part) {
        let _ = disk.remove(&part);
    }
    match (saved, has_from, has_to) {
        // Saved: `to` is the real one.
        (true, true, true) if same(from, to) => {
            if disk.remove(from).is_ok() && entry.direction == Direction::Back {
                tidy_archive(disk, from);
            }
            Settled::Quiet
        }
        (true, true, true) => Settled::BothKept(from.to_path_buf(), to.to_path_buf()),
        (true, false, true) => Settled::Quiet,
        (true, true, false) => match entry.direction {
            Direction::File => Settled::NotWhereLinked(from.to_path_buf()),
            Direction::Back => Settled::UnlinkedInArchive(from.to_path_buf()),
        },
        // Not saved: `from` is the real one.
        (false, true, true) if same(from, to) => {
            if disk.remove(to).is_ok() && entry.direction == Direction::File {
                tidy_archive(disk, to);
            }
            Settled::Quiet
        }
        (false, true, true) => Settled::BothKept(from.to_path_buf(), to.to_path_buf()),
        (false, true, false) => Settled::Quiet,
        (false, false, true) => match entry.direction {
            Direction::File => Settled::UnlinkedInArchive(to.to_path_buf()),
            Direction::Back => Settled::NotWhereLinked(to.to_path_buf()),
        },
        (_, false, false) if disk.exists(&part) => Settled::OnlyPartLeft(part),
        (_, false, false) => Settled::Gone(entry.archive_path().to_path_buf()),
    }
}

/// Removes the category's and the account's folder above a bill that left
/// the archive, each only if it is empty now; the archive itself stays.
pub fn tidy_archive(disk: &dyn Disk, archived: &Path) {
    let category = archived.parent();
    if let Some(category) = category {
        disk.remove_empty_folder(category);
        if let Some(account) = category.parent() {
            disk.remove_empty_folder(account);
        }
    }
}

/// A path as JSON text, or an error for one that isn't Unicode.
fn path_value(path: &Path) -> io::Result<Value> {
    path.to_str()
        .map(|text| json!(text))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not Unicode"))
}

/// A new move ID: the time in nanoseconds and a counter, in hex.
fn new_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("{nanos:x}-{:x}", COUNT.fetch_add(1, Ordering::Relaxed))
}

#[cfg(test)]
mod tests;
