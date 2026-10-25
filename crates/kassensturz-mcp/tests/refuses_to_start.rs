// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The program refuses to start unless AI is switched on (D-013, D-050), and
//! a build without AI never starts. Runs the real program with a settings
//! folder of its own.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A folder for one test, removed afterwards.
struct Folder(PathBuf);

impl Folder {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("kassensturz-mcp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("test folder");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run(settings_folder: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kassensturz-mcp"))
        .env("XDG_CONFIG_HOME", settings_folder)
        .env("HOME", settings_folder)
        .output()
        .expect("the program runs")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[cfg(feature = "ai")]
#[test]
fn refuses_to_start_without_a_settings_file() {
    let folder = Folder::new("none");
    let output = run(folder.path());
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("switched off"),
        "{}",
        stderr(&output)
    );
    assert!(output.stdout.is_empty());
}

#[cfg(feature = "ai")]
#[test]
fn refuses_to_start_while_ai_is_off_in_the_settings() {
    let folder = Folder::new("off");
    std::fs::write(folder.path().join("kassensturzrc"), "[AI]\nEnabled=false\n").unwrap();
    let output = run(folder.path());
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("switched off"),
        "{}",
        stderr(&output)
    );
}

// Only Linux knows where the settings file is yet (D-177); elsewhere the
// switch reads as off.
#[cfg(all(feature = "ai", target_os = "linux"))]
#[test]
fn passes_the_switch_when_ai_is_on_and_says_it_is_not_built_yet() {
    let folder = Folder::new("on");
    std::fs::write(folder.path().join("kassensturzrc"), "[AI]\nEnabled=true\n").unwrap();
    let output = run(folder.path());
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("not built yet"),
        "{}",
        stderr(&output)
    );
}

#[cfg(not(feature = "ai"))]
#[test]
fn a_build_without_ai_never_starts_even_when_the_settings_say_on() {
    let folder = Folder::new("none-built");
    std::fs::write(folder.path().join("kassensturzrc"), "[AI]\nEnabled=true\n").unwrap();
    let output = run(folder.path());
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("without AI"),
        "{}",
        stderr(&output)
    );
}
