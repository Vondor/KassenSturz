// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Whether the server reads the user's real setup or the app's test profile
//! (M6-B208, Q-466).
//!
//! While an app in the test profile runs (`KASSENSTURZ_TEST=1`, Q-399), the
//! server reads that profile and never the real one: its AI switch, its
//! permissions, its book and its request folder. The AI tool starts this
//! program, not the app, so it doesn't get the app's moved folders; it finds
//! the running test app by the test folder's mark
//! (`kassensturz_core::test_profile`) and then starts itself again with the
//! folders moved as the app moves them, before anything is read.
//!
//! Which profile it reads is fixed for as long as it runs. When a test app
//! starts or ends while it runs, every call is refused until the AI tool is
//! restarted: it never falls back to the real book during a test, and never
//! keeps reading the test folder after one.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use kassensturz_core::test_profile;

/// Set only when the program starts itself again in the test profile: the
/// test folder.
const TEST_ROOT: &str = "KASSENSTURZ_MCP_TEST_ROOT";

/// The refusal of a call when a test app started after the server, which
/// reads the real setup.
pub(crate) const TEST_APP_STARTED: &str = "mcp.test-app-started";

/// The refusal of a call when the test app the server reads ended.
pub(crate) const TEST_APP_ENDED: &str = "mcp.test-app-ended";

/// The refusal of a call when it can't be told whether a test app runs.
pub(crate) const TEST_APP_UNKNOWN: &str = "mcp.test-app-unknown";

/// Which setup the server reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Profile {
    /// The user's real setup, while no test app runs. `test_root` is where a
    /// test app would mark that it runs; `None` where no data folder is known,
    /// so no test app can run either.
    Real { test_root: Option<PathBuf> },
    /// The test profile with this test folder, while a test app runs.
    Test { root: PathBuf },
}

