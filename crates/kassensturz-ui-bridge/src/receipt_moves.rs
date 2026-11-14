// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Moving bills between the receipt inbox and its archive (US7, #333,
//! D-752, `docs/RECEIPT_INBOX.md` §6 and §7), apart from Qt.
//!
//! A confirmed set files its bills: each is copied, checked and put in
//! place in the archive before the book is saved, and its original is
//! removed only after ([`Mover::place`], [`Mover::finish`]); a book that
//! couldn't save takes every copy back ([`Mover::take_back`]). Undo of that
//! change moves the bills back into the inbox the same safe way, and Redo
//! files them again, both named by the journal next to `kassensturzrc`
//! ([`Mover::prepare_undo`], [`Mover::prepare_redo`]). A move a crash left
//! open is settled by the facts at start-up ([`Mover::settle`]).
//!
//! Undo, Redo and settling are the user's own book and files, so they work
//! in every build and with AI off; only filing from a set the AI prepared
//! needs the AI build (`ai_chat::confirm_with_bills_in`).

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

use kassensturz_core::receipt_inbox::free_file_name;
use kassensturz_platform::bill_files::{
    self, Direction, Disk, Fingerprint, JOURNAL_FILE, Journal, Placed, RealDisk, Settled,
};
use kassensturz_store::Book;

