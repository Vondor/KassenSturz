// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The saved views as a QML object (M6a.7, D-278, D-279), on the book the
//! app has open (`session.rs`). The work is in `saved_views.rs`; this file
//! turns it into plain values. A change tells the other screens to read the
//! book again through the `changed` signal, and is in the change history, so
//! the menu's Undo takes it back.
//!
//! `view` holds `views`: one object per saved view with `id`, `name`,
//! `usable`, `kind` and `dataset`. The calls that can fail return an empty
//! object when they worked (`open` and `save` return what they made) or an
//! object with the error's `code` and its parameters as text.

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QVariant};
use kassensturz_core::Report;
use kassensturz_store::Book;

use crate::errors::{ErrorText, report_text};
use crate::locale::QtLocale;
use crate::number_format::{UserLocale, app_locale};
use crate::saved_views::{self, Opened, Row, ViewsError};
use crate::session;
use crate::variant::{error_object, list, map, text};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// The views saved in the open book.
        #[qobject]
        #[qml_element]
        #[qproperty(QVariant, view, READ, NOTIFY)]
        type SavedViews = super::SavedViewsRust;
    }

    extern "RustQt" {
        /// Reads the open book again, e.g. after Undo.
        #[qinvokable]
        fn refresh(self: Pin<&mut SavedViews>);

        /// Saves what is on the workbench (a component as JSON text and one
        /// of the fixed period names, or empty) as a new view. Returns the
        /// new view's `id`, or the error.
        #[qinvokable]
        fn save(
            self: Pin<&mut SavedViews>,
            name: &QString,
            period: &QString,
            component: &QString,
        ) -> QVariant;

        /// Opens a saved view: returns `id`, `name`, `period` and the
        /// `component` as JSON text for `ComponentProperties.load`, or the
        /// error.
        #[qinvokable]
        fn open(self: &SavedViews, id: &QString) -> QVariant;

        /// Replaces what a saved view shows; its name stays.
        #[qinvokable]
        fn update(
            self: Pin<&mut SavedViews>,
            id: &QString,
            period: &QString,
            component: &QString,
        ) -> QVariant;

        /// Gives a saved view another name.
        #[qinvokable]
        fn rename(self: Pin<&mut SavedViews>, id: &QString, name: &QString) -> QVariant;

        /// Deletes a saved view. Undo brings it back.
        #[qinvokable]
        fn remove(self: Pin<&mut SavedViews>, id: &QString) -> QVariant;
    }

    extern "RustQt" {
        /// The book changed through this object: the other screens read it
        /// again.
        #[qsignal]
        fn changed(self: Pin<&mut SavedViews>);
    }
}

/// The Rust side of [`qobject::SavedViews`].
#[derive(Default)]
pub struct SavedViewsRust {
    view: QVariant,
}

fn row_object(row: &Row) -> QVariant {
    map([
        ("id", text(&row.id)),
        ("name", text(&row.name)),
        ("usable", QVariant::from(&row.usable)),
        ("kind", text(&row.kind)),
        ("dataset", text(&row.dataset)),
    ])
}

fn view_object(rows: &[Row]) -> QVariant {
    map([("views", list(rows.iter().map(row_object)))])
}

fn opened_object(opened: &Opened) -> QVariant {
    map([
        ("id", text(&opened.id)),
        ("name", text(&opened.name)),
        ("period", text(&opened.period)),
        ("component", text(&opened.component)),
    ])
}

/// The error as QML gets it. A view that is not in the book anymore is the
/// same message as for a Rule or an account that was removed meanwhile.
fn error_text(book: &Book, error: &ViewsError, locale: &UserLocale<QtLocale>) -> ErrorText {
    match error {
        ViewsError::Store(error) => report_text(book, &error.report(), locale),
        ViewsError::Report(error) => report_text(book, &error.report(), locale),
        ViewsError::NoSuchView => ErrorText::plain("input.no-such-entry"),
    }
}

/// Reads the open book, or says why not.
fn read_book<T>(f: impl FnOnce(&Book) -> Result<T, ViewsError>) -> Result<T, ErrorText> {
    let locale = app_locale();
    session::with_book(|book| f(book).map_err(|error| error_text(book, &error, &locale)))
        .unwrap_or_else(|| Err(ErrorText::plain("input.no-such-entry")))
}

/// Changes the open book, or says why not.
fn change<T>(f: impl FnOnce(&mut Book) -> Result<T, ViewsError>) -> Result<T, ErrorText> {
    let locale = app_locale();
    session::with_book_mut(|book| f(book).map_err(|error| error_text(book, &error, &locale)))
        .unwrap_or_else(|| Err(ErrorText::plain("input.no-such-entry")))
}

