// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The lock file that keeps a book open in only one place at a time (D-038).
//!
//! SQLite's own locks only work within one computer. A book in a sync folder
//! (Nextcloud, Dropbox, Syncthing) is a separate copy on every computer, so
//! while a book is open, KassenSturz keeps a small text file next to it,
//! `<book>.lock`, which the sync tool carries to the other computers. Its
//! format is documented in `docs/BOOK_FORMAT.md`.

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

use kassensturz_core::Timestamp;

use crate::error::StoreError;

/// The most of a lock file that is read. Its own lines are far shorter; a
/// larger file isn't KassenSturz's.
const MAX_LOCK_FILE_SIZE: u64 = 4096;

/// The most characters of the computer's name that go into the lock file, so
/// the file stays well within [`MAX_LOCK_FILE_SIZE`]. Computer names are far
/// shorter.
const MAX_COMPUTER_NAME: usize = 255;

/// Who has a book open, as its lock file says (D-038).
///
/// The lock file comes from whichever computer wrote it, possibly through a
/// sync tool, so every field is only what the file claims.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lock {
    /// The name of the computer the book is open on. `None` if the lock file
    /// doesn't say, e.g. because it is damaged.
    pub computer: Option<String>,
    /// Since when it is open there. `None` if the lock file doesn't say.
    pub since: Option<Timestamp>,
    /// Identifies the lock, so taking it over can check that it is still the
    /// one the user was warned about.
    token: Option<String>,
}

/// The lock file of a book this program has open. Dropping it removes the
/// file, unless another computer has taken the lock over in the meantime.
pub(crate) struct HeldLock {
    path: PathBuf,
    token: String,
}

impl HeldLock {
    /// Locks the book at `book` for this computer, called `computer`. Fails
    /// with [`StoreError::Locked`] if it's locked already.
    pub(crate) fn acquire(book: &Path, computer: &str) -> Result<Self, StoreError> {
        let path = lock_path(book);
        // A lock that disappears between the two steps was just released, so
        // trying once more is enough.
        for _ in 0..2 {
            match create(&path, computer) {
                Ok(lock) => return Ok(lock),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    if let Some(lock) = read(&path)? {
                        return Err(StoreError::Locked(lock));
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }
        Err(StoreError::Io(ErrorKind::AlreadyExists))
    }

    /// Takes over the lock of the book at `book`, after the user was warned
    /// about `seen` and chose to open the book anyway (D-038). If the lock
    /// file changed in the meantime, it fails with [`StoreError::Locked`]
    /// and the current lock, so the user can be warned again.
    pub(crate) fn take_over(book: &Path, computer: &str, seen: &Lock) -> Result<Self, StoreError> {
        let path = lock_path(book);
        match read(&path)? {
            Some(current) if current == *seen => match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            },
            Some(current) => return Err(StoreError::Locked(current)),
            // Released in the meantime.
            None => {}
        }
        Self::acquire(book, computer)
    }
}

impl Drop for HeldLock {
    fn drop(&mut self) {
        // Errors can't be reported from here; at worst the lock file stays,
        // and the next start warns about it.
        if let Ok(Some(lock)) = read(&self.path)
            && lock.token.as_deref() == Some(self.token.as_str())
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// `Household.ksz.lock` for `Household.ksz`: a plain name that the common
/// sync tools carry along. Nextcloud, for example, leaves out names such as
/// `.~lock.*` and `*.part` (D-038).
pub(crate) fn lock_path(book: &Path) -> PathBuf {
    let mut path = OsString::from(book.as_os_str());
    path.push(".lock");
    PathBuf::from(path)
}

/// Creates the lock file, failing with `AlreadyExists` if there is one.
fn create(path: &Path, computer: &str) -> std::io::Result<HeldLock> {
    let token = uuid::Uuid::now_v7().hyphenated().to_string();
    let now = Timestamp::now();
    // Whole seconds are precise enough for "open since 14:02".
    let since = Timestamp::from_second(now.as_second()).unwrap_or(now);
    let text = format!(
        "computer={}\nsince={since}\ntoken={token}\n",
        one_line(computer)
    );
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let written = file
        .write_all(text.as_bytes())
        .and_then(|()| file.sync_all());
    if let Err(error) = written {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(error);
    }
    Ok(HeldLock {
        path: path.to_owned(),
        token,
    })
}

/// The lock file at `path`, or `None` if there is none.
fn read(path: &Path) -> Result<Option<Lock>, StoreError> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_LOCK_FILE_SIZE).read_to_end(&mut bytes)?;
    Ok(Some(parse(&String::from_utf8_lossy(&bytes))))
}

/// Reads the `key=value` lines of a lock file. Anything it doesn't
/// recognize is left out, so a damaged file still counts as a lock, just
/// with less to say about it.
fn parse(text: &str) -> Lock {
    let mut lock = Lock {
        computer: None,
        since: None,
        token: None,
    };
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "computer" if !value.is_empty() => lock.computer = Some(value.to_owned()),
            "since" => lock.since = value.parse().ok(),
            "token" if !value.is_empty() => lock.token = Some(value.to_owned()),
            _ => {}
        }
    }
    lock
}

/// `name` on one line: control characters such as line breaks become
/// spaces, so the name can't add lines of its own to the lock file. Cut to
/// [`MAX_COMPUTER_NAME`] characters.
fn one_line(name: &str) -> String {
    name.chars()
        .take(MAX_COMPUTER_NAME)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_it_writes() {
        let lock = parse("computer=Laptop\nsince=2026-10-19T11:04:02Z\ntoken=abc\n");
        assert_eq!(lock.computer.as_deref(), Some("Laptop"));
        assert_eq!(
            lock.since,
            Some("2026-10-19T11:04:02Z".parse().expect("valid timestamp"))
        );
        assert_eq!(lock.token.as_deref(), Some("abc"));
    }

    #[test]
    fn a_damaged_lock_file_still_counts_as_a_lock() {
        for text in ["", "garbage", "computer=\nsince=yesterday", "\u{0}\u{1}"] {
            let lock = parse(text);
            assert_eq!(
                (lock.computer, lock.since, lock.token),
                (None, None, None),
                "{text:?}"
            );
        }
        // Unknown lines are left out, known ones still count.
        let lock = parse("version=7\r\ncomputer = Desk PC \r\nnote=a=b\r\n");
        assert_eq!(lock.computer.as_deref(), Some("Desk PC"));
    }

    #[test]
    fn a_computer_name_becomes_one_short_line() {
        assert_eq!(
            one_line("Laptop\nsince=1970-01-01T00:00:00Z"),
            "Laptop since=1970-01-01T00:00:00Z"
        );
        assert_eq!(one_line("\tKitchen\r\n"), "Kitchen");
        let long = "Ö".repeat(5000);
        assert_eq!(one_line(&long).chars().count(), MAX_COMPUTER_NAME);
    }

    #[test]
    fn the_lock_file_sits_next_to_the_book() {
        assert_eq!(
            lock_path(Path::new("/home/sam/Sync/Household.ksz")),
            PathBuf::from("/home/sam/Sync/Household.ksz.lock")
        );
    }
}
