// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! B-333: restoring an "older backup" (from before layout 34, no identity)
//! gives the book a NEW identity when it is reopened, so every backup tied to
//! its old identity, including the one made of the book just before the
//! restore, disappears from the list to restore from.

use std::fs;
use std::path::PathBuf;

use kassensturz_core::AccountId;
use kassensturz_store::{Book, Computer, PreparedRestore, backups_of, older_backups_of};

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
fn restoring_an_older_backup_keeps_the_backup_of_the_book_as_it_was_reachable() {
    let folder = TempFolder::new();
    let computer = Computer {
        name: "Laptop".into(),
        backup_folder: folder.0.join("Backups"),
    };
    let book_path = folder.0.join("Household.ksz");
    let backups = computer.backup_folder.clone();

    // An older backup, as a build before layout 34 left it: no identity.
    let book = Book::create(&book_path, &computer).expect("created");
    fs::create_dir_all(&backups).expect("folder");
    let any = book.back_up_now(&book_path, &backups).expect("backed up");
    let older = backups.join("Household 2026-09-01 09-00-00 UTC.ksz");
    fs::copy(&any, &older).expect("copied");
    rusqlite::Connection::open(&older)
        .and_then(|conn| conn.execute_batch("DROP INDEX transaction_tags_by_rule; ALTER TABLE transaction_tags DROP COLUMN rule_id; ALTER TABLE transaction_tags DROP COLUMN by_rule; ALTER TABLE changed_transaction_tags DROP COLUMN rule_id; ALTER TABLE changed_transaction_tags DROP COLUMN by_rule; DROP TABLE rule_tags; DROP TABLE changed_rule_tags; DROP TABLE removed_transaction_tags; DROP TABLE changed_removed_transaction_tags; DROP TABLE parked_ai_rule_marks; DROP TABLE book_identity; PRAGMA user_version = 33;"))
        .expect("made old");
    let id_before = book.book_id().expect("an identity");
    assert_eq!(
        older_backups_of(&book_path, &backups)
            .expect("listed")
            .len(),
        1
    );

    // The restore as the app does it (book_session.rs restore_backup):
    // check and copy, back up the book as it is now, close, put in place,
    // open again.
    let prepared = PreparedRestore::new(&older, &book_path, &id_before).expect("prepared");
    let as_it_was = book
        .back_up_now(&book_path, &backups)
        .expect("backup of the book as it is now");
    drop(book);
    prepared.put_in_place().expect("put in place");
    let reopened = Book::open(&book_path, &computer).expect("reopened");
    let id_after = reopened.book_id().expect("an identity");

    // BOOK_FORMAT.md: "the book as it was is the newest backup". It must
    // still be offered for the open book, so the user can undo the restore.
    let own = backups_of(&id_after, &backups).expect("listed");
    let older_now = older_backups_of(&book_path, &backups).expect("listed");
    let listed: Vec<_> = own
        .iter()
        .chain(&older_now)
        .map(|b| b.path.clone())
        .collect();
    assert!(
        listed.contains(&as_it_was),
        "the backup of the book as it was before the restore is not listed any more: \
         identity before {id_before:?}, after {id_after:?}; listed: {listed:#?}"
    );
}
