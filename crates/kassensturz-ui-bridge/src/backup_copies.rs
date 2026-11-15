// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Backup copies in a folder the user chose, and the list of backups to
//! restore from (#806). The folder belongs to this computer, so it is kept in
//! the app's settings (D-168), not in the book: choosing one changes no book.
//!
//! A copy is written there each time a book opens, unless the folder has a
//! copy of it from the last day, and every automatic backup is copied there
//! too (Q-546 = R, D-763). A folder that
//! is missing or full never stops the book from opening: the Settings page
//! says what happened and offers another folder. The folder's real path is
//! display text only and never reaches a log, an AI or an MCP result (D-730).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use kassensturz_core::Timestamp;
use kassensturz_core::book_folder::FolderGrant;
use kassensturz_store::{
    BookId, CopyProblem, KeptCopy, backups_of, move_backups_of, older_backups_of,
    older_move_backups_of,
};

use crate::session;
use crate::settings::Settings;

/// The settings group of the backups.
const GROUP: &str = "Backups";
/// Where the copies go, kept like the book opened last.
const FOLDER: &str = "CopiesFolder";
/// How that folder is shown: display text only (D-730).
const SHOWN_AS: &str = "CopiesFolderShownAs";
/// Whether `SHOWN_AS` is the real path, not only the folder's name.
const SHOWN_IS_REAL_PATH: &str = "CopiesFolderShownIsRealPath";
/// How many copies of each book the folder keeps.
const KEEP: &str = "CopiesKeep";

/// How many copies of each book the Settings page offers to keep, as the
/// words the settings file keeps them by; the page owns the labels.
pub const CHOICES: [&str; 4] = ["3", "5", "10", "20"];

/// The number kept when none was chosen: as many as the app's own folder
/// keeps (D-033).
pub const DEFAULT_KEEP: &str = "10";

/// The folder the user chose for the copies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chosen {
    /// Where the copies are written.
    pub path: PathBuf,
    /// What the user is shown: display text only (D-730).
    pub shown_as: String,
    /// Whether `shown_as` is the real path.
    pub shown_is_real_path: bool,
}

/// The folder chosen for the copies, if one is.
pub fn chosen(settings: &Settings) -> Option<Chosen> {
    let path = settings.path(GROUP, FOLDER)?;
    let shown_as = settings.text(GROUP, SHOWN_AS);
    Some(Chosen {
        shown_as: if shown_as.is_empty() {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        } else {
            shown_as
        },
        shown_is_real_path: settings.text(GROUP, SHOWN_IS_REAL_PATH) == "true",
        path,
    })
}

/// Keeps `grant` as the folder for the copies. Returns whether it was saved.
pub fn choose(settings: &Settings, grant: &FolderGrant) -> bool {
    settings.set_path(GROUP, FOLDER, &grant.path)
        && settings.set_text(GROUP, SHOWN_AS, &grant.shown_as)
        && settings.set_text(
            GROUP,
            SHOWN_IS_REAL_PATH,
            if grant.shown_is_real_path {
                "true"
            } else {
                "false"
            },
        )
}

/// Stops keeping copies in another folder; the copies there stay. Returns
/// whether that was saved.
pub fn forget(settings: &Settings) -> bool {
    let saved = [FOLDER, SHOWN_AS, SHOWN_IS_REAL_PATH]
        .into_iter()
        .all(|key| settings.set_text(GROUP, key, ""));
    if saved {
        set_last(None);
    }
    saved
}

/// The number of copies chosen, as the word the page offers: the default
/// for a word nobody here wrote.
pub fn keep_choice(settings: &Settings) -> &'static str {
    let stored = settings.text(GROUP, KEEP);
    CHOICES
        .into_iter()
        .find(|choice| *choice == stored)
        .unwrap_or(DEFAULT_KEEP)
}

/// Keeps how many copies to keep. Returns whether it was saved; a word the
/// page doesn't offer is refused.
pub fn set_keep_choice(settings: &Settings, choice: &str) -> bool {
    CHOICES.contains(&choice) && settings.set_text(GROUP, KEEP, choice)
}

/// The number a choice stands for.
fn keep_of(choice: &str) -> usize {
    choice.parse().unwrap_or(10)
}

/// What happened the last time a copy was to be kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The folder has a copy of the open book, made at `made`; `new` if
    /// it was made just now.
    Kept {
        /// When the newest copy there was made.
        made: Timestamp,
        /// Whether it was made just now.
        new: bool,
    },
    /// No copy could be kept there.
    Problem(CopyProblem),
}

/// The last outcome while the app runs; `None` before the first.
static LAST: Mutex<Option<Outcome>> = Mutex::new(None);

fn set_last(outcome: Option<Outcome>) {
    if let Ok(mut last) = LAST.lock() {
        *last = outcome;
    }
}

