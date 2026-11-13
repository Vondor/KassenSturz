// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Moving a real book between temporary folders (D-731,
//! `docs/BOOK_FOLDER.md` §7 step 2): success, each refusal, a failure and a
//! stop after each step, a writer while the move holds the lock, the check
//! of the copy, and the backups kept apart from the automatic ones.

use std::panic::{AssertUnwindSafe, catch_unwind};

use kassensturz_core::AccountKind;

use super::*;
use crate::test_folder::TempFolder;
use crate::tests::{EUR, account, date, eur, transaction};

/// Makes a move fail or stop at one [`Stage`].
pub(crate) mod hook {
    use std::cell::RefCell;
    use std::path::Path;

    use super::super::{MoveCause, Stage};
    use crate::StoreError;

    /// What happens when the move reaches the stage.
    pub(crate) enum Act {
        /// The step fails, as if the disk refused.
        Fail,
        /// The app stops there, as in a crash: nothing after it runs.
        Stop,
        /// Something else happens there, e.g. another program writes.
        Run(Box<dyn FnOnce(&Path)>),
    }

    thread_local! {
        static AT: RefCell<Option<(Stage, Act)>> = const { RefCell::new(None) };
    }

    /// The stop a test's move panics with.
    pub(crate) struct Stopped;

    pub(crate) fn set(stage: Stage, act: Act) {
        AT.with(|at| *at.borrow_mut() = Some((stage, act)));
    }

    pub(crate) fn reached(stage: Stage, path: &Path) -> Result<(), MoveCause> {
        let act = AT.with(|at| {
            let mut at = at.borrow_mut();
            match at.as_ref() {
                Some((wanted, _)) if *wanted == stage => at.take().map(|(_, act)| act),
                _ => None,
            }
        });
        match act {
            None => Ok(()),
            Some(Act::Fail) => Err(MoveCause::Store(StoreError::Io(
                std::io::ErrorKind::StorageFull,
            ))),
            Some(Act::Stop) => std::panic::panic_any(Stopped),
            Some(Act::Run(run)) => {
                run(path);
                Ok(())
            }
        }
    }
}

use hook::Act;

/// The app's settings, as far as a move uses them.
#[derive(Default)]
struct Settings {
    note: Option<MoveNote>,
    remembered: Option<PathBuf>,
    refuse_note: bool,
    refuse_remember: bool,
    renames: usize,
}

impl MoveSteps for Settings {
    fn in_progress(&self) -> bool {
        self.note.is_some()
    }

    fn note(&mut self, note: &MoveNote) -> bool {
        if self.refuse_note {
            return false;
        }
        self.note = Some(note.clone());
        true
    }

    fn clear_note(&mut self) {
        self.note = None;
    }

    /// A rename that never replaces a file: a hard link refuses an existing
    /// name, then the old name goes (the fallback D-730 names).
    fn rename_no_replace(&mut self, from: &Path, to: &Path) -> std::io::Result<()> {
        self.renames += 1;
        fs::hard_link(from, to)?;
        fs::remove_file(from)
    }

    fn remember(&mut self, book: &Path) -> bool {
        if self.refuse_remember {
            return false;
        }
        self.remembered = Some(book.to_owned());
        true
    }
}

struct Setup {
    folder: TempFolder,
    computer: Computer,
    from: PathBuf,
    target: PathBuf,
    checking: AccountId,
    opening: AccountId,
}

impl Setup {
    fn new() -> Self {
        let folder = TempFolder::new();
        let computer = Computer {
            name: "Laptop".into(),
            backup_folder: folder.join("backups"),
        };
        let own = folder.join("own");
        let target = folder.join("Sync");
        fs::create_dir(&own).expect("folder made");
        fs::create_dir(&target).expect("folder made");
        let from = own.join("Household.ksz");
        let mut book = Book::create(&from, &computer).expect("book created");
        book.use_currency(EUR).expect("currency used");
        let checking = account(&mut book, "Checking", AccountKind::Asset, EUR);
        let opening = account(&mut book, "Opening", AccountKind::Equity, EUR);
        let groceries = account(&mut book, "Groceries", AccountKind::Expense, EUR);
        book.add_transaction(&transaction(
            date(2026, 10, 1),
            "Start",
            &[(checking, eur(250_000)), (opening, eur(-250_000))],
        ))
        .expect("added");
        book.add_transaction(&transaction(
            date(2026, 10, 2),
            "Corner Shop",
            &[(groceries, eur(1_234)), (checking, eur(-1_234))],
        ))
        .expect("added");
        drop(book);
        Self {
            folder,
            computer,
            from,
            target,
            checking,
            opening,
        }
    }

