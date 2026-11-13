// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Moving the open book into another folder, the app's side (D-731,
//! `docs/BOOK_FOLDER.md` §4): the "move in progress" note and the last move
//! in the settings, the rename through the platform, and the session the
//! book is taken out of and put back into. The steps themselves are
//! `kassensturz_store::Book::move_to`.
//!
//! Paths are shown as plain text only and never written to a log, since
//! they usually hold the user's name (D-730).

use std::path::{Path, PathBuf};

use kassensturz_core::Timestamp;
use kassensturz_store::{
    Computer, Interrupted, MoveCause, MoveNote, MovePlan, MoveRefused, MoveSteps, StoreError,
    plan_move, remove_left_book,
};

use crate::books::OpenBook;
use crate::platform::{self, Drive};
use crate::session;
use crate::settings::Settings;
use crate::test_profile::{self, allows_in};

/// The settings group of the note and of the last move.
const GROUP: &str = "BookMove";
const NOTE_FROM: &str = "MovingFrom";
const NOTE_TO: &str = "MovingTo";
const NOTE_SINCE: &str = "MovingSince";
const NOTE_BACKUP: &str = "MovingBackup";
/// Where the book was before the last finished move, for "Move it back".
const LAST_FROM: &str = "MovedFrom";
/// Where it went, so the last move is shown only for that book.
const LAST_TO: &str = "MovedTo";
/// When that move finished.
const LAST_ON: &str = "MovedOn";

/// Why a move doesn't start or didn't finish: a code the page writes a
/// message for (D-034).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveProblem {
    /// The code, e.g. `book-move.same-folder` or `book-move.failed-copy`.
    pub code: &'static str,
    /// A file of the move that couldn't be removed, if one was left.
    pub left: Option<PathBuf>,
}

impl MoveProblem {
    fn code(code: &'static str) -> Self {
        Self { code, left: None }
    }
}

impl From<MoveRefused> for MoveProblem {
    fn from(refused: MoveRefused) -> Self {
        Self::code(refused.code())
    }
}

/// A finished move, for the done page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Done {
    /// Where the book is now.
    pub to: PathBuf,
    /// Where it was.
    pub from: PathBuf,
    /// The backup made before moving.
    pub backup: PathBuf,
    /// The old file, if it couldn't be removed.
    pub old_left: Option<PathBuf>,
}

/// The last finished move, for "Moved on … from …" and "Move it back".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LastMove {
    /// Where the book was.
    pub from: PathBuf,
    /// When it moved.
    pub on: Option<Timestamp>,
}

/// The note as the settings keep it, if a move is in progress or was cut
/// short.
pub fn note(settings: &Settings) -> Option<MoveNote> {
    Some(MoveNote {
        from: settings.path(GROUP, NOTE_FROM)?,
        to: settings.path(GROUP, NOTE_TO)?,
        since: settings
            .text(GROUP, NOTE_SINCE)
            .parse()
            .unwrap_or_else(|_| Timestamp::now()),
        backup: settings.path(GROUP, NOTE_BACKUP),
    })
}

fn clear_note(settings: &Settings) {
    for key in [NOTE_FROM, NOTE_TO, NOTE_SINCE, NOTE_BACKUP] {
        settings.set_text(GROUP, key, "");
    }
}

/// The last finished move, if the book at `book` is the one that moved
/// there.
pub fn last_move(settings: &Settings, book: &Path) -> Option<LastMove> {
    if settings.path(GROUP, LAST_TO).as_deref() != Some(book) {
        return None;
    }
    Some(LastMove {
        from: settings.path(GROUP, LAST_FROM)?,
        on: settings.text(GROUP, LAST_ON).parse().ok(),
    })
}

/// What a move needs from the app: the settings, and the platform's rename.
struct AppSteps<'a> {
    settings: &'a Settings,
}

