// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Automatic backups before the book's layout is updated (AGENTS.md §3.1
//! rule 8, D-033).
//!
//! A backup is a complete copy of the book, made by SQLite while the book is
//! open. It is a book itself, with the same extension, so restoring one means
//! opening it (D-037). Backups go into a folder in KassenSturz's own data
//! folder, which keeps the last [`KEEP`] of each book. A folder the user
//! chose gets copies too, with the same names (#806, `backup_copies.rs`).
//! Names and retention are documented in `docs/BOOK_FORMAT.md`.

use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use kassensturz_core::Timestamp;
use rusqlite::Connection;

use crate::error::StoreError;

/// How many backups of each book are kept (D-033).
pub(crate) const KEEP: usize = 10;

/// How many earlier months keep one backup each in the app's own folder, on
/// top of the last [`KEEP`] (B-255): a harm noticed late, such as a Rule
/// that books wrongly on every import, can outlast both the change
/// history and ten backups, so the oldest backup of each of the last year's
/// months stays.
pub(crate) const KEEP_MONTHS: usize = 12;

/// How old the newest backup in the app's own folder may be before opening
/// the book makes another one (B-255): thirty days.
pub(crate) const MONTHLY_SECONDS: i64 = 30 * 24 * 60 * 60;

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
    // Absolute, so SQLite never takes the copy's name for a URI.
    let folder = std::path::absolute(folder)?;
    fs::create_dir_all(&folder)?;
    let path = write_backup(conn, book, &folder, usize::MAX)?;
    prune_own(&folder, stem_of(book)?, &path, Timestamp::now());
    Ok(path)
}

/// Removes the oldest backups of the book `stem` in the app's own folder:
/// the last [`KEEP`] stay, `just_made` among them, and so does the oldest
/// one of each of the [`KEEP_MONTHS`] newest months before the month of
/// `now` that have one (B-255). Errors are ignored, as in [`prune`].
pub(crate) fn prune_own(folder: &Path, stem: &str, just_made: &Path, now: Timestamp) {
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
    // The stamps begin with the month, e.g. `2026-11`.
    let this_month = now.strftime("%Y-%m").to_string();
    let mut months: Vec<&str> = older
        .iter()
        .take(excess)
        .map(|(stamp, _, _)| &stamp[..7])
        .filter(|month| *month < this_month.as_str())
        .collect();
    months.dedup();
    let kept_months: Vec<&str> = months.iter().rev().take(KEEP_MONTHS).copied().collect();
    let mut seen = Vec::new();
    for (stamp, _, path) in older.iter().take(excess) {
        let month = &stamp[..7];
        if kept_months.contains(&month) && !seen.contains(&month) {
            seen.push(month);
            continue;
        }
        let _ = fs::remove_file(path);
    }
}

/// Copies the book that `conn` has open, which is at `book`, into `folder`,
/// which must be there already, then removes that book's oldest backups
/// beyond the last `keep`. Returns the copy's path.
pub(crate) fn write_backup(
    conn: &Connection,
    book: &Path,
    folder: &Path,
    keep: usize,
) -> Result<PathBuf, StoreError> {
    let stem = stem_of(book)?;
    // Absolute, so SQLite never takes the copy's name for a URI.
    let folder = std::path::absolute(folder)?;
    let stamp = Timestamp::now().strftime(STAMP_FORMAT).to_string();
    // The copy is written under another name first, so a copy that was cut
    // short, e.g. by a crash, is never taken for a backup, and a sync tool
    // never uploads half a copy.
    let (path, partial) = claim(&folder, stem, &stamp)?;
    let copied =
        copy(conn, &partial).and_then(|()| fs::rename(&partial, &path).map_err(StoreError::from));
    if let Err(error) = copied {
        let _ = fs::remove_file(&partial);
        return Err(error);
    }
    prune(&folder, stem, &path, keep);
    Ok(path)
}

/// The name a book's backups begin with: its file name without `.ksz`.
pub(crate) fn stem_of(book: &Path) -> Result<&str, StoreError> {
    book.file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or(StoreError::Io(ErrorKind::InvalidFilename))
}

