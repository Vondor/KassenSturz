// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The QML object behind "Where your book is kept" and the move page
//! (D-731, `docs/BOOK_FOLDER.md` §2 and §4): where the book is, the checks
//! before a move, the move itself, "Move it back", and what a move that was
//! cut short left at the start. The work is in `book_move.rs`.
//!
//! Folders are shown in the user's words, as plain text only; a path the app
//! opens never reaches a log or an AI or MCP result (D-730).

use core::pin::Pin;
use std::path::{Path, PathBuf};

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QVariant};
use kassensturz_store::Interrupted;

use crate::book_move::{self, Done, Explained, MoveProblem, ShownFolder};
use crate::granted_folders::{self, Granted, SystemPortal};
use crate::locale::QtLocale;
use crate::platform;
use crate::session;
use crate::settings::Settings;
use crate::variant::{list, map, text};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// Where the book is kept, and moving it (D-731). `available` says
        /// whether this computer offers choosing a folder (D-730); `place`
        /// is where the open book is; `interrupted` is what a move that was
        /// cut short left, found at the start, or empty.
        #[qobject]
        #[qml_element]
        #[qproperty(bool, available, READ, CONSTANT)]
        #[qproperty(bool, grants_available, READ, CONSTANT)]
        #[qproperty(QVariant, place, READ, NOTIFY)]
        #[qproperty(QVariant, interrupted, READ, NOTIFY)]
        type BookMove = super::BookMoveRust;
    }

    extern "RustQt" {
        /// Reads `place` again, e.g. after another book was opened.
        #[qinvokable]
        fn refresh(self: Pin<&mut BookMove>);

        /// Checks a move into `folder` (a granted folder's `path`) under
        /// `name`, before anything is written: `ok` with `size` and
        /// `shownAs`, or `code`.
        #[qinvokable]
        fn check(self: &BookMove, folder: &QString, name: &QString) -> QVariant;

        /// Moves the book into `folder` as `name`: `ok` with `shownAs`,
        /// `own`, `backupName`, `oldLeft` and `oldLeftShownAs`, or `code`
        /// and `left` (whether a file of the move is still there).
        #[qinvokable]
        #[cxx_name = "moveTo"]
        fn move_to(self: Pin<&mut BookMove>, folder: &QString, name: &QString) -> QVariant;

        /// Moves the book back to where it was before the last move, as
        /// `moveTo` answers.
        #[qinvokable]
        #[cxx_name = "moveBack"]
        fn move_back(self: Pin<&mut BookMove>) -> QVariant;

        /// Moves the book into KassenSturz's own folder, as `moveTo`
        /// answers.
        #[qinvokable]
        #[cxx_name = "moveToOwnFolder"]
        fn move_to_own_folder(self: Pin<&mut BookMove>) -> QVariant;

        /// The folders KassenSturz may open, each with `id`, `shownAs`
        /// (plain text), `realPath` and `usedBy`, a list of what still uses
        /// it: `book` or `backup-copies` (#873).
        #[qinvokable]
        #[cxx_name = "grantedFolders"]
        fn granted_folders(self: &BookMove) -> QVariant;

        /// Takes the access to the folder `id` back. The files stay. `ok`,
        /// or `code`.
        #[qinvokable]
        #[cxx_name = "forgetFolder"]
        fn forget_folder(self: &BookMove, id: &QString) -> QVariant;

        /// Ends what `interrupted` is about: removes the book it offers if
        /// `remove`, then forgets it. `ok`, or `code`.
        #[qinvokable]
        fn settle(self: Pin<&mut BookMove>, remove: bool) -> QVariant;
    }

    impl cxx_qt::Initialize for BookMove {}
}

/// The Rust side of [`qobject::BookMove`].
#[derive(Default)]
pub struct BookMoveRust {
    available: bool,
    grants_available: bool,
    place: QVariant,
    interrupted: QVariant,
    explained: Option<Explained>,
}

impl cxx_qt::Initialize for qobject::BookMove {
    fn initialize(mut self: Pin<&mut Self>) {
        // Before any book opens: a move that was cut short decides which
        // one is the book (D-731).
        let explained = book_move::interrupted_at_start(&Settings::user());
        let shown = explained
            .as_ref()
            .map(interrupted_object)
            .unwrap_or_default();
        let mut state = self.as_mut().rust_mut();
        state.available = platform::can_choose_book_folder();
        // Only the Flatpak has grants of its own to list and take back.
        state.grants_available = platform::in_flatpak_sandbox()
            && platform::can_choose_book_folder()
            && platform::runtime_dir().is_some();
        state.interrupted = shown;
        state.explained = explained;
    }
}