impl MoveSteps for AppSteps<'_> {
    fn in_progress(&self) -> bool {
        note(self.settings).is_some()
    }

    fn note(&mut self, note: &MoveNote) -> bool {
        // The paths first: a note without both is no note. A note written
        // again with the backup's name is never cleared if that fails: it
        // still explains a stop.
        self.settings.set_path(GROUP, NOTE_FROM, &note.from)
            && self.settings.set_path(GROUP, NOTE_TO, &note.to)
            && self
                .settings
                .set_text(GROUP, NOTE_SINCE, &note.since.to_string())
            && match &note.backup {
                Some(backup) => self.settings.set_path(GROUP, NOTE_BACKUP, backup),
                None => self.settings.set_text(GROUP, NOTE_BACKUP, ""),
            }
    }

    fn clear_note(&mut self) {
        clear_note(self.settings);
    }

    fn rename_no_replace(&mut self, from: &Path, to: &Path) -> std::io::Result<()> {
        platform::rename_no_replace(from, to)
    }

    fn remember(&mut self, book: &Path) -> bool {
        self.settings.set_last_book(book)
    }
}

/// Checks a move of the open book into `folder` as `name`, before anything
/// is written (D-731).
pub fn plan(settings: &Settings, folder: &Path, name: &str) -> Result<MovePlan, MoveProblem> {
    let Some(from) = session::path() else {
        return Err(MoveProblem::code("book-move.no-book"));
    };
    // In the test profile no book goes outside the test folder (Q-399).
    if !allows_in(test_profile::root(), &folder.join("book")) {
        return Err(MoveProblem::code("book-move.outside-test"));
    }
    // A single file the portal handed over can't hold a book; a folder it
    // exported can (D-730).
    if platform::drive(&folder.join("book.ksz")) == Drive::Sandboxed {
        return Err(MoveProblem::code("book-move.sandboxed"));
    }
    Ok(plan_move(
        &from,
        folder,
        name,
        platform::free_space(folder),
        note(settings).is_some(),
    )?)
}

/// Moves the open book as `plan` says (D-731). The book is taken out of the
/// session and put back, moved or, if the move failed, where it was, so it
/// stays open either way. `computer` names the backup folder (D-033).
pub fn run(settings: &Settings, plan: &MovePlan, computer: &Computer) -> Result<Done, MoveProblem> {
    let Some(open) = session::take() else {
        return Err(MoveProblem::code("book-move.no-book"));
    };
    if open.path != plan.from {
        session::set(open);
        return Err(MoveProblem::code("book-move.no-book"));
    }
    let OpenBook {
        book,
        path,
        on_network_drive,
    } = open;
    let mut steps = AppSteps { settings };
    match book.move_to(plan, computer, &mut steps) {
        Ok(moved) => {
            let on_network_drive = platform::drive(&moved.to) == Drive::Network;
            session::set(OpenBook {
                book: moved.book,
                path: moved.to.clone(),
                on_network_drive,
            });
            settings.set_path(GROUP, LAST_FROM, &path);
            settings.set_path(GROUP, LAST_TO, &moved.to);
            settings.set_text(GROUP, LAST_ON, &Timestamp::now().to_string());
            Ok(Done {
                to: moved.to,
                from: path,
                backup: moved.backup,
                old_left: moved.old_left,
            })
        }
        Err(failed) => {
            let failed = *failed;
            session::set(OpenBook {
                book: failed.book,
                path,
                on_network_drive,
            });
            let code = match failed.cause {
                MoveCause::Refused(refused) => refused.code(),
                _ => failed.step.code(),
            };
            Err(MoveProblem {
                code,
                left: failed.left,
            })
        }
    }
}

/// What the app found at the start about a move that was cut short, after
/// it made sure the right book opens: the old one if the new one wasn't the
/// book opened last yet (D-731).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Explained {
    /// What was left.
    pub interrupted: Interrupted,
    /// The note it was read from.
    pub note: MoveNote,
}

impl Explained {
    /// The book the user may remove: the unused copy or the old file.
    pub fn removable(&self) -> Option<&Path> {
        match &self.interrupted {
            Interrupted::BeforeSwitch { copy } => copy.as_deref(),
            Interrupted::AfterSwitch { old } => old.as_deref(),
        }
    }
}

/// Reads a note left by a move that was cut short, if there is one, before
/// any book is opened at the start. The unfinished copy is removed, and the
/// old book is remembered again if the new one wasn't yet.
pub fn interrupted_at_start(settings: &Settings) -> Option<Explained> {
    let note = note(settings)?;
    let remembered = settings.last_book();
    let interrupted = note.interrupted(remembered.as_deref());
    if matches!(interrupted, Interrupted::BeforeSwitch { .. })
        && remembered.as_deref() != Some(note.from.as_path())
    {
        settings.set_last_book(&note.from);
    }
    Some(Explained { interrupted, note })
}

