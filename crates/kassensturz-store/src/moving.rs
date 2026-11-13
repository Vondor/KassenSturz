// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Moving a book into another folder, with a backup and a way back (D-731,
//! `docs/BOOK_FOLDER.md` §4).
//!
//! Only moving is offered, never a second working copy. [`plan_move`]
//! checks the folder before anything is written, and [`Book::move_to`] runs
//! the design's steps 0 to 6 while the old book holds SQLite's write lock:
//! a "move in progress" note, a backup kept apart from the ten automatic
//! ones, a copy to `<name>.ksz.part`, a check of the copy against the open
//! book (layout, every table's rows, every account's balance), a rename that
//! never replaces a file, the switch to the new book and the removal of the
//! old file. If a step fails, the old book stays open and nothing is left
//! but the backup. If the app stops in the middle, [`MoveNote::interrupted`]
//! tells at the next start what happened.
//!
//! What the platform does (the settings file, a rename that never replaces a
//! file) comes in through [`MoveSteps`], so this module needs neither Qt nor
//! the settings. Nothing here writes a path into a log.

use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use kassensturz_core::{AccountId, Timestamp};
use rusqlite::types::Value;
use rusqlite::{Connection, OpenFlags};

use crate::lock::{self, lock_path};
use crate::reader::BUSY_WAIT;
use crate::{Book, Computer, StoreError, is_book_file};

/// How many "before moving" backups of each book are kept, apart from the
/// ten automatic ones (D-731).
pub const KEEP_MOVE_BACKUPS: usize = 3;

/// The extension of a book (D-037).
const EXTENSION: &str = ".ksz";

/// What the copy is called until it is checked: a name the common sync tools
/// leave out, so a half copy never travels (D-731).
const PART: &str = ".part";

/// What follows the stamp in a "before moving" backup's name.
const BEFORE_MOVING: &str = " before moving";

/// When a backup was made, in UTC, as `backup.rs` writes it: without colons,
/// which Windows doesn't allow in names.
const STAMP_FORMAT: &str = "%Y-%m-%d %H-%M-%S UTC";

/// What [`STAMP_FORMAT`] gives, with `0` for each digit.
const STAMP_PATTERN: &str = "0000-00-00 00-00-00 UTC";

/// The most "before moving" backups of one book within one second that get a
/// name of their own.
const MAX_PER_SECOND: u32 = 100;

/// The most bytes a book's name may have, so the name with `.ksz.lock` stays
/// within what every file system allows.
const MAX_NAME_BYTES: usize = 200;

/// Why a book isn't moved into a folder, found before anything is written
/// (D-731). Each has its own code (D-034).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveRefused {
    /// The name is empty, or has a character no file name may have.
    BadName,
    /// The folder isn't there (any more).
    NoFolder,
    /// The book is in that folder already.
    SameFolder,
    /// A file with the book's name, or its lock file, is there already;
    /// nothing is ever replaced (D-113).
    AlreadyExists,
    /// A leftover journal, WAL or SHM file, or an unfinished copy, with the
    /// book's name is there: SQLite would take a leftover journal as the new
    /// book's own and "repair" it with another file's data.
    LeftoverFiles,
    /// A move is still noted as in progress.
    InProgress,
    /// The folder can't be written to.
    ReadOnly,
    /// The free space, where the system tells it, is less than twice the
    /// book's size.
    NoSpace,
}

impl MoveRefused {
    /// The code the app's message is found by (D-034).
    pub fn code(self) -> &'static str {
        match self {
            Self::BadName => "book-move.bad-name",
            Self::NoFolder => "book-move.no-folder",
            Self::SameFolder => "book-move.same-folder",
            Self::AlreadyExists => "book-move.already-exists",
            Self::LeftoverFiles => "book-move.leftover-files",
            Self::InProgress => "book-move.in-progress",
            Self::ReadOnly => "book-move.read-only",
            Self::NoSpace => "book-move.no-space",
        }
    }
}

/// A move [`plan_move`] found nothing against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MovePlan {
    /// Where the book is now.
    pub from: PathBuf,
    /// Where it goes: `<folder>/<name>.ksz`.
    pub to: PathBuf,
    /// The book's size in bytes, for the page that asks.
    pub size: u64,
}

