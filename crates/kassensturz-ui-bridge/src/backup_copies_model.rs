// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The backup copies in a folder the user chose and the backups to restore
//! from, as a QML object for Settings › Backups and the restore page
//! (#806). QML gets codes and the folder's display text; the pages own the
//! words (D-034). The folder's real path is shown as plain text only.

use core::pin::Pin;
use std::path::{Path, PathBuf};

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QVariant};
use kassensturz_core::Timestamp;

use crate::backup_copies::{self, CHOICES, Listed, Outcome};
use crate::locale::QtLocale;
use crate::platform;
use crate::session;
use crate::settings::Settings;
use crate::variant::{list, map, text};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt's string type.
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        /// A value of any Qt type.
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// The folder for backup copies: `chosen`, how it is shown
        /// (`shownAs`, plain text only, and `shownIsRealPath`), how many
        /// copies of each book it keeps (`keep`, a word such as "10"), and
        /// `status`, what happened the last time a copy was to be made
        /// there: `state` "off", "waiting", "kept" or "problem", with
        /// `when` and `new` for "kept" and `code` for "problem".
        #[qobject]
        #[qml_element]
        #[qproperty(bool, chosen, READ, NOTIFY)]
        #[qproperty(QString, shown_as, cxx_name = "shownAs", READ, NOTIFY)]
        #[qproperty(bool, shown_is_real_path, cxx_name = "shownIsRealPath", READ, NOTIFY)]
        #[qproperty(QString, keep, READ, NOTIFY)]
        #[qproperty(QVariant, status, READ, NOTIFY)]
        type BackupCopies = super::BackupCopiesRust;
    }

    extern "RustQt" {
        /// Keeps `path`, the `path` of a folder `FolderChooser` granted, as
        /// the folder for the copies, and makes a copy of the open book
        /// there. Returns `ok`, or `code` for why it isn't used.
        #[qinvokable]
        fn choose(self: Pin<&mut BackupCopies>, path: &QString) -> QVariant;

        /// Stops making copies in another folder; the copies there stay.
        /// Returns whether that was saved.
        #[qinvokable]
        fn forget(self: Pin<&mut BackupCopies>) -> bool;

        /// Chooses how many copies of each book to keep, by its word.
        /// Returns whether it was saved; a word the page doesn't offer is
        /// refused.
        #[qinvokable]
        #[cxx_name = "chooseKeep"]
        fn choose_keep(self: Pin<&mut BackupCopies>, choice: &QString) -> bool;

        /// Tries again to make a copy of the open book there, e.g. after
        /// the stick was plugged in.
        #[qinvokable]
        #[cxx_name = "tryAgain"]
        fn try_again(self: Pin<&mut BackupCopies>);

        /// Reads the settings and the last outcome again, e.g. when the
        /// page is shown.
        #[qinvokable]
        fn refresh(self: Pin<&mut BackupCopies>);

        /// The backups of the open book, newest first: `items`, each with
        /// `path` (to hand to `BookSession.restoreBackup`), `when` and
        /// `inChosen`, and `problem`, the code for why the chosen folder
        /// couldn't be read, or "".
        #[qinvokable]
        fn backups(self: &BackupCopies) -> QVariant;
    }

    impl cxx_qt::Initialize for BackupCopies {}
}

/// The Rust side of [`qobject::BackupCopies`].
#[derive(Default)]
pub struct BackupCopiesRust {
    chosen: bool,
    shown_as: QString,
    shown_is_real_path: bool,
    keep: QString,
    status: QVariant,
}

impl cxx_qt::Initialize for qobject::BackupCopies {
    fn initialize(self: Pin<&mut Self>) {
        self.refresh();
    }
}

impl qobject::BackupCopies {
    fn choose(mut self: Pin<&mut Self>, path: &QString) -> QVariant {
        let chosen = PathBuf::from(path.to_string());
        let code = match platform::choose_folder(&chosen, false) {
            Err(refused) => Some(refused.code()),
            Ok(grant) if is_own_backup_folder(&grant.path) => Some("backup-copy.same-folder"),
            Ok(grant) => (!backup_copies::choose(&Settings::user(), &grant))
                .then_some("backup-copy.not-saved"),
        };
        if let Some(code) = code {
            return map([("ok", QVariant::from(&false)), ("code", text(code))]);
        }
        backup_copies::keep_copy_of_open_book(&Settings::user(), &own_backup_folder());
        self.as_mut().refresh();
        map([("ok", QVariant::from(&true))])
    }

    fn forget(mut self: Pin<&mut Self>) -> bool {
        let saved = backup_copies::forget(&Settings::user());
        self.as_mut().refresh();
        saved
    }

    fn choose_keep(mut self: Pin<&mut Self>, choice: &QString) -> bool {
        let choice = choice.to_string();
        if !CHOICES.contains(&choice.as_str()) {
            return false;
        }
        let saved = backup_copies::set_keep_choice(&Settings::user(), &choice);
        self.as_mut().refresh();
        saved
    }

    fn try_again(mut self: Pin<&mut Self>) {
        backup_copies::keep_copy_of_open_book(&Settings::user(), &own_backup_folder());
        self.as_mut().refresh();
    }

