// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tells the program the commit it is built from, so the AI tool can be
//! told which build it talks to (D-152, D-174).

use std::process::Command;

fn main() {
    println!(
        "cargo:rustc-env=KASSENSTURZ_BUILD_VERSION={}",
        build_version()
    );
    println!(
        "cargo:rustc-env=KASSENSTURZ_BUILD_COMMIT={}",
        build_commit()
    );
}

/// The short id of the commit this build is made from, found the way the
/// app finds it (`kassensturz-ui-bridge/build.rs`): the `KASSENSTURZ_COMMIT`
/// variable, the file `build-commit` in the repository's top folder (CI
/// writes it before the Flatpak build, whose copy of the sources has no
/// `.git`), and git itself. Empty when none of them knows.
fn build_commit() -> String {
    println!("cargo:rerun-if-env-changed=KASSENSTURZ_COMMIT");
    println!("cargo:rerun-if-changed=../../build-commit");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    let from_git = || {
        let output = Command::new("git")
            .args(["rev-parse", "--short=7", "HEAD"])
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    };
    let commit = std::env::var("KASSENSTURZ_COMMIT")
        .ok()
        .or_else(|| std::fs::read_to_string("../../build-commit").ok())
        .or_else(from_git)
        .unwrap_or_default();
    commit
        .trim()
        .chars()
        .take(7)
        .filter(char::is_ascii_hexdigit)
        .collect()
}

/// The version of this build in D-152's form (D-226): the
/// `KASSENSTURZ_VERSION` variable, the file `build-version` in the
/// repository's top folder (CI writes it before the Flatpak build, as it does
/// `build-commit`), `tools/build-version.sh`, and at last the version in
/// `Cargo.toml`. A value that is not three numbers joined by points is left out.
fn build_version() -> String {
    println!("cargo:rerun-if-env-changed=KASSENSTURZ_VERSION");
    println!("cargo:rerun-if-changed=../../build-version");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    let from_script = || {
        let output = Command::new("sh")
            .arg("../../tools/build-version.sh")
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    };
    let is_version = |text: &str| {
        let parts: Vec<&str> = text.split('.').collect();
        parts.len() == 3
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
    };
    [
        std::env::var("KASSENSTURZ_VERSION").ok(),
        std::fs::read_to_string("../../build-version").ok(),
        from_script(),
    ]
    .into_iter()
    .flatten()
    .map(|text| text.trim().to_owned())
    .find(|text| is_version(text))
    .unwrap_or_else(|| std::env::var("CARGO_PKG_VERSION").unwrap_or_default())
}