    fn open(&self) -> Book {
        Book::open(&self.from, &self.computer).expect("book opened")
    }

    fn plan(&self) -> MovePlan {
        plan_move(&self.from, &self.target, "Household", None, false).expect("planned")
    }

    fn to(&self) -> PathBuf {
        self.target.join("Household.ksz")
    }

    fn settings(&self) -> Settings {
        Settings {
            remembered: Some(self.from.clone()),
            ..Settings::default()
        }
    }

    fn move_backups(&self) -> Vec<String> {
        names_in(&self.computer.backup_folder)
            .into_iter()
            .filter(|name| move_backup_stamp(name, "Household").is_some())
            .collect()
    }
}

fn names_in(folder: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

#[test]
fn a_book_moves_with_a_backup_and_nothing_left_behind() {
    let setup = Setup::new();
    let book = setup.open();
    let balance = book.balance(setup.checking).expect("balance");
    let plan = setup.plan();
    assert_eq!(plan.to, setup.to());
    assert_eq!(plan.size, fs::metadata(&setup.from).expect("size").len());

    let mut settings = setup.settings();
    let mut moved = book
        .move_to(&plan, &setup.computer, &mut settings)
        .expect("moved");

    assert_eq!(moved.to, setup.to());
    assert_eq!(moved.old_left, None);
    assert_eq!(settings.remembered.as_deref(), Some(setup.to().as_path()));
    assert_eq!(settings.note, None, "the note is cleared");
    // The old folder is empty: no book, no lock, no journal.
    assert_eq!(
        names_in(setup.from.parent().expect("folder")),
        Vec::<String>::new()
    );
    // The new folder holds the book and its lock, no copy.
    assert_eq!(
        names_in(&setup.target),
        ["Household.ksz", "Household.ksz.lock"]
    );
    // The same money, and the new book takes changes.
    assert_eq!(
        moved.book.balance(setup.checking).expect("balance"),
        balance
    );
    moved
        .book
        .add_transaction(&transaction(
            date(2026, 10, 3),
            "Bakery",
            &[(setup.opening, eur(-300)), (setup.checking, eur(300))],
        ))
        .expect("added to the moved book");
    // The backup is a book with the same balance, named as a move backup.
    assert_eq!(setup.move_backups().len(), 1);
    assert_eq!(
        moved.backup.parent(),
        Some(setup.computer.backup_folder.as_path())
    );
    let backup = Book::open_to_read(&moved.backup).expect("backup opens");
    assert_eq!(backup.balance(setup.checking).expect("balance"), balance);
    drop(moved);
    assert_eq!(names_in(&setup.target), ["Household.ksz"]);
}

#[test]
fn a_book_moves_back_the_same_way() {
    let setup = Setup::new();
    let mut settings = setup.settings();
    let moved = setup
        .open()
        .move_to(&setup.plan(), &setup.computer, &mut settings)
        .expect("moved");
    let own = setup.from.parent().expect("folder");
    let back = plan_move(&moved.to, own, "Household", None, false).expect("planned");
    let back = moved
        .book
        .move_to(&back, &setup.computer, &mut settings)
        .expect("moved back");
    assert_eq!(back.to, setup.from);
    assert_eq!(settings.remembered.as_deref(), Some(setup.from.as_path()));
    assert_eq!(names_in(&setup.target), Vec::<String>::new());
    assert_eq!(setup.move_backups().len(), 2);
    assert_eq!(
        back.book.balance(setup.checking).expect("balance"),
        eur(248_766)
    );
}

#[test]
fn a_new_name_is_taken_with_the_extension() {
    let setup = Setup::new();
    let plan =
        plan_move(&setup.from, &setup.target, " Our money.KSZ ", None, false).expect("planned");
    assert_eq!(plan.to, setup.target.join("Our money.ksz"));
    let moved = setup
        .open()
        .move_to(&plan, &setup.computer, &mut setup.settings())
        .expect("moved");
    assert_eq!(moved.to, plan.to);
}

#[test]
fn book_names_that_no_file_system_takes_everywhere_are_refused() {
    assert_eq!(book_file_name("Household"), Ok("Household.ksz".into()));
    assert_eq!(book_file_name("Household.ksz"), Ok("Household.ksz".into()));
    assert_eq!(
        book_file_name("Haushalt 2026"),
        Ok("Haushalt 2026.ksz".into())
    );
    assert_eq!(book_file_name("Ünser Geld"), Ok("Ünser Geld.ksz".into()));
    for name in [
        "",
        "   ",
        ".ksz",
        ".hidden",
        "dot.",
        "a/b",
        "a\\b",
        "C:",
        "a*",
        "a?",
        "a\"b",
        "<a>",
        "a|b",
        "line\nbreak",
        "..",
    ] {
        assert_eq!(book_file_name(name), Err(MoveRefused::BadName), "{name:?}");
    }
    assert_eq!(
        book_file_name(&"a".repeat(MAX_NAME_BYTES + 1)),
        Err(MoveRefused::BadName)
    );
}

#[test]
fn each_refusal_has_its_own_reason_and_writes_nothing() {
    let setup = Setup::new();
    let own = setup.from.parent().expect("folder").to_owned();
    let refused = |folder: &Path, name: &str| {
        plan_move(&setup.from, folder, name, None, false).expect_err("refused")
    };
    assert_eq!(refused(&setup.target, "a/b"), MoveRefused::BadName);
    assert_eq!(
        refused(&setup.folder.join("gone"), "Household"),
        MoveRefused::NoFolder
    );
    assert_eq!(refused(&own, "Household"), MoveRefused::SameFolder);
    // The same folder, reached another way.
    assert_eq!(
        refused(&setup.folder.join("own/../own"), "Other"),
        MoveRefused::SameFolder
    );
    assert_eq!(
        plan_move(&setup.from, &setup.target, "Household", None, true),
        Err(MoveRefused::InProgress)
    );

    for (file, reason) in [
        ("Household.ksz", MoveRefused::AlreadyExists),
        ("Household.ksz.lock", MoveRefused::AlreadyExists),
        ("Household.ksz-journal", MoveRefused::LeftoverFiles),
        ("Household.ksz-wal", MoveRefused::LeftoverFiles),
        ("Household.ksz-shm", MoveRefused::LeftoverFiles),
        ("Household.ksz.part", MoveRefused::LeftoverFiles),
    ] {
        let path = setup.target.join(file);
        fs::write(&path, "made up").expect("written");
        assert_eq!(refused(&setup.target, "Household"), reason, "{file}");
        assert_eq!(
            fs::read_to_string(&path).expect("kept"),
            "made up",
            "{file}"
        );
        fs::remove_file(&path).expect("removed");
    }
    assert_eq!(names_in(&setup.target), Vec::<String>::new());
}

#[test]
fn a_move_needs_twice_the_books_size_where_the_system_tells_the_free_space() {
    let setup = Setup::new();
    let size = fs::metadata(&setup.from).expect("size").len();
    assert_eq!(
        plan_move(
            &setup.from,
            &setup.target,
            "Household",
            Some(2 * size - 1),
            false
        ),
        Err(MoveRefused::NoSpace)
    );
    assert!(
        plan_move(
            &setup.from,
            &setup.target,
            "Household",
            Some(2 * size),
            false
        )
        .is_ok()
    );
    assert!(plan_move(&setup.from, &setup.target, "Household", None, false).is_ok());
    assert_eq!(names_in(&setup.target), Vec::<String>::new());
}

#[cfg(unix)]
#[test]
fn a_folder_that_cant_be_written_to_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let setup = Setup::new();
    fs::set_permissions(&setup.target, fs::Permissions::from_mode(0o555)).expect("read-only");
    // Someone who may write anywhere (root, e.g. in a container) can't test
    // this here.
    let writable = fs::write(setup.target.join("probe"), "").is_ok();
    let result = plan_move(&setup.from, &setup.target, "Household", None, false);
    fs::set_permissions(&setup.target, fs::Permissions::from_mode(0o755)).expect("writable");
    if writable {
        return;
    }
    assert_eq!(result, Err(MoveRefused::ReadOnly));
}

