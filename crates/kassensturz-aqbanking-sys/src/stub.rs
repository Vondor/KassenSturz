// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Finds the stub library for tests (feature `stub`): the workspace crate
//! `kassensturz-aqbanking-stub`, a shared library that exports every
//! function the bindings declare and plays scenarios of good and broken
//! bank data (threat T-09). It is built with Cargo on first use, in the
//! same target folder as the tests, and loaded with [`Libraries::load_from`]
//! or [`session::Session::open_from`] in place of the real libraries.
//!
//! [`Libraries::load_from`]: crate::Libraries::load_from
//! [`session::Session::open_from`]: crate::session::Session::open_from

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// The name of the file the stub reads its scenario from, in the system's
/// temporary folder, followed by the process ID: `<name>-<pid>`. The test
/// writes the scenario's name into it before loading the stub.
pub const SCENARIO_FILE_PREFIX: &str = "kassensturz-aqbanking-stub";

/// The file the stub writes its journal to, inside the data folder it was
/// given: one line per call that a test may want to check (the setup
/// commands with their options, the fetch's account and date). Never a
/// PIN.
pub const JOURNAL_FILE: &str = "ks-stub-journal.txt";

/// Where the test process's scenario file is.
pub fn scenario_file() -> PathBuf {
    std::env::temp_dir().join(format!("{SCENARIO_FILE_PREFIX}-{}", std::process::id()))
}

/// Selects the scenario the stub plays for this process. Tests in one
/// process that use different scenarios run one at a time.
pub fn set_scenario(name: &str) {
    std::fs::write(scenario_file(), name).expect("the scenario file can be written");
}

/// Builds the stub library once and returns its path.
///
/// Under a runner that starts one process per test (cargo-nextest, D-957)
/// many processes call this at the same moment. Cargo takes the file it
/// names, removes it and links it again on every build, even a build with
/// nothing to do, so a process that loaded it while another one's build
/// ran found no file (the macOS flake B-459). So the builds run one at a
/// time under a lock, and the library that is loaded is a copy beside
/// cargo's file that is only ever replaced by an atomic rename.
pub fn library() -> &'static Path {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(std::env::temp_dir().join("kassensturz-aqbanking-stub-build.lock"))
            .expect("the build lock can be opened");
        lock.lock().expect("the build lock can be taken");
        let built = build();
        let stable = stable_copy(&built);
        drop(lock);
        stable
    })
}

/// The copy of `built` that nothing rewrites: it stays as it is while its
/// bytes match, and a changed build replaces it by an atomic rename. The
/// caller holds the build lock.
fn stable_copy(built: &Path) -> PathBuf {
    let name = built.file_name().expect("the library has a file name");
    let mut copy_name = std::ffi::OsString::from("ks-stable-");
    copy_name.push(name);
    let copy = built.with_file_name(copy_name);
    let bytes = std::fs::read(built).expect("the built stub library can be read");
    if std::fs::read(&copy).ok().as_deref() != Some(bytes.as_slice()) {
        let mut temp_name = copy.as_os_str().to_owned();
        temp_name.push(format!(".{}.tmp", std::process::id()));
        let temp = PathBuf::from(temp_name);
        std::fs::write(&temp, &bytes).expect("the stub library's copy can be written");
        std::fs::rename(&temp, &copy).expect("the stub library's copy can be put in place");
    }
    copy
}

fn build() -> PathBuf {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("kassensturz-aqbanking-stub")
        .join("Cargo.toml");
    let output = Command::new(cargo)
        .args([
            "build",
            "--locked",
            "--offline",
            "--message-format=json",
            "--manifest-path",
        ])
        .arg(&manifest)
        .output()
        .expect("cargo runs");
    assert!(
        output.status.success(),
        "the stub library didn't build:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .filter(|line| {
            line.contains("\"compiler-artifact\"") && line.contains("kassensturz-aqbanking-stub")
        })
        .flat_map(|line| line.split('"').map(str::to_owned).collect::<Vec<_>>())
        .map(|field| field.replace("\\\\", "\\"))
        .find(|field| {
            let path = Path::new(field);
            path.is_absolute()
                && path.extension().is_some_and(|extension| {
                    extension == "so" || extension == "dll" || extension == "dylib"
                })
        })
        .map(PathBuf::from)
        .expect("cargo named the stub library's file")
}