/// What the start found.
pub(crate) enum Start {
    /// Read this profile.
    Read(Profile),
    /// A test app runs: start again with these variables set.
    Again(Vec<(&'static str, PathBuf)>),
    /// It can't be told whether a test app runs: refuse.
    Unknown(String),
}

/// Which profile to read, at the start, before anything reads the settings.
pub(crate) fn at_start() -> Start {
    if let Some(root) = std::env::var_os(TEST_ROOT).filter(|root| !root.is_empty()) {
        let root = PathBuf::from(root);
        return match started_again_in(&root, |name| std::env::var_os(name)) {
            Ok(()) => Start::Read(Profile::Test { root }),
            Err(message) => Start::Unknown(message),
        };
    }
    let Some(root) = test_profile::real_data_home().map(|home| test_profile::root_in(&home)) else {
        return Start::Read(Profile::Real { test_root: None });
    };
    match test_profile::app_running(&root) {
        Ok(false) => Start::Read(Profile::Real {
            test_root: Some(root),
        }),
        Ok(true) => {
            let mut variables = test_profile::variables(&root);
            variables.push((TEST_ROOT, root));
            Start::Again(variables)
        }
        Err(error) => Start::Unknown(format!(
            "it can't be told whether KassenSturz runs in its test profile ({error}), so no \
             book is read. Restart this AI tool."
        )),
    }
}

/// Checks the start again in the test folder `root`, with `variable` reading
/// the environment: `root` is a test folder, not `/` or a relative path, and
/// every folder the profile moves is the one under `root`, so nothing can be
/// read from a real folder in its name.
fn started_again_in(
    root: &Path,
    variable: impl Fn(&str) -> Option<OsString>,
) -> Result<(), String> {
    let refused = || {
        format!(
            "{TEST_ROOT} doesn't name a test folder with the test profile's folders, so no book \
             is read. Restart this AI tool."
        )
    };
    if !root.is_absolute() || root.parent().is_none() || root.file_name().is_none() {
        return Err(refused());
    }
    for (name, value) in test_profile::variables(root) {
        if variable(name).map(PathBuf::from) != Some(value) {
            return Err(refused());
        }
    }
    Ok(())
}

/// Starts this program again with `variables` set and the same arguments.
/// Returns only if that failed, with the message for the user.
#[cfg(unix)]
pub(crate) fn start_again(variables: Vec<(&'static str, PathBuf)>) -> String {
    use std::os::unix::process::CommandExt;

    let program = match std::env::current_exe() {
        Ok(program) => program,
        Err(error) => return format!("KassenSturz runs in its test profile, but {error}"),
    };
    let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    let error = std::process::Command::new(program)
        .args(arguments)
        .envs(variables)
        .exec();
    format!("KassenSturz runs in its test profile, but this program couldn't follow it: {error}")
}

/// Elsewhere the program never gets this far (D-177).
#[cfg(not(unix))]
pub(crate) fn start_again(_variables: Vec<(&'static str, PathBuf)>) -> String {
    "KassenSturz runs in its test profile, which this AI tool can't read here.".to_owned()
}

impl Profile {
    /// Runs `read` and returns its answer, or the refusal when no call may
    /// be answered, checked both before and after `read`: a test app that
    /// starts or ends while it reads drops the answer (M6-B208).
    pub(crate) fn guarded<T>(&self, read: impl FnOnce() -> T) -> Result<T, &'static str> {
        if let Some(code) = self.refusal() {
            return Err(code);
        }
        let answer = read();
        match self.refusal() {
            Some(code) => Err(code),
            None => Ok(answer),
        }
    }

    /// The test folder, in the test profile.
    pub(crate) fn test_root(&self) -> Option<&Path> {
        match self {
            Self::Real { .. } => None,
            Self::Test { root } => Some(root),
        }
    }

    /// Why no call may be answered now, as a code: a test app started or
    /// ended since the start, or it can't be told. `None` while the profile
    /// read is still the right one.
    pub(crate) fn refusal(&self) -> Option<&'static str> {
        match self {
            Self::Real { test_root: None } => None,
            Self::Real {
                test_root: Some(root),
            } => match test_profile::app_running(root) {
                Ok(false) => None,
                Ok(true) => Some(TEST_APP_STARTED),
                Err(_) => Some(TEST_APP_UNKNOWN),
            },
            Self::Test { root } => match test_profile::app_running(root) {
                Ok(true) => None,
                Ok(false) => Some(TEST_APP_ENDED),
                Err(_) => Some(TEST_APP_UNKNOWN),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "kassensturz-mcp-profile-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("folder");
        path
    }

    #[test]
    fn an_answer_read_while_a_test_app_started_or_ended_is_dropped() {
        let root = folder("race");
        // A test app starts while the real book is read.
        let real = Profile::Real {
            test_root: Some(root.clone()),
        };
        let mut mark = None;
        let answer = real.guarded(|| {
            mark = Some(test_profile::hold_mark(&root).expect("marked"));
            "real accounts"
        });
        assert_eq!(answer, Err(TEST_APP_STARTED));

        // The test app ends while its book is read.
        let test = Profile::Test { root: root.clone() };
        assert_eq!(test.guarded(|| "test accounts"), Ok("test accounts"));
        let answer = test.guarded(|| {
            drop(mark.take());
            "test accounts"
        });
        assert_eq!(answer, Err(TEST_APP_ENDED));

        // Nothing changes while it reads: the answer stays.
        assert_eq!(real.guarded(|| "real accounts"), Ok("real accounts"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_start_again_is_refused_unless_every_folder_is_in_the_test_folder() {
        // An absolute path on every system, also on Windows with its drive.
        let data_home = std::env::temp_dir().join("someone-data");
        let root = &test_profile::root_in(&data_home);
        let moved: Vec<(&str, OsString)> = test_profile::variables(root)
            .into_iter()
            .map(|(name, value)| (name, value.into_os_string()))
            .collect();
        let reading = |variables: Vec<(&'static str, OsString)>| {
            move |name: &str| {
                variables
                    .iter()
                    .find(|(variable, _)| *variable == name)
                    .map(|(_, value)| value.clone())
            }
        };
        assert_eq!(started_again_in(root, reading(moved.clone())), Ok(()));
        // One folder left at the real one, or missing.
        for (index, _) in moved.iter().enumerate() {
            let mut changed = moved.clone();
            changed[index].1 = data_home.join("config").into_os_string();
            assert!(started_again_in(root, reading(changed)).is_err());
            let mut missing = moved.clone();
            missing.remove(index);
            assert!(started_again_in(root, reading(missing)).is_err());
        }
        // The whole file system, or a relative path, as the test folder.
        for root in ["/", "kassensturz-test", ""] {
            let root = Path::new(root);
            let moved = test_profile::variables(root)
                .into_iter()
                .map(|(name, value)| (name, value.into_os_string()))
                .collect();
            assert!(started_again_in(root, reading(moved)).is_err(), "{root:?}");
        }
    }
}
