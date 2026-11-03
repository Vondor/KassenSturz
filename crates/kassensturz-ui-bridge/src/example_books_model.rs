// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The QML object behind the example-book chooser (M7a.2): which books come
//! with the app, and making a new book from one. The work is in
//! `example_books.rs`; this file only moves text between it and QML (D-065).

use core::pin::Pin;

use cxx_qt_lib::{QString, QVariant};

use crate::book_from_file_model::{fail, problem_object};
use crate::books::Books;
use crate::errors::ErrorText;
use crate::example_books::{self, ExampleError};
use crate::number_format::app_locale;
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
        /// The example books that come with the app.
        #[qobject]
        #[qml_element]
        type ExampleBooks = super::ExampleBooksRust;
    }

    extern "RustQt" {
        /// The bundled books, in the order to show them: a list of objects
        /// with `id` and `suggested` (whether it is the one to start with).
        /// Names and descriptions are written in QML.
        #[qinvokable]
        fn books(self: &ExampleBooks) -> QVariant;

        /// Makes a new book called `name` from the example `id` and opens
        /// it (`created` follows). Any book that is open is left as it is,
        /// and the bundled book is never opened itself. An error object says
        /// why it didn't work, and nothing was made; otherwise the object is
        /// empty.
        #[qinvokable]
        fn make(self: Pin<&mut ExampleBooks>, id: &QString, name: &QString) -> QVariant;
    }

    unsafe extern "RustQt" {
        /// A new book was made and is open: the window shows it.
        #[qsignal]
        fn created(self: Pin<&mut ExampleBooks>);

    }
}

/// The Rust side of [`qobject::ExampleBooks`]. It holds nothing: the book
/// made is the session's.
#[derive(Default)]
pub struct ExampleBooksRust;

impl qobject::ExampleBooks {
    fn books(&self) -> QVariant {
        books_object()
    }

    fn make(self: Pin<&mut Self>, id: &QString, name: &QString) -> QVariant {
        let locale = app_locale();
        match example_books::create(
            &Books::for_user(),
            &id.to_string(),
            &name.to_string(),
            &locale,
        ) {
            Ok((open, _)) => {
                session::set(open);
                self.created();
                map([])
            }
            Err(error) => failure(&error),
        }
    }
}

/// The list of bundled books as QML reads it.
fn books_object() -> QVariant {
    list(example_books::all().iter().map(|example| {
        map([
            ("id", text(example.id)),
            (
                "suggested",
                QVariant::from(&(example.id == example_books::SUGGESTED)),
            ),
        ])
    }))
}

/// Why a book wasn't made, as QML reads it.
fn failure(error: &ExampleError) -> QVariant {
    match error {
        ExampleError::Unknown => error_object(&ErrorText::plain("input.no-such-entry")),
        ExampleError::Fill(error) => fail(error),
        ExampleError::Problem(problem) => problem_object(problem),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book_from_file::FromFileError;
    use crate::variant::read_back::{elements, flag_of, keys, text_of};

    #[test]
    fn the_list_of_books_reads_back_as_qml_reads_it() {
        let rows = elements(&books_object()).expect("a list");
        assert_eq!(rows.len(), example_books::all().len());
        let first = &rows[0];
        assert_eq!(keys(first), ["id", "suggested"]);
        assert_eq!(text_of(first, "id"), example_books::all()[0].id);
        assert!(!flag_of(first, "suggested") || example_books::all()[0].id == "suburban-family");
    }

    #[test]
    fn a_failure_reads_back_with_its_code() {
        let unknown = failure(&ExampleError::Unknown);
        assert_eq!(text_of(&unknown, "code"), "input.no-such-entry");
        let fill = failure(&ExampleError::Fill(FromFileError::NoSuchEntry));
        assert_eq!(text_of(&fill, "code"), "input.no-such-entry");
        let problem = failure(&ExampleError::Problem(crate::books::ProblemText {
            code: "book.no-app-folder".into(),
            params: Vec::new(),
        }));
        assert_eq!(text_of(&problem, "code"), "book.no-app-folder");
    }
}
