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
#[cfg(not(windows))]
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
    /// The lock of a lock file that is there but can't be read: all that is
    /// known is that the book is in use (B-315).
    fn unreadable() -> Self {
        Self {
            computer: None,
            since: None,
            token: None,
            holder: Box::default(),
        }
    }

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
                    match read(&path) {
                        Ok(Some(lock)) => {
                            if !left_behind_here(&lock, computer) {
                                return Err(StoreError::Locked(lock));
                            }
                            clear_if_unchanged(&path, &lock)?;
                        }
                        Ok(None) => {}
                        // Something is there that can't be read, e.g. a share
                        // that keeps the file shut while the other computer
                        // has it: still a lock, so the book isn't opened and
                        // the user is told it is in use (B-315).
                        Err(StoreError::Io(_)) => {
                            return Err(StoreError::Locked(Lock::unreadable()));
                        }
                        Err(other) => return Err(other),
                    }
                }
                // Windows reports a file the other side holds, or is deleting,
                // as "access denied" instead of "exists" (B-315).
                Err(error) if error.kind() == ErrorKind::PermissionDenied && path.exists() => {
                    return Err(StoreError::Locked(
                        read(&path).ok().flatten().unwrap_or_else(Lock::unreadable),
                    ));
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
    /// Whether the lock file is gone or names another token. A lock file
    /// that can't be read (the drive dropped) is not a take-over: the write
    /// that follows reports the drive's own error (D-732, B-306).
    pub(crate) fn is_taken_over(&self) -> bool {
        match read(&self.path) {
            Ok(Some(lock)) => lock.token.as_deref() != Some(self.token.as_str()),
            // A lock file that is gone is a take-over only while the book is
            // still there: with a drive pulled out, the book goes too, even
            // if an empty folder is left where it was mounted (B-316).
            // Checked by reading from the book, not by looking it up: a
            // portal or a share can keep answering for a drive that has been
            // pulled out (B-316).
            Ok(None) => book_can_be_read(&self.path.with_extension("")),
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

/// Whether the book at `book` can be read: its folder lists it, it opens and
/// a byte comes back.
fn book_can_be_read(book: &Path) -> bool {
    // The folder's listing must still name the book too: a read can be
    // answered from what the system kept, a listing is asked of the drive.
    let listed = book
        .parent()
        .zip(book.file_name())
        .is_some_and(|(folder, name)| {
            fs::read_dir(folder).is_ok_and(|mut entries| {
                entries.any(|entry| entry.is_ok_and(|e| e.file_name() == name))
            })
        });
    listed
        && fs::File::open(book)
            .and_then(|mut file| file.read(&mut [0; 1]))
            .is_ok()
}

/// Whether the book is held shut by another program, as the system says
/// when it is opened for reading: a sharing or lock violation on Windows,
/// "busy" elsewhere. A share can answer so while the lock file is not yet
/// visible here, e.g. a directory listing the client still has cached
/// (B-315).
fn book_is_held_shut(book: &Path) -> bool {
    let Err(error) = fs::File::open(book).and_then(|mut file| file.read(&mut [0; 1])) else {
        return false;
    };
    matches!(
        error.kind(),
        ErrorKind::ResourceBusy | ErrorKind::WouldBlock
    ) || (cfg!(windows) && matches!(error.raw_os_error(), Some(32 | 33)))
}

/// What to say when the book can't be opened with `error` after the lock
/// was taken or while the lock file may not be visible: if SQLite says the
/// book is busy or locked, or the book is held shut, another program has it
/// open, so the answer is the in-use page and not a general error (B-315).
pub(crate) fn in_use_instead(book: &Path, error: StoreError) -> StoreError {
    let busy = match &error {
        StoreError::Database(database) => matches!(
            database.0.sqlite_error_code(),
            Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)
        ),
        StoreError::Io(_) => book_is_held_shut(book),
        _ => false,
    };
    if busy {
        StoreError::Locked(Lock::unreadable())
    } else {
        error
    }
}

/// Turns an I/O error met before the book's lock is taken into the lock
/// page if the book has a lock file: a share can refuse to read the book
/// while another computer has it open, and "in use there" is the useful
/// answer, not "could not be read" (B-315). Any other error stays as it is.
pub(crate) fn locked_instead(book: &Path, error: StoreError) -> StoreError {
    if !matches!(error, StoreError::Io(_)) {
        return error;
    }
    let path = lock_path(book);
    match read(&path) {
        Ok(Some(lock)) if !held_here(&lock) => StoreError::Locked(lock),
        Err(StoreError::Io(_)) if path.exists() => StoreError::Locked(Lock::unreadable()),
        _ => in_use_instead(book, error),
    }
}

/// The lock file at `path`, or `None` if there is none.
pub(crate) fn read(path: &Path) -> Result<Option<Lock>, StoreError> {
    let mut file = match open_fresh(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut bytes = Vec::new();
    #[cfg(windows)]
    {
        // An unbuffered read needs a buffer aligned to the disk's sectors;
        // 4096 bytes is the most of a lock file that is read, as above.
        #[repr(align(4096))]
        struct Aligned([u8; 4096]);
        let mut buffer = Aligned([0; 4096]);
        let got = file.read(&mut buffer.0)?;
        bytes.extend_from_slice(&buffer.0[..got]);
    }
    #[cfg(not(windows))]
    {
        Read::by_ref(&mut file)
            .take(MAX_LOCK_FILE_SIZE)
            .read_to_end(&mut bytes)?;
    }
    Ok(Some(parse(&String::from_utf8_lossy(&bytes))))
}

/// Opens the lock file for reading so that what is read is what the other
/// computer wrote. Windows' network client keeps what it read of a file and
/// may hand it back although another computer replaced the file (B-306):
/// an unbuffered open goes to the server each time. If the folder doesn't
/// allow that, the ordinary open is used.
fn open_fresh(path: &Path) -> std::io::Result<fs::File> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_NO_BUFFERING: u32 = 0x2000_0000;
        match OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_NO_BUFFERING)
            .open(path)
        {
            Err(error) if error.kind() == ErrorKind::NotFound => return Err(error),
            Ok(file) => return Ok(file),
            Err(_) => {}
        }
    }
    fs::File::open(path)
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