/// What a failed move must leave: the old book open, usable and still the
/// book opened last, nothing in the new folder, no note.
fn assert_nothing_changed(setup: &Setup, failed: Box<MoveFailed>, settings: &Settings) {
    let mut failed = *failed;
    assert_eq!(failed.left, None);
    assert_eq!(settings.note, None, "the note is cleared");
    assert_eq!(settings.remembered.as_deref(), Some(setup.from.as_path()));
    assert_eq!(names_in(&setup.target), Vec::<String>::new());
    assert!(lock_path(&setup.from).exists(), "the old book stays locked");
    // Still open, and it takes changes: the write lock has ended.
    failed
        .book
        .add_transaction(&transaction(
            date(2026, 10, 4),
            "Bakery",
            &[(setup.opening, eur(-300)), (setup.checking, eur(300))],
        ))
        .expect("added to the old book");
}

#[test]
fn a_failure_at_each_step_leaves_the_old_book_open_and_no_copy() {
    for (stage, step) in [
        (Stage::Locked, MoveStep::Lock),
        (Stage::BackedUp, MoveStep::Backup),
        (Stage::Copied, MoveStep::Copy),
        (Stage::Checked, MoveStep::Check),
        (Stage::Placed, MoveStep::Place),
        (Stage::Opened, MoveStep::Switch),
        (Stage::Remembered, MoveStep::Switch),
    ] {
        let setup = Setup::new();
        let mut settings = setup.settings();
        hook::set(stage, Act::Fail);
        let failed = setup
            .open()
            .move_to(&setup.plan(), &setup.computer, &mut settings)
            .expect_err("failed");
        assert_eq!(failed.step, step, "{stage:?}");
        assert_nothing_changed(&setup, failed, &settings);
    }
}

