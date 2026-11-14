// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Undo, redo and the change history as a QML object (M3.8), on the book the
//! app has open (`session.rs`).
//!
//! QML gets every change as an action key and text (D-065); `ChangeNames.qml`
//! puts the key into words (D-034). Undo and redo change the book, so they
//! tell the other screens to read it again through the `changed` signal; the
//! history's own settings change no data and only refresh this object.

use core::pin::Pin;
use std::collections::HashMap;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QVariant};
use kassensturz_core::Timestamp;
use kassensturz_store::Book;

use crate::errors::ErrorText;
use crate::history::{self, ChangeText, HistoryError, SummaryText, View};
use crate::locale::QtLocale;
use crate::number_format::{UserLocale, app_locale};
use crate::receipt_moves::{self, Moved, StepError, Stayed};
use kassensturz_platform::bill_files::Settled;
use std::path::Path;
use crate::session;
use crate::variant::{error_object, list, map, text};

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
        /// The change history of the book the app has open.
        #[qobject]
        #[qml_element]
        #[qproperty(QVariant, undo_change, cxx_name = "undoChange", READ, NOTIFY)]
        #[qproperty(QVariant, redo_change, cxx_name = "redoChange", READ, NOTIFY)]
        #[qproperty(QVariant, changes, READ, NOTIFY)]
        #[qproperty(i32, total, READ, NOTIFY)]
        #[qproperty(i32, shown, READ, NOTIFY)]
        #[qproperty(QString, retention, READ, NOTIFY)]
        type ChangeHistory = super::ChangeHistoryRust;
    }

    extern "RustQt" {
        /// Reads the open book's history again, e.g. after another screen
        /// changed the book.
        #[qinvokable]
        fn refresh(self: Pin<&mut ChangeHistory>);

        /// Takes the newest change back. Returns an empty map if there was
        /// nothing to undo, the `change` it took back, or the error's `code`.
        #[qinvokable]
        fn undo(self: Pin<&mut ChangeHistory>) -> QVariant;

        /// Makes the change undone last again. Returns like `undo`.
        #[qinvokable]
        fn redo(self: Pin<&mut ChangeHistory>) -> QVariant;

        /// Takes the newest change back and leaves the bills it filed from
        /// the receipt inbox where they are: the deliberate way out when
        /// `undo` answered `bill.undo-blocked` (US7, #333). Returns like
        /// `undo`.
        #[qinvokable]
        #[cxx_name = "undoAnyway"]
        fn undo_anyway(self: Pin<&mut ChangeHistory>) -> QVariant;

        /// Settles the receipt inbox's moves a crash left open, by the
        /// facts and never by removing the last copy (US7, #333,
        /// `docs/RECEIPT_INBOX.md` §6). Returns what the user should hear:
        /// a list of objects with a `kind` (`unlinked`, `notWhereLinked`,
        /// `bothKept`, `onlyPart` or `gone`), a shown `path` and, for
        /// `bothKept`, the `other` path.
        #[qinvokable]
        #[cxx_name = "settleBills"]
        fn settle_bills(self: &ChangeHistory) -> QVariant;

        /// Sets how long the book keeps its history: "forever" or a number
        /// of days. Returns an empty map, or the error's `code`.
        #[qinvokable]
        #[cxx_name = "setRetention"]
        fn set_retention(self: Pin<&mut ChangeHistory>, key: &QString) -> QVariant;

        /// How many changes the setting `key` would delete from the book
        /// now; 0 if it isn't a setting.
        #[qinvokable]
        fn expiring(self: &ChangeHistory, key: &QString) -> i32;

        /// Deletes the whole history for good. Returns an empty map, or the
        /// error's `code`.
        #[qinvokable]
        #[cxx_name = "cleanUp"]
        fn clean_up(self: Pin<&mut ChangeHistory>) -> QVariant;
    }

    extern "RustQt" {
        /// The book changed through this object: the other screens read it
        /// again.
        #[qsignal]
        fn changed(self: Pin<&mut ChangeHistory>);
    }

    impl cxx_qt::Initialize for ChangeHistory {}
}

/// The Rust side of [`qobject::ChangeHistory`].
#[derive(Default)]
pub struct ChangeHistoryRust {
    undo_change: QVariant,
    redo_change: QVariant,
    changes: QVariant,
    total: i32,
    shown: i32,
    retention: QString,
}

