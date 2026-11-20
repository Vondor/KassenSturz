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

/// The folders in the earlier folder that are not copied: the logs are
/// kept by `kassensturz_core` in the data folder as before, and a copy of
/// them would only be an old one; the requests of the MCP server (D-050)
/// are read from the data folder by both programs and are never wanted in
/// the user's Documents (B-429).
const NOT_COPIED: &[&str] = &["debug-log", "bank-log", "mcp-requests"];

/// In a snap the bank connection's folder stays in the snap's data folder
/// (B-429): Documents is often a sync folder, and the threat model keeps
/// that folder out of one (T-07). Losing it with the snap only means
/// setting the bank connection up again. The Store app copies it (D-856).
fn stays_in_snap_data(name: &str) -> bool {
    name == "aqbanking" && crate::platform::in_snap_sandbox()
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
            if top && (NOT_COPIED.contains(&text) || stays_in_snap_data(text)) {
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
