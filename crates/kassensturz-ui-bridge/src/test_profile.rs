// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The test profile (M7a.9, `docs/SETUP_WIZARD.md` › "A test never touches
//! the real setup", Q-399): a test never touches the setup of whoever runs
//! it. Started with `KASSENSTURZ_TEST=1` or with the fake bank
//! (`KASSENSTURZ_FAKE_BANK`), the app keeps every book, backup, lock file,
//! setting, recent-books entry and log in a test folder of its own, and never
//! opens or reads a book outside it.
//!
//! It works by moving the folders before anything reads them, as the first
//! statement of `main`, before Qt starts and before any thread exists. The
//! XDG base folders are moved, which Qt on Linux and in the Flatpak follows,
//! and so does the Rust code that reads them itself
//! (`ai_switch::settings_file`, the debug log's folder, the MCP server's
//! `LastBook` lookup). On Windows `APPDATA` and `LOCALAPPDATA` are moved as
//! well. Qt on Windows doesn't read those, so there `main` also switches on
//! Qt's own test locations (`QStandardPaths::setTestModeEnabled`), and the
//! two paths the app itself keeps, its data folder and its settings file,
//! come from here on every system ([`data_folder`], [`settings_file`]).
//!
//! `path_audit` below checks every path function in a child process with a
//! made-up home, and scans the sources for new reads of these variables.

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use crate::bank_connection::FAKE_BANK_VARIABLE;

/// The variable that starts the test profile without the fake bank.
pub const TEST_VARIABLE: &str = "KASSENSTURZ_TEST";

/// The test folder's name, beside the real data folder, never inside it.
pub const FOLDER_NAME: &str = "kassensturz-test";

/// The settings file's name, as KConfig names the app's own (`Settings::user`).
const SETTINGS_FILE: &str = "kassensturzrc";

/// The test folder, once [`apply_at_start`] ran: `None` outside the profile.
static ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Whether `test` and `fake_bank` (the two variables' values) start the
/// test profile: `KASSENSTURZ_TEST=1`, or the fake bank with any value.
/// `KASSENSTURZ_DEBUG_LOG` alone doesn't, since it is also used to look
/// into a problem with a real book.
pub fn wanted(test: Option<&OsString>, fake_bank: Option<&OsString>) -> bool {
    test.is_some_and(|value| value == "1") || fake_bank.is_some_and(|value| !value.is_empty())
}

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
fn real_data_home() -> Option<PathBuf> {
    let nonempty = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
    if cfg!(windows) {
        return nonempty("LOCALAPPDATA").map(PathBuf::from);
    }
    nonempty("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| nonempty("HOME").map(|home| PathBuf::from(home).join(".local/share")))
}

/// Starts the test profile if it is wanted. Runs first in `main`, before Qt
/// starts and before any thread exists. Returns whether it is on.
pub fn apply_at_start() -> bool {
    let wanted = wanted(
        std::env::var_os(TEST_VARIABLE).as_ref(),
        std::env::var_os(FAKE_BANK_VARIABLE).as_ref(),
    );
    let root = wanted
        .then(real_data_home)
        .flatten()
        .map(|home| root_in(&home));
    if let Some(root) = &root {
        for (name, value) in variables(root) {
            // A folder that can't be made shows up as an error where it is
            // used; the profile still never falls back to the real one.
            let _ = std::fs::create_dir_all(&value);
            // SAFETY: nothing but `main`'s first statements runs yet, so no
            // other thread reads or writes the environment.
            unsafe { std::env::set_var(name, value) };
        }
    }
    let on = root.is_some();
    let _ = ROOT.set(root);
    on
}

/// The test folder, if the app runs in the test profile.
pub fn root() -> Option<&'static Path> {
    ROOT.get().and_then(Option::as_deref)
}

/// Whether the app runs in the test profile.
pub fn on() -> bool {
    root().is_some()
}

/// The folder for apps' data in the test profile, which `platform::app_folder`
/// uses in place of Qt's.
pub fn data_folder() -> Option<PathBuf> {
    root().map(|root| root.join("data"))
}

/// The app's settings file in the test profile, which `Settings::user` uses
/// in place of KConfig's own lookup.
pub fn settings_file() -> Option<PathBuf> {
    root().map(|root| root.join("config").join(SETTINGS_FILE))
}

/// Whether the profile allows a book or a book's source file at `path`:
/// always outside the test profile, and inside it only within the test
/// folder.
pub fn allows(path: &Path) -> bool {
    allows_in(root(), path)
}

/// [`allows`] for a given test folder.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn os(value: &str) -> OsString {
        OsString::from(value)
    }

    #[test]
    fn the_test_variable_or_the_fake_bank_start_it_and_the_debug_log_alone_does_not() {
        assert!(wanted(Some(&os("1")), None));
        assert!(wanted(None, Some(&os("1"))));
        assert!(wanted(None, Some(&os("keychain"))));
        assert!(!wanted(None, None));
        assert!(!wanted(Some(&os("0")), None));
        assert!(!wanted(Some(&os("")), Some(&os(""))));
    }

    #[test]
    fn every_moved_folder_is_inside_the_test_folder_beside_the_real_data() {
        let home = Path::new("/home/someone/.local/share");
        let root = root_in(home);
        assert_eq!(
            root,
            Path::new("/home/someone/.local/share/kassensturz-test")
        );
        // Beside the app's real data folder, never inside it.
        assert!(!root.starts_with(home.join(crate::platform::APP_ID)));
        let variables = variables(&root);
        for name in [
            "XDG_DATA_HOME",
            "XDG_CONFIG_HOME",
            "XDG_CACHE_HOME",
            "XDG_STATE_HOME",
        ] {
            let (_, value) = variables
                .iter()
                .find(|(variable, _)| *variable == name)
                .unwrap_or_else(|| panic!("{name} is moved"));
            assert!(value.starts_with(&root), "{name}: {}", value.display());
        }
        assert_eq!(variables.len(), if cfg!(windows) { 6 } else { 4 });
        assert!(variables.iter().all(|(_, value)| value.starts_with(&root)));
    }

    #[test]
    fn only_books_inside_the_test_folder_are_allowed_in_it() {
        let root = Path::new("/data/kassensturz-test");
        let inside = root.join("data/io.github.vondor.KassenSturz/Example – Space Colony.ksz");
        assert!(allows_in(Some(root), &inside));
        assert!(!allows_in(
            Some(root),
            Path::new("/data/io.github.vondor.KassenSturz/Household.ksz")
        ));
        assert!(!allows_in(
            Some(root),
            &root.join("../io.github.vondor.KassenSturz/Household.ksz")
        ));
        // A folder whose name only starts like the test folder's.
        assert!(!allows_in(
            Some(root),
            Path::new("/data/kassensturz-test-other/Book.ksz")
        ));
        // Outside the profile every book is the user's choice.
        assert!(allows_in(None, Path::new("/anywhere/Household.ksz")));
    }
}

