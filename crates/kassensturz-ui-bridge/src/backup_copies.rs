// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Backup copies in a folder the user chose, and the list of backups to
//! restore from (#806). The folder belongs to this computer, so it is kept in
//! the app's settings (D-168), not in the book: choosing one changes no book.
//!
//! A copy is written there each time a book opens, unless the folder has a
//! copy of it from the last day, and every automatic backup is copied there
//! too (Q-546, built on the recommended option while it waits). A folder that
//! is missing or full never stops the book from opening: the Settings page
//! says what happened and offers another folder. The folder's real path is
//! display text only and never reaches a log, an AI or an MCP result (D-730).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use kassensturz_core::Timestamp;
use kassensturz_core::book_folder::FolderGrant;
use kassensturz_store::{CopyProblem, KeptCopy, backups_of};

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
}

/// The backups of the book at `book`, newest first: those in the app's own
/// folder `backups` and those in the chosen folder. A copy with the same
/// name in both is listed once, from the app's own folder. Also says why
/// the chosen folder couldn't be read, if it couldn't.
pub fn listed(
    book: &Path,
    backups: &Path,
    chosen: Option<&Path>,
) -> (Vec<Listed>, Option<CopyProblem>) {
    let mut found: Vec<Listed> = backups_of(book, backups)
        .unwrap_or_default()
        .into_iter()
        .map(|file| Listed {
            path: file.path,
            made: file.made,
            in_chosen: false,
        })
        .collect();
    let mut problem = None;
    if let Some(folder) = chosen {
        match backups_of(book, folder) {
            Ok(copies) => {
                for copy in copies {
                    let twin = found
                        .iter()
                        .any(|listed| listed.path.file_name() == copy.path.file_name());
                    if !twin {
                        found.push(Listed {
                            path: copy.path,
                            made: copy.made,
                            in_chosen: true,
                        });
                    }
                }
            }
            Err(error) => problem = Some(CopyProblem::of(&error)),
        }
    }
    // Newest first; within one second, by name, as the store lists them.
    found.sort_by(|a, b| {
        b.made
            .cmp(&a.made)
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

    #[test]
    fn lists_both_folders_newest_first_and_each_copy_once() {
        let folder = TestFolder::new();
        let own = folder.join("Backups");
        let stick = folder.join("Stick");
        fs::create_dir(&own).expect("made");
        fs::create_dir(&stick).expect("made");
        for (dir, name) in [
            (&own, "Household 2026-10-01 09-00-00 UTC.ksz"),
            (&own, "Household 2026-10-03 09-00-00 UTC.ksz"),
            // The same backup, copied to the stick.
            (&stick, "Household 2026-10-03 09-00-00 UTC.ksz"),
            (&stick, "Household 2026-10-02 09-00-00 UTC.ksz"),
            (&stick, "Garden 2026-10-04 09-00-00 UTC.ksz"),
        ] {
            fs::write(dir.join(name), "made up").expect("written");
        }
        let book = folder.join("Household.ksz");
        let (found, problem) = listed(&book, &own, Some(&stick));
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
                ("Household 2026-10-03 09-00-00 UTC.ksz".to_owned(), false),
                ("Household 2026-10-02 09-00-00 UTC.ksz".to_owned(), true),
                ("Household 2026-10-01 09-00-00 UTC.ksz".to_owned(), false),
            ]
        );

        // A stick that isn't plugged in: the app's own backups, and why.
        let (found, problem) = listed(&book, &own, Some(&folder.join("Gone")));
        assert_eq!(found.len(), 2);
        assert_eq!(problem, Some(CopyProblem::Missing));
    }
}
