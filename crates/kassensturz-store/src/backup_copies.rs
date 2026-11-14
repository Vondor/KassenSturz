// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Backup copies in a folder the user chose, the list of a book's backups,
//! and putting a backup back in place of the book (#806).
//!
//! The automatic backups stay in KassenSturz's own data folder (D-033), on
//! the same disk as the book. A folder the user chooses, e.g. on a USB stick
//! or in a folder a sync app uploads, also gets copies, so a broken disk
//! doesn't take the book and every backup with it. The copies have the
//! backups' names (`backup.rs`), are written as `.part` files and then
//! renamed, so a sync tool never uploads half a copy, and the folder keeps
//! the last few of each book. A folder that is missing or full is reported,
//! never an error that stops a backup in the app's own folder or the book
//! from opening.

use std::fs::{self, File, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use kassensturz_core::Timestamp;
use rusqlite::OpenFlags;

use crate::backup::{self, exists, parse_name, partial_path, stamp_time, stem_of};
use crate::error::StoreError;
use crate::{Book, is_book_file, layout};

/// How long a copy in the chosen folder counts as recent, so no other is
/// made when the book opens again: a day.
const RECENT_SECONDS: i64 = 24 * 60 * 60;

/// One backup of a book, as [`backups_of`] lists them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupFile {
    /// Where it is.
    pub path: PathBuf,
    /// When it was made, from its name.
    pub made: Timestamp,
}

/// The backups of the book at `book` in `folder`, newest first: the files
/// whose names the automatic backups get (`docs/BOOK_FORMAT.md`). Other
/// files, the backups of other books and unfinished `.part` files are left
/// out.
///
/// # Errors
///
/// [`StoreError::Io`] if the folder can't be read, with
/// [`ErrorKind::NotFound`] if it isn't there.
pub fn backups_of(book: &Path, folder: &Path) -> Result<Vec<BackupFile>, StoreError> {
    let stem = stem_of(book)?;
    let mut found: Vec<(Timestamp, u32, PathBuf)> = fs::read_dir(folder)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let (stamp, number) = parse_name(&name, stem)?;
            let made = stamp_time(stamp)?;
            // A folder or a link with a backup's name isn't one.
            let is_file = entry.file_type().ok()?.is_file();
            is_file.then(|| (made, number, entry.path()))
        })
        .collect();
    found.sort_by_key(|entry| std::cmp::Reverse((entry.0, entry.1)));
    Ok(found
        .into_iter()
        .map(|(made, _, path)| BackupFile { path, made })
        .collect())
}

/// The newest backup of the book at `book` in `folder` that this KassenSturz
/// can open, or `None` if there is none or the folder can't be read. It
/// is what the page for a book made by a newer KassenSturz offers: the copy
/// the newer version saved before it updated the book (B-285). A backup this
/// version can't open (a newer layout, a damaged file) is passed over.
pub fn newest_usable_backup(book: &Path, folder: &Path) -> Option<BackupFile> {
    backups_of(book, folder)
        .ok()?
        .into_iter()
        .find(|backup| check_backup(&backup.path).is_ok())
}

/// What [`Book::keep_copy`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeptCopy {
    /// It put a new copy there.
    Made(BackupFile),
    /// The folder has a copy of the book from the last day already.
    Recent(BackupFile),
}

/// Why a copy couldn't be kept in the chosen folder, for the plain words
/// the Settings page shows (D-034).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyProblem {
    /// The folder isn't there, or isn't a folder: e.g. a stick that isn't
    /// plugged in, or a folder that was renamed.
    Missing,
    /// The disk is full.
    Full,
    /// The folder can't be written to.
    ReadOnly,
    /// It is KassenSturz's own backup folder.
    SameFolder,
    /// Anything else.
    Other,
}

