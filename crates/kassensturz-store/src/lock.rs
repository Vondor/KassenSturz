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
use std::sync::{Mutex, PoisonError};

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
    /// What the lock file says about the program that holds it. Boxed, so
    /// the lock stays small enough to travel in error values.
    holder: Box<Holder>,
}

/// The program that holds a lock, as the lock file says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Holder {
    /// The program's number, on the computer named above, as the system
    /// numbers programs, and when it started (Linux's `/proc`), so a lock of
    /// a program that is still running there can be told from one a crash
    /// left behind.
    process: Option<(u32, u64)>,
    /// Which start of the computer and which process-number space the
    /// program lives in (Linux: the boot id and the PID namespace). A program
    /// in another namespace, such as a Flatpak next to a snap, can't be
    /// looked up in `/proc`, so its lock can't be called a crash's leftover
    /// (D-776).
    place: Option<Place>,
    /// The program opened the book without file locks, so SQLite can't keep
    /// a second writer out and this file is the only guard (D-776).
    without_file_locks: bool,
}

impl Lock {
    /// Whether opening the book at `book` anyway is refused on the computer
    /// called `computer`, because the program that holds this lock may still
    /// run there and either side has no file locks, so SQLite couldn't keep
    /// two writers apart (Q-570, D-776). Closing it there, or restarting the
    /// computer, is the way out; the UI says so instead of offering "open
    /// anyway" again.
    pub fn stays_locked_here(&self, book: &Path, computer: &str) -> bool {
        self.computer.as_deref() == Some(computer)
            && may_still_run(self)
            && (self.holder.without_file_locks || crate::lockless::locks_refused(book))
    }
}