/// The step of a move that failed (D-731).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveStep {
    /// The checks before the move found something against it since they
    /// were last made.
    Checks,
    /// Step 0: the "move in progress" note couldn't be written, or the book
    /// couldn't take its write lock.
    Lock,
    /// Step 1: the backup before moving.
    Backup,
    /// Step 2: the copy into the new folder.
    Copy,
    /// Step 3: the copy isn't the same as the book.
    Check,
    /// Step 4: the copy couldn't be put in place.
    Place,
    /// Step 5: the new book couldn't be opened or remembered.
    Switch,
}

impl MoveStep {
    /// The code the app's message is found by (D-034).
    pub fn code(self) -> &'static str {
        match self {
            Self::Checks => "book-move.failed-checks",
            Self::Lock => "book-move.failed-lock",
            Self::Backup => "book-move.failed-backup",
            Self::Copy => "book-move.failed-copy",
            Self::Check => "book-move.failed-check",
            Self::Place => "book-move.failed-place",
            Self::Switch => "book-move.failed-switch",
        }
    }
}

/// What the check of the copy found different from the open book (step 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Difference {
    /// The copy doesn't begin like a book.
    NotABook,
    /// SQLite's integrity check found damage in the copy.
    Damaged,
    /// The copy has another layout version.
    Layout,
    /// The copy's tables, indexes or triggers differ.
    Schema,
    /// The copy has other accounts.
    Accounts,
    /// The account has another balance in the copy.
    Balance(AccountId),
    /// The table's rows differ.
    Table(String),
}

/// Why a step of a move failed.
#[derive(Debug)]
pub enum MoveCause {
    /// Reading or writing failed.
    Store(StoreError),
    /// The checks before the move refused it.
    Refused(MoveRefused),
    /// The copy isn't the same as the book.
    Differs(Difference),
    /// The platform couldn't write the settings.
    Settings,
}

impl From<StoreError> for MoveCause {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<std::io::Error> for MoveCause {
    fn from(error: std::io::Error) -> Self {
        Self::Store(error.into())
    }
}

impl From<rusqlite::Error> for MoveCause {
    fn from(error: rusqlite::Error) -> Self {
        Self::Store(error.into())
    }
}

/// A move that failed: the old book, still open and still the book opened
/// last, and what failed.
pub struct MoveFailed {
    /// The book, open as before.
    pub book: Book,
    /// The step that failed.
    pub step: MoveStep,
    /// Why.
    pub cause: MoveCause,
    /// A file of the move that couldn't be removed, if one was left: the
    /// unfinished copy, or the copy put in place before the switch failed.
    pub left: Option<PathBuf>,
}

impl std::fmt::Debug for MoveFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Without the paths: they may hold the user's name (D-730).
        f.debug_struct("MoveFailed")
            .field("step", &self.step)
            .field("cause", &self.cause)
            .field("left", &self.left.is_some())
            .finish_non_exhaustive()
    }
}

/// A finished move.
pub struct Moved {
    /// The book in its new folder, open.
    pub book: Book,
    /// Where it is now.
    pub to: PathBuf,
    /// The backup made before moving (step 1).
    pub backup: PathBuf,
    /// The old file, if removing it failed (step 6), e.g. in a read-only
    /// folder: it is still there.
    pub old_left: Option<PathBuf>,
}

impl std::fmt::Debug for Moved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Without the paths: they may hold the user's name (D-730).
        f.debug_struct("Moved")
            .field("old_left", &self.old_left.is_some())
            .finish_non_exhaustive()
    }
}

/// The "move in progress" note in the app's settings (D-731): written before
/// anything else, cleared when the move has ended either way. If the app
/// stops in the middle, it is still there at the next start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveNote {
    /// Where the book was.
    pub from: PathBuf,
    /// Where it was going.
    pub to: PathBuf,
    /// When the move began.
    pub since: Timestamp,
    /// The backup before moving, once it is made.
    pub backup: Option<PathBuf>,
}

/// What a move that was cut short left, as the next start finds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Interrupted {
    /// It stopped before the new book became the book opened last: the old
    /// book is the book. The app remembers `from` again.
    BeforeSwitch {
        /// A `<name>.ksz` in the new folder, an unused copy, offered for
        /// removal.
        copy: Option<PathBuf>,
    },
    /// It stopped after the new book became the book opened last.
    AfterSwitch {
        /// The old file, if it is still there, offered for removal, with
        /// the backup the note names.
        old: Option<PathBuf>,
    },
}

/// What a move needs the platform for.
pub trait MoveSteps {
    /// Whether a "move in progress" note is there already.
    fn in_progress(&self) -> bool;

    /// Writes the "move in progress" note, or writes it again with more to
    /// say. Returns whether it was saved.
    fn note(&mut self, note: &MoveNote) -> bool;