/// What happened the last time a copy was to be kept while the app runs.
pub fn last_outcome() -> Option<Outcome> {
    LAST.lock().ok().and_then(|last| *last)
}

/// Makes a backup of the open book in `backups`, the app's own backup
/// folder, when its newest one there is a month old or there is none, so a
/// month-old state can still be found (B-255). Called when a book opens,
/// before [`keep_copy_of_open_book`], which then copies it to the chosen
/// folder too. Never fails: a backup that can't be made doesn't stop the
/// book, and the backup before an update still refuses to go on (D-033).
pub fn keep_monthly_backup_of_open_book(backups: &Path) {
    if let Some(book) = session::path() {
        let _ = session::with_book(|open| open.keep_monthly_backup(&book, backups));
    }
}

/// Keeps a copy of the open book in the chosen folder, after the book was
/// opened or the folder chosen. `backups` is the app's own backup folder,
/// whose newest backup is copied when the chosen folder lacks it. `None`
/// when no folder is chosen or no book is open. Never fails: a problem is
/// the outcome, for the Settings page.
pub fn keep_copy_of_open_book(settings: &Settings, backups: &Path) -> Option<Outcome> {
    let chosen = chosen(settings)?;
    let book = session::path()?;
    let keep = keep_of(keep_choice(settings));
    let kept = session::with_book(|open| open.keep_copy(&book, backups, &chosen.path, keep))?;
    let outcome = outcome_of(&kept);
    set_last(Some(outcome));
    Some(outcome)
}

fn outcome_of(kept: &Result<KeptCopy, kassensturz_store::StoreError>) -> Outcome {
    match kept {
        Ok(KeptCopy::Made(file)) => Outcome::Kept {
            made: file.made,
            new: true,
        },
        Ok(KeptCopy::Recent(file)) => Outcome::Kept {
            made: file.made,
            new: false,
        },
        Err(error) => Outcome::Problem(CopyProblem::of(error)),
    }
}

/// The code the page shows words for (D-034, `docs/ERROR_CODES.md`).
pub fn problem_code(problem: CopyProblem) -> &'static str {
    match problem {
        CopyProblem::Missing => "backup-copy.missing",
        CopyProblem::Full => "backup-copy.full",
        CopyProblem::ReadOnly => "backup-copy.read-only",
        CopyProblem::SameFolder => "backup-copy.same-folder",
        CopyProblem::Other => "backup-copy.failed",
    }
}

/// A backup the restore page lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    /// Where it is.
    pub path: PathBuf,
    /// When it was made.
    pub made: Timestamp,
    /// Whether it is in the folder the user chose, not in the app's own.
    pub in_chosen: bool,
    /// Whether it is from before books had an identity (layout 34, D-806):
    /// it has the book's name but nothing says it is this book's, so the
    /// page lists it apart.
    pub older: bool,
}

