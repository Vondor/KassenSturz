// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The book's tags as a QML object, for Settings › Tags (US4.8,
//! docs/TAGS.md §1): each tag with how often it is used, and renaming,
//! merging and removing one. Each change is one user action that one Undo
//! takes back (D-086, Q-495). Errors reach QML as codes (D-034).

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QVariant};
use kassensturz_core::Report;
use kassensturz_store::{Book, StoreError};

use crate::errors::{ErrorText, report_text};
use crate::locale::QtLocale;
use crate::number_format::{UserLocale, app_locale};
use crate::register_model::tag_choices_object;
use crate::session;
use crate::tags;
use crate::variant::{error_object, map};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt's string type.
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qvariant.h");
        /// Qt's variant type.
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// The book's tags, sorted by name: a list of `{id, name,
        /// transactions, parts}`, where the two counts say how many
        /// transactions carry the tag on the whole of them and how many
        /// parts carry it as their own.
        #[qobject]
        #[qml_element]
        #[qproperty(QVariant, tags, READ, NOTIFY)]
        type BookTags = super::BookTagsRust;
    }

    extern "RustQt" {
        /// Reads the tags again, e.g. after Undo.
        #[qinvokable]
        fn refresh(self: Pin<&mut BookTags>);

        /// Gives the tag `id` the name `name`. Returns an empty map, or the
        /// error (`store.tag-name`, `store.duplicate-tag-name`, ...).
        #[qinvokable]
        fn rename(self: Pin<&mut BookTags>, id: &QString, name: &QString) -> QVariant;

        /// Removes the tag `id` from every transaction and part and from
        /// the book. Returns an empty map, or the error.
        #[qinvokable]
        fn remove(self: Pin<&mut BookTags>, id: &QString) -> QVariant;

        /// Puts the tag `into` wherever `from` is and removes `from`.
        /// Returns an empty map, or the error.
        #[qinvokable]
        fn merge(self: Pin<&mut BookTags>, from: &QString, into: &QString) -> QVariant;
    }

    extern "RustQt" {
        /// The book changed through this object: the other screens read it
        /// again.
        #[qsignal]
        fn changed(self: Pin<&mut BookTags>);
    }

    impl cxx_qt::Initialize for BookTags {}
}

/// The Rust side of [`qobject::BookTags`].
#[derive(Default)]
pub struct BookTagsRust {
    tags: QVariant,
}

impl cxx_qt::Initialize for qobject::BookTags {
    fn initialize(self: Pin<&mut Self>) {
        self.refresh();
    }
}

impl qobject::BookTags {
    fn refresh(mut self: Pin<&mut Self>) {
        let choices = match session::with_book(tags::tag_choices) {
            Some(Ok(choices)) => choices,
            Some(Err(error)) => {
                eprintln!("kassensturz: couldn't read the tags: {error:?}");
                Vec::new()
            }
            None => Vec::new(),
        };
        self.as_mut().rust_mut().tags = tag_choices_object(&choices);
        self.as_mut().tags_changed();
    }

    fn rename(self: Pin<&mut Self>, id: &QString, name: &QString) -> QVariant {
        let (id, name) = (id.to_string(), name.to_string());
        let result = change(|book| tags::rename(book, &id, &name));
        self.finish(result)
    }

    fn remove(self: Pin<&mut Self>, id: &QString) -> QVariant {
        let id = id.to_string();
        let result = change(|book| tags::remove(book, &id));
        self.finish(result)
    }

    fn merge(self: Pin<&mut Self>, from: &QString, into: &QString) -> QVariant {
        let (from, into) = (from.to_string(), into.to_string());
        let result = change(|book| tags::merge(book, &from, &into));
        self.finish(result)
    }

    /// Shows the new state after a change, or returns why it failed.
    fn finish(mut self: Pin<&mut Self>, result: Result<(), ErrorText>) -> QVariant {
        match result {
            Ok(()) => {
                self.as_mut().refresh();
                self.as_mut().changed();
                map([])
            }
            Err(error) => error_object(&error),
        }
    }
}

/// Changes the open book, or says why not.
fn change(f: impl FnOnce(&mut Book) -> Result<(), StoreError>) -> Result<(), ErrorText> {
    let locale: UserLocale<QtLocale> = app_locale();
    session::with_book_mut(|book| {
        f(book).map_err(|error| report_text(book, &error.report(), &locale))
    })
    .unwrap_or_else(|| Err(ErrorText::plain("input.no-such-entry")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tags::TagChoice;
    use crate::tags::TagText;
    use crate::variant::read_back::{elements, keys, number_of, text_of};

    #[test]
    fn the_tags_object_reads_back_as_qml_reads_it() {
        let object = tag_choices_object(&[TagChoice {
            tag: TagText {
                id: "tag-1".into(),
                name: "Holiday".into(),
            },
            transactions: 3,
            parts: 2,
        }]);
        let rows = elements(&object).expect("a list");
        assert_eq!(rows.len(), 1);
        assert_eq!(keys(&rows[0]), ["id", "name", "parts", "transactions"]);
        assert_eq!(text_of(&rows[0], "name"), "Holiday");
        assert_eq!(number_of(&rows[0], "transactions"), 3);
        assert_eq!(number_of(&rows[0], "parts"), 2);
    }
}