impl CopyProblem {
    /// What `error`, from [`Book::keep_copy`], says about the folder.
    #[must_use]
    pub fn of(error: &StoreError) -> Self {
        use rusqlite::ErrorCode;
        match error {
            StoreError::Io(kind) => match kind {
                ErrorKind::NotFound | ErrorKind::NotADirectory => Self::Missing,
                ErrorKind::StorageFull | ErrorKind::QuotaExceeded | ErrorKind::FileTooLarge => {
                    Self::Full
                }
                ErrorKind::PermissionDenied | ErrorKind::ReadOnlyFilesystem => Self::ReadOnly,
                ErrorKind::InvalidInput => Self::SameFolder,
                _ => Self::Other,
            },
            // SQLite writes the new copy (VACUUM INTO).
            StoreError::Database(database) => match database.0.sqlite_error_code() {
                Some(ErrorCode::DiskFull) => Self::Full,
                Some(ErrorCode::ReadOnly | ErrorCode::PermissionDenied) => Self::ReadOnly,
                Some(ErrorCode::CannotOpen) => Self::Missing,
                _ => Self::Other,
            },
            _ => Self::Other,
        }
    }
}

impl Book {
    /// Makes a backup of this book, which is at `book`, in `folder` now, as
    /// before an update (D-033), e.g. before a backup is put in its place.
    /// Returns where it is.
    ///
    /// # Errors
    ///
    /// [`StoreError::Io`] if the folder can't be made or written to.
    pub fn back_up_now(&self, book: &Path, folder: &Path) -> Result<PathBuf, StoreError> {
        backup::back_up(&self.conn, book, folder)
    }

    /// Makes a backup of this book, which is at `book`, in `folder`, the
    /// app's own backup folder, when the newest one there is older than
    /// thirty days or there is none, so a month-old state of the book can
    /// still be found (B-255, attack B7 of the security review). Called
    /// when the book is opened. Returns the new backup, or `None` when a
    /// recent one is there. The folder keeps the last ten and the oldest
    /// backup of each of the last twelve months before this one.
    ///
    /// A backup dated in the future, e.g. after the clock was put back,
    /// doesn't count as recent.
    ///
    /// # Errors
    ///
    /// [`StoreError::Io`] if the folder can't be made or written to.
    pub fn keep_monthly_backup(
        &self,
        book: &Path,
        folder: &Path,
    ) -> Result<Option<BackupFile>, StoreError> {
        let now = Timestamp::now();
        let recent = match backups_of(book, folder) {
            Ok(found) => found.into_iter().any(|backup| {
                backup.made <= now
                    && now.as_second() - backup.made.as_second() < backup::MONTHLY_SECONDS
            }),
            Err(StoreError::Io(ErrorKind::NotFound)) => false,
            Err(error) => return Err(error),
        };
        if recent {
            return Ok(None);
        }
        let path = backup::back_up(&self.conn, book, folder)?;
        Ok(Some(BackupFile { path, made: now }))
    }

    /// Keeps a copy of this book, which is at `book`, in `copies`, the
    /// folder the user chose, and removes that book's oldest copies there
    /// beyond the last `keep` (#806). If `backups`, the app's own backup
    /// folder, has a backup that `copies` lacks and that is newer than
    /// every copy there, that backup is copied; else, if no copy there is
    /// from the last day, a new one of the book as it is now. The folder is
    /// never made: a missing one is a stick that isn't plugged in.
    ///
    /// # Errors
    ///
    /// [`StoreError::Io`] if `copies` isn't there ([`ErrorKind::NotFound`]),
    /// isn't a folder ([`ErrorKind::NotADirectory`]), is the app's own
    /// backup folder ([`ErrorKind::InvalidInput`]), or can't be written to,
    /// e.g. [`ErrorKind::StorageFull`] or
    /// [`ErrorKind::ReadOnlyFilesystem`]. Nothing else is changed then.
    pub fn keep_copy(
        &self,
        book: &Path,
        backups: &Path,
        copies: &Path,
        keep: usize,
    ) -> Result<KeptCopy, StoreError> {
        let copies = std::path::absolute(copies)?;
        if !fs::metadata(&copies)?.is_dir() {
            return Err(StoreError::Io(ErrorKind::NotADirectory));
        }
        if same_folder(&copies, backups) {
            return Err(StoreError::Io(ErrorKind::InvalidInput));
        }
        let stem = stem_of(book)?;
        let present = backups_of(book, &copies)?;
        let newest_copy = present.first();
        let newest_backup = backups_of(book, backups)
            .ok()
            .and_then(|found| found.into_iter().next());
        if let Some(local) = newest_backup
            && newest_copy.is_none_or(|copy| local.made > copy.made)
        {
            let name = local
                .path
                .file_name()
                .ok_or(StoreError::Io(ErrorKind::InvalidFilename))?;
            if !exists(&copies.join(name))? {
                let path = copy_file(&local.path, &copies, stem, keep)?;
                return Ok(KeptCopy::Made(BackupFile {
                    path,
                    made: local.made,
                }));
            }
        }
        let now = Timestamp::now();
        if let Some(copy) = newest_copy
            && copy.made <= now
            && now.as_second() - copy.made.as_second() < RECENT_SECONDS
        {
            return Ok(KeptCopy::Recent(copy.clone()));
        }
        let path = backup::write_backup(&self.conn, book, &copies, keep)?;
        Ok(KeptCopy::Made(BackupFile { path, made: now }))
    }
}

