// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The logins whose saved PIN must not be sent on its own (B-413, threat
//! T-05): the bank refused it and deleting it from where PINs are saved
//! failed (a password manager that was locked, closed or said no), or it was
//! typed and saved and the connection broke before the bank accepted it.
//! The marks live in a small file in KassenSturz's own AqBanking folder, so
//! they outlast the app: without them a new start would send the saved PIN
//! again, and each start would cost one more wrong-PIN attempt at the bank,
//! which blocks online access after a few.
//!
//! The file holds login numbers only, one per line, never a PIN. It is
//! readable only by the user and replaced whole on every change, so it is
//! never half written. Neither the book nor the app's settings change.

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::backend::LoginId;

/// The file's name in the AqBanking folder.
const FILE: &str = "kassensturz-pin-marks";

/// The most marks kept: far more logins than anyone sets up. A file that
/// another program made longer is read as marking every login.
const MAX_MARKS: usize = 1000;

fn path(folder: &Path) -> PathBuf {
    folder.join(FILE)
}

/// What the file says: the marked logins, or `None` when it can't be read
/// or isn't what KassenSturz writes, which counts as every login marked, so
/// no saved PIN goes out on a guess.
fn read(folder: &Path) -> Option<BTreeSet<String>> {
    let text = match fs::read_to_string(path(folder)) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Some(BTreeSet::new()),
        Err(_) => return None,
    };
    let marks: BTreeSet<String> = text.lines().map(str::to_owned).collect();
    let plain = |line: &String| !line.is_empty() && line.bytes().all(|b| b.is_ascii_digit());
    (marks.len() <= MAX_MARKS && marks.iter().all(plain)).then_some(marks)
}

/// Whether `login`'s saved PIN is marked.
pub(crate) fn is_marked(folder: &Path, login: &LoginId) -> bool {
    read(folder).is_none_or(|marks| marks.contains(login.as_str()))
}

/// Marks `login`'s saved PIN. Only AqBanking's login numbers are marked;
/// any other login (one still being set up) has none to mark.
pub(crate) fn mark(folder: &Path, login: &LoginId) -> io::Result<()> {
    if !login.as_str().bytes().all(|b| b.is_ascii_digit()) || login.as_str().is_empty() {
        return Ok(());
    }
    // A file that can't be read already marks every login, and is left as
    // it is so no other login's mark is lost.
    let Some(mut marks) = read(folder) else {
        return Ok(());
    };
    if marks.insert(login.as_str().to_owned()) {
        write(folder, &marks)?;
    }
    Ok(())
}

/// Takes the mark off `login`'s saved PIN.
pub(crate) fn unmark(folder: &Path, login: &LoginId) -> io::Result<()> {
    // A file that can't be read is left as it is: which logins it marked
    // can't be told, so every login stays marked (the user types the PIN).
    let Some(mut marks) = read(folder) else {
        return Ok(());
    };
    if marks.remove(login.as_str()) {
        write(folder, &marks)?;
    }
    Ok(())
}

/// Replaces the file with `marks`, through a file beside it, readable only
/// by the user; no file at all when nothing is marked.
fn write(folder: &Path, marks: &BTreeSet<String>) -> io::Result<()> {
    let target = path(folder);
    if marks.is_empty() {
        return match fs::remove_file(&target) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        };
    }
    let partial = folder.join(format!("{FILE}.part"));
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let written = options.open(&partial).and_then(|mut file| {
        for mark in marks {
            writeln!(file, "{mark}")?;
        }
        file.sync_all()
    });
    match written.and_then(|()| fs::rename(&partial, &target)) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&partial);
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "kassensturz-bank-pin-marks-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).expect("folder made");
        folder
    }

    #[test]
    fn a_mark_outlasts_the_run_and_comes_off_again() {
        let folder = folder("marks");
        let (first, second) = (LoginId::new("3"), LoginId::new("17"));
        assert!(!is_marked(&folder, &first));
        mark(&folder, &first).expect("marked");
        mark(&folder, &second).expect("marked");
        mark(&folder, &first).expect("marked again");
        assert!(is_marked(&folder, &first));
        assert!(is_marked(&folder, &second));
        assert_eq!(
            fs::read_to_string(path(&folder)).expect("read"),
            "17\n3\n",
            "login numbers only"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(path(&folder))
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        unmark(&folder, &first).expect("unmarked");
        assert!(!is_marked(&folder, &first));
        assert!(is_marked(&folder, &second));
        unmark(&folder, &second).expect("unmarked");
        assert!(!path(&folder).exists(), "no file when nothing is marked");
        assert!(!folder.join(format!("{FILE}.part")).exists());
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_login_being_set_up_has_no_mark() {
        let folder = folder("pending");
        mark(&folder, &LoginId::pending()).expect("nothing to do");
        assert!(!path(&folder).exists());
        let _ = fs::remove_dir_all(&folder);
    }

    /// A file another program changed, or one that can't be read, marks
    /// every login: no saved PIN goes out on a guess.
    #[test]
    fn a_file_not_as_written_marks_every_login() {
        let folder = folder("strange");
        fs::write(path(&folder), "3\nsend it anyway\n").expect("written");
        assert!(is_marked(&folder, &LoginId::new("5")));
        fs::remove_file(path(&folder)).expect("removed");
        fs::create_dir(path(&folder)).expect("a folder in its place");
        assert!(is_marked(&folder, &LoginId::new("5")));
        let _ = fs::remove_dir_all(&folder);
    }
}