/// Where a program runs on its computer: see [`Lock::place`].
#[derive(Clone, Debug, PartialEq, Eq)]
struct Place {
    boot: String,
    namespace: String,
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
        // A lock that disappears between the two steps was just released,
        // and one this computer certainly left behind is cleared (D-781), so
        // a third try is the most it takes.
        for _ in 0..3 {
            match create(&path, computer) {
                Ok(lock) => return Ok(lock),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    if let Some(lock) = read(&path)? {
                        if !left_behind_here(&lock, computer) {
                            return Err(StoreError::Locked(lock));
                        }
                        clear_if_unchanged(&path, &lock)?;
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
        if seen.stays_locked_here(book, computer) {
            return Err(StoreError::Locked(seen.clone()));
        }
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

impl HeldLock {
    /// Whether the lock file is still this lock: there, and with this lock's
    /// token. A book without file locks checks it before every change, since
    /// the file is then its only guard against a second writer (D-776): a
    /// program that doesn't know that, or a sync tool, may have replaced it.
    pub(crate) fn is_still_ours(&self) -> bool {
        matches!(read(&self.path), Ok(Some(lock)) if lock.token.as_deref() == Some(self.token.as_str()))
    }
}

impl HeldLock {
    /// Whether the lock file is gone or names another token. A lock file
    /// that can't be read (the drive dropped) is not a take-over: the write
    /// that follows reports the drive's own error (D-732, B-306).
    pub(crate) fn is_taken_over(&self) -> bool {
        match read(&self.path) {
            Ok(Some(lock)) => lock.token.as_deref() != Some(self.token.as_str()),
            // A folder that is gone (the drive dropped) is not a take-over
            // either.
            Ok(None) => self.path.parent().is_some_and(Path::is_dir),
            Err(_) => false,
        }
    }
}

impl Drop for HeldLock {
    fn drop(&mut self) {
        held().retain(|token| *token != self.token);
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
    let process = own_process()
        .map(|(pid, start)| format!("pid={pid}\nstart={start}\n"))
        .unwrap_or_default();
    let place = own_place()
        .map(|place| format!("boot={}\nnamespace={}\n", place.boot, place.namespace))
        .unwrap_or_default();
    // `path` is the lock file; the book is the name without `.lock`.
    let book = path.with_extension("");
    let locks = if crate::lockless::locks_refused(&book) {
        "locks=none\n"
    } else {
        ""
    };
    let text = format!(
        "computer={}\nsince={since}\ntoken={token}\n{process}{place}{locks}",
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
    held().push(token.clone());
    Ok(HeldLock {
        path: path.to_owned(),
        token,
    })
}

/// The tokens of the locks this program holds, so a lock file this computer
/// left behind when the app stopped can be told apart from the lock of a book
/// open here right now (D-731).
static HELD: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn held() -> std::sync::MutexGuard<'static, Vec<String>> {
    HELD.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Whether `lock` is one this program holds: the book is open here.
pub(crate) fn held_here(lock: &Lock) -> bool {
    lock.token
        .as_ref()
        .is_some_and(|token| held().contains(token))
}

/// The lock file at `path`, or `None` if there is none.
pub(crate) fn read(path: &Path) -> Result<Option<Lock>, StoreError> {
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
        holder: Box::default(),
    };
    let (mut pid, mut start) = (None, None);
    let (mut boot, mut namespace) = (None, None);
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "computer" if !value.is_empty() => lock.computer = Some(value.to_owned()),
            "since" => lock.since = value.parse().ok(),
            "token" if !value.is_empty() => lock.token = Some(value.to_owned()),
            "pid" => pid = value.parse().ok(),
            "start" => start = value.parse().ok(),
            "boot" if !value.is_empty() => boot = Some(value.to_owned()),
            "namespace" if !value.is_empty() => namespace = Some(value.to_owned()),
            "locks" => lock.holder.without_file_locks = value == "none",
            _ => {}
        }
    }
    lock.holder.process = pid.zip(start);
    lock.holder.place = boot
        .zip(namespace)
        .map(|(boot, namespace)| Place { boot, namespace });
    lock
}

/// This program's number and start time, if the system tells (Linux; on
/// Windows the start time is not known and counts as 0).
fn own_process() -> Option<(u32, u64)> {
    let pid = std::process::id();
    // This program certainly runs; on Windows, asking `tasklist` about
    // itself would also depend on this program's file name.
    #[cfg(windows)]
    return Some((pid, 0));
    #[cfg(not(windows))]
    Some((pid, process_start(pid)?))
}

/// When process `pid` started, in clock ticks since boot: field 22 of
/// `/proc/<pid>/stat`, counted after the program's name, which may hold
/// spaces and brackets. Windows has no such number for a program that
/// std can look up, so there a running program counts as started at 0.
#[cfg(not(windows))]
fn process_start(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after_name = &stat[stat.rfind(')')? + 1..];
    after_name.split_whitespace().nth(19)?.parse().ok()
}

/// See above: 0 for a program of KassenSturz that runs, none otherwise.
#[cfg(windows)]
fn process_start(pid: u32) -> Option<u64> {
    windows_process_runs(pid).then_some(0)
}

/// Whether process `pid` runs and is a KassenSturz program (the app, the
/// command-line tool or the MCP server, all named `kassensturz…`), as
/// Windows' own `tasklist` says. Another program that took over the number
/// of one that ended is thereby not mistaken for it. Slow (it starts a
/// program), so it runs only when a lock file of this computer is found.
#[cfg(windows)]
fn windows_process_runs(pid: u32) -> bool {
    use std::os::windows::process::CommandExt;
    const NO_WINDOW: u32 = 0x0800_0000;
    let Ok(output) = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .creation_flags(NO_WINDOW)
        .output()
    else {
        // Can't look: it may run.
        return true;
    };
    let text = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
    // A line is `"image.exe","1234",…`; with no such program there is a note
    // instead, which doesn't start with a quote.
    text.lines()
        .filter_map(|line| line.strip_prefix('"'))
        .filter_map(|line| line.split('"').next())
        .any(|image| image.starts_with("kassensturz"))
}

/// This program's place, if the system tells (Linux; on Windows a fixed
/// place, since the start of the computer can't be told apart there and only
/// whether the program runs decides).
#[cfg(windows)]
fn own_place() -> Option<Place> {
    Some(Place {
        boot: "windows".to_owned(),
        namespace: "windows".to_owned(),
    })
}

/// This program's place, if the system tells (Linux).
#[cfg(not(windows))]
fn own_place() -> Option<Place> {
    let boot = fs::read_to_string("/proc/sys/kernel/random/boot_id").ok()?;
    let namespace = fs::read_link("/proc/self/ns/pid").ok()?;
    Some(Place {
        boot: boot.trim().to_owned(),
        namespace: namespace.to_string_lossy().into_owned(),
    })
}

/// Whether `lock` is one this computer certainly left behind, so the book
/// can be opened without asking (D-781, Q-580). That is so only if
///
/// - it names this computer, by the name the lock file carries, and the
///   program that holds it is not this one;
/// - it says which program wrote it and where that program ran; and
/// - this computer has restarted since (the start of the computer differs),
///   or the program shares this program's process-number space and no longer
///   runs.
///
/// Anything else keeps the page that asks (D-731): another computer's lock,
/// also over a synced folder, where a different start of the computer is
/// simply another machine; a program of another process-number space (a
/// Flatpak next to a snap) that can't be looked up; and a lock without those
/// details. The book itself is safe either way: SQLite takes back a write a
/// crash cut short at the next open.
pub(crate) fn left_behind_here(lock: &Lock, computer: &str) -> bool {
    if lock.computer.as_deref() != Some(computer) || held_here(lock) {
        return false;
    }
    let (Some((pid, start)), Some(theirs), Some(ours)) =
        (lock.holder.process, &lock.holder.place, own_place())
    else {
        return false;
    };
    if theirs.boot != ours.boot {
        true
    } else {
        theirs.namespace == ours.namespace && !process_runs(pid, start)
    }
}

/// Removes the lock file at `path` if it is still `seen`: a lock that was
/// released or replaced in the meantime is left alone.
fn clear_if_unchanged(path: &Path, seen: &Lock) -> Result<(), StoreError> {
    match read(path)? {
        Some(current) if current == *seen => match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        },
        _ => Ok(()),
    }
}

/// Whether the program that wrote `lock` (on this computer) may still run.
/// It is gone for certain if the computer has restarted since, or if it
/// shares this program's process-number space and `/proc` has no such
/// program. In another space (a Flatpak next to a snap) it can't be looked
/// up, so it counts as running. A lock without a number or place is taken
/// to be a crash's leftover, as before.
fn may_still_run(lock: &Lock) -> bool {
    let Some((pid, start)) = lock.holder.process else {
        // A lock without a process number comes from a build that doesn't
        // write one (the Flatpak of before D-776) or is damaged. It can't be
        // looked up, so it counts as running unless the computer has
        // restarted since it was made; "since" says when (D-776).
        return !made_before_boot(lock.since);
    };
    match (&lock.holder.place, own_place()) {
        (Some(theirs), Some(ours)) => {
            if theirs.boot != ours.boot {
                false
            } else if theirs.namespace != ours.namespace {
                true
            } else {
                process_runs(pid, start)
            }
        }
        _ => process_runs(pid, start),
    }
}

/// Whether the computer has restarted since `since`, as far as Linux tells
/// (`/proc/uptime`). A lock without a date, or on a system that doesn't tell,
/// is not "before boot": the safe side.
fn made_before_boot(since: Option<Timestamp>) -> bool {
    let (Some(since), Ok(uptime)) = (since, fs::read_to_string("/proc/uptime")) else {
        return false;
    };
    let Some(seconds) = uptime
        .split_whitespace()
        .next()
        .and_then(|text| text.split('.').next())
        .and_then(|whole| whole.parse::<i64>().ok())
    else {
        return false;
    };
    since.as_second() < Timestamp::now().as_second() - seconds
}

/// Whether process `pid`, which started at `start`, is still running.
pub(crate) fn process_runs(pid: u32, start: u64) -> bool {
    process_start(pid) == Some(start)
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
mod left_behind_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lock_stays_small_enough_to_travel_in_error_values() {
        // Clippy's `result_large_err` allows 128 bytes; a book's problem
        // holds a lock and a path (24 bytes) and a tag.
        eprintln!("size of Lock: {}", std::mem::size_of::<Lock>());
        assert!(std::mem::size_of::<Lock>() <= 88);
        assert!(std::mem::size_of::<crate::StoreError>() <= 112);
    }

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
