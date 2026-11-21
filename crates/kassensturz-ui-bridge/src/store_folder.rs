// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! KassenSturz's own folder in the Microsoft Store's package (D-856, #1178).
//!
//! Windows keeps everything a packaged app writes to `%LOCALAPPDATA%` in the
//! package's own `LocalCache` and deletes it when the app is uninstalled or
//! reset. Until D-856 the Store build kept its book and the backups there,
//! so uninstalling it took the book and every backup with it (B-382). Now
//! its own folder is `Documents\KassenSturz` (`platform::app_folder`), which
//! Windows leaves alone. What the earlier folder holds comes over in two
//! steps, both without replacing anything:
//!
//! 1. **At the start, once:** everything but the books is copied into the
//!    new folder: the backups, the bank connection's folder, the plugins.
//!    Copied, not moved, so the earlier folder stays as it was.
//! 2. **A book still in the earlier folder** is offered a move into the new
//!    one once a start, with "Not now" and "Keep it there". The move is the
//!    one of D-731: a backup first, then copy, check, switch and remove.
//!    A book is never copied, so there are never two working books.
//!
//! Outside a Windows app package, and in the test profile, nothing here
//! runs. Paths are never logged (D-730).

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use crate::settings::Settings;

/// The name of KassenSturz's own folder in the user's Documents folder.
pub const FOLDER_NAME: &str = "KassenSturz";

/// The settings group of this module.
const GROUP: &str = "StoreFolder";
/// Set once the earlier folder's files were copied over.
const COPIED: &str = "CopiedFromPackage";
/// The book the user chose to keep in the earlier folder: no more offers
/// for it.
const KEEP: &str = "KeepBookInPackage";
/// The book the user chose to keep in a OneDrive folder (D-968): no more
/// offers for it.
const KEEP_SYNCED: &str = "KeepBookInSyncFolder";
/// Whether the user answered the offer to move the bank connection's folder
/// out of `Documents\KassenSturz` (D-967): `true` once answered.
const BANK_ANSWERED: &str = "BankFolderAnswered";
/// `true` while the user's agreed move of the bank connection's folder
/// waits for the next start, `done` after it.
const MOVE_BANK: &str = "MoveBankFolder";
/// Set while the last try to move the bank folder failed (B-450).
const BANK_MOVE_FAILED: &str = "MoveBankFolderFailed";

/// The folders in the earlier folder that are not copied: the logs are
/// kept by `kassensturz_core` in the data folder as before, and a copy of
/// them would only be an old one; the requests of the MCP server (D-050)
/// are read from the data folder by both programs and are never wanted in
/// the user's Documents (B-429).
const NOT_COPIED: &[&str] = &["debug-log", "bank-log", "mcp-requests"];

/// The bank connection's folder stays in the app's own data folder in a
/// snap (B-429) and in the Store app (B-437, #1348): Documents is often a
/// sync folder, and the threat model keeps that folder out of one (T-07).
/// Losing it with the app only means setting the bank connection up again.
fn stays_in_private_data(name: &str) -> bool {
    stays_in_private_data_of(
        name,
        crate::platform::in_snap_sandbox() || crate::platform::in_app_package(),
    )
}

/// [`stays_in_private_data`], with whether the app runs in a snap or in the
/// Store's package given as `packaged`.
fn stays_in_private_data_of(name: &str, packaged: bool) -> bool {
    packaged && name == crate::platform::BANK_FOLDER
}

/// What the start's copy did, for the tests.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Copied {
    /// Files copied.
    pub files: usize,
    /// Files left out because a file of that name was there already.
    pub kept: usize,
}

/// Copies the earlier folder's files into the new one once, at the start of
/// the Store build. Call it after the application object exists and before
/// any book opens.
pub fn copy_at_start() {
    if crate::test_profile::on() || !crate::platform::own_folder_outside_app_data() {
        return;
    }
    let (Some(from), Some(to)) = (
        crate::platform::qt_app_folder(),
        crate::platform::app_folder(),
    ) else {
        return;
    };
    let settings = Settings::user();
    if let Err(error) = copy_once(&settings, &from, &to) {
        // Tried again at the next start; the earlier folder is unchanged.
        eprintln!("kassensturz: couldn't copy the files of the earlier folder: {error}");
    }
    move_bank_folder_at_start(&settings);
}