/// Copies the database `conn` has open into `to`, the empty file claimed for
/// it, and makes sure the copy is on the disk.
fn copy(conn: &Connection, to: &Path) -> Result<(), StoreError> {
    vacuum_into(conn, to)?;
    // VACUUM INTO doesn't wait until the copy is on the disk, so it's synced
    // here, before the update changes the book. Windows syncs only files
    // opened for writing.
    OpenOptions::new().write(true).open(to)?.sync_all()?;
    Ok(())
}

/// Writes a copy of the database `conn` has open into `to`, an empty file.
///
/// VACUUM INTO writes into an empty file, but not into a database
/// (sqlite.org/lang_vacuum.html). SQLite takes file locks on the copy, and
/// some places refuse them: in a snap, the folders the portal hands out
/// (B-265). Then the copy is made in a private folder of the app, where
/// locks work, and put into `to` as plain bytes, which needs none.
pub(crate) fn vacuum_into(conn: &Connection, to: &Path) -> Result<(), StoreError> {
    let name = to
        .to_str()
        .ok_or(StoreError::Io(ErrorKind::InvalidFilename))?;
    let Err(first) = conn.execute("VACUUM INTO ?1", [name]) else {
        return Ok(());
    };
    // Only an empty file is written into: anything else is the user's, and
    // is never replaced here.
    if fs::metadata(to).map_or(true, |meta| meta.len() != 0) {
        return Err(first.into());
    }
    via_private_folder(conn, to).map_err(|error| match error {
        // The same trouble in the private folder is not about `to`.
        StoreError::Database(_) => first.into(),
        other => other,
    })
}

/// The copy made in a private folder, then written into the empty file `to`.
fn via_private_folder(conn: &Connection, to: &Path) -> Result<(), StoreError> {
    let private = crate::lockless::PrivateFolder::new()?;
    let staged = private.path().join("copy.ksz");
    let staged_name = staged
        .to_str()
        .ok_or(StoreError::Io(ErrorKind::InvalidFilename))?;
    conn.execute("VACUUM INTO ?1", [staged_name])?;
    fs::copy(&staged, to)?;
    Ok(())
}