    /// Clears the note.
    fn clear_note(&mut self);

    /// Renames `from` to `to` in the same folder, failing with
    /// `AlreadyExists` instead of ever replacing a file at `to` (D-731, step
    /// 0 item 6 of D-730).
    fn rename_no_replace(&mut self, from: &Path, to: &Path) -> std::io::Result<()>;

    /// Remembers `book` as the book opened last. Returns whether that was
    /// saved.
    fn remember(&mut self, book: &Path) -> bool;
}

/// The file name for a book called `name`, with `.ksz` added unless it's
/// there, or [`MoveRefused::BadName`] for a name no file system takes
/// everywhere (Windows' too, so a book in a sync folder can travel).
pub fn book_file_name(name: &str) -> Result<String, MoveRefused> {
    let name = name.trim();
    let stem = match name.len().checked_sub(EXTENSION.len()) {
        Some(at) if name.is_char_boundary(at) && name[at..].eq_ignore_ascii_case(EXTENSION) => {
            name[..at].trim_end()
        }
        _ => name,
    };
    let refused = stem.is_empty()
        || stem.len() > MAX_NAME_BYTES
        || stem.starts_with('.')
        || stem.ends_with('.')
        || stem.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        });
    if refused {
        return Err(MoveRefused::BadName);
    }
    Ok(format!("{stem}{EXTENSION}"))
}

/// Checks a move of the book at `from` into `folder` under the name `name`
/// before anything is written (D-731): the folder is there and isn't the
/// book's, nothing with the book's name or its side files is there, the
/// folder can be written to, and `free_space`, the bytes free where the
/// system tells it, is at least twice the book's size. `in_progress` says
/// whether a move is still noted as in progress.
///
/// The write test creates and removes `<name>.ksz.part`; nothing else is
/// written.
pub fn plan_move(
    from: &Path,
    folder: &Path,
    name: &str,
    free_space: Option<u64>,
    in_progress: bool,
) -> Result<MovePlan, MoveRefused> {
    if in_progress {
        return Err(MoveRefused::InProgress);
    }
    let file_name = book_file_name(name)?;
    let folder = std::path::absolute(folder).map_err(|_| MoveRefused::NoFolder)?;
    if !fs::metadata(&folder).is_ok_and(|metadata| metadata.is_dir()) {
        return Err(MoveRefused::NoFolder);
    }
    let from = std::path::absolute(from).map_err(|_| MoveRefused::NoFolder)?;
    let same = from
        .parent()
        .and_then(|parent| fs::canonicalize(parent).ok())
        .is_some_and(|parent| fs::canonicalize(&folder).is_ok_and(|folder| folder == parent));
    if same {
        return Err(MoveRefused::SameFolder);
    }
    let to = folder.join(&file_name);
    if exists(&to) || exists(&lock_path(&to)) {
        return Err(MoveRefused::AlreadyExists);
    }
    let leftovers = ["-journal", "-wal", "-shm", PART];
    if leftovers
        .iter()
        .any(|suffix| exists(&with_suffix(&to, suffix)))
    {
        return Err(MoveRefused::LeftoverFiles);
    }
    let size = fs::metadata(&from)
        .map_err(|_| MoveRefused::NoFolder)?
        .len();
    if let Some(free) = free_space
        && size.checked_mul(2).is_none_or(|needed| free < needed)
    {
        return Err(MoveRefused::NoSpace);
    }
    // Whether the folder takes a new file, as the copy will be one.
    let part = with_suffix(&to, PART);
    match OpenOptions::new().write(true).create_new(true).open(&part) {
        Ok(file) => {
            drop(file);
            // The probe is ours; if it can't be removed, the move's own copy
            // can't be made under this name either.
            fs::remove_file(&part).map_err(|_| MoveRefused::ReadOnly)?;
        }
        Err(error) => {
            return Err(match error.kind() {
                ErrorKind::AlreadyExists => MoveRefused::LeftoverFiles,
                ErrorKind::NotFound => MoveRefused::NoFolder,
                ErrorKind::StorageFull | ErrorKind::QuotaExceeded => MoveRefused::NoSpace,
                _ => MoveRefused::ReadOnly,
            });
        }
    }
    Ok(MovePlan { from, to, size })
}