/// The bank connection's folder the user agreed to move out of
/// `Documents\KassenSturz` (D-967): moved at the start, before anything
/// uses it. A move that fails leaves it where it was, and it is tried again
/// at the next start.
fn move_bank_folder_at_start(settings: &Settings) {
    if !crate::platform::in_app_package() || settings.text(GROUP, MOVE_BANK) != "true" {
        return;
    }
    let (Some(documents), Some(private)) = (
        crate::platform::store_documents_folder(),
        crate::platform::qt_app_folder(),
    ) else {
        return;
    };
    match move_bank_folder(&documents, &private) {
        Ok(()) => {
            let _ = settings.set_text(GROUP, MOVE_BANK, "done");
            let _ = settings.set_text(GROUP, BANK_MOVE_FAILED, "false");
        }
        Err(error) => {
            eprintln!("kassensturz: couldn't move the bank connection's folder: {error}");
            // The window tells the user (a GUI app has no console).
            let _ = settings.set_text(GROUP, BANK_MOVE_FAILED, "true");
        }
    }
}

/// Copies `from` into `to` unless the settings say it was done, then notes
/// it. A folder `from` that isn't there needs nothing.
pub fn copy_once(settings: &Settings, from: &Path, to: &Path) -> io::Result<Copied> {
    if settings.text(GROUP, COPIED) == "true" {
        return Ok(Copied::default());
    }
    let copied = if from.is_dir() {
        copy_missing(from, to, true)?
    } else {
        Copied::default()
    };
    // Unsaved settings copy again next time, which replaces nothing.
    let _ = settings.set_text(GROUP, COPIED, "true");
    Ok(copied)
}

/// Copies every file of `from` that `to` lacks, folder by folder. At the
/// top (`top`), books and their neighbours and the log folders are left
/// out. An existing file is never replaced.
fn copy_missing(from: &Path, to: &Path, top: bool) -> io::Result<Copied> {
    let mut copied = Copied::default();
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(text) = name.to_str() else {
            continue;
        };
        let kind = entry.file_type()?;
        let target = to.join(&name);
        if kind.is_dir() {
            if top && (NOT_COPIED.contains(&text) || stays_in_private_data(text)) {
                continue;
            }
            let inner = copy_missing(&entry.path(), &target, false)?;
            copied.files += inner.files;
            copied.kept += inner.kept;
        } else if kind.is_file() {
            if top && is_book_file(text) {
                continue;
            }
            if copy_new(&entry.path(), &target)? {
                copied.files += 1;
            } else {
                copied.kept += 1;
            }
        }
        // Links are left out: a packaged app's folder has none of its own.
    }
    Ok(copied)
}

/// Whether `name` is a book or a file that belongs to one: its lock file,
/// SQLite's journal files, or a move's unfinished copy. These are never
/// copied: a book moves (D-731).
fn is_book_file(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [
        ".ksz",
        ".ksz.lock",
        ".ksz-journal",
        ".ksz-wal",
        ".ksz-shm",
        ".ksz.part",
    ]
    .iter()
    .any(|ending| name.ends_with(ending))
}

/// Copies `from` to `to` unless `to` exists. Returns whether it copied. A
/// half-written copy is removed again.
fn copy_new(from: &Path, to: &Path) -> io::Result<bool> {
    let mut target = match OpenOptions::new().write(true).create_new(true).open(to) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Ok(false),
        Err(error) => return Err(error),
    };
    let written = io::copy(&mut File::open(from)?, &mut target).and_then(|_| target.sync_all());
    if let Err(error) = written {
        drop(target);
        let _ = fs::remove_file(to);
        return Err(error);
    }
    Ok(true)
}

/// Whether the book at `book` is in `earlier`, the folder Windows deletes
/// with the app, and the user hasn't chosen to keep it there.
pub fn offer_move(settings: &Settings, book: &Path, earlier: &Path) -> bool {
    in_folder(book, earlier) && settings.path(GROUP, KEEP).as_deref() != Some(book)
}