impl qobject::BookMove {
    fn refresh(mut self: Pin<&mut Self>) {
        let place = place_object(&Settings::user());
        self.as_mut().rust_mut().place = place;
        self.as_mut().place_changed();
    }

    fn check(&self, folder: &QString, name: &QString) -> QVariant {
        let folder = PathBuf::from(folder.to_string());
        match book_move::plan(&Settings::user(), &folder, &name.to_string()) {
            Ok(plan) => map([
                ("ok", QVariant::from(&true)),
                ("size", text(&platform::data_size(plan.size))),
                (
                    "shownAs",
                    shown_object(&book_move::shown_folder_of(&plan.to)),
                ),
            ]),
            // The book's size doesn't depend on the problem, so the page
            // keeps saying it while the name or the folder needs another
            // choice.
            Err(problem) => match book_move::open_book_size() {
                Some(size) => map([
                    ("ok", QVariant::from(&false)),
                    ("code", text(problem.code)),
                    ("left", QVariant::from(&problem.left.is_some())),
                    ("size", text(&platform::data_size(size))),
                ]),
                None => problem_object(&problem),
            },
        }
    }

    fn move_to(self: Pin<&mut Self>, folder: &QString, name: &QString) -> QVariant {
        let folder = PathBuf::from(folder.to_string());
        let name = name.to_string();
        self.moved(&folder, &name)
    }

    fn move_back(self: Pin<&mut Self>) -> QVariant {
        let settings = Settings::user();
        let back = session::path()
            .and_then(|book| book_move::last_move(&settings, &book))
            .map(|last| last.from);
        match back.as_deref().and_then(split) {
            Some((folder, name)) => self.moved(&folder, &name),
            None => problem_object(&MoveProblem {
                code: "book-move.no-way-back",
                left: None,
            }),
        }
    }

    fn move_to_own_folder(self: Pin<&mut Self>) -> QVariant {
        let name = session::path()
            .as_deref()
            .and_then(split)
            .map(|(_, name)| name);
        match (platform::app_folder(), name) {
            (Some(folder), Some(name)) => {
                // The own folder may not be there yet in a fresh profile.
                let _ = std::fs::create_dir_all(&folder);
                self.moved(&folder, &name)
            }
            _ => problem_object(&MoveProblem {
                code: "book-move.no-book",
                left: None,
            }),
        }
    }

    fn granted_folders(&self) -> QVariant {
        let Some(runtime) = platform::runtime_dir() else {
            return list([]);
        };
        let uses = granted_folders::uses_now(&Settings::user());
        folders_object(&granted_folders::list(
            &granted_folders::Portal::documents(&SystemPortal),
            &runtime,
            &uses,
        ))
    }

    fn forget_folder(&self, id: &QString) -> QVariant {
        let Some(runtime) = platform::runtime_dir() else {
            return forget_problem_object(granted_folders::Problem::NotThere);
        };
        let uses = granted_folders::uses_now(&Settings::user());
        match granted_folders::forget(&id.to_string(), &runtime, &uses, &mut SystemPortal) {
            Ok(()) => map([("ok", QVariant::from(&true))]),
            Err(problem) => forget_problem_object(problem),
        }
    }

    fn settle(mut self: Pin<&mut Self>, remove: bool) -> QVariant {
        let Some(explained) = self.rust().explained.clone() else {
            return map([("ok", QVariant::from(&true))]);
        };
        let settings = Settings::user();
        match book_move::settle(&settings, &explained, remove, &platform::computer()) {
            Ok(()) => {
                let mut state = self.as_mut().rust_mut();
                state.explained = None;
                state.interrupted = QVariant::default();
                self.as_mut().interrupted_changed();
                self.as_mut().refresh();
                map([("ok", QVariant::from(&true))])
            }
            Err(problem) => problem_object(&problem),
        }
    }

    /// Plans and runs a move, then reads `place` again.
    fn moved(mut self: Pin<&mut Self>, folder: &Path, name: &str) -> QVariant {
        let settings = Settings::user();
        let result = book_move::plan(&settings, folder, name)
            .and_then(|plan| book_move::run(&settings, &plan, &platform::computer()));
        self.as_mut().refresh();
        match result {
            Ok(done) => done_object(&done),
            Err(problem) => problem_object(&problem),
        }
    }
}

/// The folder `book` is in and its file's name without `.ksz`.
fn split(book: &Path) -> Option<(PathBuf, String)> {
    let folder = book.parent()?.to_owned();
    let name = book.file_stem()?.to_str()?.to_owned();
    Some((folder, name))
}

/// A folder as QML shows it: `own`, or `text` in the user's words.
fn shown_object(shown: &ShownFolder) -> QVariant {
    map([
        ("own", QVariant::from(&shown.own)),
        ("text", text(&shown.text)),
    ])
}