impl Book {
    /// Moves this book, which is open at `plan.from`, to `plan.to` (D-731):
    ///
    /// 0. notes the move in the settings and takes SQLite's write lock,
    ///    which it holds until step 5, so no change can land in the book
    ///    after the copy is taken;
    /// 1. backs the book up into `computer`'s backup folder, as one of the
    ///    last [`KEEP_MOVE_BACKUPS`] "before moving" backups;
    /// 2. copies it to `<name>.ksz.part` in the new folder;
    /// 3. checks the copy against the book: KassenSturz's application ID,
    ///    SQLite's integrity check, the layout version, the tables, indexes
    ///    and triggers, every account's balance and every table's rows;
    /// 4. renames the copy to `<name>.ksz` without ever replacing a file;
    /// 5. opens the new book, with its own lock, and remembers it as the
    ///    book opened last; only then the old book is closed;
    /// 6. removes the old file and clears the note.
    ///
    /// If a step fails, the unfinished copy is removed, the book stays open
    /// and stays the book opened last, the note is cleared, and the book
    /// comes back in [`MoveFailed`]. The checks of [`plan_move`] run again
    /// first, apart from the free space, since the folder may have changed
    /// since the user was asked.
    pub fn move_to(
        self,
        plan: &MovePlan,
        computer: &Computer,
        steps: &mut dyn MoveSteps,
    ) -> Result<Moved, Box<MoveFailed>> {
        let fail = |book: Book, step, cause, left| {
            Box::new(MoveFailed {
                book,
                step,
                cause,
                left,
            })
        };
        let (Some(folder), Some(name)) = (plan.to.parent(), plan.to.file_name()) else {
            return Err(fail(
                self,
                MoveStep::Checks,
                MoveCause::Refused(MoveRefused::BadName),
                None,
            ));
        };
        if let Err(refused) = plan_move(
            &plan.from,
            folder,
            &name.to_string_lossy(),
            None,
            steps.in_progress(),
        ) {
            return Err(fail(
                self,
                MoveStep::Checks,
                MoveCause::Refused(refused),
                None,
            ));
        }

        // Step 0.
        if !self.conn.is_autocommit() {
            let cause = MoveCause::Store(StoreError::Io(ErrorKind::ResourceBusy));
            return Err(fail(self, MoveStep::Lock, cause, None));
        }
        let mut note = MoveNote {
            from: plan.from.clone(),
            to: plan.to.clone(),
            since: Timestamp::now(),
            backup: None,
        };
        if !steps.note(&note) {
            // What part of it was saved goes too.
            steps.clear_note();
            return Err(fail(self, MoveStep::Lock, MoveCause::Settings, None));
        }
        if let Err(error) = self
            .conn
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(MoveCause::from)
            .and_then(|()| checkpoint(Stage::Locked, &plan.to))
        {
            let _ = self.end_lock();
            steps.clear_note();
            return Err(fail(self, MoveStep::Lock, error, None));
        }

        // Steps 1 to 4.
        let backup = match self.copy_into_place(plan, computer, steps, &mut note) {
            Ok(backup) => backup,
            Err(failure) => {
                let left = failure.cleanup(plan, steps);
                let _ = self.end_lock();
                steps.clear_note();
                return Err(fail(self, failure.step, failure.cause, left));
            }
        };

        // Step 5.
        let new = match switch_to(plan, computer, steps) {
            Ok(new) => new,
            Err(cause) => {
                // The new book is closed again by now; it must not stay as a
                // second working book.
                let left = unplace(plan, steps);
                // The old book is remembered again, in case the new one was.
                steps.remember(&plan.from);
                let _ = self.end_lock();
                steps.clear_note();
                return Err(fail(self, MoveStep::Switch, cause, left));
            }
        };
        // The old book's write lock ends, then the book is closed, which
        // removes its lock file.
        let _ = self.end_lock();
        drop(self);

        // Step 6. The backup from step 1 is the same book, so nothing is lost.
        let old_left = match checkpoint(Stage::Closed, &plan.from)
            .and_then(|()| fs::remove_file(&plan.from).map_err(MoveCause::from))
        {
            Ok(()) => None,
            Err(_) => Some(plan.from.clone()),
        };
        steps.clear_note();
        Ok(Moved {
            book: new,
            to: plan.to.clone(),
            backup,
            old_left,
        })
    }

    /// Ends the transaction step 0 began. Nothing was written in it, so it
    /// is rolled back.
    fn end_lock(&self) -> Result<(), StoreError> {
        if self.conn.is_autocommit() {
            return Ok(());
        }
        self.conn.execute_batch("ROLLBACK")?;
        Ok(())
    }

