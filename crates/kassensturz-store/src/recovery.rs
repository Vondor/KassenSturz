// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A save that was cut short (B-364).
//!
//! When KassenSturz is killed or the computer loses power during a save,
//! the book's file can hold part of that save, and SQLite's rollback journal
//! next to it holds what the changed pages were before. SQLite takes the
//! save back by itself the next time a connection that may write reads the
//! book. Where that fails (round 7 met a Flatpak opening a book through the
//! document portal that kept failing with a general error), the book could
//! no longer be opened at all, and the page said to open a backup, although
//! nothing was lost.
//!
//! So opening a book with such a journal takes the save back as its own
//! step, before anything else reads the book:
//!
//! 1. in place, as SQLite does, on a connection of its own;
//! 2. if that fails, in a private copy of the book and its journal, where
//!    SQLite takes the save back; the copy that passes SQLite's check then
//!    takes the book's place. The cut-short book and its journal are kept
//!    beside it under a name of their own, never deleted, so nothing that
//!    was in the folder is lost;
//! 3. if that fails too, [`StoreError::UnfinishedSave`] says so, and the
//!    book and its journal stay as they were.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

use crate::error::StoreError;
use crate::lockless::{self, PrivateFolder, journal_is_hot};

/// The first bytes of a rollback journal's header
/// (sqlite.org/fileformat.html, "The Rollback Journal").
const JOURNAL_MAGIC: [u8; 8] = [0xd9, 0xd5, 0x05, 0xf9, 0x20, 0xa1, 0x63, 0xd7];

#[cfg(test)]
thread_local! {
    /// Makes the tests of this thread see SQLite fail to take a save back
    /// in the book's folder, as it does through the document portal.
    pub(crate) static IN_PLACE_FAILS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Makes the tests of this thread see the private copy fail too.
    pub(crate) static COPY_FAILS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// What became of a save cut short.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// There was none.
    None,
    /// SQLite took it back in the book's folder.
    InPlace,
    /// It was taken back in a private copy, which took the book's place;
    /// the cut-short book and its journal are kept at this path (and the
    /// same with `-journal`).
    ThroughCopy(PathBuf),
}

/// The rollback journal of the book at `book`.
pub(crate) fn journal_of(book: &Path) -> PathBuf {
    let mut name = book.as_os_str().to_owned();
    name.push("-journal");
    PathBuf::from(name)
}

/// Takes back the save to the book at `book` that was cut short, if there
/// is one; see the module's documentation. The book's lock must be held,
/// so no other KassenSturz is saving to it at this moment.
pub(crate) fn finish_cut_short_save(book: &Path) -> Result<Outcome, StoreError> {
    let journal = journal_of(book);
    if !journal_is_hot(&journal) {
        return Ok(Outcome::None);
    }
    let in_place = take_back_in_place(book);
    if in_place.is_ok() && !journal_is_hot(&journal) {
        return Ok(Outcome::InPlace);
    }
    take_back_through_copy(book, &journal)
        .map(Outcome::ThroughCopy)
        .map_err(|_| StoreError::UnfinishedSave)
}

