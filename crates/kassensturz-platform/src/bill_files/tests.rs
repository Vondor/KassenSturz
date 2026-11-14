// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The safe move and the journal, with a failure at every step
//! (`docs/RECEIPT_INBOX.md` §10, step 1). Synthetic files only.

use std::cell::Cell;

use super::*;

/// A folder of its own for one test, removed afterwards.
struct Folder(PathBuf);

impl Folder {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("ks-bill-files-{}", new_id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The steps a [`Faulty`] disk can fail at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Copy,
    NoSpace,
    Verify,
    Rename,
    Remove,
}

/// The real disk, with a rename that refuses to replace a file, made to
/// fail at one step.
struct Faulty {
    real: RealDisk<fn(&Path, &Path) -> io::Result<()>>,
    fail: Cell<Option<Step>>,
}

fn no_replace(from: &Path, to: &Path) -> io::Result<()> {
    // A hard link refuses an existing name, like RENAME_NOREPLACE.
    std::fs::hard_link(from, to)?;
    std::fs::remove_file(from)
}

impl Faulty {
    fn new(fail: Option<Step>) -> Self {
        Self {
            real: RealDisk::new(no_replace as fn(&Path, &Path) -> io::Result<()>),
            fail: Cell::new(fail),
        }
    }

    fn fails(&self, step: Step) -> bool {
        self.fail.get() == Some(step)
    }
}

impl Disk for Faulty {
    fn fingerprint(&self, path: &Path) -> io::Result<Fingerprint> {
        self.real.fingerprint(path)
    }
    fn copy_new(&self, from: &Path, to: &Path) -> io::Result<()> {
        if self.fails(Step::NoSpace) {
            // Half a copy, then the drive is full.
            std::fs::write(to, b"half")?;
            return Err(io::Error::from(io::ErrorKind::StorageFull));
        }
        if self.fails(Step::Copy) {
            return Err(io::Error::other("copy"));
        }
        self.real.copy_new(from, to)?;
        if self.fails(Step::Verify) {
            // The drive wrote something else.
            std::fs::write(to, b"different bytes")?;
        }
        Ok(())
    }
    fn same(&self, a: &Path, b: &Path) -> io::Result<bool> {
        self.real.same(a, b)
    }
    fn rename_no_replace(&self, from: &Path, to: &Path) -> io::Result<()> {
        if self.fails(Step::Rename) {
            return Err(io::Error::other("rename"));
        }
        self.real.rename_no_replace(from, to)
    }
    fn sync_folder(&self, folder: &Path) -> io::Result<()> {
        self.real.sync_folder(folder)
    }
    fn remove(&self, path: &Path) -> io::Result<()> {
        if self.fails(Step::Remove) {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        self.real.remove(path)
    }
    fn exists(&self, path: &Path) -> bool {
        self.real.exists(path)
    }
    fn is_folder(&self, path: &Path) -> bool {
        self.real.is_folder(path)
    }
    fn make_folders(&self, path: &Path) -> io::Result<()> {
        self.real.make_folders(path)
    }
    fn names_in(&self, folder: &Path) -> io::Result<Vec<String>> {
        self.real.names_in(folder)
    }
    fn remove_empty_folder(&self, path: &Path) {
        self.real.remove_empty_folder(path);
    }
}

/// An inbox with one invented bill and an archive folder.
struct Setup {
    folder: Folder,
    bill: PathBuf,
    target: PathBuf,
}

const BYTES: &[u8] = b"%PDF-1.7 invented bill RE-0042 Papierladen Nord 24.50 EUR";

fn setup() -> Setup {
    let folder = Folder::new();
    std::fs::create_dir_all(folder.join("Inbox")).unwrap();
    std::fs::create_dir_all(folder.join("Archive")).unwrap();
    let bill = folder.join("Inbox/RE-0042.pdf");
    std::fs::write(&bill, BYTES).unwrap();
    let target = folder.join("Archive/Checking/Groceries/261105_RE-0042.pdf");
    Setup {
        folder,
        bill,
        target,
    }
}

fn disk() -> Faulty {
    Faulty::new(None)
}

#[test]
fn a_move_copies_checks_and_removes_the_original_only_at_the_end() {
    let s = setup();
    let print = disk().fingerprint(&s.bill).unwrap();
    let placed = place(&disk(), &s.bill, &s.target, Some(print)).unwrap();
    // Both are there until the book has saved the link.
    assert_eq!(std::fs::read(&s.target).unwrap(), BYTES);
    assert!(s.bill.exists());
    assert!(!part_of(&s.target).exists());
    finish(&disk(), &placed).unwrap();
    assert!(!s.bill.exists());
    assert_eq!(std::fs::read(&s.target).unwrap(), BYTES);
}

#[test]
fn a_failure_at_any_step_before_the_save_leaves_only_the_original() {
    for (step, expected) in [
        (Step::Copy, MoveFailure::Copy),
        (Step::NoSpace, MoveFailure::NoSpace),
        (Step::Verify, MoveFailure::Verify),
        (Step::Rename, MoveFailure::Copy),
    ] {
        let s = setup();
        let failed = place(&Faulty::new(Some(step)), &s.bill, &s.target, None);
        assert_eq!(failed, Err(expected), "{step:?}");
        assert_eq!(std::fs::read(&s.bill).unwrap(), BYTES, "{step:?}");
        assert!(!s.target.exists(), "{step:?}");
        assert!(!part_of(&s.target).exists(), "{step:?}");
    }
}

#[test]
fn a_bill_that_changed_after_the_ai_read_it_stays() {
    let s = setup();
    let print = disk().fingerprint(&s.bill).unwrap();
    std::fs::write(&s.bill, b"a longer, other bill than the one the AI read").unwrap();
    assert_eq!(
        place(&disk(), &s.bill, &s.target, Some(print)),
        Err(MoveFailure::Changed)
    );
    assert!(!s.target.exists());
    std::fs::remove_file(&s.bill).unwrap();
    assert_eq!(
        place(&disk(), &s.bill, &s.target, None),
        Err(MoveFailure::NotFound)
    );
}

#[test]
fn a_bill_swapped_for_one_of_the_same_size_and_time_stays() {
    // B-284: another file of the same size, with the old time put back.
    let s = setup();
    let print = disk().fingerprint(&s.bill).unwrap();
    let then = std::fs::metadata(&s.bill).unwrap().modified().unwrap();
    let mut swapped = std::fs::read(&s.bill).unwrap();
    swapped.reverse();
    swapped[0] ^= 1;
    std::fs::write(&s.bill, &swapped).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&s.bill)
        .unwrap()
        .set_modified(then)
        .unwrap();
    let now = disk().fingerprint(&s.bill).unwrap();
    assert_eq!((now.size, now.modified), (print.size, print.modified));
    assert!(!print.matches(&now));
    assert_eq!(
        place(&disk(), &s.bill, &s.target, Some(print)),
        Err(MoveFailure::Changed)
    );
    assert!(!s.target.exists());
    assert_eq!(std::fs::read(&s.bill).unwrap(), swapped);
    // A journal line from before the checksum still matches by size and
    // time.
    let older = Fingerprint {
        digest: None,
        ..print
    };
    assert!(older.matches(&print) && print.matches(&older));
}

#[test]
fn nothing_is_ever_replaced_not_even_a_leftover_copy() {
    let s = setup();
    std::fs::create_dir_all(s.target.parent().unwrap()).unwrap();
    std::fs::write(&s.target, b"the user's own file").unwrap();
    assert_eq!(
        place(&disk(), &s.bill, &s.target, None),
        Err(MoveFailure::Exists)
    );
    assert_eq!(std::fs::read(&s.target).unwrap(), b"the user's own file");
    std::fs::remove_file(&s.target).unwrap();
    std::fs::write(part_of(&s.target), b"someone's").unwrap();
    assert_eq!(
        place(&disk(), &s.bill, &s.target, None),
        Err(MoveFailure::Exists)
    );
    assert_eq!(std::fs::read(part_of(&s.target)).unwrap(), b"someone's");
    assert!(s.bill.exists());
}

#[cfg(unix)]
#[test]
fn a_link_in_the_inbox_is_not_followed() {
    let s = setup();
    let outside = s.folder.join("secret.pdf");
    std::fs::write(&outside, b"outside the inbox").unwrap();
    let link = s.folder.join("Inbox/link.pdf");
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    assert_eq!(
        place(&disk(), &link, &s.target, None),
        Err(MoveFailure::Changed)
    );
    assert!(!s.target.exists());
}

#[test]
fn a_gone_inbox_or_a_save_that_failed_leaves_the_original() {
    let s = setup();
    let placed = place(&disk(), &s.bill, &s.target, None).unwrap();
    // The book refused the link: the copy goes, the original stays.
    take_back(&disk(), &placed);
    assert!(!s.target.exists());
    assert_eq!(std::fs::read(&s.bill).unwrap(), BYTES);
    // The copy is never taken back while the original is missing.
    let placed = place(&disk(), &s.bill, &s.target, None).unwrap();
    std::fs::rename(&s.bill, s.folder.join("elsewhere.pdf")).unwrap();
    take_back(&disk(), &placed);
    assert!(s.target.exists());
    let gone = s.folder.join("Gone/RE.pdf");
    assert_eq!(
        place(&disk(), &gone, &s.target, None),
        Err(MoveFailure::InboxGone)
    );
}

#[test]
fn a_refused_remove_leaves_the_bill_filed_and_a_copy_in_the_inbox() {
    let s = setup();
    let placed = place(&disk(), &s.bill, &s.target, None).unwrap();
    assert_eq!(
        finish(&Faulty::new(Some(Step::Remove)), &placed),
        Err(MoveFailure::NoPermission)
    );
    assert!(s.bill.exists() && s.target.exists());
}

/// The journal next to the setup, and an open move noted in it.
fn journaled(s: &Setup, direction: Direction) -> (Journal, String) {
    let journal = Journal::at(s.folder.join(JOURNAL_FILE));
    let print = disk().fingerprint(&s.bill).unwrap();
    let id = journal
        .open_move(direction, &s.bill, &s.target, print)
        .unwrap();
    (journal, id)
}

#[test]
fn the_journal_keeps_each_move_with_its_history_action() {
    let s = setup();
    let (journal, id) = journaled(&s, Direction::File);
    journal.saved(&id, "0a1b").unwrap();
    // A damaged line is passed over.
    let mut text = std::fs::read_to_string(s.folder.join(JOURNAL_FILE)).unwrap();
    text.push_str("not json\n{\"move\":\"x\"}\n");
    std::fs::write(s.folder.join(JOURNAL_FILE), text).unwrap();
    let entries = journal.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].change.as_deref(), Some("0a1b"));
    assert!(!entries[0].closed);
    assert_eq!(entries[0].archive_path(), s.target);
    assert!(journal.filed_by("0a1b").is_empty(), "not closed yet");
    journal.close(&id).unwrap();
    assert_eq!(journal.filed_by("0a1b").len(), 1);
    assert!(journal.taken_back_by("0a1b").is_empty());
    // An Undo moved it back: now Redo has it, Undo has nothing.
    let back = journal
        .open_move(
            Direction::Back,
            &s.target,
            &s.bill,
            Fingerprint {
                size: 1,
                modified: None,
                digest: None,
            },
        )
        .unwrap();
    journal.saved(&back, "0a1b").unwrap();
    journal.close(&back).unwrap();
    assert!(journal.filed_by("0a1b").is_empty());
    assert_eq!(journal.taken_back_by("0a1b").len(), 1);
}