    /// Steps 1 to 4 of [`Book::move_to`], while this book holds the write
    /// lock. Returns the backup's path.
    fn copy_into_place(
        &self,
        plan: &MovePlan,
        computer: &Computer,
        steps: &mut dyn MoveSteps,
        note: &mut MoveNote,
    ) -> Result<PathBuf, StepFailed> {
        // A second connection reads the book for the copies: SQLite makes
        // them only outside a transaction, and this book's connection holds
        // the write lock in one. Reading beside a write lock is allowed in
        // the rollback journal (D-033).
        let reader = open_reader(&plan.from).map_err(StepFailed::at(MoveStep::Backup, false))?;

        // Step 1.
        let backup = back_up_before_moving(&reader, &plan.from, &computer.backup_folder)
            .map_err(MoveCause::from)
            .and_then(|backup| checkpoint(Stage::BackedUp, &backup).map(|()| backup))
            .map_err(StepFailed::at(MoveStep::Backup, false))?;
        note.backup = Some(backup.clone());
        // The note already says enough to explain a stop; this only adds
        // the backup's name.
        steps.note(note);

        // Step 2.
        let part = with_suffix(&plan.to, PART);
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&part)
            .map_err(|error| StepFailed {
                step: MoveStep::Copy,
                cause: error.into(),
                // Not ours: never removed.
                part_is_ours: false,
                placed: false,
            })?;
        copy_into(&reader, &part)
            .map_err(MoveCause::from)
            .and_then(|()| checkpoint(Stage::Copied, &part))
            .map_err(StepFailed::at(MoveStep::Copy, true))?;

        // Step 3.
        check_copy(&reader, self, &part)
            .and_then(|()| checkpoint(Stage::Checked, &part))
            .map_err(StepFailed::at(MoveStep::Check, true))?;
        drop(reader);

        // Step 4.
        steps
            .rename_no_replace(&part, &plan.to)
            .map_err(MoveCause::from)
            .map_err(StepFailed::at(MoveStep::Place, true))?;
        sync_folder(&plan.to)
            .map_err(MoveCause::from)
            .and_then(|()| checkpoint(Stage::Placed, &plan.to))
            .map_err(|cause| StepFailed {
                step: MoveStep::Place,
                cause,
                part_is_ours: true,
                placed: true,
            })?;
        Ok(backup)
    }
}

/// Step 5 of [`Book::move_to`]: opens the new book with its own lock and
/// remembers it as the book opened last. On an error the new book is closed
/// again, which removes its lock file.
fn switch_to(
    plan: &MovePlan,
    computer: &Computer,
    steps: &mut dyn MoveSteps,
) -> Result<Book, MoveCause> {
    let new = Book::open(&plan.to, computer)?;
    checkpoint(Stage::Opened, &plan.to)?;
    if !steps.remember(&plan.to) {
        return Err(MoveCause::Settings);
    }
    checkpoint(Stage::Remembered, &plan.to)?;
    Ok(new)
}

/// A failed step among 1 to 4, with what it left.
struct StepFailed {
    step: MoveStep,
    cause: MoveCause,
    /// Whether the `.part` file was claimed by this move.
    part_is_ours: bool,
    /// Whether the copy was renamed to the book's name already.
    placed: bool,
}

impl StepFailed {
    fn at(step: MoveStep, part_is_ours: bool) -> impl FnOnce(MoveCause) -> Self {
        move |cause| Self {
            step,
            cause,
            part_is_ours,
            placed: false,
        }
    }

    /// Removes what the move made in the new folder. Returns a file that
    /// couldn't be removed.
    fn cleanup(&self, plan: &MovePlan, steps: &mut dyn MoveSteps) -> Option<PathBuf> {
        if self.placed {
            return unplace(plan, steps);
        }
        let part = with_suffix(&plan.to, PART);
        if self.part_is_ours && fs::remove_file(&part).is_err() && exists(&part) {
            return Some(part);
        }
        None
    }
}

/// Takes the copy put in place at `plan.to` back out: renamed back to
/// `.part`, then removed, so there are never two working books. Returns a
/// file that couldn't be removed.
fn unplace(plan: &MovePlan, steps: &mut dyn MoveSteps) -> Option<PathBuf> {
    let part = with_suffix(&plan.to, PART);
    let gone = match steps.rename_no_replace(&plan.to, &part) {
        Ok(()) => fs::remove_file(&part).is_ok() || !exists(&part),
        Err(_) => false,
    };
    if gone {
        return None;
    }
    // The rename back failed: the copy is removed under its book name.
    if fs::remove_file(&plan.to).is_ok() || !exists(&plan.to) {
        return (exists(&part)).then_some(part);
    }
    Some(plan.to.clone())
}