#[test]
fn settings_that_cant_be_written_stop_the_move_before_anything_changes() {
    let setup = Setup::new();
    let mut settings = Settings {
        refuse_note: true,
        ..setup.settings()
    };
    let failed = setup
        .open()
        .move_to(&setup.plan(), &setup.computer, &mut settings)
        .expect_err("failed");
    assert_eq!(failed.step, MoveStep::Lock);
    assert!(matches!(failed.cause, MoveCause::Settings));
    assert_eq!(setup.move_backups().len(), 0, "nothing was backed up");
    assert_nothing_changed(&setup, failed, &settings);

    // The new book can't be remembered: it is taken out again, so there are
    // never two working books.
    let setup = Setup::new();
    let mut settings = Settings {
        refuse_remember: true,
        ..setup.settings()
    };
    let failed = setup
        .open()
        .move_to(&setup.plan(), &setup.computer, &mut settings)
        .expect_err("failed");
    assert_eq!(failed.step, MoveStep::Switch);
    assert!(matches!(failed.cause, MoveCause::Settings));
    assert_nothing_changed(&setup, failed, &settings);
}

#[test]
fn the_old_file_that_cant_be_removed_is_named() {
    let setup = Setup::new();
    let mut settings = setup.settings();
    hook::set(Stage::Closed, Act::Fail);
    let moved = setup
        .open()
        .move_to(&setup.plan(), &setup.computer, &mut settings)
        .expect("moved");
    assert_eq!(moved.old_left.as_deref(), Some(setup.from.as_path()));
    assert!(setup.from.exists());
    assert!(!lock_path(&setup.from).exists(), "the old book is closed");
    assert_eq!(settings.note, None);
    assert_eq!(settings.remembered.as_deref(), Some(setup.to().as_path()));
}

/// Runs a move that stops at `stage` as in a crash, and returns the
/// settings as the next start finds them.
fn stopped_at(setup: &Setup, stage: Stage) -> Settings {
    let mut settings = setup.settings();
    hook::set(stage, Act::Stop);
    let plan = setup.plan();
    let book = setup.open();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let _ = book.move_to(&plan, &setup.computer, &mut settings);
    }));
    let stop = result.expect_err("stopped");
    assert!(stop.is::<hook::Stopped>(), "{stage:?}");
    settings
}