/// The backups of the book at `book`, which has the identity `id`, newest
/// first: those in the app's own folder `backups` and those in the chosen
/// folder. The book's own come first (tied to `id`, not to its name, D-806),
/// then the older ones without an identity that have its name, flagged
/// [`Listed::older`], for the page to list apart. A copy with the same name
/// in both folders is listed once, from the app's own folder. Also says why
/// the chosen folder couldn't be read, if it couldn't.
pub fn listed(
    book: &Path,
    id: &BookId,
    backups: &Path,
    chosen: Option<&Path>,
) -> (Vec<Listed>, Option<CopyProblem>) {
    let item = |file: kassensturz_store::BackupFile, in_chosen: bool, older: bool| Listed {
        path: file.path,
        made: file.made,
        in_chosen,
        older,
    };
    let mut found: Vec<Listed> = backups_of(id, backups)
        .unwrap_or_default()
        .into_iter()
        .map(|file| item(file, false, false))
        .collect();
    // The backups made before a move of the book (D-731), kept apart from
    // the ten automatic ones but restored like them.
    found.extend(
        move_backups_of(id, backups)
            .unwrap_or_default()
            .into_iter()
            .map(|file| item(file, false, false)),
    );
    found.extend(
        older_backups_of(book, backups)
            .unwrap_or_default()
            .into_iter()
            .chain(older_move_backups_of(book, backups).unwrap_or_default())
            .map(|file| item(file, false, true)),
    );
    let mut problem = None;
    if let Some(folder) = chosen {
        match backups_of(id, folder) {
            Ok(copies) => {
                let older = older_backups_of(book, folder).unwrap_or_default();
                for (copy, is_older) in copies
                    .into_iter()
                    .map(|copy| (copy, false))
                    .chain(older.into_iter().map(|copy| (copy, true)))
                {
                    let twin = found
                        .iter()
                        .any(|listed| listed.path.file_name() == copy.path.file_name());
                    if !twin {
                        found.push(item(copy, true, is_older));
                    }
                }
            }
            Err(error) => problem = Some(CopyProblem::of(&error)),
        }
    }
    // The book's own first, then the older ones; newest first in each;
    // within one second, by name, as the store lists them.
    found.sort_by(|a, b| {
        a.older
            .cmp(&b.older)
            .then_with(|| b.made.cmp(&a.made))
            .then_with(|| b.path.file_name().cmp(&a.path.file_name()))
    });
    (found, problem)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::test_folder::TestFolder;

    #[test]
    fn keeps_the_chosen_folder_and_forgets_it() {
        let folder = TestFolder::new();
        let settings = Settings::at(&folder.join("kassensturzrc"));
        assert_eq!(chosen(&settings), None);
        let grant = FolderGrant {
            path: folder.join("Stick"),
            shown_as: "Stick".into(),
            shown_is_real_path: false,
        };
        assert!(choose(&settings, &grant));
        let again = Settings::at(&folder.join("kassensturzrc"));
        assert_eq!(
            chosen(&again),
            Some(Chosen {
                path: folder.join("Stick"),
                shown_as: "Stick".into(),
                shown_is_real_path: false,
            })
        );
        let written = fs::read_to_string(folder.join("kassensturzrc")).expect("written");
        assert!(written.contains("[Backups]"), "{written}");
        assert!(forget(&again));
        assert_eq!(chosen(&Settings::at(&folder.join("kassensturzrc"))), None);
    }

    #[test]
    fn keeps_how_many_copies_to_keep() {
        let folder = TestFolder::new();
        let settings = Settings::at(&folder.join("kassensturzrc"));
        assert_eq!(keep_choice(&settings), DEFAULT_KEEP);
        assert!(set_keep_choice(&settings, "3"));
        assert_eq!(keep_choice(&settings), "3");
        assert!(!set_keep_choice(&settings, "7"));
        assert_eq!(keep_choice(&settings), "3");
        // A word another build wrote gives the default.
        assert!(settings.set_text(GROUP, KEEP, "many"));
        assert_eq!(keep_choice(&settings), DEFAULT_KEEP);
        for choice in CHOICES {
            assert!(keep_of(choice) >= 3, "{choice}");
        }
        assert_eq!(keep_of(DEFAULT_KEEP), 10);
    }

    #[test]
    fn every_problem_has_its_own_code() {
        let codes: Vec<&str> = [
            CopyProblem::Missing,
            CopyProblem::Full,
            CopyProblem::ReadOnly,
            CopyProblem::SameFolder,
            CopyProblem::Other,
        ]
        .into_iter()
        .map(problem_code)
        .collect();
        let mut unique = codes.clone();
        unique.dedup();
        assert_eq!(unique.len(), codes.len());
        assert!(codes.iter().all(|code| code.starts_with("backup-copy.")));
    }

    /// A real book called `Household.ksz` in `folder`'s folder `dir`, and
    /// its identity.
    fn a_book(folder: &TestFolder, dir: &str) -> (PathBuf, BookId) {
        let dir = folder.join(dir);
        fs::create_dir_all(&dir).expect("made");
        let path = dir.join("Household.ksz");
        let computer = kassensturz_store::Computer {
            name: "Laptop".into(),
            backup_folder: folder.join("Backups"),
        };
        let book = kassensturz_store::Book::create(&path, &computer).expect("book made");
        let id = book.book_id().expect("an identity");
        (path, id)
    }

    fn file_names(found: &[Listed]) -> Vec<String> {
        found
            .iter()
            .map(|listed| {
                listed
                    .path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    }

    #[test]
    fn lists_both_folders_newest_first_and_each_copy_once() {
        let folder = TestFolder::new();
        let own = folder.join("Backups");
        let stick = folder.join("Stick");
        fs::create_dir(&own).expect("made");
        fs::create_dir(&stick).expect("made");
        let (book, id) = a_book(&folder, "A");
        let tag = id.tag();
        for (dir, name) in [
            (
                &own,
                format!("Household [{tag}] 2026-10-01 09-00-00 UTC.ksz"),
            ),
            (
                &own,
                format!("Household [{tag}] 2026-10-03 09-00-00 UTC.ksz"),
            ),
            // The same backup, copied to the stick.
            (
                &stick,
                format!("Household [{tag}] 2026-10-03 09-00-00 UTC.ksz"),
            ),
            (
                &stick,
                format!("Household [{tag}] 2026-10-02 09-00-00 UTC.ksz"),
            ),
            (
                &stick,
                "Garden [ffffffffffff] 2026-10-04 09-00-00 UTC.ksz".to_owned(),
            ),
        ] {
            fs::write(dir.join(name), "made up").expect("written");
        }
        let (found, problem) = listed(&book, &id, &own, Some(&stick));
        assert_eq!(problem, None);
        let shown: Vec<(String, bool)> = found
            .iter()
            .map(|listed| {
                (
                    listed
                        .path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    listed.in_chosen,
                )
            })
            .collect();
        assert_eq!(
            shown,
            [
                (
                    format!("Household [{tag}] 2026-10-03 09-00-00 UTC.ksz"),
                    false
                ),
                (
                    format!("Household [{tag}] 2026-10-02 09-00-00 UTC.ksz"),
                    true
                ),
                (
                    format!("Household [{tag}] 2026-10-01 09-00-00 UTC.ksz"),
                    false
                ),
            ]
        );
        assert!(found.iter().all(|listed| !listed.older));

        // A stick that isn't plugged in: the app's own backups, and why.
        let (found, problem) = listed(&book, &id, &own, Some(&folder.join("Gone")));
        assert_eq!(found.len(), 2);
        assert_eq!(problem, Some(CopyProblem::Missing));
    }

    #[test]
    fn lists_the_backups_made_before_a_move() {
        let folder = TestFolder::new();
        let own = folder.join("Backups");
        fs::create_dir(&own).expect("made");
        let (book, id) = a_book(&folder, "A");
        let tag = id.tag();
        for name in [
            format!("Household [{tag}] 2026-10-01 09-00-00 UTC.ksz"),
            format!("Household [{tag}] 2026-10-02 09-00-00 UTC vor dem Verschieben.ksz"),
            format!("Household [{tag}] 2026-10-02 09-00-00 UTC vor dem Verschieben 2.ksz"),
            "Garden [ffffffffffff] 2026-10-03 09-00-00 UTC vor dem Verschieben.ksz".to_owned(),
            format!("Household [{tag}] 2026-10-04 09-00-00 UTC vor dem Verschieben.ksz.part"),
        ] {
            fs::write(own.join(name), "made up").expect("written");
        }
        let (found, problem) = listed(&book, &id, &own, None);
        assert_eq!(problem, None);
        assert_eq!(
            file_names(&found),
            [
                format!("Household [{tag}] 2026-10-02 09-00-00 UTC vor dem Verschieben 2.ksz"),
                format!("Household [{tag}] 2026-10-02 09-00-00 UTC vor dem Verschieben.ksz"),
                format!("Household [{tag}] 2026-10-01 09-00-00 UTC.ksz"),
            ]
        );
    }

    /// B-328, #1080, D-806: with two books of the same name, each page lists
    /// its own book's backups only; the ones from before books had an
    /// identity are listed after them, flagged, and never as the book's own.
    #[test]
    fn two_books_with_the_same_name_list_only_their_own_backups() {
        let folder = TestFolder::new();
        let own = folder.join("Backups");
        fs::create_dir(&own).expect("made");
        let (book_a, id_a) = a_book(&folder, "A");
        let (book_b, id_b) = a_book(&folder, "B");
        assert_eq!(book_a.file_name(), book_b.file_name());
        let (tag_a, tag_b) = (id_a.tag(), id_b.tag());
        for name in [
            format!("Household [{tag_a}] 2026-10-05 09-00-00 UTC.ksz"),
            format!("Household [{tag_b}] 2026-10-04 09-00-00 UTC.ksz"),
            format!("Household [{tag_b}] 2026-10-03 09-00-00 UTC vor dem Verschieben.ksz"),
            // From before books had an identity, the book's name only.
            "Household 2026-10-02 09-00-00 UTC.ksz".to_owned(),
            "Household 2026-10-01 09-00-00 UTC vor dem Verschieben.ksz".to_owned(),
        ] {
            fs::write(own.join(name), "made up").expect("written");
        }
        let (for_a, _) = listed(&book_a, &id_a, &own, None);
        let (for_b, _) = listed(&book_b, &id_b, &own, None);
        assert_eq!(
            file_names(&for_a),
            [
                format!("Household [{tag_a}] 2026-10-05 09-00-00 UTC.ksz"),
                "Household 2026-10-02 09-00-00 UTC.ksz".to_owned(),
                "Household 2026-10-01 09-00-00 UTC vor dem Verschieben.ksz".to_owned(),
            ]
        );
        assert_eq!(
            for_a.iter().map(|listed| listed.older).collect::<Vec<_>>(),
            [false, true, true]
        );
        assert_eq!(
            file_names(&for_b),
            [
                format!("Household [{tag_b}] 2026-10-04 09-00-00 UTC.ksz"),
                format!("Household [{tag_b}] 2026-10-03 09-00-00 UTC vor dem Verschieben.ksz"),
                "Household 2026-10-02 09-00-00 UTC.ksz".to_owned(),
                "Household 2026-10-01 09-00-00 UTC vor dem Verschieben.ksz".to_owned(),
            ]
        );
        assert_eq!(
            for_b.iter().map(|listed| listed.older).collect::<Vec<_>>(),
            [false, false, true, true]
        );
    }
}