/// Ends what a cut-short move left: removes the book `explained` offers if
/// `remove`, then clears the note. Never the book that is open, nor one open
/// on another computer, as `computer` tells them apart.
pub fn settle(
    settings: &Settings,
    explained: &Explained,
    remove: bool,
    computer: &Computer,
) -> Result<(), MoveProblem> {
    if remove && let Some(path) = explained.removable() {
        if session::path().as_deref() == Some(path) {
            return Err(MoveProblem::code("book-move.remove-open"));
        }
        match remove_left_book(path, computer) {
            Ok(()) => {}
            Err(StoreError::Locked(_)) => return Err(MoveProblem::code("book-move.remove-locked")),
            Err(_) => return Err(MoveProblem::code("book-move.remove-failed")),
        }
    }
    clear_note(settings);
    Ok(())
}

/// How a folder is shown: KassenSturz's own, or the user's folder in their
/// words (the real path, or in the Flatpak the folder's name, D-730).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShownFolder {
    /// Whether it is KassenSturz's own data folder.
    pub own: bool,
    /// The folder as the user knows it; empty for the own folder.
    pub text: String,
}

/// How the folder `file` is in is shown.
pub fn shown_folder_of(file: &Path) -> ShownFolder {
    let folder = file.parent().unwrap_or(file);
    if platform::app_folder().as_deref() == Some(folder) {
        return ShownFolder {
            own: true,
            text: String::new(),
        };
    }
    let text = match platform::choose_folder(folder) {
        Ok(grant) => grant.shown_as,
        // The home folder itself, say: shown as it is.
        Err(_) => folder.display().to_string(),
    };
    ShownFolder { own: false, text }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use kassensturz_store::Book;

    use super::*;
    use crate::books::Books;
    use crate::test_folder::TestFolder;

    /// A book open in the session, in `folder/own`, with its settings file.
    fn opened(folder: &TestFolder) -> (Books, Settings) {
        fs::create_dir_all(folder.join("own")).expect("folder made");
        fs::create_dir_all(folder.join("Sync")).expect("folder made");
        let books = Books::in_folder(folder.path(), "Laptop");
        let open = books
            .create(&folder.join("own/Household.ksz"))
            .expect("book made");
        session::set(open);
        (books, Settings::at(&folder.join("kassensturzrc")))
    }

    fn computer(folder: &TestFolder) -> Computer {
        Computer {
            name: "Laptop".into(),
            backup_folder: folder.join("data/backups"),
        }
    }

    #[test]
    fn the_open_book_moves_and_stays_open_and_remembered() {
        let folder = TestFolder::new();
        let (_books, settings) = opened(&folder);
        let plan = plan(&settings, &folder.join("Sync"), "Household").expect("planned");
        let done = run(&settings, &plan, &computer(&folder)).expect("moved");
        assert_eq!(done.to, folder.join("Sync/Household.ksz"));
        assert_eq!(session::path(), Some(done.to.clone()));
        assert_eq!(settings.last_book(), Some(done.to.clone()));
        assert_eq!(note(&settings), None);
        assert_eq!(
            last_move(&settings, &done.to).map(|last| last.from),
            Some(folder.join("own/Household.ksz"))
        );
        assert_eq!(last_move(&settings, &done.from), None);
        assert!(!folder.join("own/Household.ksz").exists());
        assert!(done.backup.exists());
        // The screens reach the moved book.
        assert_eq!(
            session::with_book(|book| book.is_empty().expect("read")),
            Some(true)
        );
        session::close();
    }

    #[test]
    fn a_refused_move_keeps_the_open_book_where_it_is() {
        let folder = TestFolder::new();
        let (_books, settings) = opened(&folder);
        let own = folder.join("own");
        assert_eq!(
            plan(&settings, &own, "Household"),
            Err(MoveProblem::code("book-move.same-folder"))
        );
        fs::write(folder.join("Sync/Household.ksz-journal"), "made up").expect("written");
        assert_eq!(
            plan(&settings, &folder.join("Sync"), "Household"),
            Err(MoveProblem::code("book-move.leftover-files"))
        );
        assert_eq!(session::path(), Some(own.join("Household.ksz")));
        session::close();
    }

    #[test]
    fn the_note_round_trips_through_the_settings_file() {
        let folder = TestFolder::new();
        let settings = Settings::at(&folder.join("kassensturzrc"));
        let note = MoveNote {
            from: folder.join("own/Household.ksz"),
            to: folder.join("Sync/Household.ksz"),
            since: "2026-11-13T03:00:00Z".parse().expect("time"),
            backup: Some(
                folder.join("data/backups/Household 2026-11-13 03-00-00 UTC before moving.ksz"),
            ),
        };
        let mut steps = AppSteps {
            settings: &settings,
        };
        assert!(!steps.in_progress());
        assert!(steps.note(&note));
        assert!(steps.in_progress());
        assert_eq!(
            super::note(&Settings::at(&folder.join("kassensturzrc"))),
            Some(note)
        );
        steps.clear_note();
        assert!(!steps.in_progress());
    }

    #[test]
    fn the_platforms_rename_never_replaces_a_file() {
        let folder = TestFolder::new();
        let from = folder.join("Household.ksz.part");
        let to = folder.join("Household.ksz");
        fs::write(&from, "copy").expect("written");
        fs::write(&to, "another file").expect("written");
        let error = platform::rename_no_replace(&from, &to).expect_err("refused");
        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&to).expect("kept"), "another file");
        assert_eq!(fs::read_to_string(&from).expect("kept"), "copy");
        fs::remove_file(&to).expect("removed");
        platform::rename_no_replace(&from, &to).expect("renamed");
        assert_eq!(fs::read_to_string(&to).expect("moved"), "copy");
        assert!(!from.exists());
    }

    #[test]
    fn a_cut_short_move_opens_the_old_book_and_offers_the_copy() {
        let folder = TestFolder::new();
        let settings = Settings::at(&folder.join("kassensturzrc"));
        fs::create_dir_all(folder.join("own")).expect("folder made");
        fs::create_dir_all(folder.join("Sync")).expect("folder made");
        let from = folder.join("own/Household.ksz");
        let to = folder.join("Sync/Household.ksz");
        drop(Book::create(&from, &computer(&folder)).expect("made"));
        // The copy was put in place, then the app stopped.
        drop(Book::create(&to, &computer(&folder)).expect("made"));
        fs::write(folder.join("Sync/Household.ksz.part"), "half").expect("written");
        let mut steps = AppSteps {
            settings: &settings,
        };
        steps.note(&MoveNote {
            from: from.clone(),
            to: to.clone(),
            since: Timestamp::now(),
            backup: None,
        });
        // The settings name neither book, e.g. after an edit by hand.
        settings.set_last_book(&folder.join("elsewhere.ksz"));

        let explained = interrupted_at_start(&settings).expect("explained");
        assert_eq!(
            explained.interrupted,
            Interrupted::BeforeSwitch {
                copy: Some(to.clone())
            }
        );
        assert_eq!(
            settings.last_book(),
            Some(from.clone()),
            "the old book opens"
        );
        assert!(!folder.join("Sync/Household.ksz.part").exists());
        settle(&settings, &explained, true, &computer(&folder)).expect("settled");
        assert!(!to.exists());
        assert!(from.exists());
        assert_eq!(note(&settings), None);
    }

    #[test]
    fn a_cut_short_move_after_the_switch_offers_the_old_file_only_on_a_yes() {
        let folder = TestFolder::new();
        let settings = Settings::at(&folder.join("kassensturzrc"));
        fs::create_dir_all(folder.join("own")).expect("folder made");
        fs::create_dir_all(folder.join("Sync")).expect("folder made");
        let from = folder.join("own/Household.ksz");
        let to = folder.join("Sync/Household.ksz");
        drop(Book::create(&from, &computer(&folder)).expect("made"));
        drop(Book::create(&to, &computer(&folder)).expect("made"));
        let mut steps = AppSteps {
            settings: &settings,
        };
        steps.note(&MoveNote {
            from: from.clone(),
            to: to.clone(),
            since: Timestamp::now(),
            backup: None,
        });
        settings.set_last_book(&to);

        let explained = interrupted_at_start(&settings).expect("explained");
        assert_eq!(explained.removable(), Some(from.as_path()));
        assert_eq!(settings.last_book(), Some(to.clone()));
        // "Keep it": nothing is removed, the note is gone.
        settle(&settings, &explained, false, &computer(&folder)).expect("settled");
        assert!(from.exists() && to.exists());
        assert_eq!(interrupted_at_start(&settings), None);
    }
}