/// Whether `a` and `b` are the same folder; `false` when either can't be
/// told, e.g. the app's own folder has no backups yet.
fn same_folder(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Copies the finished backup `from` into `folder` under its own name,
/// written as a `.part` file first, then removes the oldest copies of the
/// book `stem` there beyond the last `keep`. Returns the copy's path.
fn copy_file(from: &Path, folder: &Path, stem: &str, keep: usize) -> Result<PathBuf, StoreError> {
    let name = from
        .file_name()
        .ok_or(StoreError::Io(ErrorKind::InvalidFilename))?;
    let path = folder.join(name);
    let partial = partial_path(&path);
    // Claimed first, so no other copy writes there at the same time.
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&partial)?;
    let copied = copy_and_sync(from, &partial).and_then(|()| {
        // A copy is never put in place of a file that is there.
        if exists(&path)? {
            return Err(StoreError::Io(ErrorKind::AlreadyExists));
        }
        fs::rename(&partial, &path).map_err(StoreError::from)
    });
    if let Err(error) = copied {
        let _ = fs::remove_file(&partial);
        return Err(error);
    }
    backup::prune(folder, stem, &path, keep);
    Ok(path)
}

/// Copies the file `from` into `to` and makes sure the copy is on the disk.
fn copy_and_sync(from: &Path, to: &Path) -> Result<(), StoreError> {
    fs::copy(from, to)?;
    // Windows syncs only files opened for writing.
    OpenOptions::new().write(true).open(to)?.sync_all()?;
    Ok(())
}

/// A backup made ready to be put in place of a book (#806): checked, and
/// copied next to the book as `<book>.restore.part`. Made while the book is
/// still open, so making a backup of the book afterwards can't remove the
/// one being restored. [`PreparedRestore::put_in_place`] then renames the
/// copy over the book, once the book is closed, so the book is either the
/// old one or the whole backup, never half of it. Dropped without that, the
/// copy is removed and the book stays as it is.
#[derive(Debug)]
pub struct PreparedRestore {
    partial: PathBuf,
    book: PathBuf,
}

impl PreparedRestore {
    /// Checks the backup at `backup` and copies it next to the book at
    /// `book`. The backup must be a KassenSturz book (D-037) in a layout
    /// this KassenSturz opens, and SQLite's quick check must find nothing
    /// wrong. The backup itself is left as it is.
    ///
    /// # Errors
    ///
    /// - [`StoreError::NotABook`] if `backup` isn't a book or fails the
    ///   check;
    /// - [`StoreError::UnsupportedLayout`] or
    ///   [`StoreError::DevelopmentLayout`] if this KassenSturz can't open
    ///   its layout;
    /// - [`StoreError::Io`] if it can't be read or copied.
    ///
    /// The book isn't changed in any of these cases.
    pub fn new(backup: &Path, book: &Path) -> Result<Self, StoreError> {
        let backup = std::path::absolute(backup)?;
        let book = std::path::absolute(book)?;
        check_backup(&backup)?;
        let mut partial = book.as_os_str().to_owned();
        partial.push(".restore.part");
        let partial = PathBuf::from(partial);
        // Only a restore that was cut short leaves this name behind.
        match fs::remove_file(&partial) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&partial)?;
        let prepared = Self { partial, book };
        copy_and_sync(&backup, &prepared.partial)?;
        Ok(prepared)
    }

    /// Puts the backup in place of the book, which must be closed by now.
    ///
    /// # Errors
    ///
    /// [`StoreError::Io`] if the rename fails; the book is unchanged then.
    pub fn put_in_place(self) -> Result<(), StoreError> {
        fs::rename(&self.partial, &self.book)?;
        sync_folder(&self.book);
        Ok(())
    }
}