/// Settles with the book linking the archive path or not.
fn settle(journal: &Journal, linked: bool) -> Vec<Settled> {
    journal.settle(&disk(), &move |_: &Path| linked)
}

#[test]
fn a_crash_after_the_copy_but_before_the_save_removes_the_copy() {
    let s = setup();
    let (journal, _) = journaled(&s, Direction::File);
    place(&disk(), &s.bill, &s.target, None).unwrap();
    assert_eq!(settle(&journal, false), Vec::new());
    assert!(!s.target.exists());
    assert_eq!(std::fs::read(&s.bill).unwrap(), BYTES);
    // Settled once is settled.
    assert!(journal.entries().iter().all(|entry| entry.closed));
}

#[test]
fn a_crash_during_the_copy_removes_the_half_copy() {
    let s = setup();
    let (journal, _) = journaled(&s, Direction::File);
    std::fs::create_dir_all(s.target.parent().unwrap()).unwrap();
    std::fs::copy(&s.bill, part_of(&s.target)).unwrap();
    assert_eq!(settle(&journal, false), Vec::new());
    assert!(!part_of(&s.target).exists());
    assert!(s.bill.exists());
}

#[test]
fn a_crash_after_the_save_removes_the_original() {
    let s = setup();
    let (journal, _) = journaled(&s, Direction::File);
    place(&disk(), &s.bill, &s.target, None).unwrap();
    assert_eq!(settle(&journal, true), Vec::new());
    assert!(!s.bill.exists());
    assert_eq!(std::fs::read(&s.target).unwrap(), BYTES);
}

