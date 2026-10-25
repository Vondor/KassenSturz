// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The program refuses to start unless AI is switched on (D-013, D-050) and
//! it has a book to read (D-196), and a build without AI never starts. Runs
//! the real program with a settings folder of its own.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

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

fn run(settings_folder: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kassensturz-mcp"))
        .args(arguments)
        .stdin(Stdio::null())
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
    let output = run(folder.path(), &[]);
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
    let output = run(folder.path(), &[]);
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
fn passes_the_switch_and_refuses_when_there_is_no_book_to_read() {
    let folder = Folder::new("on-no-book");
    std::fs::write(folder.path().join("kassensturzrc"), "[AI]\nEnabled=true\n").unwrap();
    let output = run(folder.path(), &[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("hasn't opened a book yet"),
        "{}",
        stderr(&output)
    );

    let missing = folder.path().join("missing.ksz");
    let output = run(folder.path(), &["--book", missing.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("store.not-found"),
        "{}",
        stderr(&output)
    );

    let output = run(folder.path(), &["--port", "8080"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("usage"), "{}", stderr(&output));
    assert!(output.stdout.is_empty());
}

/// Without `--book` the server reads the book the app opened last, from
/// the path the app wrote with KConfig (D-196).
#[cfg(all(feature = "ai", target_os = "linux"))]
#[test]
fn reads_the_book_the_app_opened_last() {
    let folder = Folder::new("last-book");
    let books = folder.path().join("Documents");
    std::fs::create_dir_all(&books).unwrap();
    let path = books.join("Household.ksz");
    let computer = kassensturz_store::Computer {
        name: "Laptop".into(),
        backup_folder: folder.path().join("backups"),
    };
    drop(kassensturz_store::Book::create(&path, &computer).expect("book created"));
    std::fs::write(
        folder.path().join("kassensturzrc"),
        "[AI]\nEnabled=true\n\n[Books]\nLastBook[$e]=$HOME/Documents/Household.ksz\n",
    )
    .unwrap();
    // Nothing on its input: the AI tool closed it at once, so the server
    // ends without having been asked anything.
    let output = run(folder.path(), &[]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(output.stdout.is_empty());
}

#[cfg(not(feature = "ai"))]
#[test]
fn a_build_without_ai_never_starts_even_when_the_settings_say_on() {
    let folder = Folder::new("none-built");
    std::fs::write(folder.path().join("kassensturzrc"), "[AI]\nEnabled=true\n").unwrap();
    let output = run(folder.path(), &[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("without AI"),
        "{}",
        stderr(&output)
    );
}