impl Drop for PreparedRestore {
    fn drop(&mut self) {
        // Gone already after a rename.
        let _ = fs::remove_file(&self.partial);
    }
}
/// Checks that `backup` is a book this KassenSturz can open, reading it
/// only.
fn check_backup(backup: &Path) -> Result<(), StoreError> {
    if !fs::metadata(backup)?.is_file() || !is_book_file(backup)? {
        return Err(StoreError::NotABook);
    }
    // Where the folder refuses file locks this reads a private copy
    // (Q-570, D-776).
    let conn = crate::lockless::open_read_only(
        backup,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let version: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    match version {
        known if (layout::FIRST_VERSION..=layout::VERSION).contains(&known) => {}
        found if (layout::FIRST_DEVELOPMENT_VERSION..layout::FIRST_VERSION).contains(&found) => {
            return Err(StoreError::DevelopmentLayout { found });
        }
        found => return Err(StoreError::UnsupportedLayout { found }),
    }
    let check: String = conn.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
    if check != "ok" {
        return Err(StoreError::NotABook);
    }
    Ok(())
}

/// Makes sure the rename of the file at `path` is on the disk, where the
/// system offers that for folders. Errors are ignored: the file itself is
/// synced, and Windows can't open folders this way.
fn sync_folder(path: &Path) {
    if cfg!(unix)
        && let Some(folder) = path.parent()
        && let Ok(folder) = File::open(folder)
    {
        let _ = folder.sync_all();
    }
}

#[cfg(test)]
mod tests {
    use kassensturz_core::{Account, AccountId, AccountKind, CurrencyCode};
    use rusqlite::Connection;

    use super::*;
    use crate::Computer;
    use crate::test_folder::TempFolder;

    fn computer(folder: &TempFolder) -> Computer {
        Computer {
            name: "Laptop".into(),
            backup_folder: folder.join("Backups"),
        }
    }

    /// A made-up book at `Household.ksz` with one account called `name`.
    fn book_with(folder: &TempFolder, name: &str) -> (Book, PathBuf) {
        let path = folder.join("Household.ksz");
        let mut book = Book::create(&path, &computer(folder)).expect("book made");
        add_account(&mut book, name);
        (book, path)
    }

    fn add_account(book: &mut Book, name: &str) {
        let eur = CurrencyCode::new("EUR").expect("a currency code");
        book.use_currency(eur).expect("currency used");
        book.add_account(&Account {
            id: AccountId::generate(),
            name: name.into(),
            kind: AccountKind::Asset,
            currency: eur,
            account_type: None,
            closed: false,
            parent: None,
            number: None,
            notes: None,
            institution: None,
        })
        .expect("account added");
    }

    fn account_names(book: &Book) -> Vec<String> {
        let mut names: Vec<String> = book
            .accounts(true)
            .expect("accounts")
            .into_iter()
            .map(|account| account.name)
            .collect();
        names.sort();
        names
    }

    /// B-285: the page for a book made by a newer KassenSturz offers the
    /// newest backup this version can open, not a newer one's or a damaged one.
    #[test]
    fn the_newest_usable_backup_passes_over_ones_this_version_cannot_open() {
        let folder = TempFolder::new();
        let (book, path) = book_with(&folder, "Checking");
        let backups = computer(&folder).backup_folder;
        assert_eq!(newest_usable_backup(&path, &backups), None, "no folder");
        let usable = book.back_up_now(&path, &backups).expect("backed up");
        // A newer one, made by a newer layout: a copy with a higher version.
        let newer = backups.join("Household 2099-01-01 00-00-00 UTC.ksz");
        fs::copy(&usable, &newer).expect("copied");
        Connection::open(&newer)
            .expect("opened")
            .pragma_update(None, "user_version", crate::layout::VERSION + 1)
            .expect("version set");
        // And a damaged one, newer still.
        fs::write(
            backups.join("Household 2100-01-01 00-00-00 UTC.ksz"),
            "damaged",
        )
        .expect("written");
        let found = newest_usable_backup(&path, &backups).expect("one is usable");
        assert_eq!(found.path, usable);
    }

    #[test]
    fn opening_makes_a_backup_once_the_newest_is_a_month_old() {
        let folder = TempFolder::new();
        let (book, path) = book_with(&folder, "Checking");
        let backups = computer(&folder).backup_folder;
        // None yet: one is made, and the folder with it.
        let first = book
            .keep_monthly_backup(&path, &backups)
            .expect("backed up")
            .expect("a backup made");
        assert_eq!(first.path.parent(), Some(backups.as_path()));
        // Recent now: none is added.
        assert_eq!(
            book.keep_monthly_backup(&path, &backups).expect("checked"),
            None
        );
        assert_eq!(names_in(&backups).len(), 1);
        // A backup 31 days old only: a new one is made.
        fs::remove_file(&first.path).expect("removed");
        let old = Timestamp::from_second(Timestamp::now().as_second() - 31 * 24 * 60 * 60)
            .expect("a time")
            .strftime("%Y-%m-%d %H-%M-%S UTC")
            .to_string();
        fs::write(backups.join(format!("Household {old}.ksz")), "made up").expect("written");
        assert!(
            book.keep_monthly_backup(&path, &backups)
                .expect("backed up")
                .is_some()
        );
        assert_eq!(names_in(&backups).len(), 2);
    }

    fn names_in(folder: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(folder)
            .expect("folder read")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .into_string()
                    .expect("UTF-8")
            })
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_copy_is_made_once_a_day_and_opens_as_the_book() {
        let folder = TempFolder::new();
        let (book, path) = book_with(&folder, "Giro");
        let stick = folder.join("Stick");
        fs::create_dir(&stick).expect("folder made");

        let first = book
            .keep_copy(&path, &computer(&folder).backup_folder, &stick, 5)
            .expect("copy kept");
        let KeptCopy::Made(made) = first else {
            panic!("no copy made: {first:?}");
        };
        assert_eq!(made.path.parent(), Some(stick.as_path()));
        // No `.part` is left, and the app's own folder wasn't made for it.
        assert_eq!(names_in(&stick).len(), 1);
        assert!(!computer(&folder).backup_folder.exists());

        // Opened again the same day: the copy is recent, none is added.
        let again = book
            .keep_copy(&path, &computer(&folder).backup_folder, &stick, 5)
            .expect("copy kept");
        assert!(matches!(&again, KeptCopy::Recent(file) if file.path == made.path));
        assert_eq!(names_in(&stick).len(), 1);

        drop(book);
        let other = TempFolder::new();
        let copy = Book::open(&made.path, &computer(&other)).expect("the copy opens");
        assert_eq!(account_names(&copy), ["Giro"]);
    }

    #[test]
    fn a_copy_older_than_a_day_is_followed_by_a_new_one_and_only_keep_are_left() {
        let folder = TempFolder::new();
        let (book, path) = book_with(&folder, "Giro");
        let stick = folder.join("Stick");
        fs::create_dir(&stick).expect("folder made");
        // Made-up older copies of this book, and files that must stay.
        for day in 1..=4 {
            fs::write(
                stick.join(format!("Household 2026-10-0{day} 09-00-00 UTC.ksz")),
                "made up",
            )
            .expect("written");
        }
        let others = [
            "Garden 2026-10-01 09-00-00 UTC.ksz",
            "Household.ksz",
            "Notes.txt",
        ];
        for name in others {
            fs::write(stick.join(name), "made up").expect("written");
        }

        let kept = book
            .keep_copy(&path, &computer(&folder).backup_folder, &stick, 3)
            .expect("copy kept");
        let KeptCopy::Made(made) = kept else {
            panic!("no copy made: {kept:?}");
        };
        let mut expected: Vec<String> = [
            "Household 2026-10-03 09-00-00 UTC.ksz".to_owned(),
            "Household 2026-10-04 09-00-00 UTC.ksz".to_owned(),
            made.path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("name")
                .to_owned(),
        ]
        .into_iter()
        .chain(others.map(String::from))
        .collect();
        expected.sort();
        assert_eq!(names_in(&stick), expected);
    }

    #[test]
    fn a_backup_the_folder_lacks_is_copied_there() {
        let folder = TempFolder::new();
        let (book, path) = book_with(&folder, "Giro");
        let backups = computer(&folder).backup_folder;
        let local = book.back_up_now(&path, &backups).expect("backed up");
        let stick = folder.join("Stick");
        fs::create_dir(&stick).expect("folder made");

        let kept = book
            .keep_copy(&path, &backups, &stick, 5)
            .expect("copy kept");
        let KeptCopy::Made(made) = kept else {
            panic!("no copy made: {kept:?}");
        };
        // The same file under the same name.
        assert_eq!(made.path.file_name(), local.file_name());
        assert_eq!(
            fs::read(&made.path).expect("copy read"),
            fs::read(&local).expect("backup read")
        );
        assert_eq!(names_in(&stick).len(), 1);
        // It is recent now, so nothing more is added.
        let again = book
            .keep_copy(&path, &backups, &stick, 5)
            .expect("copy kept");
        assert!(matches!(again, KeptCopy::Recent(_)));
        assert_eq!(names_in(&stick).len(), 1);
    }

    #[test]
    fn a_missing_or_wrong_folder_is_reported_and_nothing_is_written() {
        let folder = TempFolder::new();
        let (book, path) = book_with(&folder, "Giro");
        let backups = computer(&folder).backup_folder;
        let stick = folder.join("Stick");

        let missing = book.keep_copy(&path, &backups, &stick, 5);
        assert!(
            matches!(missing, Err(StoreError::Io(ErrorKind::NotFound))),
            "{missing:?}"
        );
        // A stick that isn't plugged in: its folder is never made.
        assert!(!stick.exists());

        fs::write(&stick, "a file").expect("written");
        let not_folder = book.keep_copy(&path, &backups, &stick, 5);
        assert!(
            matches!(not_folder, Err(StoreError::Io(ErrorKind::NotADirectory))),
            "{not_folder:?}"
        );

        book.back_up_now(&path, &backups).expect("backed up");
        let same = book.keep_copy(&path, &backups, &backups, 5);
        assert!(
            matches!(same, Err(StoreError::Io(ErrorKind::InvalidInput))),
            "{same:?}"
        );
        assert_eq!(names_in(&backups).len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_that_cant_be_written_leaves_no_part_file() {
        use std::os::unix::fs::PermissionsExt;
        let folder = TempFolder::new();
        let (book, path) = book_with(&folder, "Giro");
        let stick = folder.join("Stick");
        fs::create_dir(&stick).expect("folder made");
        fs::set_permissions(&stick, fs::Permissions::from_mode(0o555)).expect("read-only");
        // Root can write anyway; the check means nothing then.
        if OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(stick.join("probe"))
            .is_ok()
        {
            return;
        }
        let refused = book.keep_copy(&path, &computer(&folder).backup_folder, &stick, 5);
        assert!(matches!(refused, Err(StoreError::Io(_))), "{refused:?}");
        fs::set_permissions(&stick, fs::Permissions::from_mode(0o755)).expect("writable");
        assert!(names_in(&stick).is_empty());
    }

    #[test]
    fn backups_are_listed_newest_first_and_other_files_are_left_out() {
        let folder = TempFolder::new();
        for name in [
            "Household 2026-10-01 09-00-00 UTC.ksz",
            "Household 2026-10-03 09-00-00 UTC.ksz",
            "Household 2026-10-03 09-00-00 UTC 2.ksz",
            "Household 2026-10-02 09-00-00 UTC.ksz.part",
            "Garden 2026-10-04 09-00-00 UTC.ksz",
            "Household.ksz",
        ] {
            fs::write(folder.join(name), "made up").expect("written");
        }
        fs::create_dir(folder.join("Household 2026-10-05 09-00-00 UTC.ksz")).expect("made");

        let listed: Vec<String> = backups_of(Path::new("/books/Household.ksz"), folder.path())
            .expect("listed")
            .into_iter()
            .map(|file| {
                file.path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .expect("name")
                    .to_owned()
            })
            .collect();
        assert_eq!(
            listed,
            [
                "Household 2026-10-03 09-00-00 UTC 2.ksz",
                "Household 2026-10-03 09-00-00 UTC.ksz",
                "Household 2026-10-01 09-00-00 UTC.ksz",
            ]
        );
        let missing = backups_of(Path::new("/books/Household.ksz"), &folder.join("gone"));
        assert!(matches!(missing, Err(StoreError::Io(ErrorKind::NotFound))));
    }

    #[test]
    fn a_backup_is_put_in_place_of_the_closed_book() {
        let folder = TempFolder::new();
        let (mut book, path) = book_with(&folder, "Giro");
        let backups = computer(&folder).backup_folder;
        let backup = book.back_up_now(&path, &backups).expect("backed up");
        add_account(&mut book, "Savings");

        // Made ready while the book is open; then the book is backed up as
        // it is now, often enough that the backup restored is pruned.
        let prepared = PreparedRestore::new(&backup, &path).expect("made ready");
        for _ in 0..backup::KEEP {
            book.back_up_now(&path, &backups).expect("backed up");
        }
        assert!(!backup.exists());
        drop(book);
        prepared.put_in_place().expect("put in place");

        let restored = Book::open(&path, &computer(&folder)).expect("book opens");
        assert_eq!(account_names(&restored), ["Giro"]);
        // No `.part` is left next to the book, and the book as it was
        // before is the newest backup.
        assert!(!folder.join("Household.ksz.restore.part").exists());
        let newest = backups_of(&path, &backups).expect("listed");
        let before = Book::open(&newest[0].path, &computer(&TempFolder::new())).expect("opens");
        assert_eq!(account_names(&before), ["Giro", "Savings"]);
    }

    #[test]
    fn a_restore_given_up_leaves_the_book_and_no_part_file() {
        let folder = TempFolder::new();
        let (book, path) = book_with(&folder, "Giro");
        let backup = book
            .back_up_now(&path, &computer(&folder).backup_folder)
            .expect("backed up");
        drop(book);
        let before = fs::read(&path).expect("book read");
        let prepared = PreparedRestore::new(&backup, &path).expect("made ready");
        assert!(folder.join("Household.ksz.restore.part").exists());
        drop(prepared);
        assert!(!folder.join("Household.ksz.restore.part").exists());
        assert_eq!(fs::read(&path).expect("book read"), before);
    }

    #[test]
    fn a_backup_that_isnt_a_usable_book_changes_nothing() {
        let folder = TempFolder::new();
        let (book, path) = book_with(&folder, "Giro");
        drop(book);
        let before = fs::read(&path).expect("book read");

        let not_a_book = folder.join("Household 2026-10-01 09-00-00 UTC.ksz");
        fs::write(&not_a_book, "made up").expect("written");
        assert!(matches!(
            PreparedRestore::new(&not_a_book, &path),
            Err(StoreError::NotABook)
        ));

        // A book from a newer KassenSturz.
        let newer = folder.join("Household 2026-10-02 09-00-00 UTC.ksz");
        fs::copy(&path, &newer).expect("copied");
        Connection::open(&newer)
            .and_then(|conn| conn.pragma_update(None, "user_version", layout::VERSION + 1))
            .expect("version set");
        assert!(matches!(
            PreparedRestore::new(&newer, &path),
            Err(StoreError::UnsupportedLayout { .. })
        ));

        assert!(matches!(
            PreparedRestore::new(&folder.join("gone.ksz"), &path),
            Err(StoreError::Io(ErrorKind::NotFound))
        ));
        assert_eq!(fs::read(&path).expect("book read"), before);
        assert!(!folder.join("Household.ksz.restore.part").exists());
    }
}