impl MoveNote {
    /// What the move this note is about left, read at the next start, before
    /// any book is opened (D-731). `remembered` is the book opened last as
    /// the settings say. Removes the unfinished copy `<name>.ksz.part`, the
    /// only file that is ever removed without the user's yes.
    pub fn interrupted(&self, remembered: Option<&Path>) -> Interrupted {
        let part = with_suffix(&self.to, PART);
        if fs::symlink_metadata(&part).is_ok_and(|metadata| metadata.is_file()) {
            let _ = fs::remove_file(&part);
        }
        // The new book counts only if it is there and is a book: the old
        // one is never offered for removal otherwise.
        let new_is_book = is_book_file(&self.to).unwrap_or(false);
        if remembered == Some(self.to.as_path()) && new_is_book {
            Interrupted::AfterSwitch {
                old: exists(&self.from).then(|| self.from.clone()),
            }
        } else {
            Interrupted::BeforeSwitch {
                copy: new_is_book.then(|| self.to.clone()),
            }
        }
    }
}

/// Removes the book at `path` that a move left, once the user said yes
/// (D-731): the unused copy, or the old file. Only a book is removed, and
/// only if it isn't open: a lock file this computer, `computer`, left when
/// the app stopped is removed with it; the lock of a book this program has
/// open, or another computer's, refuses with [`StoreError::Locked`]. (A
/// second KassenSturz on this computer with the book open can't be told
/// apart from a lock left behind; D-038's lock file says only the computer.)
pub fn remove_left_book(path: &Path, computer: &Computer) -> Result<(), StoreError> {
    if !is_book_file(path)? {
        return Err(StoreError::NotABook);
    }
    for leftover in ["-journal", "-wal", "-shm"] {
        if exists(&with_suffix(path, leftover)) {
            // A journal means the book may need it to be whole; it stays.
            return Err(StoreError::Io(ErrorKind::ResourceBusy));
        }
    }
    let lock_file = lock_path(path);
    if let Some(found) = lock::read(&lock_file)?
        && (lock::held_here(&found) || found.computer.as_deref() != Some(computer.name.as_str()))
    {
        return Err(StoreError::Locked(found));
    }
    fs::remove_file(path)?;
    match fs::remove_file(&lock_file) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// The stamp of `name` if it is the name of a "before moving" backup of the
/// book `stem` (`<stem> <stamp> before moving.ksz`, or `… before moving
/// 2.ksz` and so on), for the list of backups (#806). The ten automatic
/// backups' names never match, nor theirs this one.
pub fn move_backup_stamp<'a>(name: &'a str, stem: &str) -> Option<&'a str> {
    move_backup_name(name, stem).map(|(stamp, _)| stamp)
}

/// The "before moving" backups of the book at `book` in `folder`, newest
/// first, for the list to restore from (#806). They are listed apart from
/// [`crate::backups_of`], which lists the automatic ones and which the
/// copies in a chosen folder are counted by, so a move never changes
/// which copies a chosen folder keeps.
///
/// # Errors
///
/// [`StoreError::Io`] if the folder can't be read, with
/// [`ErrorKind::NotFound`] if it isn't there.
pub fn move_backups_of(book: &Path, folder: &Path) -> Result<Vec<crate::BackupFile>, StoreError> {
    let stem = crate::backup::stem_of(book)?;
    let mut found: Vec<(Timestamp, u32, PathBuf)> = fs::read_dir(folder)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let (stamp, number) = move_backup_name(&name, stem)?;
            let made = crate::backup::stamp_time(stamp)?;
            // A folder or a link with a backup's name isn't one.
            let is_file = entry.file_type().ok()?.is_file();
            is_file.then(|| (made, number, entry.path()))
        })
        .collect();
    found.sort_by_key(|entry| std::cmp::Reverse((entry.0, entry.1)));
    Ok(found
        .into_iter()
        .map(|(made, _, path)| crate::BackupFile { path, made })
        .collect())
}