impl cxx_qt::Initialize for qobject::ChangeHistory {
    fn initialize(self: Pin<&mut Self>) {
        self.refresh();
    }
}

impl qobject::ChangeHistory {
    fn refresh(mut self: Pin<&mut Self>) {
        let locale = app_locale();
        let read = session::with_book(|book| history::view(book, &date_time, &locale));
        let view = match read {
            Some(Ok(view)) => view,
            Some(Err(error)) => {
                eprintln!("kassensturz: couldn't read the history: {error:?}");
                return;
            }
            None => return,
        };
        self.as_mut().show(&view);
    }

    fn undo(self: Pin<&mut Self>) -> QVariant {
        self.step(false, false)
    }

    fn redo(self: Pin<&mut Self>) -> QVariant {
        self.step(true, false)
    }

    fn undo_anyway(self: Pin<&mut Self>) -> QVariant {
        self.step(false, true)
    }

    fn settle_bills(&self) -> QVariant {
        let Some(journal) = receipt_moves::journal() else {
            return list(std::iter::empty());
        };
        let disk = receipt_moves::disk();
        let mover = receipt_moves::Mover {
            disk: &disk,
            journal: &journal,
        };
        let settled = session::with_book(|book| mover.settle(book)).unwrap_or_default();
        settled_object(&settled, &receipt_moves::shown)
    }

    /// Undoes or, with `redo`, redoes one change with the bills it filed
    /// (`docs/RECEIPT_INBOX.md` §7), and shows the new state. `anyway`
    /// leaves the bills where they are.
    fn step(mut self: Pin<&mut Self>, redo: bool, anyway: bool) -> QVariant {
        let journal = receipt_moves::journal();
        let disk = receipt_moves::disk();
        let archive = kassensturz_core::ai_switch::inbox_folders().map(|(_, archive)| archive);
        let result = change(|book, locale| {
            let stepped = receipt_moves::step(
                book,
                redo,
                anyway,
                journal.as_ref(),
                &disk,
                archive.as_deref(),
                |book| {
                    if redo {
                        history::redo(book, &date_time, locale)
                    } else {
                        history::undo(book, &date_time, locale)
                    }
                },
            );
            match stepped {
                Ok((done, moved)) => Ok(Ok((done, moved))),
                Err(StepError::Blocked(stayed)) => Ok(Err(stayed)),
                Err(StepError::Book(error)) => Err(error),
            }
        });
        match result {
            Ok(Ok((Some(done), moved))) => {
                self.as_mut().refresh();
                self.as_mut().changed();
                stepped_object(&done, &moved)
            }
            Ok(Ok((None, _))) => map([]),
            Ok(Err(stayed)) => error_object(&blocked_text(redo, &stayed)),
            Err(error) => error_object(&error),
        }
    }

    fn set_retention(mut self: Pin<&mut Self>, key: &QString) -> QVariant {
        let key = key.to_string();
        match change(|book, _| history::set_retention(book, &key)) {
            Ok(()) => {
                self.as_mut().refresh();
                map([])
            }
            Err(error) => error_object(&error),
        }
    }

    fn expiring(&self, key: &QString) -> i32 {
        let key = key.to_string();
        session::with_book(|book| history::expiring(book, &key, Timestamp::now()))
            .and_then(Result::ok)
            .map_or(0, count)
    }

    fn clean_up(mut self: Pin<&mut Self>) -> QVariant {
        match change(|book, _| history::clean_up(book)) {
            Ok(()) => {
                self.as_mut().refresh();
                map([])
            }
            Err(error) => error_object(&error),
        }
    }

    /// Shows `view`.
    fn show(mut self: Pin<&mut Self>, view: &View) {
        let filed = filed_now();
        let optional = |change: Option<&ChangeText>| {
            change.map_or_else(|| map([]), |change| change_object_in(change, &filed))
        };
        let mut state = self.as_mut().rust_mut();
        state.undo_change = optional(view.undo.as_ref());
        state.redo_change = optional(view.redo.as_ref());
        state.changes = list(
            view.changes
                .iter()
                .map(|change| change_object_in(change, &filed)),
        );
        state.total = count(view.total);
        state.shown = count(view.changes.len());
        state.retention = QString::from(view.retention.as_str());
        self.as_mut().undo_change_changed();
        self.as_mut().redo_change_changed();
        self.as_mut().changes_changed();
        self.as_mut().total_changed();
        self.as_mut().shown_changed();
        self.as_mut().retention_changed();
    }
}