/// The path audit (`docs/SETUP_WIZARD.md`): every path the app uses resolves
/// inside the test folder, and a made-up real setup is never read or changed.
/// The check runs in a child process of the test binary, so it can set the
/// environment as `main` does without touching the other tests.
#[cfg(test)]
mod path_audit {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use crate::books::Books;
    use crate::settings::Settings;
    use crate::test_folder::TestFolder;

    /// Set in the child only: the made-up home the child checks against.
    const CHILD: &str = "KASSENSTURZ_PATH_AUDIT_HOME";

    /// Where a user's real data and settings are, below `home`.
    fn real_data(home: &Path) -> PathBuf {
        if cfg!(windows) {
            home.join("AppData/Local")
        } else {
            home.join(".local/share")
        }
    }

    fn real_config(home: &Path) -> PathBuf {
        if cfg!(windows) {
            home.join("AppData/Roaming")
        } else {
            home.join(".config")
        }
    }

    fn real_book(home: &Path) -> PathBuf {
        real_data(home)
            .join(crate::platform::APP_ID)
            .join("Haushaltsbuch.ksz")
    }

    #[test]
    fn every_path_stays_in_the_test_folder() {
        if std::env::var_os(CHILD).is_some() {
            return;
        }
        let home = TestFolder::new();
        // A made-up real setup: the app's own book, and settings that name
        // it as the book opened last.
        let book = real_book(home.path());
        std::fs::create_dir_all(book.parent().expect("book's folder")).expect("folder made");
        let real = Books::in_folder(&home.join("scratch"), "real")
            .create(&book)
            .expect("made-up real book made");
        drop(real);
        let settings = real_config(home.path()).join("kassensturzrc");
        std::fs::create_dir_all(settings.parent().expect("settings' folder")).expect("folder made");
        assert!(Settings::at(&settings).set_last_book(&book));
        let book_before = std::fs::read(&book).expect("book read");
        let settings_before = std::fs::read(&settings).expect("settings read");

        let mut child = Command::new(std::env::current_exe().expect("test binary"));
        child
            .args([
                "--exact",
                "test_profile::path_audit::child",
                "--test-threads=1",
                "--nocapture",
            ])
            .env(CHILD, home.path())
            .env("HOME", home.path())
            .env("APPDATA", real_config(home.path()))
            .env("LOCALAPPDATA", real_data(home.path()))
            .env(super::TEST_VARIABLE, "1")
            .env_remove("XDG_DATA_HOME")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("XDG_CACHE_HOME")
            .env_remove("XDG_STATE_HOME")
            .env_remove(crate::bank_connection::FAKE_BANK_VARIABLE);
        let output = child.output().expect("child ran");
        let printed = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && printed.contains("1 passed"),
            "{printed}{}",
            String::from_utf8_lossy(&output.stderr)
        );