/// The stamp and number of a "before moving" backup's name; the number is 1
/// if the name has none.
fn move_backup_name<'a>(name: &'a str, stem: &str) -> Option<(&'a str, u32)> {
    let rest = name
        .strip_prefix(stem)?
        .strip_prefix(' ')?
        .strip_suffix(EXTENSION)?;
    let stamp = rest.get(..STAMP_PATTERN.len())?;
    let is_stamp = stamp
        .bytes()
        .zip(STAMP_PATTERN.bytes())
        .all(|(byte, pattern)| match pattern {
            b'0' => byte.is_ascii_digit(),
            _ => byte == pattern,
        });
    if !is_stamp {
        return None;
    }
    let number = match rest[STAMP_PATTERN.len()..].strip_prefix(BEFORE_MOVING)? {
        "" => 1,
        suffix => {
            let digits = suffix.strip_prefix(' ')?;
            if digits.starts_with('0') || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            digits.parse().ok().filter(|&number| number >= 2)?
        }
    };
    Some((stamp, number))
}

/// Step 1: copies the book `reader` reads, which is at `book`, into
/// `folder` as `<stem> <stamp> before moving.ksz`, then removes that book's
/// oldest "before moving" backups beyond the last [`KEEP_MOVE_BACKUPS`].
fn back_up_before_moving(
    reader: &Connection,
    book: &Path,
    folder: &Path,
) -> Result<PathBuf, StoreError> {
    let stem = book
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or(StoreError::Io(ErrorKind::InvalidFilename))?;
    let folder = std::path::absolute(folder)?;
    fs::create_dir_all(&folder)?;
    let stamp = Timestamp::now().strftime(STAMP_FORMAT).to_string();
    let (path, partial) = claim_backup(&folder, stem, &stamp)?;
    let made = copy_into(reader, &partial).and_then(|()| {
        // Never over another file, should one have appeared.
        if exists(&path) {
            return Err(StoreError::Io(ErrorKind::AlreadyExists));
        }
        fs::rename(&partial, &path).map_err(StoreError::from)
    });
    if let Err(error) = made {
        let _ = fs::remove_file(&partial);
        return Err(error);
    }
    prune_move_backups(&folder, stem, &path);
    Ok(path)
}

/// The first free name for a "before moving" backup, with the name the copy
/// is written under first, claimed by creating an empty file.
fn claim_backup(folder: &Path, stem: &str, stamp: &str) -> Result<(PathBuf, PathBuf), StoreError> {
    for number in 1..=MAX_PER_SECOND {
        let name = if number == 1 {
            format!("{stem} {stamp}{BEFORE_MOVING}{EXTENSION}")
        } else {
            format!("{stem} {stamp}{BEFORE_MOVING} {number}{EXTENSION}")
        };
        let path = folder.join(name);
        if exists(&path) {
            continue;
        }
        let partial = with_suffix(&path, PART);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&partial)
        {
            Ok(_) => return Ok((path, partial)),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(StoreError::Io(ErrorKind::AlreadyExists))
}

/// Removes the oldest "before moving" backups of the book `stem` until the
/// last [`KEEP_MOVE_BACKUPS`] are left, `just_made` always among them. Other
/// files, the automatic backups among them, are left alone. Errors are
/// ignored: a backup too many does no harm.
fn prune_move_backups(folder: &Path, stem: &str, just_made: &Path) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    let mut older: Vec<(String, u32, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let (stamp, number) = move_backup_name(&name, stem)?;
            Some((stamp.to_owned(), number, entry.path()))
        })
        .filter(|(_, _, path)| path != just_made)
        .collect();
    older.sort();
    let excess = (older.len() + 1).saturating_sub(KEEP_MOVE_BACKUPS);
    for (_, _, path) in older.into_iter().take(excess) {
        let _ = fs::remove_file(path);
    }
}

/// A read-only connection to the book at `path`, beside the app's own.
fn open_reader(path: &Path) -> Result<Connection, MoveCause> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(BUSY_WAIT)?;
    Ok(conn)
}

/// Copies the database `reader` has open into `to`, the empty file claimed
/// for it, and makes sure the copy is on the disk.
fn copy_into(reader: &Connection, to: &Path) -> Result<(), StoreError> {
    let name = to
        .to_str()
        .ok_or(StoreError::Io(ErrorKind::InvalidFilename))?;
    // VACUUM INTO writes into an empty file, and reads the book in one read
    // transaction, so the copy is one state of it
    // (sqlite.org/lang_vacuum.html).
    reader.execute("VACUUM INTO ?1", [name])?;
    // Windows syncs only files opened for writing.
    OpenOptions::new().write(true).open(to)?.sync_all()?;
    Ok(())
}