/// Changes the open book, or says why not.
fn change<T>(
    f: impl FnOnce(&mut Book, &UserLocale<QtLocale>) -> Result<T, HistoryError>,
) -> Result<T, ErrorText> {
    let locale = app_locale();
    session::with_book_mut(|book| {
        f(book, &locale).map_err(|error| history::error_text(book, &error, &locale))
    })
    .unwrap_or_else(|| Err(ErrorText::plain("input.no-such-entry")))
}

/// `time` in the user's time zone and format.
fn date_time(time: Timestamp) -> String {
    QtLocale::date_time(time)
}

fn count(value: usize) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

/// What undo or redo did as QML reads it: the `change`, and the bills it
/// moved (`bills`), those it left in the archive (`leftInArchive`) and the
/// old copies it couldn't remove (`copiesLeft`), as shown paths.
fn stepped_object(change: &ChangeText, moved: &Moved) -> QVariant {
    let shown = |paths: &[std::path::PathBuf]| {
        list(
            paths
                .iter()
                .map(|path| text(&receipt_moves::shown(path))),
        )
    };
    map([
        ("change", change_object(change)),
        ("bills", QVariant::from(&count(moved.moved))),
        ("leftInArchive", shown(&moved.left_in_archive)),
        ("copiesLeft", shown(&moved.copies_left)),
    ])
}

/// What settling found, as QML reads it (`settleBills`), with paths as
/// `shown` writes them.
fn settled_object(settled: &[Settled], shown: &dyn Fn(&Path) -> String) -> QVariant {
    list(settled.iter().filter_map(|settled| {
        let (kind, path, other) = match settled {
            Settled::Quiet => return None,
            Settled::UnlinkedInArchive(path) => ("unlinked", path, None),
            Settled::NotWhereLinked(path) => ("notWhereLinked", path, None),
            Settled::BothKept(path, other) => ("bothKept", path, Some(other)),
            Settled::OnlyPartLeft(path) => ("onlyPart", path, None),
            Settled::Gone(path) => ("gone", path, None),
        };
        Some(map([
            ("kind", text(kind)),
            ("path", text(&shown(path))),
            ("other", text(&other.map(|other| shown(other)).unwrap_or_default())),
        ]))
    }))
}

/// Why an undo or redo moved nothing (§7): `bill.undo-blocked` or
/// `bill.redo-blocked`, with the bills' file names (`files`, one per line)
/// and why the first can't move (`reason`).
fn blocked_text(redo: bool, stayed: &[Stayed]) -> ErrorText {
    let files: Vec<String> = stayed
        .iter()
        .map(|bill| receipt_moves::shown(&bill.file))
        .collect();
    ErrorText {
        code: if redo {
            "bill.redo-blocked"
        } else {
            "bill.undo-blocked"
        },
        params: vec![
            ("files", files.join("\n")),
            (
                "reason",
                stayed.first().map_or("", |bill| bill.reason).to_owned(),
            ),
        ],
    }
}

/// How many bills each change filed, by its ID, from this computer's
/// journal.
fn filed_now() -> HashMap<String, usize> {
    receipt_moves::journal()
        .map(|journal| receipt_moves::filed_counts(&journal))
        .unwrap_or_default()
}

fn change_object(change: &ChangeText) -> QVariant {
    change_object_in(change, &filed_now())
}

/// The change as QML reads it; `filed` says how many bills it filed from
/// the receipt inbox (`filedBills`, US7, #333).
fn change_object_in(change: &ChangeText, filed: &HashMap<String, usize>) -> QVariant {
    let filed = filed.get(&change.id).copied().unwrap_or(0);
    map([
        ("filedBills", QVariant::from(&count(filed))),
        ("action", text(change.action)),
        ("madeAt", text(&change.made_at)),
        ("undone", QVariant::from(&change.undone)),
        ("fileName", text(&change.file_name)),
        (
            "summary",
            change
                .summary
                .as_ref()
                .map_or_else(|| map([]), summary_object),
        ),
    ])
}

fn summary_object(summary: &SummaryText) -> QVariant {
    map([
        ("added", QVariant::from(&count(summary.added))),
        ("removed", QVariant::from(&count(summary.removed))),
        ("changed", QVariant::from(&count(summary.changed))),
        (
            "accounts",
            list(summary.accounts.iter().map(|name| text(name))),
        ),
        ("firstDate", text(&summary.first_date)),
        ("lastDate", text(&summary.last_date)),
    ])
}