/// Lets SQLite take the save back in the book's folder: a connection that
/// may write and reads the book does so before anything else.
fn take_back_in_place(book: &Path) -> Result<(), StoreError> {
    #[cfg(test)]
    if IN_PLACE_FAILS.with(std::cell::Cell::get) {
        return Err(StoreError::UnfinishedSave);
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let (conn, _) = lockless::open_writer(book, flags)?;
    // A connection SQLite could only open read-only can't take it back.
    if conn.is_readonly(rusqlite::MAIN_DB)? {
        return Err(StoreError::UnfinishedSave);
    }
    read_schema(&conn)
}

/// Reads the book's layout list, which makes SQLite take back a save that
/// was cut short, and fails if that didn't work.
fn read_schema(conn: &Connection) -> Result<(), StoreError> {
    conn.query_row("SELECT count(*) FROM sqlite_schema", [], |row| {
        row.get::<_, i64>(0)
    })?;
    Ok(())
}

/// Takes the save back in a private copy of the book and its journal and
/// puts the copy in the book's place, keeping the cut-short pair beside it.
/// Returns where that pair is now.
fn take_back_through_copy(book: &Path, journal: &Path) -> Result<PathBuf, StoreError> {
    #[cfg(test)]
    if COPY_FAILS.with(std::cell::Cell::get) {
        return Err(StoreError::UnfinishedSave);
    }
    let folder = PrivateFolder::new()?;
    let copy = folder.path().join("book.ksz");
    copy_part(book, &copy, size_before_the_save(journal)?)?;
    fs::copy(journal, journal_of(&copy))?;
    {
        let conn = Connection::open_with_flags(
            &copy,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        read_schema(&conn)?;
        let check: String = conn.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
        if check != "ok" {
            return Err(StoreError::UnfinishedSave);
        }
    }
    if journal_of(&copy).exists() || !crate::is_book_file(&copy)? {
        return Err(StoreError::UnfinishedSave);
    }
    put_in_place(book, journal, &copy)
}

/// The size the book had before the save, from its journal's header: SQLite
/// cuts the book back to it when it takes the save back, so the pages after
/// it, which only the save added, need not be copied. A save can make a
/// book grow by hundreds of megabytes (B-362), and the private folder may
/// be in memory. `None` if the header can't be read, then all is copied.
fn size_before_the_save(journal: &Path) -> Result<Option<u64>, StoreError> {
    let mut header = [0u8; 28];
    let mut file = fs::File::open(journal)?;
    if file.read_exact(&mut header).is_err() || header[..8] != JOURNAL_MAGIC {
        return Ok(None);
    }
    let word = |at: usize| {
        u64::from(u32::from_be_bytes(
            header[at..at + 4].try_into().expect("4"),
        ))
    };
    let (pages, page_size) = (word(16), word(24));
    if pages == 0 || !(512..=65536).contains(&page_size) {
        return Ok(None);
    }
    Ok(Some(pages * page_size))
}

/// Copies the first `size` bytes of `from` (all of it if `None`) to `to`.
fn copy_part(from: &Path, to: &Path, size: Option<u64>) -> Result<(), StoreError> {
    let Some(size) = size else {
        fs::copy(from, to)?;
        return Ok(());
    };
    let mut source = fs::File::open(from)?.take(size);
    let mut target = fs::File::create_new(to)?;
    std::io::copy(&mut source, &mut target)?;
    target.flush()?;
    Ok(())
}

/// Puts `taken_back`, the book with the save taken back, in the place of
/// `book`. The book and its `journal` are renamed to a name of their own
/// first, so the pair stays usable (SQLite finds a journal by the book's
/// name) and nothing is deleted. Each step that fails undoes the ones
/// before it. Returns the name the pair has now.
fn put_in_place(book: &Path, journal: &Path, taken_back: &Path) -> Result<PathBuf, StoreError> {
    let folder = book.parent().ok_or(StoreError::UnfinishedSave)?;
    let name = book
        .file_name()
        .ok_or(StoreError::UnfinishedSave)?
        .to_string_lossy()
        .into_owned();
    let unique = uuid::Uuid::now_v7().simple().to_string();
    let incoming = folder.join(format!(".{name}.taken-back-{unique}"));
    let kept = folder.join(format!("{name}.cut-short-{unique}"));
    fs::copy(taken_back, &incoming)?;
    let cleanup = |error: std::io::Error| {
        let _ = fs::remove_file(&incoming);
        StoreError::from(error)
    };
    fs::File::open(&incoming)
        .and_then(|file| file.sync_all())
        .map_err(cleanup)?;
    fs::rename(book, &kept).map_err(cleanup)?;
    if let Err(error) = fs::rename(journal, journal_of(&kept)) {
        let _ = fs::rename(&kept, book);
        return Err(cleanup(error));
    }
    if let Err(error) = fs::rename(&incoming, book) {
        let _ = fs::rename(journal_of(&kept), journal);
        let _ = fs::rename(&kept, book);
        return Err(cleanup(error));
    }
    sync_folder(folder);
    Ok(kept)
}

/// Writes the folder's list of files to the disk, so the renames survive a
/// power cut; a system that can't do that (Windows) keeps them anyway.
fn sync_folder(folder: &Path) {
    #[cfg(unix)]
    if let Ok(folder) = fs::File::open(folder) {
        let _ = folder.sync_all();
    }
    #[cfg(not(unix))]
    let _ = folder;
}

#[cfg(test)]
mod tests;