#[test]
fn a_stop_after_each_step_is_explained_at_the_next_start() {
    for (stage, copy_left) in [
        (Stage::Locked, false),
        (Stage::BackedUp, false),
        (Stage::Copied, false),
        (Stage::Checked, false),
        (Stage::Placed, true),
        (Stage::Opened, true),
    ] {
        let setup = Setup::new();
        let settings = stopped_at(&setup, stage);
        let note = settings.note.clone().expect("the note is still there");
        assert_eq!(note.from, setup.from);
        assert_eq!(note.to, setup.to());
        let interrupted = note.interrupted(settings.remembered.as_deref());
        let copy = copy_left.then(|| setup.to());
        assert_eq!(interrupted, Interrupted::BeforeSwitch { copy }, "{stage:?}");
        // The unfinished copy is gone; the old book is untouched.
        assert!(!with_suffix(&setup.to(), PART).exists(), "{stage:?}");
        assert!(setup.from.exists(), "{stage:?}");
        Book::open_to_read(&setup.from).expect("the old book opens");
        // An unused copy is removed on the user's yes.
        if let Interrupted::BeforeSwitch { copy: Some(copy) } = interrupted {
            remove_left_book(&copy, &setup.computer).expect("removed");
            assert_eq!(names_in(&setup.target), Vec::<String>::new(), "{stage:?}");
        }
    }

    for stage in [Stage::Remembered, Stage::Closed] {
        let setup = Setup::new();
        let settings = stopped_at(&setup, stage);
        let note = settings.note.clone().expect("the note is still there");
        assert!(note.backup.as_ref().is_some_and(|backup| backup.exists()));
        let interrupted = note.interrupted(settings.remembered.as_deref());
        assert_eq!(
            interrupted,
            Interrupted::AfterSwitch {
                old: Some(setup.from.clone())
            },
            "{stage:?}"
        );
        Book::open_to_read(&setup.to()).expect("the new book opens");
        remove_left_book(&setup.from, &setup.computer).expect("old file removed");
        assert_eq!(
            names_in(setup.from.parent().expect("folder")),
            Vec::<String>::new()
        );
    }
}

#[test]
fn the_old_book_is_never_offered_for_removal_without_a_new_one() {
    let setup = Setup::new();
    let note = MoveNote {
        from: setup.from.clone(),
        to: setup.to(),
        since: Timestamp::now(),
        backup: None,
    };
    // The settings name the new book, but it isn't there.
    assert_eq!(
        note.interrupted(Some(&setup.to())),
        Interrupted::BeforeSwitch { copy: None }
    );
    // Nor is a file that isn't a book.
    fs::write(setup.to(), "made up").expect("written");
    assert_eq!(
        note.interrupted(Some(&setup.to())),
        Interrupted::BeforeSwitch { copy: None }
    );
    assert_eq!(fs::read_to_string(setup.to()).expect("kept"), "made up");
}

#[test]
fn only_a_book_that_isnt_open_elsewhere_is_removed() {
    let setup = Setup::new();
    let text = setup.target.join("Notes.ksz");
    fs::write(&text, "made up").expect("written");
    assert!(matches!(
        remove_left_book(&text, &setup.computer),
        Err(StoreError::NotABook)
    ));
    assert!(text.exists());

    // Open on another computer.
    fs::write(
        lock_path(&setup.from),
        "computer=Desk PC\nsince=2026-11-13T03:00:00Z\ntoken=x\n",
    )
    .expect("written");
    assert!(matches!(
        remove_left_book(&setup.from, &setup.computer),
        Err(StoreError::Locked(_))
    ));
    // Open on this one, right now.
    fs::remove_file(lock_path(&setup.from)).expect("removed");
    let book = setup.open();
    assert!(remove_left_book(&setup.from, &setup.computer).is_err());
    drop(book);
    assert!(setup.from.exists());
    // Left locked by this computer in a stop: removed with its lock.
    fs::write(
        lock_path(&setup.from),
        "computer=Laptop\nsince=2026-11-13T03:00:00Z\ntoken=x\n",
    )
    .expect("written");
    remove_left_book(&setup.from, &setup.computer).expect("removed");
    assert!(!setup.from.exists());
    assert!(!lock_path(&setup.from).exists());
}