/// The folder Windows deletes with the app, where the Store build kept its
/// books before D-856; `None` outside the Store build and in the test
/// profile, so nothing is ever offered there.
pub fn earlier_folder() -> Option<PathBuf> {
    if crate::test_profile::on() || !crate::platform::own_folder_outside_app_data() {
        return None;
    }
    crate::platform::qt_app_folder()
}

/// Notes that the user keeps `book` where it is: no more offers for it.
pub fn keep_where_it_is(settings: &Settings, book: &Path) -> bool {
    settings.set_path(GROUP, KEEP, book)
}

/// Whether the start offers to move `book` out of a OneDrive folder
/// (D-968): it is inside one of `roots` and the user hasn't chosen to keep
/// it there.
pub fn offer_sync_move(settings: &Settings, book: &Path, roots: &[PathBuf]) -> bool {
    crate::platform::in_sync_folder(book, roots)
        && settings.path(GROUP, KEEP_SYNCED).as_deref() != Some(book)
}

/// Notes that the user keeps `book` in its OneDrive folder.
pub fn keep_in_sync_folder(settings: &Settings, book: &Path) -> bool {
    settings.set_path(GROUP, KEEP_SYNCED, book)
}

/// Makes the local own folder `local` ready for a book that moves out of
/// OneDrive (D-968): everything of `documents` but the books, the logs and
/// the bank connection is copied into it, without replacing a file. From
/// then on `local` is the app's own folder, so the backups and plugins are
/// found there. `documents` stays as it was.
///
/// The app picks `local` as its folder as soon as it exists, so a
/// preparation that fails half-way (a backup that exists only online, no
/// network) must not leave a half-filled folder behind: one this call made
/// is removed again and the error returned (B-451, #1366). The user's own
/// earlier `local` folder is never removed.
pub fn prepare_local_folder(documents: &Path, local: &Path) -> io::Result<Copied> {
    prepare_local_folder_with(documents, local, |from, to| copy_missing(from, to, true))
}

fn prepare_local_folder_with(
    documents: &Path,
    local: &Path,
    copy: impl FnOnce(&Path, &Path) -> io::Result<Copied>,
) -> io::Result<Copied> {
    let existed = local.exists();
    let made = if documents.is_dir() {
        copy(documents, local)
    } else {
        fs::create_dir_all(local).map(|()| Copied::default())
    };
    if made.is_err() && !existed {
        let _ = fs::remove_dir_all(local);
    }
    made
}

/// Whether the start offers to move the bank connection's folder out of
/// `documents` (D-967): it is there and the user hasn't answered yet.
pub fn offer_bank_move(settings: &Settings, documents: &Path) -> bool {
    documents.join(crate::platform::BANK_FOLDER).is_dir()
        && settings.text(GROUP, BANK_ANSWERED) != "true"
}

/// Whether the last try to move the bank connection's folder failed and
/// the earlier folder in `Documents` is still in use.
pub fn bank_move_failed(settings: &Settings) -> bool {
    settings.text(GROUP, BANK_MOVE_FAILED) == "true"
}

/// Notes the user's answer to the bank folder offer: `move` at the next
/// start, or keep it where it is.
pub fn answer_bank_move(settings: &Settings, move_it: bool) -> bool {
    let noted = !move_it || settings.set_text(GROUP, MOVE_BANK, "true");
    noted && settings.set_text(GROUP, BANK_ANSWERED, "true")
}

