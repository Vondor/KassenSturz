// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! B-341: backups whose names are dated in the future (made while the clock
//! was set ahead) must not make every later backup delete the previous real
//! one; they are kept, and the real ones are pruned among themselves.

use std::fs;
use std::path::PathBuf;

use kassensturz_core::AccountId;
use kassensturz_store::{Book, Computer};

struct TempFolder(PathBuf);

impl TempFolder {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("ks-backups-ahead-{}", AccountId::generate()));
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
fn backups_from_a_clock_set_ahead_do_not_wipe_out_the_real_ones() {
    let folder = TempFolder::new();
    let computer = Computer {
        name: "Laptop".into(),
        backup_folder: folder.0.join("Backups"),
    };
    let path = folder.0.join("Household.ksz");
    let book = Book::create(&path, &computer).expect("created");
    let backups = &computer.backup_folder;

    // Ten backups made while the clock said 2099: real backups of the book.
    let made = book.back_up_now(&path, backups).expect("backed up");
    for day in 1..=10 {
        fs::copy(
            &made,
            backups.join(format!("Household 2099-01-{day:02} 09-00-00 UTC.ksz")),
        )
        .expect("copied");
    }
    fs::remove_file(&made).expect("removed");

    // The clock is right again; the book is backed up five more times
    // (e.g. five openings, each with the monthly backup, which never counts
    // a future backup as recent).
    let mut real = Vec::new();
    for _ in 0..5 {
        real.push(book.back_up_now(&path, backups).expect("backed up"));
    }
    let names: Vec<String> = fs::read_dir(backups)
        .expect("read")
        .map(|e| e.expect("entry").file_name().into_string().expect("UTF-8"))
        .collect();
    let ahead = names
        .iter()
        .filter(|name| name.starts_with("Household 2099"))
        .count();
    assert_eq!(real.len(), 5);
    assert!(
        real.iter().all(|path| path.exists()),
        "every real backup survives: {names:?}"
    );
    assert_eq!(ahead, 10, "the backups made ahead are kept too: {names:?}");
}