thread_local! {
    /// The category folder of a bill whose transaction has no category, in
    /// the user's language: `Main.qml` hands it in (D-034), since the core
    /// and the bridge return no text. Per thread, like the open book.
    static UNSORTED: RefCell<String> = const { RefCell::new(String::new()) };

    /// The archive the tests choose, in place of the user's settings.
    #[cfg(test)]
    static TEST_ARCHIVE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// The English name until the window hands in the user's.
const UNSORTED_ENGLISH: &str = "Not sorted yet";

/// Keeps `name` as the folder name for bills without a category.
pub fn set_unsorted_name(name: &str) {
    UNSORTED.with(|unsorted| name.trim().clone_into(&mut unsorted.borrow_mut()));
}

/// The folder name for bills without a category.
pub fn unsorted_name() -> String {
    UNSORTED.with(|unsorted| {
        let unsorted = unsorted.borrow();
        if unsorted.is_empty() {
            UNSORTED_ENGLISH.to_owned()
        } else {
            unsorted.clone()
        }
    })
}

/// The archive this user chose, while AI is on and the inbox is chosen too.
#[cfg(all(not(test), feature = "ai"))]
pub fn archive() -> Option<PathBuf> {
    kassensturz_core::ai_switch::inbox_folders().map(|(_, archive)| archive)
}

/// A build without AI never files bills (D-039), so it has no archive.
#[cfg(all(not(test), not(feature = "ai")))]
pub fn archive() -> Option<PathBuf> {
    None
}

/// The archive the test chose with [`choose_archive_for_test`]: the tests
/// never read the user's own settings.
#[cfg(test)]
pub fn archive() -> Option<PathBuf> {
    TEST_ARCHIVE.with(|archive| archive.borrow().clone())
}

/// Chooses the archive [`archive`] answers on this thread.
#[cfg(test)]
pub fn choose_archive_for_test(archive: Option<PathBuf>) {
    TEST_ARCHIVE.with(|chosen| *chosen.borrow_mut() = archive);
}

/// The settings key of how the bill folder is shown (B-267): its real
/// path where the system said it, display text only (D-730).
pub const BILL_SHOWN_KEY: &str = "BillFolderShownAs";

/// The same for the receipt inbox.
pub const INBOX_SHOWN_KEY: &str = "InboxFolderShownAs";

/// The same for the archive.
pub const ARCHIVE_SHOWN_KEY: &str = "ArchiveFolderShownAs";

/// `path` as the user knows it: inside the inbox, the archive or the bill
/// folder from that folder's shown path on, not the sandbox's (B-267).
/// Display text only.
pub fn shown(path: &Path) -> String {
    shown_in(&crate::settings::Settings::user(), path)
}

/// [`shown`] with the folders of `settings`.
#[cfg(feature = "ai")]
pub fn shown_in(settings: &crate::settings::Settings, path: &Path) -> String {
    use kassensturz_core::ai_switch::{
        ARCHIVE_FOLDER_KEY, BILL_FOLDER_KEY, GROUP, INBOX_FOLDER_KEY,
    };
    for (key, shown_key) in [
        (INBOX_FOLDER_KEY, INBOX_SHOWN_KEY),
        (ARCHIVE_FOLDER_KEY, ARCHIVE_SHOWN_KEY),
        (BILL_FOLDER_KEY, BILL_SHOWN_KEY),
    ] {
        let folder = settings.text(GROUP, key);
        let shown = settings.text(GROUP, shown_key);
        if folder.is_empty() || shown.is_empty() {
            continue;
        }
        if let Ok(rest) = path.strip_prefix(&folder) {
            return Path::new(&shown).join(rest).to_string_lossy().into_owned();
        }
    }
    path.to_string_lossy().into_owned()
}

/// A build without AI keeps no inbox or archive, so a path shows as it is.
#[cfg(not(feature = "ai"))]
pub fn shown_in(_settings: &crate::settings::Settings, path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The journal of this computer's moves, next to `kassensturzrc`.
#[cfg(feature = "ai")]
pub fn journal() -> Option<Journal> {
    let settings = kassensturz_core::ai_switch::settings_file()?;
    Some(Journal::at(settings.parent()?.join(JOURNAL_FILE)))
}

/// A build without AI never files bills (D-039), so it keeps no journal
/// and Undo and Redo move no files.
#[cfg(not(feature = "ai"))]
pub fn journal() -> Option<Journal> {
    None
}

/// The real files, with the platform's rename that never replaces one.
pub fn disk() -> RealDisk<fn(&Path, &Path) -> io::Result<()>> {
    RealDisk::new(crate::platform::rename_no_replace)
}

/// A bill that stayed where it was, and why, as a reason word
/// (`docs/ERROR_CODES.md`, `bill.move-failed`): one of
/// [`bill_files::MoveFailure::reason`], or `no-journal`, `no-folders`,
/// `other-inbox`, `too-long` or `outside`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stayed {
    /// The file that didn't move.
    pub file: PathBuf,
    /// Why.
    pub reason: &'static str,
}

/// Bills put in place, waiting for the book: the originals are all still
/// there.
#[derive(Debug)]
pub struct Pending {
    direction: Direction,
    placed: Vec<(Placed, String)>,
    /// The bills that stayed.
    pub stayed: Vec<Stayed>,
}

impl Pending {
    fn new(direction: Direction) -> Self {
        Self {
            direction,
            placed: Vec::new(),
            stayed: Vec::new(),
        }
    }

    /// Where each bill was put, by where it came from.
    pub fn placed(&self) -> impl Iterator<Item = &Placed> {
        self.placed.iter().map(|(placed, _)| placed)
    }
}

/// What the moves of one step did once the book was saved.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Moved {
    /// How many bills moved.
    pub moved: usize,
    /// The bills that stayed where they were.
    pub stayed: Vec<Stayed>,
    /// Old copies left behind: the bill moved and the book links its new
    /// place, but its old copy couldn't be removed.
    pub copies_left: Vec<PathBuf>,
    /// Bills an Undo unlinked that stay in the archive: no journal on this
    /// computer moved them (another computer, or "Undo anyway").
    pub left_in_archive: Vec<PathBuf>,
}

/// Moves bills on `disk`, noting each in `journal` first.
pub struct Mover<'a> {
    /// The files.
    pub disk: &'a dyn Disk,
    /// The journal of this computer.
    pub journal: &'a Journal,
}

