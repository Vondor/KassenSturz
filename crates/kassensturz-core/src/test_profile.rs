// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the app's test profile keeps everything (Q-399), and how a program
//! of its own, such as the MCP server an AI tool starts, tells that an app in
//! the test profile is running (M6-B208, Q-466).
//!
//! The app (`kassensturz-ui-bridge/src/test_profile.rs`) moves its folders
//! into the test folder at its start. The MCP server isn't started by the
//! app, so it doesn't get those folders; it finds out from the test folder's
//! mark instead: every app in the test profile holds a shared lock on the
//! file [`MARK`] in the test folder while it runs. The system lets go of
//! that lock when the app ends, also when it crashes, so a lock that can't
//! be taken means a test app runs now.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{self, ErrorKind};
use std::path::{Component, Path, PathBuf};

/// The test folder's name, beside the real data folder, never inside it.
pub const FOLDER_NAME: &str = "kassensturz-test";

/// The file in the test folder that a running test app holds a lock on.
pub const MARK: &str = "app-running";

/// The test folder for a user whose data folder is `data_home` (the real
/// `XDG_DATA_HOME`, or `HOME/.local/share`, or on Windows `LOCALAPPDATA`).
pub fn root_in(data_home: &Path) -> PathBuf {
    data_home.join(FOLDER_NAME)
}

/// The variables the profile sets, and their values, for `root`.
pub fn variables(root: &Path) -> Vec<(&'static str, PathBuf)> {
    let mut variables = vec![
        ("XDG_DATA_HOME", root.join("data")),
        ("XDG_CONFIG_HOME", root.join("config")),
        ("XDG_CACHE_HOME", root.join("cache")),
        ("XDG_STATE_HOME", root.join("state")),
    ];
    if cfg!(windows) {
        variables.push(("APPDATA", root.join("config")));
        variables.push(("LOCALAPPDATA", root.join("data")));
    }
    variables
}

/// The real data folder, as the environment names it before the profile
/// moves it.
pub fn real_data_home() -> Option<PathBuf> {
    let nonempty = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
    if cfg!(windows) {
        return nonempty("LOCALAPPDATA").map(PathBuf::from);
    }
    nonempty("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| nonempty("HOME").map(|home| PathBuf::from(home).join(".local/share")))
}

/// Whether the profile with the test folder `root` allows a book or a
/// book's source file at `path`: always outside the test profile (`None`),
/// and inside it only within the test folder.
pub fn allows_in(root: Option<&Path>, path: &Path) -> bool {
    let Some(root) = root else {
        return true;
    };
    // Compared without following links, which may not exist yet; a path
    // with `..` in it is refused rather than worked out.
    !path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
        && path.starts_with(root)
}

/// Marks the test folder `root` as in use by a running test app, for as long
/// as the returned file stays open. Several test apps may hold it at once.
pub fn hold_mark(root: &Path) -> io::Result<File> {
    // Read and write access, not append only: Windows locks only a file
    // opened for reading or writing.
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(MARK))?;
    file.lock_shared()?;
    Ok(file)
}

/// Whether an app in the test profile with the test folder `root` runs now.
/// An error means it can't be told, which a caller treats as running, so
/// the real book is never read in its place.
pub fn app_running(root: &Path) -> io::Result<bool> {
    let file = match File::open(root.join(MARK)) {
        Ok(file) => file,
        // No test app ever ran here.
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    match file.try_lock() {
        // Nobody holds it; closing the file lets go of it at once.
        Ok(()) => Ok(false),
        Err(TryLockError::WouldBlock) => Ok(true),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "kassensturz-core-test-profile-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("folder");
        path
    }

    #[test]
    fn a_held_mark_says_a_test_app_runs_and_a_dropped_one_does_not() {
        let root = folder("mark");
        assert!(!app_running(&root).expect("told"), "no app ran yet");
        let first = hold_mark(&root).expect("held");
        let second = hold_mark(&root).expect("a second app holds it too");
        assert!(app_running(&root).expect("told"));
        drop(first);
        assert!(app_running(&root).expect("told"), "one app still runs");
        drop(second);
        // The file stays, but nobody holds it: no test app runs.
        assert!(root.join(MARK).exists());
        assert!(!app_running(&root).expect("told"));
        // Asking doesn't keep it: an app that starts next holds it again.
        let again = hold_mark(&root).expect("held again");
        assert!(app_running(&root).expect("told"));
        drop(again);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn only_books_inside_the_test_folder_are_allowed_in_it() {
        let root = Path::new("/data/kassensturz-test");
        assert!(allows_in(Some(root), &root.join("data/Example.ksz")));
        assert!(!allows_in(Some(root), Path::new("/data/Household.ksz")));
        assert!(!allows_in(Some(root), &root.join("../Household.ksz")));
        assert!(!allows_in(
            Some(root),
            Path::new("/data/kassensturz-test-other/Book.ksz")
        ));
        assert!(allows_in(None, Path::new("/anywhere/Household.ksz")));
    }
}