#[test]
fn another_writer_waits_while_the_move_holds_the_lock_and_a_reader_reads() {
    let setup = Setup::new();
    let from = setup.from.clone();
    let checking = setup.checking;
    hook::set(
        Stage::Copied,
        Act::Run(Box::new(move |_| {
            // Another program's write, e.g. a second KassenSturz that took
            // the lock file over: it is told the book is busy.
            let other = Connection::open(&from).expect("opened");
            other
                .busy_timeout(std::time::Duration::from_millis(50))
                .expect("timeout set");
            let written = other.execute("UPDATE accounts SET name = 'Changed'", []);
            assert_eq!(
                written.expect_err("busy").sqlite_error_code(),
                Some(rusqlite::ErrorCode::DatabaseBusy)
            );
            // The MCP server's read goes on (D-195).
            let reader = Book::open_to_read(&from).expect("opened to read");
            assert_eq!(reader.balance(checking).expect("read"), eur(248_766));
        })),
    );
    let moved = setup
        .open()
        .move_to(&setup.plan(), &setup.computer, &mut setup.settings())
        .expect("moved");
    let names: Vec<String> = moved
        .book
        .all_accounts()
        .expect("accounts")
        .into_iter()
        .map(|account| account.name)
        .collect();
    assert!(!names.iter().any(|name| name == "Changed"));
}

/// Changes the copy between step 2 and step 3, like a disk that returns
/// other data than it was given.
fn changed_copy(sql: &'static str) -> Box<MoveFailed> {
    let setup = Setup::new();
    hook::set(
        Stage::Copied,
        Act::Run(Box::new(move |part| {
            let copy = Connection::open(part).expect("copy opened");
            copy.execute_batch(sql).expect("changed");
        })),
    );
    let mut settings = setup.settings();
    let failed = setup
        .open()
        .move_to(&setup.plan(), &setup.computer, &mut settings)
        .expect_err("refused");
    assert_eq!(failed.step, MoveStep::Check);
    assert_eq!(failed.left, None);
    assert_eq!(names_in(&setup.target), Vec::<String>::new());
    failed
}

#[test]
fn the_check_catches_a_changed_balance() {
    let failed = changed_copy("UPDATE splits SET amount = amount + 1 WHERE amount = 1234");
    assert!(
        matches!(failed.cause, MoveCause::Differs(Difference::Balance(_))),
        "{:?}",
        failed.cause
    );
}

#[test]
fn the_check_catches_a_changed_row() {
    let failed = changed_copy("UPDATE transactions SET payee = 'Other' WHERE payee = 'Start'");
    assert!(
        matches!(&failed.cause, MoveCause::Differs(Difference::Table(table)) if table == "transactions"),
        "{:?}",
        failed.cause
    );
}

#[test]
fn the_check_catches_a_damaged_or_foreign_copy() {
    let failed = changed_copy("PRAGMA application_id = 1");
    assert!(
        matches!(failed.cause, MoveCause::Differs(Difference::NotABook)),
        "{:?}",
        failed.cause
    );
}

#[test]
fn the_rename_never_replaces_a_file_that_appeared_in_the_meantime() {
    let setup = Setup::new();
    let stranger = setup.to();
    hook::set(
        Stage::Checked,
        Act::Run(Box::new(move |_| {
            fs::write(&stranger, "another program's file").expect("written");
        })),
    );
    let mut settings = setup.settings();
    let failed = setup
        .open()
        .move_to(&setup.plan(), &setup.computer, &mut settings)
        .expect_err("refused");
    assert_eq!(failed.step, MoveStep::Place);
    assert!(matches!(
        failed.cause,
        MoveCause::Store(StoreError::Io(ErrorKind::AlreadyExists))
    ));
    assert_eq!(
        fs::read_to_string(setup.to()).expect("kept"),
        "another program's file"
    );
    assert_eq!(names_in(&setup.target), ["Household.ksz"]);
    assert_eq!(settings.remembered.as_deref(), Some(setup.from.as_path()));
}

