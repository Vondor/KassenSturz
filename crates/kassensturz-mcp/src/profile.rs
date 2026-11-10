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
        return Start::Read(Profile::Test {
            root: PathBuf::from(root),
        });
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