    fn refresh(mut self: Pin<&mut Self>) {
        let settings = Settings::user();
        let chosen = backup_copies::chosen(&settings);
        let status = status_object(
            chosen.is_some(),
            backup_copies::last_outcome(),
            QtLocale::date_time,
        );
        let mut state = self.as_mut().rust_mut();
        state.chosen = chosen.is_some();
        state.shown_as = QString::from(
            chosen
                .as_ref()
                .map(|chosen| chosen.shown_as.as_str())
                .unwrap_or_default(),
        );
        state.shown_is_real_path = chosen
            .as_ref()
            .is_some_and(|chosen| chosen.shown_is_real_path);
        state.keep = QString::from(backup_copies::keep_choice(&settings));
        state.status = status;
        self.as_mut().chosen_changed();
        self.as_mut().shown_as_changed();
        self.as_mut().shown_is_real_path_changed();
        self.as_mut().keep_changed();
        self.as_mut().status_changed();
    }

    fn backups(&self) -> QVariant {
        let Some(book) = session::path() else {
            return backups_object(&[], None, QtLocale::date_time);
        };
        let chosen = backup_copies::chosen(&Settings::user()).map(|chosen| chosen.path);
        let (found, problem) =
            backup_copies::listed(&book, &own_backup_folder(), chosen.as_deref());
        backups_object(
            &found,
            problem.map(backup_copies::problem_code),
            QtLocale::date_time,
        )
    }
}

/// The app's own backup folder (D-033).
fn own_backup_folder() -> PathBuf {
    platform::computer().backup_folder
}

/// Whether `folder` is the app's own backup folder, or inside it, where the
/// copies would be pruned with the backups.
fn is_own_backup_folder(folder: &Path) -> bool {
    let own = own_backup_folder();
    let real = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
    real(folder).starts_with(real(&own))
}

/// `status` as QML reads it.
fn status_object(
    chosen: bool,
    outcome: Option<Outcome>,
    date_time: impl Fn(Timestamp) -> String,
) -> QVariant {
    if !chosen {
        return map([("state", text("off"))]);
    }
    match outcome {
        None => map([("state", text("waiting"))]),
        Some(Outcome::Kept { made, new }) => map([
            ("state", text("kept")),
            ("when", text(&date_time(made))),
            ("new", QVariant::from(&new)),
        ]),
        Some(Outcome::Problem(problem)) => map([
            ("state", text("problem")),
            ("code", text(backup_copies::problem_code(problem))),
        ]),
    }
}

/// `backups()` as QML reads it.
fn backups_object(
    found: &[Listed],
    problem: Option<&str>,
    date_time: impl Fn(Timestamp) -> String,
) -> QVariant {
    map([
        (
            "items",
            list(found.iter().map(|listed| {
                map([
                    ("path", text(&listed.path.to_string_lossy())),
                    ("when", text(&date_time(listed.made))),
                    ("inChosen", QVariant::from(&listed.in_chosen)),
                ])
            })),
        ),
        ("problem", text(problem.unwrap_or_default())),
    ])
}

#[cfg(test)]
mod tests {
    use kassensturz_store::CopyProblem;

    use super::*;
    use crate::variant::read_back::{flag_of, keys, list_of, text_of};

    fn stamp(seconds: i64) -> Timestamp {
        Timestamp::from_second(seconds).expect("a time")
    }

    fn shown(time: Timestamp) -> String {
        format!("at {}", time.as_second())
    }

    #[test]
    fn the_status_reads_back_as_qml_reads_it() {
        let off = status_object(false, None, shown);
        assert_eq!(keys(&off), ["state"]);
        assert_eq!(text_of(&off, "state"), "off");

        let waiting = status_object(true, None, shown);
        assert_eq!(text_of(&waiting, "state"), "waiting");

        let kept = status_object(
            true,
            Some(Outcome::Kept {
                made: stamp(1_792_407_842),
                new: true,
            }),
            shown,
        );
        assert_eq!(keys(&kept), ["new", "state", "when"]);
        assert_eq!(text_of(&kept, "state"), "kept");
        assert_eq!(text_of(&kept, "when"), "at 1792407842");
        assert!(flag_of(&kept, "new"));

        let problem = status_object(true, Some(Outcome::Problem(CopyProblem::Full)), shown);
        assert_eq!(keys(&problem), ["code", "state"]);
        assert_eq!(text_of(&problem, "state"), "problem");
        assert_eq!(text_of(&problem, "code"), "backup-copy.full");
    }

    #[test]
    fn the_backups_read_back_as_qml_reads_them() {
        let found = [
            Listed {
                path: PathBuf::from("/data/backups/Household 2026-10-03 09-00-00 UTC.ksz"),
                made: stamp(1_791_018_000),
                in_chosen: false,
            },
            Listed {
                path: PathBuf::from("/media/Stick/Household 2026-10-02 09-00-00 UTC.ksz"),
                made: stamp(1_790_931_600),
                in_chosen: true,
            },
        ];
        let object = backups_object(&found, Some("backup-copy.missing"), shown);
        assert_eq!(keys(&object), ["items", "problem"]);
        assert_eq!(text_of(&object, "problem"), "backup-copy.missing");
        let items = list_of(&object, "items");
        assert_eq!(items.len(), 2);
        assert_eq!(keys(&items[0]), ["inChosen", "path", "when"]);
        assert_eq!(
            text_of(&items[1], "path"),
            "/media/Stick/Household 2026-10-02 09-00-00 UTC.ksz"
        );
        assert_eq!(text_of(&items[1], "when"), "at 1790931600");
        assert!(!flag_of(&items[0], "inChosen"));
        assert!(flag_of(&items[1], "inChosen"));

        let empty = backups_object(&[], None, shown);
        assert!(list_of(&empty, "items").is_empty());
        assert_eq!(text_of(&empty, "problem"), "");
    }
}