/// Where the open book is: `open`, `folder` (as `shown_object`), `name`
/// (without `.ksz`), and after a move `moved` with `movedOn` and
/// `movedFrom`.
fn place_object(settings: &Settings) -> QVariant {
    let Some(book) = session::path() else {
        return map([("open", QVariant::from(&false))]);
    };
    let name = split(&book).map(|(_, name)| name).unwrap_or_default();
    let last = book_move::last_move(settings, &book);
    map([
        ("open", QVariant::from(&true)),
        ("folder", shown_object(&book_move::shown_folder_of(&book))),
        ("name", text(&name)),
        ("moved", QVariant::from(&last.is_some())),
        (
            "movedOn",
            text(
                &last
                    .as_ref()
                    .and_then(|last| last.on)
                    .map(QtLocale::date_time)
                    .unwrap_or_default(),
            ),
        ),
        (
            "movedFrom",
            last.as_ref()
                .map(|last| shown_object(&book_move::shown_folder_of(&last.from)))
                .unwrap_or_default(),
        ),
    ])
}

fn done_object(done: &Done) -> QVariant {
    let backup_name = done
        .backup
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    map([
        ("ok", QVariant::from(&true)),
        (
            "shownAs",
            shown_object(&book_move::shown_folder_of(&done.to)),
        ),
        ("backupName", text(&backup_name)),
        ("oldLeft", QVariant::from(&done.old_left.is_some())),
        (
            "oldLeftShownAs",
            done.old_left
                .as_deref()
                .map(|old| shown_object(&book_move::shown_folder_of(old)))
                .unwrap_or_default(),
        ),
    ])
}

/// The folders KassenSturz may open as QML gets them: a list of objects with
/// `id`, `shownAs` (display text), `realPath` and `usedBy`.
fn folders_object(folders: &[Granted]) -> QVariant {
    list(folders.iter().map(|folder| {
        map([
            ("id", text(&folder.id)),
            ("shownAs", text(&folder.shown_as)),
            ("realPath", QVariant::from(&folder.shown_is_real_path)),
            (
                "usedBy",
                list(folder.used_by.iter().map(|used| text(used.key()))),
            ),
        ])
    }))
}

/// A folder that wasn't forgotten: `ok` is false, with the `code`.
fn forget_problem_object(problem: granted_folders::Problem) -> QVariant {
    map([
        ("ok", QVariant::from(&false)),
        ("code", text(problem.code())),
    ])
}

fn problem_object(problem: &MoveProblem) -> QVariant {
    map([
        ("ok", QVariant::from(&false)),
        ("code", text(problem.code)),
        ("left", QVariant::from(&problem.left.is_some())),
    ])
}

/// What a cut-short move left, as QML shows it at the start: `after`
/// (whether the move had finished), `offer` (whether a book can be
/// removed), `offerFolder` (where it is) and `backupName`.
fn interrupted_object(explained: &Explained) -> QVariant {
    let after = matches!(explained.interrupted, Interrupted::AfterSwitch { .. });
    let offer = explained.removable();
    let backup_name = explained
        .note
        .backup
        .as_deref()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    map([
        ("after", QVariant::from(&after)),
        ("offer", QVariant::from(&offer.is_some())),
        (
            "offerFolder",
            offer
                .map(|path| shown_object(&book_move::shown_folder_of(path)))
                .unwrap_or_default(),
        ),
        (
            "newFolder",
            shown_object(&book_move::shown_folder_of(&explained.note.to)),
        ),
        ("backupName", text(&backup_name)),
    ])
}

#[cfg(test)]
mod tests {
    use kassensturz_core::Timestamp;
    use kassensturz_store::MoveNote;

    use super::*;
    use crate::variant::read_back::{flag_of, keys, object_of, text_of};

    #[test]
    fn a_finished_move_reads_back_as_qml_reads_it() {
        let done = Done {
            to: PathBuf::from("/data/Sync/Household.ksz"),
            from: PathBuf::from("/data/own/Household.ksz"),
            backup: PathBuf::from(
                "/data/backups/Household 2026-11-13 03-00-00 UTC before moving.ksz",
            ),
            old_left: Some(PathBuf::from("/data/own/Household.ksz")),
        };
        let object = done_object(&done);
        assert_eq!(
            keys(&object),
            ["backupName", "ok", "oldLeft", "oldLeftShownAs", "shownAs"]
        );
        assert!(flag_of(&object, "ok"));
        assert_eq!(
            text_of(&object, "backupName"),
            "Household 2026-11-13 03-00-00 UTC before moving.ksz"
        );
        assert!(flag_of(&object, "oldLeft"));
        let shown = object_of(&object, "shownAs");
        assert_eq!(keys(&shown), ["own", "text"]);
        assert!(!flag_of(&shown, "own"));
        assert_eq!(text_of(&shown, "text"), "/data/Sync");
        assert_eq!(
            text_of(&object_of(&object, "oldLeftShownAs"), "text"),
            "/data/own"
        );
    }