#[test]
fn settling_never_removes_the_last_copy() {
    // Linked, original gone, file in the archive: done.
    let s = setup();
    let (journal, _) = journaled(&s, Direction::File);
    place(&disk(), &s.bill, &s.target, None).unwrap();
    std::fs::remove_file(&s.bill).unwrap();
    assert_eq!(settle(&journal, true), Vec::new());
    assert!(s.target.exists());

    // Not linked, original gone, file in the archive: kept and told.
    let s = setup();
    let (journal, _) = journaled(&s, Direction::File);
    place(&disk(), &s.bill, &s.target, None).unwrap();
    std::fs::remove_file(&s.bill).unwrap();
    assert_eq!(
        settle(&journal, false),
        vec![Settled::UnlinkedInArchive(s.target.clone())]
    );
    assert!(s.target.exists());

    // Only the unchecked copy is left: kept and told.
    let s = setup();
    let (journal, _) = journaled(&s, Direction::File);
    std::fs::create_dir_all(s.target.parent().unwrap()).unwrap();
    std::fs::rename(&s.bill, part_of(&s.target)).unwrap();
    assert_eq!(
        settle(&journal, false),
        vec![Settled::OnlyPartLeft(part_of(&s.target))]
    );
    assert!(part_of(&s.target).exists());

    // Two different files: both kept.
    let s = setup();
    journaled(&s, Direction::File);
    std::fs::create_dir_all(s.target.parent().unwrap()).unwrap();
    std::fs::write(&s.target, b"another file").unwrap();
    for linked in [false, true] {
        let (journal, _) = if linked {
            journaled(&s, Direction::File)
        } else {
            (Journal::at(s.folder.join(JOURNAL_FILE)), String::new())
        };
        let told = settle(&journal, linked);
        assert_eq!(
            told,
            vec![Settled::BothKept(s.bill.clone(), s.target.clone())]
        );
    }
    assert!(s.bill.exists() && s.target.exists());

    // Both gone: told.
    let s = setup();
    let (journal, _) = journaled(&s, Direction::File);
    std::fs::remove_file(&s.bill).unwrap();
    assert_eq!(
        settle(&journal, false),
        vec![Settled::Gone(s.target.clone())]
    );
}