#[test]
fn the_checks_run_again_before_the_move() {
    let setup = Setup::new();
    let plan = setup.plan();
    fs::write(setup.target.join("Household.ksz-journal"), "made up").expect("written");
    let mut settings = setup.settings();
    let failed = setup
        .open()
        .move_to(&plan, &setup.computer, &mut settings)
        .expect_err("refused");
    assert_eq!(failed.step, MoveStep::Checks);
    assert!(matches!(
        failed.cause,
        MoveCause::Refused(MoveRefused::LeftoverFiles)
    ));
    assert_eq!(settings.renames, 0);
    assert_eq!(names_in(&setup.target), ["Household.ksz-journal"]);
}

#[test]
fn the_last_three_move_backups_are_kept_apart_from_the_ten() {
    let setup = Setup::new();
    let automatic_backups = |at: &Path| {
        let book = Book::open(at, &setup.computer).expect("book opened");
        for _ in 0..12 {
            crate::backup::back_up(&book.conn, at, &setup.computer.backup_folder)
                .expect("backed up");
        }
    };
    // Twelve automatic backups: the last ten stay.
    automatic_backups(&setup.from);
    assert_eq!(
        names_in(&setup.computer.backup_folder).len(),
        crate::backup::KEEP
    );

    // Four moves, there and back: the last three move backups stay, and
    // every automatic one.
    let mut settings = setup.settings();
    let own = setup.from.parent().expect("folder").to_owned();
    let mut at = setup.from.clone();
    let mut backups = Vec::new();
    for round in 0..4 {
        let folder = if round % 2 == 0 { &setup.target } else { &own };
        let plan = plan_move(&at, folder, "Household", None, false).expect("planned");
        let moved = Book::open(&at, &setup.computer)
            .expect("book opened")
            .move_to(&plan, &setup.computer, &mut settings)
            .expect("moved");
        backups.push(moved.backup.clone());
        at = moved.to;
    }
    assert_eq!(setup.move_backups().len(), KEEP_MOVE_BACKUPS);
    assert!(!backups[0].exists(), "the oldest move backup is gone");
    assert!(backups[1..].iter().all(|backup| backup.exists()));
    assert_eq!(
        names_in(&setup.computer.backup_folder).len(),
        crate::backup::KEEP + KEEP_MOVE_BACKUPS
    );

    // Twelve more automatic backups leave the move backups alone.
    automatic_backups(&at);
    assert!(backups[1..].iter().all(|backup| backup.exists()));
    assert_eq!(
        names_in(&setup.computer.backup_folder).len(),
        crate::backup::KEEP + KEEP_MOVE_BACKUPS
    );
}

#[test]
fn move_backup_names_are_recognized_strictly() {
    let stamp = "2026-11-13 03-04-05 UTC";
    for (name, expected) in [
        (
            "Household 2026-11-13 03-04-05 UTC before moving.ksz",
            Some((stamp, 1)),
        ),
        (
            "Household 2026-11-13 03-04-05 UTC before moving 2.ksz",
            Some((stamp, 2)),
        ),
        // The automatic backups' names.
        ("Household 2026-11-13 03-04-05 UTC.ksz", None),
        ("Household 2026-11-13 03-04-05 UTC 2.ksz", None),
        // Another book's, and names no move gives.
        ("Garden 2026-11-13 03-04-05 UTC before moving.ksz", None),
        ("Household before moving 2026-11-13 03-04-05 UTC.ksz", None),
        (
            "Household 2026-11-13 03-04-05 UTC before moving 1.ksz",
            None,
        ),
        (
            "Household 2026-11-13 03-04-05 UTC before moving 02.ksz",
            None,
        ),
        (
            "Household 2026-11-13 03-04-05 UTC before moving.ksz.part",
            None,
        ),
        ("Household 2026-11-13 03:04:05 UTC before moving.ksz", None),
    ] {
        assert_eq!(move_backup_name(name, "Household"), expected, "{name}");
        assert_eq!(
            move_backup_stamp(name, "Household"),
            expected.map(|(stamp, _)| stamp)
        );
    }
}

#[test]
fn a_failed_move_says_nothing_about_where() {
    let setup = Setup::new();
    hook::set(Stage::Copied, Act::Fail);
    let failed = setup
        .open()
        .move_to(&setup.plan(), &setup.computer, &mut setup.settings())
        .expect_err("failed");
    let text = format!("{failed:?}");
    assert!(!text.contains("Sync") && !text.contains("own"), "{text}");
}