    /// #873: the list and the refusal read back the way QML reads them.
    #[test]
    fn the_granted_folders_read_back_as_qml_reads_them() {
        use crate::granted_folders::UsedBy;
        use crate::variant::read_back::{elements, list_of};

        let object = folders_object(&[
            Granted {
                id: "a1".into(),
                shown_as: "/data/Books".into(),
                shown_is_real_path: true,
                used_by: vec![UsedBy::Book, UsedBy::BackupCopies],
            },
            Granted {
                id: "b2".into(),
                shown_as: "Sync".into(),
                shown_is_real_path: false,
                used_by: Vec::new(),
            },
        ]);
        let folders = elements(&object).expect("an array");
        assert_eq!(folders.len(), 2);
        assert_eq!(keys(&folders[0]), ["id", "realPath", "shownAs", "usedBy"]);
        assert_eq!(text_of(&folders[0], "id"), "a1");
        assert_eq!(text_of(&folders[0], "shownAs"), "/data/Books");
        assert!(flag_of(&folders[0], "realPath"));
        let used: Vec<String> = list_of(&folders[0], "usedBy")
            .iter()
            .map(|word| word.value::<QString>().expect("text").to_string())
            .collect();
        assert_eq!(used, ["book", "backup-copies"]);
        assert!(list_of(&folders[1], "usedBy").is_empty());
        assert!(!flag_of(&folders[1], "realPath"));

        let refused = forget_problem_object(granted_folders::Problem::InUseByBook);
        assert_eq!(keys(&refused), ["code", "ok"]);
        assert!(!flag_of(&refused, "ok"));
        assert_eq!(text_of(&refused, "code"), "granted-folders.in-use-by-book");
    }

    #[test]
    fn a_problem_reads_back_with_its_code() {
        let object = problem_object(&MoveProblem {
            code: "book-move.failed-copy",
            left: Some(PathBuf::from("/data/Sync/Household.ksz.part")),
        });
        assert_eq!(keys(&object), ["code", "left", "ok"]);
        assert!(!flag_of(&object, "ok"));
        assert_eq!(text_of(&object, "code"), "book-move.failed-copy");
        assert!(flag_of(&object, "left"));
    }

    #[test]
    fn a_cut_short_move_reads_back_as_qml_reads_it() {
        let note = MoveNote {
            from: PathBuf::from("/data/own/Household.ksz"),
            to: PathBuf::from("/data/Sync/Household.ksz"),
            since: Timestamp::now(),
            backup: Some(PathBuf::from(
                "/data/backups/Household 2026-11-13 03-00-00 UTC before moving.ksz",
            )),
        };
        let object = interrupted_object(&Explained {
            interrupted: Interrupted::AfterSwitch {
                old: Some(note.from.clone()),
            },
            note: note.clone(),
        });
        assert_eq!(
            keys(&object),
            ["after", "backupName", "newFolder", "offer", "offerFolder"]
        );
        assert!(flag_of(&object, "after"));
        assert!(flag_of(&object, "offer"));
        assert_eq!(
            text_of(&object_of(&object, "offerFolder"), "text"),
            "/data/own"
        );
        assert_eq!(
            text_of(&object_of(&object, "newFolder"), "text"),
            "/data/Sync"
        );
        assert_eq!(
            text_of(&object, "backupName"),
            "Household 2026-11-13 03-00-00 UTC before moving.ksz"
        );

        let object = interrupted_object(&Explained {
            interrupted: Interrupted::BeforeSwitch { copy: None },
            note,
        });
        assert!(!flag_of(&object, "after"));
        assert!(!flag_of(&object, "offer"));
    }

    #[test]
    fn where_the_book_is_reads_back_as_qml_reads_it() {
        let folder = crate::test_folder::TestFolder::new();
        let settings = Settings::at(&folder.join("kassensturzrc"));
        assert_eq!(keys(&place_object(&settings)), ["open"]);
        let books = crate::books::Books::in_folder(folder.path(), "Laptop");
        session::set(books.create(&folder.join("Household.ksz")).expect("made"));
        let object = place_object(&settings);
        assert_eq!(
            keys(&object),
            ["folder", "moved", "movedFrom", "movedOn", "name", "open"]
        );
        assert!(flag_of(&object, "open"));
        assert_eq!(text_of(&object, "name"), "Household");
        assert!(!flag_of(&object, "moved"));
        session::close();
    }
}