/// Picks the first free name for a backup of the book `stem` at `stamp`,
/// e.g. `Household 2026-10-19 11-04-02 UTC.ksz`, or `… UTC 2.ksz` and so on
/// if that is taken, so a backup never replaces another. Returns it with the
/// name the copy is written under first, which is claimed by creating an
/// empty file, so no other backup can write there.
pub(crate) fn claim(
    folder: &Path,
    stem: &str,
    stamp: &str,
) -> Result<(PathBuf, PathBuf), StoreError> {
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

/// The name a backup is written under before it is finished: with `.part`
/// added, which sync tools such as Nextcloud leave out by default, so a half
/// copy never travels (#806).
pub(crate) fn partial_path(path: &Path) -> PathBuf {
    let mut partial = path.as_os_str().to_owned();
    partial.push(".part");
    PathBuf::from(partial)
}

pub(crate) fn exists(path: &Path) -> Result<bool, StoreError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// Removes the oldest backups of the book `stem` until the last `keep` are
/// left, `just_made` always among them, even if the computer's clock was put
/// back since the others were made. Only names that [`back_up`] gives count,
/// so other files in the folder, and the backups of other books, are left
/// alone. Errors are ignored: a backup too many does no harm, and the next
/// backup tries again.
pub(crate) fn prune(folder: &Path, stem: &str, just_made: &Path, keep: usize) {
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
    let excess = (older.len() + 1).saturating_sub(keep.max(1));
    for (_, _, path) in older.into_iter().take(excess) {
        let _ = fs::remove_file(path);
    }
}

/// The stamp and number of `name` if it is the name of a backup of the book
/// `stem`; the number is 1 if the name has none.
pub(crate) fn parse_name<'a>(name: &'a str, stem: &str) -> Option<(&'a str, u32)> {
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

/// When a backup whose name has `stamp` (as [`parse_name`] gives it) was
/// made.
pub(crate) fn stamp_time(stamp: &str) -> Option<Timestamp> {
    let time = stamp.strip_suffix(" UTC")?;
    // The stamp names no offset, but is always UTC (D-061).
    Timestamp::strptime("%Y-%m-%d %H-%M-%S %z", format!("{time} +0000")).ok()
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
            ("Household 2026-10-19 11-04-02 UTC.ksz.part", None),
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
        // And it reads back as the same moment.
        assert_eq!(
            stamp_time(&stamp).map(|time| time.as_second()),
            Some(1_792_407_842)
        );
        assert_eq!(stamp_time("2026-10-19 11-04-02 CET"), None);
        assert_eq!(stamp_time("2026-13-19 11-04-02 UTC"), None);
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
            "Household 2026-10-01 08-00-00 UTC.ksz.part",
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
        prune(folder.path(), "Household", &just_made, KEEP);

        let mut expected: Vec<String> = backups[3..].iter().chain(&others).cloned().collect();
        expected.sort();
        assert_eq!(names_in(&folder), expected);
    }

    #[test]
    fn the_own_folder_also_keeps_the_oldest_backup_of_each_earlier_month() {
        let folder = TempFolder::new();
        // Three backups in each month from 2025-09 to 2026-10, then twelve
        // in November 2026, when the newest one is made.
        let mut backups = Vec::new();
        for month in 0..14 {
            let (year, number) = if month < 4 {
                (2025, month + 9)
            } else {
                (2026, month - 3)
            };
            for day in [3, 13, 23] {
                backups.push(format!(
                    "Household {year}-{number:02}-{day:02} 09-00-00 UTC.ksz"
                ));
            }
        }
        let november: Vec<String> = (1..=12)
            .map(|day| format!("Household 2026-11-{day:02} 09-00-00 UTC.ksz"))
            .collect();
        backups.extend(november.iter().cloned());
        for name in &backups {
            fs::write(folder.join(name), "made up").expect("file written");
        }
        let now = Timestamp::from_second(1_794_384_000).expect("a time"); // 2026-11-12
        assert_eq!(now.strftime("%Y-%m").to_string(), "2026-11");
        let just_made = folder.join(november.last().expect("one"));

        prune_own(folder.path(), "Household", &just_made, now);

        // The last ten of November, and the 3rd of each of the twelve
        // months before it (2025-11 to 2026-10); September and October 2025
        // are older than a year of months and go.
        let mut expected: Vec<String> = november[2..].to_vec();
        for month in 2..14 {
            let (year, number) = if month < 4 {
                (2025, month + 9)
            } else {
                (2026, month - 3)
            };
            expected.push(format!("Household {year}-{number:02}-03 09-00-00 UTC.ksz"));
        }
        expected.sort();
        assert_eq!(names_in(&folder), expected);

        // Pruned again, nothing more goes: one backup per earlier month stays.
        prune_own(folder.path(), "Household", &just_made, now);
        assert_eq!(names_in(&folder), expected);
    }

    #[test]
    fn the_backup_just_made_is_kept_even_if_the_clock_was_put_back() {
        let folder = TempFolder::new();
        let (backups, others) = backups_and_others(&folder, 10, 20);
        let just_made = "Household 2026-10-01 09-00-00 UTC.ksz";
        fs::write(folder.join(just_made), "made up").expect("file written");

        prune(folder.path(), "Household", &folder.join(just_made), KEEP);

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
    fn a_copy_made_in_the_private_folder_is_a_complete_book() {
        let folder = TempFolder::new();
        let conn = Connection::open_in_memory().expect("database opened");
        conn.execute_batch("CREATE TABLE t(x); INSERT INTO t VALUES (7);")
            .expect("filled");
        let to = folder.join("copy.ksz");
        fs::write(&to, "").expect("claimed");

        via_private_folder(&conn, &to).expect("copied");

        let copy = Connection::open(&to).expect("copy opened");
        let value: i64 = copy
            .query_row("SELECT x FROM t", [], |row| row.get(0))
            .expect("read back");
        assert_eq!(value, 7);
    }

    #[test]
    fn a_file_that_is_not_empty_is_never_replaced_by_the_fallback() {
        let folder = TempFolder::new();
        let conn = Connection::open_in_memory().expect("database opened");
        conn.execute_batch("CREATE TABLE t(x);").expect("filled");
        let to = folder.join("mine.ksz");
        fs::write(&to, "the user's own file").expect("file written");

        assert!(vacuum_into(&conn, &to).is_err());
        assert_eq!(fs::read(&to).expect("read"), b"the user's own file");
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