impl Mover<'_> {
    /// Steps 1 to 4 of one move (`docs/RECEIPT_INBOX.md` §6), noted in the
    /// journal before anything is written: the bill at `from` must still
    /// be `expected`, if given.
    pub fn place(
        &self,
        pending: &mut Pending,
        from: &Path,
        to: &Path,
        expected: Option<Fingerprint>,
    ) -> bool {
        let fingerprint = match expected {
            Some(expected) => Ok(expected),
            None => self.disk.fingerprint(from).map_err(|error| {
                if error.kind() == io::ErrorKind::NotFound {
                    "not-found"
                } else {
                    "changed"
                }
            }),
        };
        let placed = fingerprint.and_then(|fingerprint| {
            let id = self
                .journal
                .open_move(pending.direction, from, to, fingerprint)
                .map_err(|_| "no-journal")?;
            bill_files::place(self.disk, from, to, Some(fingerprint))
                .map(|placed| (placed, id.clone()))
                .map_err(|failure| {
                    let _ = self.journal.close(&id);
                    failure.reason()
                })
        });
        match placed {
            Ok(placed) => {
                pending.placed.push(placed);
                true
            }
            Err(reason) => {
                pending.stayed.push(Stayed {
                    file: from.to_path_buf(),
                    reason,
                });
                false
            }
        }
    }

    /// Starts moves that go `direction`.
    pub fn start(direction: Direction) -> Pending {
        Pending::new(direction)
    }

    /// Steps 5 and 6, once the book saved the step as its history action
    /// `change`: notes it, then removes each original after checking the
    /// copy once more. An original that can't be removed is reported;
    /// the bill has moved all the same.
    pub fn finish(&self, pending: Pending, change: Option<&str>) -> Moved {
        let mut moved = Moved {
            stayed: pending.stayed,
            ..Moved::default()
        };
        for (placed, id) in pending.placed {
            if let Some(change) = change {
                let _ = self.journal.saved(&id, change);
            }
            moved.moved += 1;
            match bill_files::finish(self.disk, &placed) {
                Ok(()) if pending.direction == Direction::Back => {
                    bill_files::tidy_archive(self.disk, &placed.from);
                }
                Ok(()) => {}
                Err(_) => moved.copies_left.push(placed.from.clone()),
            }
            let _ = self.journal.close(&id);
        }
        moved
    }

    /// The book couldn't save: every placed copy is taken back, and only
    /// while its original is still there and the same. Returns the bills
    /// that had stayed.
    pub fn take_back(&self, pending: Pending) -> Vec<Stayed> {
        for (placed, id) in &pending.placed {
            bill_files::take_back(self.disk, placed);
            if pending.direction == Direction::File {
                bill_files::tidy_archive(self.disk, &placed.to);
            }
            let _ = self.journal.close(id);
        }
        pending.stayed
    }

    /// Before the book's Undo of the history action `change`: every bill
    /// it filed and that is filed still is put back into the inbox,
    /// under its old name or a free one. If any can't be, none moves and
    /// the bills that can't are returned, so the user decides ("Undo
    /// anyway", §7).
    pub fn prepare_undo(&self, change: &str) -> Result<Pending, Vec<Stayed>> {
        let mut pending = Pending::new(Direction::Back);
        for entry in self.journal.filed_by(change) {
            let archived = entry.archive_path();
            match self.free_place(entry.inbox_path()) {
                Some(to) => {
                    self.place(&mut pending, archived, &to, None);
                }
                None => pending.stayed.push(Stayed {
                    file: archived.to_path_buf(),
                    reason: "inbox-gone",
                }),
            }
        }
        self.all_or_nothing(pending)
    }

    /// Before the book's Redo of `change`: every bill its Undo put back is
    /// filed again at the archive path it had. If any can't be (gone from
    /// the inbox, or its archive name taken now), none moves.
    pub fn prepare_redo(&self, change: &str) -> Result<Pending, Vec<Stayed>> {
        let mut pending = Pending::new(Direction::File);
        for entry in self.journal.taken_back_by(change) {
            self.place(&mut pending, entry.inbox_path(), entry.archive_path(), None);
        }
        self.all_or_nothing(pending)
    }

    fn all_or_nothing(&self, pending: Pending) -> Result<Pending, Vec<Stayed>> {
        if pending.stayed.is_empty() {
            return Ok(pending);
        }
        let stayed = pending.stayed.clone();
        self.take_back(pending);
        Err(stayed)
    }

    /// `wanted`, or a free name in its folder if a file has it; `None` if
    /// its folder is gone.
    fn free_place(&self, wanted: &Path) -> Option<PathBuf> {
        let folder = wanted.parent()?;
        if !self.disk.is_folder(folder) {
            return None;
        }
        let names = self.disk.names_in(folder).ok()?;
        let name = wanted.file_name()?.to_str()?;
        let (stem, ending) = name.rsplit_once('.').unwrap_or((name, ""));
        Some(folder.join(free_file_name(
            stem,
            ending,
            names.iter().map(String::as_str),
        )))
    }

    /// Settles every move a crash left open, by whether `book` links its
    /// archive path. Nothing is settled while the book can't be read.
    pub fn settle(&self, book: &Book) -> Vec<Settled> {
        let Ok(links) = book.attachment_links() else {
            return Vec::new();
        };
        self.journal.settle(self.disk, &|path: &Path| {
            path.to_str().is_some_and(|path| links.contains(path))
        })
    }
}

