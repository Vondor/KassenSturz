// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A lock this computer certainly left behind is cleared without asking
//! (D-781, Q-580); any other keeps the page. All data is made up.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};

use crate::test_folder::TempFolder;
use crate::{Book, Computer, StoreError};

fn computer(folder: &TempFolder) -> Computer {
    Computer {
        name: "Laptop".into(),
        backup_folder: folder.join("Backups"),
    }
}

/// A book in `folder`, closed again.
fn made(folder: &TempFolder) -> std::path::PathBuf {
    let path = folder.join("Household.ksz");
    drop(Book::create(&path, &computer(folder)).expect("book made"));
    path
}

fn is_locked(result: Result<Book, StoreError>) -> bool {
    matches!(result, Err(StoreError::Locked(_)))
}

// The child of these tests: when the parent test starts the test program
// again with this variable, it opens the book the variable names, says so,
// and waits until it is ended from outside, without closing anything.
const HOLD: &str = "KASSENSTURZ_TEST_HOLD_BOOK";

#[test]
fn a_child_that_holds_a_book_until_it_is_ended() {
    let Some(path) = std::env::var_os(HOLD) else {
        // Not started by a test of this file: nothing to do.
        return;
    };
    let path = Path::new(&path);
    let computer = Computer {
        name: "Laptop".into(),
        backup_folder: path.with_file_name("Backups"),
    };
    let book = Book::open(path, &computer).expect("child opened the book");
    println!("holding");
    // Until the parent ends this program; never returns.
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        std::hint::black_box(&book);
    }
}

/// Starts a program that holds `path` open and waits until it says so.
fn holder(path: &Path) -> Child {
    let mut child = Command::new(std::env::current_exe().expect("test program"))
        .args([
            "--exact",
            "lock::left_behind_tests::a_child_that_holds_a_book_until_it_is_ended",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(HOLD, path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("child started");
    let mut lines = BufReader::new(child.stdout.take().expect("stdout")).lines();
    let said = lines.any(|line| line.is_ok_and(|line| line.ends_with("holding")));
    assert!(said, "the child never opened the book");
    // The reader is dropped with the lines; the child writes nothing more.
    child
}

/// Ends the program the way a logout or `pkill` does where there is such a
/// way (SIGTERM), otherwise as hard as the system allows, and waits for it.
fn end(mut child: Child) {
    #[cfg(unix)]
    {
        let status = Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .status()
            .expect("kill run");
        assert!(status.success());
    }
    #[cfg(not(unix))]
    child.kill().expect("child ended");
    child.wait().expect("child gone");
}

#[test]
fn a_book_a_program_of_this_computer_still_holds_always_asks() {
    let folder = TempFolder::new();
    let path = made(&folder);
    let child = holder(&path);
    assert!(is_locked(Book::open(&path, &computer(&folder))));
    end(child);
}

// Only where the system tells whether a program runs; on macOS the lock names
// no program and keeps asking.
#[cfg(any(target_os = "linux", windows))]
#[test]
fn a_book_a_program_of_this_computer_was_ended_on_opens_without_asking() {
    let folder = TempFolder::new();
    let path = made(&folder);
    // The real program's lock file, written by the real code: it names a
    // program, its start and its place.
    let child = holder(&path);
    let lock = std::fs::read_to_string(crate::lock::lock_path(&path)).expect("lock file");
    assert!(lock.contains("pid="), "{lock}");
    // Killed, so it closes nothing and the lock file stays, as after a
    // crash, a power cut or a SIGTERM that nothing handles (the command-line
    // tool, before B-276 covered the app).
    end(child);
    assert!(crate::lock::lock_path(&path).exists());
    let book = Book::open(&path, &computer(&folder)).expect("opened without asking");
    drop(book);
    assert!(!crate::lock::lock_path(&path).exists());
}

#[test]
fn a_lock_of_another_computer_always_asks_even_when_its_program_is_gone() {
    let folder = TempFolder::new();
    let path = made(&folder);
    let child = holder(&path);
    let text = std::fs::read_to_string(crate::lock::lock_path(&path)).expect("lock file");
    end(child);
    // The same lock file, written by another computer: it says so, and a
    // program number of its own that happens to be gone here.
    let other = text.replace("computer=Laptop", "computer=Desktop");
    assert_ne!(text, other);
    std::fs::write(crate::lock::lock_path(&path), other).expect("lock file written");
    assert!(is_locked(Book::open(&path, &computer(&folder))));
}

#[test]
fn a_lock_without_a_program_or_place_always_asks() {
    let folder = TempFolder::new();
    let path = made(&folder);
    for text in [
        "computer=Laptop\nsince=2026-10-19T11:04:02Z\ntoken=old\n",
        "computer=Laptop\nsince=2026-10-19T11:04:02Z\ntoken=old\npid=4194000\nstart=1\n",
    ] {
        std::fs::write(crate::lock::lock_path(&path), text).expect("lock file written");
        assert!(is_locked(Book::open(&path, &computer(&folder))), "{text}");
    }
}

#[cfg(target_os = "linux")]
#[test]
fn the_start_of_the_computer_and_the_process_space_decide_on_linux() {
    let folder = TempFolder::new();
    let path = made(&folder);
    let child = holder(&path);
    let text = std::fs::read_to_string(crate::lock::lock_path(&path)).expect("lock file");
    end(child);
    let line = |key: &str| {
        text.lines()
            .find(|line| line.starts_with(&format!("{key}=")))
            .expect("line")
            .to_owned()
    };
    let write = |text: String| {
        std::fs::write(crate::lock::lock_path(&path), text).expect("lock file written");
    };
    // Another start of the computer, named for this computer: restarted.
    write(text.replace(&line("boot"), "boot=00000000-0000-0000-0000-000000000000"));
    let book = Book::open(&path, &computer(&folder)).expect("restart: no question");
    drop(book);
    // Another process space: it can't be looked up, so it may still run.
    write(text.replace(&line("namespace"), "namespace=pid:[1]"));
    assert!(is_locked(Book::open(&path, &computer(&folder))));
    // The same, from another computer after a different start: it asks.
    write(
        text.replace(&line("boot"), "boot=00000000-0000-0000-0000-000000000000")
            .replace("computer=Laptop", "computer=Desktop"),
    );
    assert!(is_locked(Book::open(&path, &computer(&folder))));
}
