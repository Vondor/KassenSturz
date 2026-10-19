// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Automatic backups before the book's layout is updated (AGENTS.md §3.1
//! rule 8, D-033).
//!
//! A backup is a complete copy of the book, made by SQLite while the book is
//! open. It is a book itself, with the same extension, so restoring one means
//! opening it (D-037). Backups go into a folder in KassenSturz's own data
//! folder, which keeps the last [`KEEP`] of each book. Names and retention are
//! documented in `docs/BOOK_FORMAT.md`.

use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use kassensturz_core::Timestamp;
use rusqlite::Connection;

use crate::error::StoreError;

/// How many backups of each book are kept (D-033).
pub(crate) const KEEP: usize = 10;

/// When a backup was made, in UTC: the store has no time zone data (D-061).
/// Without colons, which Windows doesn't allow in names.
const STAMP_FORMAT: &str = "%Y-%m-%d %H-%M-%S UTC";

/// What [`STAMP_FORMAT`] gives, with `0` for each digit.
const STAMP_PATTERN: &str = "0000-00-00 00-00-00 UTC";

/// The most backups of one book with the same time that get a name of their
/// own: `… UTC.ksz`, `… UTC 2.ksz` and so on.
const MAX_PER_SECOND: u32 = 100;

/// Copies the book that `conn` has open, which is at `book`, into `folder`,
/// then removes that book's oldest backups beyond the last [`KEEP`]. Returns
/// the copy's path.
pub(crate) fn back_up(
    conn: &Connection,
    book: &Path,
    folder: &Path,
) -> Result<PathBuf, StoreError> {
    let stem = book
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or(StoreError::Io(ErrorKind::InvalidFilename))?;
    // Absolute, so SQLite never takes the copy's name for a URI.
    let folder = std::path::absolute(folder)?;
    fs::create_dir_all(&folder)?;
    let stamp = Timestamp::now().strftime(STAMP_FORMAT).to_string();
    // The copy is written under another name first, so a copy that was cut
    // short, e.g. by a crash, is never taken for a backup.
    let (path, partial) = claim(&folder, stem, &stamp)?;
    let copied =
        copy(conn, &partial).and_then(|()| fs::rename(&partial, &path).map_err(StoreError::from));
    if let Err(error) = copied {
        let _ = fs::remove_file(&partial);
        return Err(error);
    }
    prune(&folder, stem, &path);
    Ok(path)
}

/// Copies the database `conn` has open into `to`, the empty file claimed for
/// it, and makes sure the copy is on the disk.
fn copy(conn: &Connection, to: &Path) -> Result<(), StoreError> {
    let name = to
        .to_str()
        .ok_or(StoreError::Io(ErrorKind::InvalidFilename))?;
    // VACUUM INTO writes into an empty file, but not into a database
    // (sqlite.org/lang_vacuum.html).
    conn.execute("VACUUM INTO ?1", [name])?;
    // VACUUM INTO doesn't wait until the copy is on the disk, so it's synced
    // here, before the update changes the book. Windows syncs only files
    // opened for writing.
    OpenOptions::new().write(true).open(to)?.sync_all()?;
    Ok(())
}

/// Picks the first free name for a backup of the book `stem` at `stamp`,
/// e.g. `Household 2026-10-19 11-04-02 UTC.ksz`, or `… UTC 2.ksz` and so on
/// if that is taken, so a backup never replaces another. Returns it with the
/// name the copy is written under first, which is claimed by creating an
/// empty file, so no other backup can write there.
fn claim(folder: &Path, stem: &str, stamp: &str) -> Result<(PathBuf, PathBuf), StoreError> {
    for number in 1..=MAX_PER_SECOND {
        let name = if number == 1 {
            format!("{stem} {stamp}.ksz")
        } else {
            format!("{stem} {stamp} {number}.ksz")
        };
        let path = folder.join(name);
        if exists(&path)? {
            continue;
        }
        let partial = partial_path(&path);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&partial)
        {
            Ok(_) => return Ok((path, partial)),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(StoreError::Io(ErrorKind::AlreadyExists))
}

fn partial_path(path: &Path) -> PathBuf {
    let mut partial = path.as_os_str().to_owned();
    partial.push(".partial");
    PathBuf::from(partial)
}

fn exists(path: &Path) -> Result<bool, StoreError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// Removes the oldest backups of the book `stem` until the last [`KEEP`] are
/// left, `just_made` always among them, even if the computer's clock was put
/// back since the others were made. Only names that [`back_up`] gives count,
/// so other files in the folder, and the backups of other books, are left
/// alone. Errors are ignored: a backup too many does no harm, and the next
/// backup tries again.
fn prune(folder: &Path, stem: &str, just_made: &Path) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    let mut older: Vec<(String, u32, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let (stamp, number) = parse_name(&name, stem)?;
            Some((stamp.to_owned(), number, entry.path()))
        })
        .filter(|(_, _, path)| path != just_made)
        .collect();
    // Oldest first: the stamps sort by time, the numbers within one second.
    older.sort();
    let excess = (older.len() + 1).saturating_sub(KEEP);
    for (_, _, path) in older.into_iter().take(excess) {
        let _ = fs::remove_file(path);
    }
}