/// Step 3: whether the copy at `copy` is the same as the book `book`, which
/// `reader` reads too.
fn check_copy(reader: &Connection, book: &Book, copy: &Path) -> Result<(), MoveCause> {
    let differs = |difference| Err(MoveCause::Differs(difference));
    if !is_book_file(copy)? {
        return differs(Difference::NotABook);
    }
    let copied = open_reader(copy)?;
    let integrity: Vec<String> = copied
        .prepare("PRAGMA integrity_check")?
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    if integrity != ["ok"] {
        return differs(Difference::Damaged);
    }
    let version = |conn: &Connection| -> Result<i32, rusqlite::Error> {
        conn.pragma_query_value(None, "user_version", |row| row.get(0))
    };
    if version(reader)? != version(&copied)? {
        return differs(Difference::Layout);
    }
    let schema = schema(reader)?;
    if schema != self::schema(&copied)? {
        return differs(Difference::Schema);
    }
    // The balances, computed by KassenSturz's own code from each file
    // (AGENTS.md §2.1).
    let copy_book = Book::open_to_read(copy)?;
    let accounts = book.all_accounts()?;
    if copy_book.all_accounts()? != accounts {
        return differs(Difference::Accounts);
    }
    for account in &accounts {
        if copy_book.balance(account.id)? != book.balance(account.id)? {
            return differs(Difference::Balance(account.id));
        }
    }
    drop(copy_book);
    for (kind, name, _) in &schema {
        if kind == "table" && !same_rows(reader, &copied, name)? {
            return differs(Difference::Table(name.clone()));
        }
    }
    Ok(())
}

/// Every table, index, view and trigger of the database `conn` has open, by
/// kind and name, with the SQL that made it.
fn schema(conn: &Connection) -> Result<Vec<(String, String, Option<String>)>, rusqlite::Error> {
    conn.prepare("SELECT type, name, sql FROM sqlite_schema ORDER BY type, name")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .collect()
}

/// Whether the table `table` has the same rows in both databases. The rows
/// are read in the order of all their columns, since a copy may number the
/// rows of a table without its own key differently.
fn same_rows(one: &Connection, other: &Connection, table: &str) -> Result<bool, rusqlite::Error> {
    let quoted = format!("\"{}\"", table.replace('"', "\"\""));
    let columns = one
        .prepare(&format!("SELECT * FROM {quoted} LIMIT 0"))?
        .column_count();
    if columns == 0 {
        return Ok(true);
    }
    let order = (1..=columns)
        .map(|column| column.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let query = format!("SELECT * FROM {quoted} ORDER BY {order}");
    let mut first = one.prepare(&query)?;
    let mut second = other.prepare(&query)?;
    if second.column_count() != columns {
        return Ok(false);
    }
    let mut first_rows = first.query([])?;
    let mut second_rows = second.query([])?;
    loop {
        match (first_rows.next()?, second_rows.next()?) {
            (None, None) => return Ok(true),
            (Some(a), Some(b)) => {
                for column in 0..columns {
                    if a.get::<_, Value>(column)? != b.get::<_, Value>(column)? {
                        return Ok(false);
                    }
                }
            }
            _ => return Ok(false),
        }
    }
}

/// Makes the folder `file` is in keep its new entry on the disk. Only Unix
/// systems can open a folder for that; elsewhere the rename is as durable as
/// the system makes it.
fn sync_folder(file: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    if let Some(folder) = file.parent() {
        fs::File::open(folder)?.sync_all()?;
    }
    #[cfg(not(unix))]
    let _ = file;
    Ok(())
}

/// `path` with `suffix` added to its name, e.g. `Household.ksz.part`.
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Whether anything is at `path`, a broken link included. An error reading
/// it counts as something there, so nothing is ever written over it.
fn exists(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(_) => true,
        Err(error) => error.kind() != ErrorKind::NotFound,
    }
}

/// The places in a move where a test makes it fail or stop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum Stage {
    /// After the write lock was taken (step 0).
    Locked,
    /// After the backup (step 1).
    BackedUp,
    /// After the copy (step 2).
    Copied,
    /// After the check (step 3).
    Checked,
    /// After the rename into place (step 4).
    Placed,
    /// After the new book was opened (step 5).
    Opened,
    /// After it was remembered (step 5).
    Remembered,
    /// After the old book was closed, before its file is removed (step 6).
    Closed,
}

#[cfg(not(test))]
#[inline]
fn checkpoint(_stage: Stage, _path: &Path) -> Result<(), MoveCause> {
    Ok(())
}

#[cfg(test)]
fn checkpoint(stage: Stage, path: &Path) -> Result<(), MoveCause> {
    tests::hook::reached(stage, path)
}

#[cfg(test)]
mod tests;