/// How many bills each history action filed from the inbox, by its ID,
/// for the history's "Filed 3 bills from the inbox" (§7).
pub fn filed_counts(journal: &Journal) -> std::collections::HashMap<String, usize> {
    let mut filed: std::collections::HashMap<String, BTreeSet<PathBuf>> =
        std::collections::HashMap::new();
    for entry in journal.entries() {
        if let (Direction::File, Some(change)) = (entry.direction, &entry.change) {
            filed
                .entry(change.clone())
                .or_default()
                .insert(entry.to.clone());
        }
    }
    filed
        .into_iter()
        .map(|(change, bills)| (change, bills.len()))
        .collect()
}

/// Why an Undo or Redo with bills didn't happen.
#[derive(Debug)]
pub enum StepError<E> {
    /// A bill can't move, so nothing moved and the book is as it was: the
    /// user may undo anyway and leave the bills where they are (§7).
    Blocked(Vec<Stayed>),
    /// The book refused the step; every bill was taken back.
    Book(E),
}

/// Undoes or, with `redo`, redoes the book's next change with `step`, and
/// moves the bills it filed with it (`docs/RECEIPT_INBOX.md` §7): the
/// bills first, put in place without removing anything, then the book,
/// then the old copies. `anyway` leaves the bills where they are: the
/// deliberate way out when one can't move. Without a `journal` nothing
/// moves, as on another computer, and an Undo says which bills it unlinked
/// that stay in the `archive`.
pub fn step<T, E>(
    book: &mut Book,
    redo: bool,
    anyway: bool,
    journal: Option<&Journal>,
    disk: &dyn Disk,
    archive: Option<&Path>,
    step: impl FnOnce(&mut Book) -> Result<T, E>,
) -> Result<(T, Moved), StepError<E>> {
    let next = if redo {
        book.change_to_redo()
    } else {
        book.change_to_undo()
    };
    let change = next.ok().flatten().map(|change| change.id.to_string());
    let before = if redo {
        BTreeSet::new()
    } else {
        book.attachment_links().unwrap_or_default()
    };
    let mover = journal.map(|journal| Mover { disk, journal });
    let pending = match (&mover, &change) {
        (Some(mover), Some(change)) if !anyway => Some(
            if redo {
                mover.prepare_redo(change)
            } else {
                mover.prepare_undo(change)
            }
            .map_err(StepError::Blocked)?,
        ),
        _ => None,
    };
    let done = match step(book) {
        Ok(done) => done,
        Err(error) => {
            if let (Some(mover), Some(pending)) = (&mover, pending) {
                mover.take_back(pending);
            }
            return Err(StepError::Book(error));
        }
    };
    let moved_back: Vec<PathBuf> = pending
        .iter()
        .flat_map(Pending::placed)
        .map(|placed| placed.from.clone())
        .collect();
    let mut moved = match (&mover, pending) {
        (Some(mover), Some(pending)) => mover.finish(pending, change.as_deref()),
        _ => Moved::default(),
    };
    if !redo && let Some(archive) = archive {
        moved.left_in_archive = unlinked_in(book, &before, archive, &moved_back);
    }
    Ok((done, moved))
}

