// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! B-334: for a damaged book (B-291) the app points to "the newest backup
//! this version can open" (`newest_usable_backup`, used by
//! `book_session.rs` `newer_layout_backup` for `Corrupt(DamagedFile)`).
//! With PR #1083 that backup is found by the identity read from the
//! damaged file itself, which a damaged file often cannot give.

use std::fs;
use std::path::PathBuf;

use kassensturz_core::AccountId;
use kassensturz_store::{Book, Computer, Corruption, StoreError, newest_usable_backup};

struct TempFolder(PathBuf);

impl TempFolder {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("kassensturz-test-{}", AccountId::generate()));
        fs::create_dir(&path).expect("temporary folder created");
        Self(path)
    }
}

impl Drop for TempFolder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_damaged_book_still_points_to_its_backup() {
    let mut failures = Vec::new();
    // The same cuts as `a_cut_short_file_is_a_damaged_book`, plus a damaged page.
    for case in [
        "half",
        "minus100",
        "two pages",
        "200 bytes",
        "page 3 overwritten",
    ] {
        let folder = TempFolder::new();
        let computer = Computer {
            name: "Laptop".into(),
            backup_folder: folder.0.join("Backups"),
        };
        let path = folder.0.join("Household.ksz");
        let book = Book::create(&path, &computer).expect("created");
        book.back_up_now(&path, &computer.backup_folder)
            .expect("backed up");
        drop(book);
        let bytes = fs::read(&path).expect("read");
        let damaged: Vec<u8> = match case {
            "half" => bytes[..bytes.len() / 2].to_vec(),
            "minus100" => bytes[..bytes.len() - 100].to_vec(),
            "two pages" => bytes[..4096 * 2].to_vec(),
            "200 bytes" => bytes[..200].to_vec(),
            _ => {
                let mut b = bytes.clone();
                for byte in &mut b[3 * 4096..3 * 4096 + 64] {
                    *byte = 0xFF;
                }
                b
            }
        };
        fs::write(&path, &damaged).expect("damaged");
        let opened = Book::open(&path, &computer);
        assert!(
            matches!(opened, Err(StoreError::Corrupt(Corruption::DamagedFile))),
            "{case}: not reported as damaged"
        );
        if newest_usable_backup(&path, &computer.backup_folder).is_none() {
            failures.push(case);
        }
    }
    assert!(
        failures.is_empty(),
        "a damaged book is not pointed to its good backup for: {failures:?}"
    );
}
