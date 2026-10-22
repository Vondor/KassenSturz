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

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QVariant};
use kassensturz_core::Timestamp;
use kassensturz_store::Book;

use crate::errors::ErrorText;
use crate::history::{self, ChangeText, HistoryError, SummaryText, View};
use crate::locale::QtLocale;
use crate::number_format::{UserLocale, app_locale};
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
        self.step(false)
    }

    fn redo(self: Pin<&mut Self>) -> QVariant {
        self.step(true)
    }

    /// Undoes or, with `redo`, redoes one change, and shows the new state.
    fn step(mut self: Pin<&mut Self>, redo: bool) -> QVariant {
        let result = change(|book, locale| {
            if redo {
                history::redo(book, &date_time, locale)
            } else {
                history::undo(book, &date_time, locale)
            }
        });
        match result {
            Ok(Some(done)) => {
                self.as_mut().refresh();
                self.as_mut().changed();
                map([("change", change_object(&done))])
            }
            Ok(None) => map([]),
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
        let mut state = self.as_mut().rust_mut();
        state.undo_change = optional(view.undo.as_ref());
        state.redo_change = optional(view.redo.as_ref());
        state.changes = list(view.changes.iter().map(change_object));
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

/// The change as an object, or an empty one for none.
fn optional(change: Option<&ChangeText>) -> QVariant {
    change.map_or_else(|| map([]), change_object)
}

fn change_object(change: &ChangeText) -> QVariant {
    map([
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