/// The links of `book` inside `archive` that `before` had and the book
/// doesn't any more, less the ones that moved: the bills an Undo unlinked
/// and left in the archive.
pub fn unlinked_in(
    book: &Book,
    before: &BTreeSet<String>,
    archive: &Path,
    moved: &[PathBuf],
) -> Vec<PathBuf> {
    let Ok(after) = book.attachment_links() else {
        return Vec::new();
    };
    before
        .difference(&after)
        .map(PathBuf::from)
        .filter(|link| link.starts_with(archive) && !moved.contains(link))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_folder::TestFolder;

    /// A rename that never replaces, without Qt, for the tests.
    fn rename(from: &Path, to: &Path) -> io::Result<()> {
        std::fs::hard_link(from, to)?;
        std::fs::remove_file(from)
    }

    fn real() -> RealDisk<fn(&Path, &Path) -> io::Result<()>> {
        RealDisk::new(rename)
    }

    #[test]
    fn the_unsorted_folder_is_english_until_the_window_names_it() {
        assert_eq!(unsorted_name(), UNSORTED_ENGLISH);
        set_unsorted_name("  Noch nicht sortiert ");
        assert_eq!(unsorted_name(), "Noch nicht sortiert");
        set_unsorted_name("");
        assert_eq!(unsorted_name(), UNSORTED_ENGLISH);
    }

    #[test]
    fn a_filed_bill_goes_back_on_undo_and_into_the_archive_again_on_redo() {
        let folder = TestFolder::new();
        let inbox = folder.join("Inbox");
        let archive = folder.join("Archive");
        std::fs::create_dir_all(&inbox).unwrap();
        std::fs::create_dir_all(&archive).unwrap();
        let bill = inbox.join("RE-0042.pdf");
        std::fs::write(&bill, b"%PDF synthetic bill").unwrap();
        let to = archive
            .join("Checking")
            .join("Groceries")
            .join("261105_RE-0042.pdf");
        let disk = real();
        let journal = Journal::at(folder.join(JOURNAL_FILE));
        let mover = Mover {
            disk: &disk,
            journal: &journal,
        };

        let mut pending = Mover::start(Direction::File);
        assert!(mover.place(&mut pending, &bill, &to, None));
        assert!(bill.exists() && to.exists(), "both until the book saved");
        let moved = mover.finish(pending, Some("change-1"));
        assert_eq!(moved.moved, 1);
        assert!(!bill.exists() && to.exists());

        // Undo: back under the old name, the empty folders tidied away.
        let back = mover.prepare_undo("change-1").expect("can move back");
        assert!(bill.exists() && to.exists(), "both until the book undid");
        let moved = mover.finish(back, Some("change-1"));
        assert_eq!((moved.moved, moved.copies_left.len()), (1, 0));
        assert!(bill.exists() && !to.exists());
        assert!(!archive.join("Checking").exists(), "empty folders go");
        assert!(archive.exists(), "the archive stays");

        // Redo: filed again at the same place.
        let again = mover.prepare_redo("change-1").expect("can file again");
        mover.finish(again, Some("change-1"));
        assert!(!bill.exists() && to.exists());
        assert_eq!(std::fs::read(&to).unwrap(), b"%PDF synthetic bill");

        // And undone once more: the journal follows the newest move.
        let back = mover.prepare_undo("change-1").expect("can move back");
        mover.finish(back, Some("change-1"));
        assert!(bill.exists() && !to.exists());
        // The history still says the change filed one bill.
        assert_eq!(filed_counts(&journal).get("change-1"), Some(&1));
    }

    #[test]
    fn undo_takes_a_free_name_when_the_old_one_is_taken_in_the_inbox() {
        let folder = TestFolder::new();
        let inbox = folder.join("Inbox");
        std::fs::create_dir_all(&inbox).unwrap();
        let bill = inbox.join("Bill.pdf");
        std::fs::write(&bill, b"first").unwrap();
        let to = folder.join("Archive").join("A").join("B").join("x.pdf");
        let disk = real();
        let journal = Journal::at(folder.join(JOURNAL_FILE));
        let mover = Mover {
            disk: &disk,
            journal: &journal,
        };
        let mut pending = Mover::start(Direction::File);
        mover.place(&mut pending, &bill, &to, None);
        mover.finish(pending, Some("c"));
        // The user drops another bill of that name into the inbox, by case.
        std::fs::write(inbox.join("BILL.PDF"), b"another").unwrap();
        let back = mover.prepare_undo("c").unwrap();
        mover.finish(back, Some("c"));
        assert_eq!(std::fs::read(inbox.join("Bill-2.pdf")).unwrap(), b"first");
        assert_eq!(std::fs::read(inbox.join("BILL.PDF")).unwrap(), b"another");
    }

    #[test]
    fn undo_and_redo_move_nothing_when_one_bill_cant_move() {
        let folder = TestFolder::new();
        let inbox = folder.join("Inbox");
        std::fs::create_dir_all(&inbox).unwrap();
        let archive = folder.join("Archive").join("A").join("B");
        let disk = real();
        let journal = Journal::at(folder.join(JOURNAL_FILE));
        let mover = Mover {
            disk: &disk,
            journal: &journal,
        };
        let mut pending = Mover::start(Direction::File);
        for name in ["one.pdf", "two.pdf"] {
            std::fs::write(inbox.join(name), name).unwrap();
            mover.place(&mut pending, &inbox.join(name), &archive.join(name), None);
        }
        mover.finish(pending, Some("c"));

        // The user deleted one bill in the archive: Undo moves neither.
        std::fs::remove_file(archive.join("two.pdf")).unwrap();
        let stayed = mover.prepare_undo("c").unwrap_err();
        assert_eq!(
            stayed,
            [Stayed {
                file: archive.join("two.pdf"),
                reason: "not-found"
            }]
        );
        assert!(archive.join("one.pdf").exists());
        assert!(
            !inbox.join("one.pdf").exists(),
            "the first copy was taken back"
        );

        // The inbox is gone: Undo can't put anything back.
        std::fs::write(archive.join("two.pdf"), "two.pdf").unwrap();
        std::fs::remove_dir(&inbox).unwrap();
        let stayed = mover.prepare_undo("c").unwrap_err();
        assert!(
            stayed.iter().all(|bill| bill.reason == "inbox-gone"),
            "{stayed:?}"
        );
        assert!(archive.join("one.pdf").exists() && archive.join("two.pdf").exists());

        // Redo refuses when the bill left the inbox after the Undo.
        std::fs::create_dir_all(&inbox).unwrap();
        mover.finish(mover.prepare_undo("c").unwrap(), Some("c"));
        std::fs::remove_file(inbox.join("one.pdf")).unwrap();
        let stayed = mover.prepare_redo("c").unwrap_err();
        assert_eq!(stayed[0].reason, "not-found");
        assert!(
            inbox.join("two.pdf").exists(),
            "the other stays in the inbox"
        );
        assert!(!archive.join("two.pdf").exists());
    }

    #[test]
    fn a_book_that_could_not_save_gets_every_copy_taken_back() {
        let folder = TestFolder::new();
        let inbox = folder.join("Inbox");
        std::fs::create_dir_all(&inbox).unwrap();
        let bill = inbox.join("b.pdf");
        std::fs::write(&bill, "b").unwrap();
        let to = folder.join("Archive").join("A").join("B").join("b.pdf");
        let disk = real();
        let journal = Journal::at(folder.join(JOURNAL_FILE));
        let mover = Mover {
            disk: &disk,
            journal: &journal,
        };
        let mut pending = Mover::start(Direction::File);
        mover.place(&mut pending, &bill, &to, None);
        // A changed bill stays (step 1).
        let changed = inbox.join("c.pdf");
        std::fs::write(&changed, "c").unwrap();
        let read_then = Fingerprint {
            size: 99,
            modified: None,
        };
        assert!(!mover.place(
            &mut pending,
            &changed,
            &to.with_file_name("c.pdf"),
            Some(read_then)
        ));
        let stayed = mover.take_back(pending);
        assert_eq!(stayed[0].reason, "changed");
        assert!(bill.exists() && !to.exists());
        assert!(!folder.join("Archive").join("A").exists());
        assert!(journal.entries().iter().all(|entry| entry.closed));
    }

    /// A book with one transaction, and the bill `link` linked to it by
    /// one change of the history.
    fn book_linking(link: &str) -> Book {
        use kassensturz_core::{
            Account, AccountId, AccountKind, AccountType, Attachment, CurrencyCode, Date, Money,
            Split, Transaction, TransactionId,
        };
        let eur = CurrencyCode::new("EUR").unwrap();
        let mut book = Book::create_in_memory().unwrap();
        book.use_currency(eur).unwrap();
        let account = |name: &str, kind, account_type| Account {
            id: AccountId::generate(),
            name: name.into(),
            kind,
            currency: eur,
            account_type,
            parent: None,
            closed: false,
            number: None,
            notes: None,
            institution: None,
        };
        let checking = account("Checking", AccountKind::Asset, Some(AccountType::Checking));
        let shop = account("Groceries", AccountKind::Expense, None);
        book.add_account(&checking).unwrap();
        book.add_account(&shop).unwrap();
        let paid = Transaction::new(
            TransactionId::generate(),
            Date::constant(2026, 11, 5),
            Some("Papierladen Nord".into()),
            vec![
                Split::new(checking.id, Money::from_minor_units(-2450, eur)),
                Split::new(shop.id, Money::from_minor_units(2450, eur)),
            ],
        )
        .unwrap();
        book.add_transaction(&paid).unwrap();
        let linked = paid.clone().with_attachments(vec![Attachment {
            link: link.into(),
            title: None,
        }]);
        book.update_transaction(&linked).unwrap();
        book
    }

    #[test]
    fn undo_moves_the_bill_back_and_redo_files_it_again_with_the_link() {
        let folder = TestFolder::new();
        let inbox = folder.join("Inbox");
        std::fs::create_dir_all(&inbox).unwrap();
        let archive = folder.join("Archive");
        let bill = inbox.join("RE-0042.pdf");
        std::fs::write(&bill, "bill").unwrap();
        let to = archive
            .join("Checking")
            .join("Groceries")
            .join("261105_RE-0042.pdf");
        let link = to.to_str().unwrap().to_owned();
        let mut book = book_linking(&link);
        let change = book.change_to_undo().unwrap().unwrap().id.to_string();
        let disk = real();
        let journal = Journal::at(folder.join(JOURNAL_FILE));
        let mover = Mover {
            disk: &disk,
            journal: &journal,
        };
        let mut pending = Mover::start(Direction::File);
        mover.place(&mut pending, &bill, &to, None);
        mover.finish(pending, Some(&change));

        let undo = |book: &mut Book| book.undo().map(|undone| undone.is_some());
        let redo = |book: &mut Book| book.redo().map(|redone| redone.is_some());
        let (done, moved) = step(
            &mut book,
            false,
            false,
            Some(&journal),
            &disk,
            Some(&archive),
            undo,
        )
        .map_err(|_| "blocked")
        .unwrap();
        assert!(done);
        assert_eq!((moved.moved, moved.left_in_archive.len()), (1, 0));
        assert!(bill.exists() && !to.exists());
        assert!(book.attachment_links().unwrap().is_empty());

        let (done, moved) = step(
            &mut book,
            true,
            false,
            Some(&journal),
            &disk,
            Some(&archive),
            redo,
        )
        .map_err(|_| "blocked")
        .unwrap();
        assert!(done);
        assert_eq!(moved.moved, 1);
        assert!(!bill.exists() && to.exists());
        assert!(book.attachment_links().unwrap().contains(&link));

        // The user moved the bill away in the archive: Undo is blocked and
        // the book stays as it is.
        let elsewhere = folder.join("elsewhere.pdf");
        std::fs::rename(&to, &elsewhere).unwrap();
        let blocked = step(
            &mut book,
            false,
            false,
            Some(&journal),
            &disk,
            Some(&archive),
            undo,
        );
        let Err(StepError::Blocked(stayed)) = blocked else {
            panic!("blocked");
        };
        assert_eq!(stayed[0].reason, "not-found");
        assert!(book.attachment_links().unwrap().contains(&link));

        // "Undo anyway": the link goes, nothing moves.
        std::fs::rename(&elsewhere, &to).unwrap();
        let (_, moved) = step(
            &mut book,
            false,
            true,
            Some(&journal),
            &disk,
            Some(&archive),
            undo,
        )
        .map_err(|_| "blocked")
        .unwrap();
        assert_eq!(moved.moved, 0);
        assert_eq!(moved.left_in_archive, std::slice::from_ref(&to));
        assert!(to.exists() && !bill.exists());
        assert!(book.attachment_links().unwrap().is_empty());
    }

    #[test]
    fn on_a_computer_without_the_journal_undo_unlinks_and_says_where_the_bill_stays() {
        let folder = TestFolder::new();
        let archive = folder.join("Archive");
        let to = archive
            .join("Checking")
            .join("Groceries")
            .join("261105_RE-0042.pdf");
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::write(&to, "bill").unwrap();
        let mut book = book_linking(to.to_str().unwrap());
        let disk = real();
        let undo = |book: &mut Book| book.undo().map(|undone| undone.is_some());
        let (_, moved) = step(&mut book, false, false, None, &disk, Some(&archive), undo)
            .map_err(|_| "blocked")
            .unwrap();
        assert_eq!(moved.left_in_archive, std::slice::from_ref(&to));
        assert!(to.exists());
    }

    #[test]
    fn deleting_the_transaction_leaves_the_bill_in_the_archive() {
        // §7: removing the transaction is no Undo of the filing, so its
        // bill stays where it was filed, and the journal moves nothing.
        let folder = TestFolder::new();
        let archive = folder.join("Archive");
        let to = archive.join("Checking").join("Groceries").join("x.pdf");
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::write(&to, "bill").unwrap();
        let mut book = book_linking(to.to_str().unwrap());
        let id = book.transactions().unwrap()[0].id();
        book.remove_transaction(id).unwrap();
        assert!(book.attachment_links().unwrap().is_empty());
        assert!(to.exists());
        let disk = real();
        let journal = Journal::at(folder.join(JOURNAL_FILE));
        let undo = |book: &mut Book| book.undo().map(|undone| undone.is_some());
        let (_, moved) = step(
            &mut book,
            false,
            false,
            Some(&journal),
            &disk,
            Some(&archive),
            undo,
        )
        .map_err(|_| "blocked")
        .unwrap();
        assert_eq!(moved.moved, 0);
        assert!(
            book.attachment_links()
                .unwrap()
                .contains(to.to_str().unwrap())
        );
    }

    #[test]
    fn settling_needs_the_book_and_keeps_the_last_copy() {
        let folder = TestFolder::new();
        let inbox = folder.join("Inbox");
        std::fs::create_dir_all(&inbox).unwrap();
        let bill = inbox.join("b.pdf");
        std::fs::write(&bill, "b").unwrap();
        let to = folder.join("Archive").join("A").join("B").join("b.pdf");
        let disk = real();
        let journal = Journal::at(folder.join(JOURNAL_FILE));
        let mover = Mover {
            disk: &disk,
            journal: &journal,
        };
        let mut pending = Mover::start(Direction::File);
        mover.place(&mut pending, &bill, &to, None);
        // The app stops here: no link in the book, both copies there.
        let _stopped_here = pending;
        let book = Book::create_in_memory().unwrap();
        assert!(mover.settle(&book).is_empty(), "nothing to tell");
        assert!(
            bill.exists() && !to.exists(),
            "the copy went, the original stays"
        );
    }
}