/// The stamp and number of `name` if it is the name of a backup of the book
/// `stem`; the number is 1 if the name has none.
fn parse_name<'a>(name: &'a str, stem: &str) -> Option<(&'a str, u32)> {
    let rest = name
        .strip_prefix(stem)?
        .strip_prefix(' ')?
        .strip_suffix(".ksz")?;
    let stamp = rest.get(..STAMP_PATTERN.len())?;
    let is_stamp = stamp
        .bytes()
        .zip(STAMP_PATTERN.bytes())
        .all(|(byte, pattern)| match pattern {
            b'0' => byte.is_ascii_digit(),
            _ => byte == pattern,
        });
    if !is_stamp {
        return None;
    }
    let number = match &rest[STAMP_PATTERN.len()..] {
        "" => 1,
        suffix => {
            let digits = suffix.strip_prefix(' ')?;
            if digits.starts_with('0') || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            digits.parse().ok().filter(|&number| number >= 2)?
        }
    };
    Some((stamp, number))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_folder::TempFolder;

    fn names_in(folder: &TempFolder) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(folder.path())
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
    fn backup_names_are_recognized_strictly() {
        let stamp = "2026-10-19 11-04-02 UTC";
        for (name, expected) in [
            ("Household 2026-10-19 11-04-02 UTC.ksz", Some((stamp, 1))),
            ("Household 2026-10-19 11-04-02 UTC 2.ksz", Some((stamp, 2))),
            (
                "Household 2026-10-19 11-04-02 UTC 37.ksz",
                Some((stamp, 37)),
            ),
            // Not a backup of this book.
            ("Household.ksz", None),
            ("Household 2 2026-10-19 11-04-02 UTC.ksz", None),
            ("Household2026-10-19 11-04-02 UTC.ksz", None),
            ("Garden 2026-10-19 11-04-02 UTC.ksz", None),
            // Not a name back_up gives.
            ("Household 2026-10-19 11-04-02 UTC.ksz.partial", None),
            ("Household 2026-10-19 11-04-02 UTC 1.ksz", None),
            ("Household 2026-10-19 11-04-02 UTC 02.ksz", None),
            ("Household 2026-10-19 11-04-02 UTC 2a.ksz", None),
            ("Household 2026-10-19 11-04-02 UTC .ksz", None),
            ("Household 2026-10-19 11:04:02 UTC.ksz", None),
            ("Household 2026-10-19 11-04-02 CET.ksz", None),
            ("Household 2026-10-19.ksz", None),
            ("Household 2026-10-19 11-04-02 UTC.kmy", None),
            ("Household 2026-1Ö-19 11-04-02 UTC.ksz", None),
        ] {
            assert_eq!(parse_name(name, "Household"), expected, "{name}");
        }
    }

    #[test]
    fn the_stamp_has_the_documented_form() {
        let stamp = Timestamp::from_second(1_792_407_842)
            .expect("valid time")
            .strftime(STAMP_FORMAT)
            .to_string();
        assert_eq!(stamp, "2026-10-19 11-04-02 UTC");
        assert_eq!(stamp.len(), STAMP_PATTERN.len());
    }

    #[test]
    fn a_backup_never_replaces_another() {
        let folder = TempFolder::new();
        let stamp = "2026-10-19 11-04-02 UTC";
        let (first, first_partial) = claim(folder.path(), "Household", stamp).expect("claimed");
        assert_eq!(first, folder.join("Household 2026-10-19 11-04-02 UTC.ksz"));
        assert_eq!(fs::metadata(&first_partial).expect("claimed").len(), 0);
        // Another backup in the same second, while the first is being made.
        let (second, _) = claim(folder.path(), "Household", stamp).expect("claimed");
        assert_eq!(
            second,
            folder.join("Household 2026-10-19 11-04-02 UTC 2.ksz")
        );
        // Finished, the first one keeps its name taken.
        fs::rename(&first_partial, &first).expect("renamed");
        let (third, _) = claim(folder.path(), "Household", stamp).expect("claimed");
        assert_eq!(
            third,
            folder.join("Household 2026-10-19 11-04-02 UTC 3.ksz")
        );
    }

    /// Writes made-up backups of the book "Household" for each day of
    /// October 2026 from `first_day` to `last_day`, plus other files that
    /// pruning must leave alone. Returns the backups' names, oldest first,
    /// and the other files' names.
    fn backups_and_others(
        folder: &TempFolder,
        first_day: u32,
        last_day: u32,
    ) -> (Vec<String>, Vec<String>) {
        let backups: Vec<String> = (first_day..=last_day)
            .map(|day| format!("Household 2026-10-{day:02} 09-00-00 UTC.ksz"))
            .collect();
        let others: Vec<String> = [
            "Household.ksz",
            "Household 2 2026-10-01 09-00-00 UTC.ksz",
            "Garden 2026-10-01 09-00-00 UTC.ksz",
            "Household 2026-10-01 08-00-00 UTC.ksz.partial",
            "Notes.txt",
        ]
        .map(String::from)
        .into();
        for name in backups.iter().chain(&others) {
            fs::write(folder.join(name), "made up").expect("file written");
        }
        (backups, others)
    }

    #[test]
    fn the_last_ten_backups_of_each_book_are_kept() {
        let folder = TempFolder::new();
        let (mut backups, others) = backups_and_others(&folder, 1, 12);
        // Two backups in the same second, right where the oldest three are
        // cut off: the second one is newer.
        let same_second = "Household 2026-10-03 09-00-00 UTC 2.ksz";
        fs::write(folder.join(same_second), "made up").expect("file written");
        backups.insert(3, same_second.into());

        let just_made = folder.join(backups.last().expect("a backup"));
        prune(folder.path(), "Household", &just_made);

        let mut expected: Vec<String> = backups[3..].iter().chain(&others).cloned().collect();
        expected.sort();
        assert_eq!(names_in(&folder), expected);
    }

    #[test]
    fn the_backup_just_made_is_kept_even_if_the_clock_was_put_back() {
        let folder = TempFolder::new();
        let (backups, others) = backups_and_others(&folder, 10, 20);
        let just_made = "Household 2026-10-01 09-00-00 UTC.ksz";
        fs::write(folder.join(just_made), "made up").expect("file written");

        prune(folder.path(), "Household", &folder.join(just_made));

        let mut expected: Vec<String> = backups[2..]
            .iter()
            .chain(&others)
            .cloned()
            .chain([just_made.to_owned()])
            .collect();
        expected.sort();
        assert_eq!(names_in(&folder), expected);
    }

    #[test]
    fn a_backup_is_a_complete_copy() {
        let folder = TempFolder::new();
        let conn = Connection::open_in_memory().expect("database opened");
        conn.execute_batch(
            "CREATE TABLE notes (text TEXT);
             INSERT INTO notes VALUES ('made up');
             PRAGMA user_version = 7;",
        )
        .expect("database written");
        let backups = folder.join("Backups");
        let path =
            back_up(&conn, Path::new("/home/sam/Household.ksz"), &backups).expect("backed up");
        assert_eq!(path.parent(), Some(backups.as_path()));
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("UTF-8 name");
        assert!(
            matches!(parse_name(name, "Household"), Some((_, 1))),
            "{name}"
        );
        assert!(!partial_path(&path).exists());

        let copy = Connection::open(&path).expect("backup opened");
        let text: String = copy
            .query_row("SELECT text FROM notes", [], |row| row.get(0))
            .expect("data copied");
        assert_eq!(text, "made up");
        let version: i32 = copy
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("version read");
        assert_eq!(version, 7);
    }
}