impl qobject::SavedViews {
    fn refresh(mut self: Pin<&mut Self>) {
        let view = match read_book(|book| Ok(saved_views::list(book)?)) {
            Ok(rows) => view_object(&rows),
            Err(error) => error_object(&error),
        };
        self.as_mut().rust_mut().view = view;
        self.as_mut().view_changed();
    }

    /// Shows the new state after a change, or returns why it failed.
    fn finish(mut self: Pin<&mut Self>, result: Result<QVariant, ErrorText>) -> QVariant {
        match result {
            Ok(done) => {
                self.as_mut().refresh();
                self.as_mut().changed();
                done
            }
            Err(error) => error_object(&error),
        }
    }

    fn save(
        self: Pin<&mut Self>,
        name: &QString,
        period: &QString,
        component: &QString,
    ) -> QVariant {
        let (name, period, component) =
            (name.to_string(), period.to_string(), component.to_string());
        let result = change(|book| saved_views::save(book, &name, &period, &component));
        self.finish(result.map(|id| map([("id", text(&id))])))
    }

    fn open(&self, id: &QString) -> QVariant {
        let id = id.to_string();
        match read_book(|book| saved_views::open(book, &id)) {
            Ok(opened) => opened_object(&opened),
            Err(error) => error_object(&error),
        }
    }

    fn update(
        self: Pin<&mut Self>,
        id: &QString,
        period: &QString,
        component: &QString,
    ) -> QVariant {
        let (id, period, component) = (id.to_string(), period.to_string(), component.to_string());
        let result = change(|book| saved_views::update(book, &id, &period, &component));
        self.finish(result.map(|()| map([])))
    }

    fn rename(self: Pin<&mut Self>, id: &QString, name: &QString) -> QVariant {
        let (id, name) = (id.to_string(), name.to_string());
        let result = change(|book| saved_views::rename(book, &id, &name));
        self.finish(result.map(|()| map([])))
    }

    fn remove(self: Pin<&mut Self>, id: &QString) -> QVariant {
        let id = id.to_string();
        let result = change(|book| saved_views::remove(book, &id));
        self.finish(result.map(|()| map([])))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variant::read_back::{flag_of, keys, list_of, text_of};

    #[test]
    fn the_list_reads_back_the_way_qml_reads_it() {
        let rows = [
            Row {
                id: "a".into(),
                name: "Spending".into(),
                usable: true,
                kind: "chart.pie".into(),
                dataset: "spending_by_category".into(),
            },
            Row {
                id: "b".into(),
                name: "Radar".into(),
                usable: false,
                kind: String::new(),
                dataset: String::new(),
            },
        ];
        let view = view_object(&rows);
        assert_eq!(keys(&view), ["views"]);
        let shown = list_of(&view, "views");
        assert_eq!(shown.len(), 2);
        assert_eq!(keys(&shown[0]), ["dataset", "id", "kind", "name", "usable"]);
        assert_eq!(text_of(&shown[0], "name"), "Spending");
        assert!(flag_of(&shown[0], "usable"));
        assert!(!flag_of(&shown[1], "usable"));
    }

    #[test]
    fn an_opened_view_reads_back_with_its_component_as_text() {
        let opened = Opened {
            id: "a".into(),
            name: "Spending".into(),
            period: "this-year".into(),
            component: "{}".into(),
        };
        let view = opened_object(&opened);
        assert_eq!(keys(&view), ["component", "id", "name", "period"]);
        assert_eq!(text_of(&view, "period"), "this-year");
        assert_eq!(text_of(&view, "component"), "{}");
    }

    #[test]
    fn errors_have_a_code_a_screen_can_put_into_words() {
        let locale = app_locale();
        let book = Book::create_in_memory().unwrap();
        let taken =
            ViewsError::Store(kassensturz_store::StoreError::ViewNameTaken { name: "A".into() });
        let text_of_error = error_text(&book, &taken, &locale);
        assert_eq!(text_of_error.code, "store.view-name-taken");
        assert_eq!(text_of_error.params, [("name", "A".to_owned())]);
        let invalid = ViewsError::Report(kassensturz_report::ReportError::DocumentInvalid {
            reason: "bad-choice",
            path: "filters.period".into(),
        });
        assert_eq!(
            error_text(&book, &invalid, &locale).code,
            "report.document-invalid"
        );
        assert_eq!(
            error_text(&book, &ViewsError::NoSuchView, &locale).code,
            "input.no-such-entry"
        );
    }
}