/// Moves the bank connection's folder from `from_parent` into
/// `to_parent` (D-967): copied to `aqbanking.part`, checked file by file,
/// a folder of that name already in `to_parent` kept under a new name,
/// then switched; only then is the folder in `from_parent` taken out of use
/// (renamed) and removed. Nothing is replaced, and until the switch the
/// earlier folder is the one in use.
pub fn move_bank_folder(from_parent: &Path, to_parent: &Path) -> io::Result<()> {
    let name = crate::platform::BANK_FOLDER;
    let from = from_parent.join(name);
    if !from.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(to_parent)?;
    let target = to_parent.join(name);
    // A move that stopped after the switch (the sync client held the old
    // folder): the new folder is the same, so only the old one is still to
    // be taken out of use. Nothing is copied again and no copy piles up
    // (B-450, #1365).
    if target.is_dir() && same_files(&from, &target).unwrap_or(false) {
        return retire(&from, from_parent, name);
    }
    let part = to_parent.join(format!("{name}.part"));
    if part.exists() {
        // An unfinished copy of an earlier start; it was never used.
        fs::remove_dir_all(&part)?;
    }
    let checked = copy_missing(&from, &part, false).and_then(|_| {
        if same_files(&from, &part)? {
            Ok(())
        } else {
            Err(io::Error::other("the copy differs"))
        }
    });
    if let Err(error) = checked {
        let _ = fs::remove_dir_all(&part);
        return Err(error);
    }
    if target.exists() {
        fs::rename(
            &target,
            free_name(to_parent, &format!("{name}.before-move")),
        )?;
    }
    fs::rename(&part, &target)?;
    retire(&from, from_parent, name)
}

/// Takes the earlier bank folder `from` out of use (renamed) and removes
/// it. A failing rename is an error, so the move is tried again; a failing
/// removal is an error too, because the leftover keeps syncing.
fn retire(from: &Path, from_parent: &Path, name: &str) -> io::Result<()> {
    let retired = free_name(from_parent, &format!("{name}.moved"));
    fs::rename(from, &retired)?;
    fs::remove_dir_all(&retired)
}

/// `base` in `folder`, or `base 2`, `base 3` and so on: the first name
/// that isn't there.
fn free_name(folder: &Path, base: &str) -> PathBuf {
    let mut candidate = folder.join(base);
    let mut number = 2;
    while candidate.exists() {
        candidate = folder.join(format!("{base} {number}"));
        number += 1;
    }
    candidate
}