        // The real setup is exactly as it was, and its book got no lock file.
        assert_eq!(std::fs::read(&book).expect("book read"), book_before);
        assert_eq!(
            std::fs::read(&settings).expect("settings read"),
            settings_before
        );
        let mut lock = book.into_os_string();
        lock.push(".lock");
        assert!(!Path::new(&lock).exists());
    }

    /// The child's half: runs only under [`every_path_stays_in_the_test_folder`].
    #[test]
    fn child() {
        let Some(home) = std::env::var_os(CHILD).map(PathBuf::from) else {
            return;
        };
        assert!(super::apply_at_start(), "the test profile is on");
        let root = super::root().expect("test folder").to_owned();
        assert_eq!(root, real_data(&home).join(super::FOLDER_NAME));
        let inside = |what: &str, path: Option<PathBuf>| {
            if let Some(path) = path {
                assert!(path.starts_with(&root), "{what}: {}", path.display());
            }
        };
        let app_folder = crate::platform::app_folder();
        assert!(app_folder.is_some());
        inside("app folder", app_folder);
        inside("backups", Some(crate::platform::computer().backup_folder));
        inside("settings file", super::settings_file());
        inside(
            "AI switch's settings file",
            kassensturz_core::ai_switch::settings_file(),
        );
        inside(
            "debug log",
            kassensturz_core::debug_log::DebugLog::user()
                .folder()
                .map(Path::to_owned),
        );
        #[cfg(feature = "ai")]
        inside(
            "MCP request folder",
            kassensturz_ai_tools::requests::RequestFolder::default_place(),
        );
        if cfg!(target_os = "linux") {
            inside("Qt's data folder", Some(crate::platform::qt_data_folder()));
        }
        // The real settings name a real book; the test profile reads its own
        // settings, opens its own book and never the real one.
        let books = Books::for_user();
        let opened = books.open_last("Haushaltsbuch").expect("own book opens");
        assert!(opened.path.starts_with(&root), "{}", opened.path.display());
        drop(opened);
        assert_eq!(
            books
                .open(&real_book(&home))
                .err()
                .expect("refused")
                .text()
                .code,
            "book.outside-test"
        );
        // The settings the profile writes stay in the test folder too.
        assert!(Settings::user().set_last_book(&root.join("data/Some.ksz")));
        assert!(super::settings_file().expect("settings file").exists());
    }

    /// Every read of a variable that names a user's folder, by file, with
    /// how many there are. A new one fails here until it is checked against
    /// the test profile and added: a later path can't slip past it.
    const READS: &[(&str, usize)] = &[
        // Follows XDG_CONFIG_HOME, checked above.
        ("kassensturz-core/src/ai_switch.rs", 2),
        // Follows XDG_DATA_HOME, checked above.
        ("kassensturz-core/src/debug_log.rs", 2),
        // Follows XDG_DATA_HOME, checked above.
        ("kassensturz-ai-tools/src/requests.rs", 2),
        // The MCP server's `$HOME` in a `LastBook` it reads from the
        // settings file `ai_switch::settings_file` finds; the app never
        // starts the server in the test profile.
        ("kassensturz-mcp/src/book_path.rs", 1),
        ("kassensturz-mcp/tests/refuses_to_start.rs", 2),
        ("kassensturz-mcp/tests/common/mod.rs", 3),
        // The command-line tool, a program of its own outside the app.
        ("kassensturz-cli/src/lib.rs", 4),
        ("kassensturz-cli/tests/commands.rs", 2),
        // KeePassXC's socket in XDG_RUNTIME_DIR, not a place for data.
        ("kassensturz-platform/src/keepassxc.rs", 1),
        // A test of the settings file's `$HOME` paths.
        ("kassensturz-ui-bridge/src/settings.rs", 1),
        // The test profile itself.
        ("kassensturz-ui-bridge/src/test_profile.rs", 23),
    ];

    /// The variables that name a user's folders, as string literals.
    fn reads_in(text: &str) -> usize {
        let mut count = 0;
        let mut rest = text;
        while let Some(start) = rest.find('"') {
            let after = &rest[start + 1..];
            let Some(end) = after.find('"') else { break };
            let word = &after[..end];
            let named = matches!(word, "HOME" | "APPDATA" | "LOCALAPPDATA")
                || (word.starts_with("XDG_")
                    && word.len() > 4
                    && word[4..]
                        .bytes()
                        .all(|byte| byte.is_ascii_uppercase() || byte == b'_'));
            count += usize::from(named);
            rest = &after[end + 1..];
        }
        count
    }

    fn rust_files(folder: &Path, found: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(folder).expect("folder listed").flatten() {
            let path = entry.path();
            if path.is_dir() {
                rust_files(&path, found);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                found.push(path);
            }
        }
    }

    #[test]
    fn no_new_code_reads_a_users_folders_past_the_test_profile() {
        let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("crates folder")
            .to_owned();
        let mut files = Vec::new();
        rust_files(&crates, &mut files);
        let mut found = Vec::new();
        for file in files {
            let text = std::fs::read_to_string(&file).expect("source read");
            let count = reads_in(&text);
            if count > 0 {
                let name = file
                    .strip_prefix(&crates)
                    .expect("inside crates")
                    .to_string_lossy()
                    .replace('\\', "/");
                found.push((name, count));
            }
        }
        found.sort();
        let mut expected: Vec<(String, usize)> = READS
            .iter()
            .map(|(name, count)| ((*name).to_owned(), *count))
            .collect();
        expected.sort();
        assert_eq!(found, expected);
    }
}