#[test]
fn an_undo_left_open_is_settled_the_other_way() {
    // The file went back into the inbox; the book's Undo didn't run (the
    // link is still there): the inbox copy goes, the archive stays.
    let s = setup();
    let placed = place(&disk(), &s.bill, &s.target, None).unwrap();
    finish(&disk(), &placed).unwrap();
    let journal = Journal::at(s.folder.join(JOURNAL_FILE));
    let print = disk().fingerprint(&s.target).unwrap();
    journal
        .open_move(Direction::Back, &s.target, &s.bill, print)
        .unwrap();
    place(&disk(), &s.target, &s.bill, None).unwrap();
    assert_eq!(settle(&journal, true), Vec::new());
    assert!(!s.bill.exists() && s.target.exists());

    // The book's Undo ran (no link): the archive copy goes.
    let journal = Journal::at(s.folder.join(JOURNAL_FILE));
    journal
        .open_move(Direction::Back, &s.target, &s.bill, print)
        .unwrap();
    place(&disk(), &s.target, &s.bill, None).unwrap();
    assert_eq!(settle(&journal, false), Vec::new());
    assert!(s.bill.exists() && !s.target.exists());
    // Its empty folders went with it, the archive itself stays.
    assert!(!s.folder.join("Archive/Checking/Groceries").exists());
    assert!(s.folder.join("Archive").exists());
}