/// Whether `a` and `b` hold the same files with the same bytes, folder by
/// folder.
fn same_files(a: &Path, b: &Path) -> io::Result<bool> {
    let names = |folder: &Path| -> io::Result<Vec<(std::ffi::OsString, bool)>> {
        let mut names = fs::read_dir(folder)?
            .map(|entry| {
                let entry = entry?;
                Ok((entry.file_name(), entry.file_type()?.is_dir()))
            })
            .collect::<io::Result<Vec<_>>>()?;
        names.sort();
        Ok(names)
    };
    let (left, right) = (names(a)?, names(b)?);
    if left != right {
        return Ok(false);
    }
    for (name, is_dir) in left {
        let same = if is_dir {
            same_files(&a.join(&name), &b.join(&name))?
        } else {
            fs::read(a.join(&name))? == fs::read(b.join(&name))?
        };
        if !same {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Whether `book` is in `folder` or one of its folders. Windows compares
/// names without regard to capitals.
pub fn in_folder(book: &Path, folder: &Path) -> bool {
    let lower = |path: &Path| -> Option<PathBuf> {
        path.to_str().map(|text| PathBuf::from(text.to_lowercase()))
    };
    if book.starts_with(folder) {
        return true;
    }
    match (lower(book), lower(folder)) {
        (Some(book), Some(folder)) => book.starts_with(folder),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_folder::TestFolder;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().expect("a folder")).expect("folder made");
        fs::write(path, text).expect("written");
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).expect("read")
    }

    /// The earlier folder as the Store build left it: a book with its
    /// neighbours, backups, the bank connection's folder, plugins, logs.
    fn earlier(folder: &TestFolder) -> PathBuf {
        let from = folder.join("package/io.github.vondor.KassenSturz");
        write(&from.join("Household.ksz"), "book");
        write(&from.join("Household.ksz.lock"), "lock");
        write(&from.join("Household.ksz-journal"), "journal");
        write(&from.join("Moon Base.KSZ"), "second book");
        write(&from.join("backups/Household 2026-11-17.ksz"), "backup 1");
        write(&from.join("backups/Household 2026-11-18.ksz"), "backup 2");
        write(&from.join("aqbanking/settings.conf"), "bank");
        write(&from.join("plugins/green.wasm"), "plugin");
        write(&from.join("debug-log/current.jsonl"), "log");
        write(&from.join("bank-log/bank-log.jsonl"), "log");
        write(&from.join("mcp-requests/pending.json"), "request");
        from
    }

    #[test]
    fn everything_but_the_books_and_the_logs_is_copied_and_the_earlier_folder_stays() {
        let folder = TestFolder::new();
        let from = earlier(&folder);
        let to = folder.join("Documents/KassenSturz");
        let settings = Settings::at(&folder.join("kassensturzrc"));

        let copied = copy_once(&settings, &from, &to).expect("copied");

        assert_eq!(copied, Copied { files: 4, kept: 0 });
        assert_eq!(
            read(&to.join("backups/Household 2026-11-17.ksz")),
            "backup 1"
        );
        assert_eq!(
            read(&to.join("backups/Household 2026-11-18.ksz")),
            "backup 2"
        );
        assert_eq!(read(&to.join("aqbanking/settings.conf")), "bank");
        assert_eq!(read(&to.join("plugins/green.wasm")), "plugin");
        for left_out in [
            "Household.ksz",
            "Household.ksz.lock",
            "Household.ksz-journal",
            "Moon Base.KSZ",
            "debug-log",
            "bank-log",
            "mcp-requests",
        ] {
            assert!(!to.join(left_out).exists(), "{left_out} was copied");
        }
        // Nothing in the earlier folder changed.
        assert_eq!(read(&from.join("Household.ksz")), "book");
        assert_eq!(
            read(&from.join("backups/Household 2026-11-18.ksz")),
            "backup 2"
        );
    }

    #[test]
    fn the_bank_folder_is_never_copied_in_a_package() {
        assert!(stays_in_private_data_of("aqbanking", true));
        assert!(!stays_in_private_data_of("aqbanking", false));
        assert!(!stays_in_private_data_of("plugins", true));
    }

    /// D-967: the move of an earlier build's bank folder out of Documents.
    #[test]
    fn the_bank_folder_moves_checked_and_keeps_what_was_there() {
        let folder = TestFolder::new();
        let documents = folder.join("Documents/KassenSturz");
        let private = folder.join("package/io.github.vondor.KassenSturz");
        write(&documents.join("aqbanking/settings.conf"), "bank now");
        write(
            &documents.join("aqbanking/backends/aqhbci/users.conf"),
            "user",
        );
        write(
            &private.join("aqbanking/settings.conf"),
            "bank before D-856",
        );

        move_bank_folder(&documents, &private).expect("moved");

        assert_eq!(read(&private.join("aqbanking/settings.conf")), "bank now");
        assert_eq!(
            read(&private.join("aqbanking/backends/aqhbci/users.conf")),
            "user"
        );
        // The package's older folder is kept, not replaced.
        assert_eq!(
            read(&private.join("aqbanking.before-move/settings.conf")),
            "bank before D-856"
        );
        assert!(!documents.join("aqbanking").exists());
        assert!(!documents.join("aqbanking.moved").exists());
        assert!(!private.join("aqbanking.part").exists());
        // Nothing to do the second time.
        move_bank_folder(&documents, &private).expect("nothing to move");
        assert_eq!(read(&private.join("aqbanking/settings.conf")), "bank now");
    }

    /// B-450: a move that stopped after the switch is finished at the next
    /// start without a new copy of the folder piling up.
    #[test]
    fn a_half_done_bank_move_is_finished_without_piling_up() {
        let folder = TestFolder::new();
        let documents = folder.join("Documents/KassenSturz");
        let private = folder.join("package/io.github.vondor.KassenSturz");
        write(&documents.join("aqbanking/settings.conf"), "bank now");
        // The state after a failed last rename: the switch is done.
        write(&private.join("aqbanking/settings.conf"), "bank now");
        move_bank_folder(&documents, &private).expect("finished");
        assert!(!documents.join("aqbanking").exists());
        assert!(!documents.join("aqbanking.moved").exists());
        assert_eq!(read(&private.join("aqbanking/settings.conf")), "bank now");
        let piled = fs::read_dir(&private)
            .expect("listed")
            .filter(|e| {
                e.as_ref()
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .contains("before-move")
            })
            .count();
        assert_eq!(piled, 0);
    }

    #[test]
    fn a_later_bank_move_keeps_every_earlier_folder() {
        let folder = TestFolder::new();
        let documents = folder.join("Documents/KassenSturz");
        let private = folder.join("private");
        write(&private.join("aqbanking/a"), "1");
        write(&private.join("aqbanking.before-move/a"), "0");
        write(&private.join("aqbanking.part/half"), "left over");
        write(&documents.join("aqbanking/a"), "2");

        move_bank_folder(&documents, &private).expect("moved");

        assert_eq!(read(&private.join("aqbanking/a")), "2");
        assert_eq!(read(&private.join("aqbanking.before-move/a")), "0");
        assert_eq!(read(&private.join("aqbanking.before-move 2/a")), "1");
        assert!(!private.join("aqbanking/half").exists());
    }

    #[test]
    fn the_bank_offer_is_answered_once() {
        let folder = TestFolder::new();
        let documents = folder.join("Documents/KassenSturz");
        let settings = Settings::at(&folder.join("kassensturzrc"));
        assert!(!offer_bank_move(&settings, &documents));
        write(&documents.join("aqbanking/settings.conf"), "bank");
        assert!(offer_bank_move(&settings, &documents));
        assert!(answer_bank_move(&settings, false));
        assert!(!offer_bank_move(&settings, &documents));
        assert_ne!(settings.text(GROUP, MOVE_BANK), "true");

        let settings = Settings::at(&folder.join("other-rc"));
        assert!(answer_bank_move(&settings, true));
        assert!(!offer_bank_move(&settings, &documents));
        assert_eq!(settings.text(GROUP, MOVE_BANK), "true");
    }

    /// D-968: a book in OneDrive is offered a move until the user keeps it.
    #[test]
    fn a_book_in_onedrive_is_offered_a_move_until_kept() {
        let folder = TestFolder::new();
        let settings = Settings::at(&folder.join("kassensturzrc"));
        let roots = [PathBuf::from(r"C:\Users\Sam\OneDrive")];
        let book = Path::new(r"C:\Users\Sam\OneDrive\Documents\KassenSturz\Household.ksz");
        let local_book = Path::new(r"C:\Users\Sam\KassenSturz\Household.ksz");
        assert!(offer_sync_move(&settings, book, &roots));
        assert!(!offer_sync_move(&settings, local_book, &roots));
        assert!(!offer_sync_move(&settings, book, &[]));
        assert!(keep_in_sync_folder(&settings, book));
        assert!(!offer_sync_move(&settings, book, &roots));
    }

    /// D-968: the local folder gets the backups and plugins, never a book.
    #[test]
    fn the_local_folder_gets_everything_but_the_books() {
        let folder = TestFolder::new();
        let documents = earlier(&folder);
        let local = folder.join("Sam/KassenSturz");
        let copied = prepare_local_folder(&documents, &local).expect("ready");
        assert!(copied.files >= 3);
        assert_eq!(
            read(&local.join("backups/Household 2026-11-18.ksz")),
            "backup 2"
        );
        assert_eq!(read(&local.join("plugins/green.wasm")), "plugin");
        assert!(!local.join("Household.ksz").exists());
        assert!(!local.join("debug-log").exists());
        assert_eq!(read(&documents.join("Household.ksz")), "book");
        // Without a Documents folder, only the local one is made.
        let empty = folder.join("Nobody/KassenSturz");
        prepare_local_folder(&folder.join("missing"), &empty).expect("made");
        assert!(empty.is_dir());
    }

    /// B-451: a preparation that fails part-way leaves no new folder, so
    /// the app's folder does not switch; an existing one is kept.
    #[test]
    fn a_preparation_that_fails_part_way_leaves_no_folder() {
        let folder = TestFolder::new();
        let documents = earlier(&folder);
        let fail = |from: &Path, to: &Path| {
            copy_missing(from, to, true)?;
            Err(io::Error::other("a backup could not be read"))
        };
        let local = folder.join("Sam/KassenSturz");
        assert!(prepare_local_folder_with(&documents, &local, fail).is_err());
        assert!(!local.exists());
        assert_eq!(
            read(&documents.join("backups/Household 2026-11-18.ksz")),
            "backup 2"
        );
        let own = folder.join("Kim/KassenSturz");
        write(&own.join("mine.txt"), "mine");
        assert!(prepare_local_folder_with(&documents, &own, fail).is_err());
        assert_eq!(read(&own.join("mine.txt")), "mine");
    }

    #[test]
    fn a_file_in_the_new_folder_is_never_replaced() {
        let folder = TestFolder::new();
        let from = earlier(&folder);
        let to = folder.join("Documents/KassenSturz");
        write(&to.join("backups/Household 2026-11-18.ksz"), "newer backup");
        let settings = Settings::at(&folder.join("kassensturzrc"));

        let copied = copy_once(&settings, &from, &to).expect("copied");

        assert_eq!(copied, Copied { files: 3, kept: 1 });
        assert_eq!(
            read(&to.join("backups/Household 2026-11-18.ksz")),
            "newer backup"
        );
    }

    #[test]
    fn the_copy_runs_once() {
        let folder = TestFolder::new();
        let from = earlier(&folder);
        let to = folder.join("Documents/KassenSturz");
        let settings = Settings::at(&folder.join("kassensturzrc"));
        copy_once(&settings, &from, &to).expect("copied");
        fs::remove_dir_all(to.join("plugins")).expect("removed");

        // A plugin the user removed in the new folder doesn't come back.
        assert_eq!(
            copy_once(&settings, &from, &to).expect("checked"),
            Copied::default()
        );
        assert!(!to.join("plugins").exists());
    }

    #[test]
    fn without_an_earlier_folder_nothing_is_copied_and_nothing_fails() {
        let folder = TestFolder::new();
        let to = folder.join("Documents/KassenSturz");
        let settings = Settings::at(&folder.join("kassensturzrc"));
        let copied = copy_once(&settings, &folder.join("package/none"), &to).expect("nothing");
        assert_eq!(copied, Copied::default());
    }

    #[test]
    fn only_a_book_in_the_earlier_folder_is_offered_a_move_until_the_user_keeps_it() {
        let folder = TestFolder::new();
        let earlier = folder.join("package/io.github.vondor.KassenSturz");
        let settings = Settings::at(&folder.join("kassensturzrc"));
        let inside = earlier.join("Household.ksz");
        let outside = folder.join("Documents/KassenSturz/Household.ksz");

        assert!(offer_move(&settings, &inside, &earlier));
        assert!(!offer_move(&settings, &outside, &earlier));

        assert!(keep_where_it_is(&settings, &inside));
        assert!(!offer_move(&settings, &inside, &earlier));
        // Another book there still gets the offer.
        assert!(offer_move(
            &settings,
            &earlier.join("Moon Base.ksz"),
            &earlier
        ));
    }

    #[test]
    fn the_folder_is_compared_without_regard_to_capitals() {
        let earlier = Path::new("/users/ada/appdata/local/io.github.vondor.kassensturz");
        assert!(in_folder(
            Path::new("/Users/Ada/AppData/Local/io.github.vondor.KassenSturz/Household.ksz"),
            earlier
        ));
        assert!(!in_folder(
            Path::new("/Users/Ada/Documents/Household.ksz"),
            earlier
        ));
    }

    /// Only a Windows app package has the folder Windows deletes with the
    /// app; nothing else is ever offered a move or copies a file.
    #[cfg(not(windows))]
    #[test]
    fn outside_a_windows_package_there_is_no_earlier_folder() {
        assert!(!crate::platform::in_app_package());
        assert!(earlier_folder().is_none());
    }

    #[test]
    fn the_books_neighbours_are_known() {
        for name in [
            "Household.ksz",
            "Household.KSZ",
            "Household.ksz.lock",
            "Household.ksz-journal",
            "Household.ksz-wal",
            "Household.ksz-shm",
            "Household.ksz.part",
        ] {
            assert!(is_book_file(name), "{name}");
        }
        for name in ["settings.conf", "green.wasm", "Household.ksz.txt"] {
            assert!(!is_book_file(name), "{name}");
        }
    }
}
