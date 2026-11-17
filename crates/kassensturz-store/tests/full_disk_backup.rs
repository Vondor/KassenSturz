// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A backup that can't be written because the disk is full (D-787, US7 Phase
//! 3 round 1, section C). A copy that fails leaves no part file, takes no
//! older backup away (rotation runs only after a copy succeeded) and leaves
//! the book as it was. The full disk is made with the system's file size
//! limit: a child program that may write files of at most 100 blocks gets
//! "file too large", which the program meets like "no space left". Linux
//! only. All data is made up.

#![cfg(target_os = "linux")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use kassensturz_core::AccountId;
use kassensturz_store::{Book, Computer, backups_of};

const CHILD: &str = "KASSENSTURZ_TEST_FULL_DISK_BOOK";

struct TempFolder(PathBuf);

impl TempFolder {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("kassensturz-test-{}", AccountId::generate()));
        fs::create_dir(&path).expect("temporary folder created");
        Self(path)
    }

    fn computer(&self) -> Computer {
        Computer {
            name: "Laptop".into(),
            backup_folder: self.0.join("Backups"),
        }
    }
}

impl Drop for TempFolder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn names_in(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder)
        .expect("folder listed")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// The child: tries one more backup, and says what came of it.
#[test]
fn full_disk_child() {
    let Ok(path) = std::env::var(CHILD) else {
        return;
    };
    let path = PathBuf::from(path);
    let folder = path.parent().expect("a folder").to_path_buf();
    let computer = Computer {
        name: "Laptop".into(),
        backup_folder: folder.join("Backups"),
    };
    let book = Book::open(&path, &computer).expect("the book opens");
    let result = book.back_up_now(&path, &computer.backup_folder);
    println!("RESULT {}", if result.is_ok() { "made" } else { "refused" });
}

#[test]
fn a_backup_that_does_not_fit_leaves_the_others_and_no_part_file() {
    let folder = TempFolder::new();
    let path = folder.0.join("Household.ksz");
    let computer = folder.computer();
    let book = Book::create(&path, &computer).expect("book made");
    let backups = computer.backup_folder.clone();
    // As many backups as the app keeps, made one after the other.
    for _ in 0..12 {
        book.back_up_now(&path, &backups).expect("backed up");
    }
    drop(book);
    let before = names_in(&backups);
    let book_before = fs::read(&path).expect("book read");
    assert!(book_before.len() > 100 * 1024, "the book fits the limit");

    let exe = std::env::current_exe().expect("this program");
    let output = Command::new("sh")
        .args([
            "-c",
            // SIGXFSZ ignored: the write fails with "file too large" instead
            // of ending the program.
            "trap '' XFSZ; ulimit -f 100; exec \"$0\" --exact full_disk_child --nocapture",
        ])
        .arg(exe)
        .env(CHILD, &path)
        .output()
        .expect("child ran");
    let said = String::from_utf8_lossy(&output.stdout);
    assert!(
        said.contains("RESULT refused"),
        "the backup was not refused: {said} {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(names_in(&backups), before, "a file was added or taken away");
    assert!(
        names_in(&backups)
            .iter()
            .all(|name| !name.contains(".part") && name.ends_with(".ksz")),
        "a part file was left"
    );
    assert_eq!(fs::read(&path).expect("book read"), book_before);
    let id = Book::open(&path, &computer)
        .expect("opens")
        .book_id()
        .expect("an identity");
    let listed = backups_of(&id, &backups).expect("listed");
    assert_eq!(listed.len(), before.len());
    Book::open(&path, &computer).expect("the book still opens");
}
