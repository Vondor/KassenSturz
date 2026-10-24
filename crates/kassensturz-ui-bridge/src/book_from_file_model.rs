// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The QML object behind starting a book from a file (M3.12): from choosing
//! the file to the report of what came in and what was left out. The work is
//! in `book_from_file.rs`; this file only moves text between it and QML
//! (D-065).

use core::pin::Pin;
use std::path::PathBuf;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QList, QMap, QMapPair_QString_QVariant, QString, QStringList, QUrl, QVariant};
use kassensturz_core::CurrencyCode;

use crate::book_from_file::{
    self, Answer, Created, FromFileError, Prepared, PreparedView, UnlistedView,
};
use crate::book_session::local_path;
use crate::books::{Books, Filled};
use crate::errors::ErrorText;
use crate::number_format::app_locale;
use crate::platform;
use crate::session;
use crate::variant::{error_object, list, map, text};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;

        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// Starting a book from a complete export or from KMyMoney's book.
        /// `step` says which screen is next: `ready` (the file was read and
        /// waits for the user's word) or `done`, empty when nothing is
        /// loaded. `view` holds what that screen shows, all text.
        #[qobject]
        #[qml_element]
        #[qproperty(QString, step, READ, NOTIFY)]
        #[qproperty(QVariant, view, READ, NOTIFY)]
        #[qproperty(QStringList, currencies, READ, CONSTANT)]
        type BookFromFile = super::BookFromFileRust;
    }

    extern "RustQt" {
        /// Reads `file` and says what it holds. An error object says why it
        /// can't be used; otherwise `step` is `ready`.
        #[qinvokable]
        fn start(self: Pin<&mut BookFromFile>, file: &QUrl) -> QVariant;

        /// Makes the book from the file read, with the user's `answers` about
        /// the currencies KassenSturz doesn't know: a list of objects with
        /// `currency`, `successor`, `rate` and `date`. The open book is
        /// filled if nothing is in it yet, otherwise a new book is made and
        /// opened. An error object says why it didn't work, and nothing was
        /// made.
        #[qinvokable]
        fn create(self: Pin<&mut BookFromFile>, answers: &QVariant) -> QVariant;

        /// Like `create`, but a new book goes where the user chose, `file`,
        /// instead of the data folder (D-125). If the open book is filled,
        /// `file` is not used.
        #[qinvokable]
        #[cxx_name = "createAt"]
        fn create_at(self: Pin<&mut BookFromFile>, answers: &QVariant, file: &QUrl) -> QVariant;

        /// Forgets the file.
        #[qinvokable]
        fn cancel(self: Pin<&mut BookFromFile>);
    }

    unsafe extern "RustQt" {
        /// The open book was filled: every screen reads it again.
        #[qsignal]
        fn changed(self: Pin<&mut BookFromFile>);

        /// A new book was made and is open: the window shows it.
        #[qsignal]
        fn created(self: Pin<&mut BookFromFile>);
    }

    impl cxx_qt::Initialize for BookFromFile {}
}

/// The Rust side of [`qobject::BookFromFile`].
#[derive(Default)]
pub struct BookFromFileRust {
    step: QString,
    view: QVariant,
    currencies: QStringList,
    prepared: Option<Prepared>,
}

impl cxx_qt::Initialize for qobject::BookFromFile {
    fn initialize(mut self: Pin<&mut Self>) {
        // The current currencies a replaced one can be answered with.
        self.as_mut().rust_mut().currencies = CurrencyCode::iso_codes()
            .map(|code| QString::from(code.as_str()))
            .collect();
    }
}

impl qobject::BookFromFile {
    fn start(mut self: Pin<&mut Self>, file: &QUrl) -> QVariant {
        self.as_mut().cancel();
        let Some(path) = local_path(file) else {
            return error_object(&ErrorText::plain("input.not-a-local-file"));
        };
        let prepared = match book_from_file::read(&path) {
            Ok(prepared) => prepared,
            Err(error) => return fail(&error),
        };
        let locale = app_locale();
        let into_open = into_open_book();
        let view = prepared_object(&prepared.view(&locale), into_open, &prepared.stem);
        self.as_mut().rust_mut().prepared = Some(prepared);
        self.set_step("ready", view);
        map([])
    }

    fn create(self: Pin<&mut Self>, answers: &QVariant) -> QVariant {
        self.make(answers, None)
    }

    fn create_at(self: Pin<&mut Self>, answers: &QVariant, file: &QUrl) -> QVariant {
        let Some(path) = local_path(file) else {
            return error_object(&ErrorText::plain("input.not-a-local-file"));
        };
        self.make(answers, Some(path))
    }

    /// Makes the book, in the data folder or at `place`.
    fn make(mut self: Pin<&mut Self>, answers: &QVariant, place: Option<PathBuf>) -> QVariant {
        let Some(prepared) = self.rust().prepared.as_ref() else {
            return error_object(&ErrorText::plain("input.no-such-entry"));
        };
        let locale = app_locale();
        let replaced = match book_from_file::replacements(prepared, &read_answers(answers), &locale)
        {
            Ok(replaced) => replaced,
            Err(error) => return fail(&error),
        };
        let previous = session::path();
        let made = if into_open_book() {
            session::with_book_mut(|book| book_from_file::fill(book, prepared, &replaced, &locale))
                .unwrap_or(Err(FromFileError::NoSuchEntry))
                .map(|created| (created, false))
        } else {
            let books = Books::for_user();
            let fill = |book: &mut _| book_from_file::fill(book, prepared, &replaced, &locale);
            let made = match &place {
                Some(place) => books.create_filled_at(place, fill),
                None => books.create_filled(&prepared.stem, fill),
            };
            match made {
                Ok((open, created)) => {
                    session::set(open);
                    Ok((created, true))
                }
                Err(Filled::Fill(error)) => Err(error),
                Err(Filled::Problem(problem)) => return problem_object(&problem.text()),
            }
        };
        match made {
            Ok((created, new_book)) => {
                let shown = created_object(&created, new_book, previous.as_deref());
                self.as_mut().rust_mut().prepared = None;
                self.as_mut().set_step("done", shown);
                if new_book {
                    self.as_mut().created();
                } else {
                    self.as_mut().changed();
                }
                map([])
            }
            Err(error) => fail(&error),
        }
    }

    fn cancel(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().prepared = None;
        self.set_step("", QVariant::default());
    }

    fn set_step(mut self: Pin<&mut Self>, step: &str, view: QVariant) {
        {
            let mut rust = self.as_mut().rust_mut();
            rust.step = QString::from(step);
            rust.view = view;
        }
        self.as_mut().step_changed();
        self.as_mut().view_changed();
    }
}

/// Whether the open book can be filled in place: nothing is in it yet.
fn into_open_book() -> bool {
    session::with_book(|book| book.is_empty().unwrap_or(false)).unwrap_or(false)
}

/// Says why a file couldn't be used.
fn fail(error: &FromFileError) -> QVariant {
    let locale = app_locale();
    let shown =
        session::with_book(|book| book_from_file::error_text_in(Some(book), error, &locale))
            .unwrap_or_else(|| book_from_file::error_text_in(None, error, &locale));
    error_object(&shown)
}

/// A problem making the new book, as the book screens get it.
fn problem_object(problem: &crate::books::ProblemText) -> QVariant {
    let mut entries = vec![("code".to_owned(), text(&problem.code))];
    entries.extend(
        problem
            .params
            .iter()
            .map(|(name, value)| (name.clone(), text(value))),
    );
    let mut object = QMap::<QMapPair_QString_QVariant>::default();
    for (key, value) in entries {
        object.insert(QString::from(key.as_str()), value);
    }
    QVariant::from(&object)
}

fn count(value: usize) -> QVariant {
    QVariant::from(&i32::try_from(value).unwrap_or(i32::MAX))
}

fn prepared_object(view: &PreparedView, into_open: bool, stem: &str) -> QVariant {
    map([
        ("fileName", text(&view.file_name)),
        ("kind", text(view.kind.keyword())),
        ("accounts", count(view.accounts)),
        ("categories", count(view.categories)),
        ("transactions", count(view.transactions)),
        ("unlisted", list(view.unlisted.iter().map(unlisted_object))),
        ("intoOpenBook", QVariant::from(&into_open)),
        (
            "canChoosePlace",
            QVariant::from(&(!into_open && platform::can_choose_book_place())),
        ),
        ("bookName", text(stem)),
    ])
}

fn unlisted_object(unlisted: &UnlistedView) -> QVariant {
    let suggestion = unlisted.suggestion.as_ref().map_or_else(
        || map([]),
        |suggestion| {
            map([
                ("successor", text(&suggestion.successor)),
                ("rate", text(&suggestion.rate)),
                ("date", text(&suggestion.date)),
            ])
        },
    );
    map([
        ("currency", text(&unlisted.currency)),
        ("suggestion", suggestion),
        (
            "hasSuggestion",
            QVariant::from(&unlisted.suggestion.is_some()),
        ),
    ])
}

fn created_object(
    created: &Created,
    new_book: bool,
    previous: Option<&std::path::Path>,
) -> QVariant {
    let names = |names: &[String]| list(names.iter().map(|name| text(name)));
    let (file_name, folder) = session::path().map_or_else(Default::default, |path| {
        (
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            path.parent()
                .map(|folder| folder.display().to_string())
                .unwrap_or_default(),
        )
    });
    let previous_name = previous
        .and_then(std::path::Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    map([
        ("newBook", QVariant::from(&new_book)),
        ("fileName", text(&file_name)),
        ("folder", text(&folder)),
        ("previousFileName", text(&previous_name)),
        ("accounts", count(created.accounts)),
        ("categories", count(created.categories)),
        ("transactions", count(created.transactions)),
        ("rates", count(created.rates)),
        ("uncategorized", count(created.uncategorized)),
        ("completed", count(created.completed)),
        ("renamed", names(&created.renamed)),
        ("movedToTop", names(&created.moved_to_top)),
        ("leftOpen", names(&created.left_open)),
        (
            "converted",
            list(created.converted.iter().map(|converted| {
                map([
                    ("old", text(&converted.old)),
                    ("new", text(&converted.new)),
                    ("rate", text(&converted.rate)),
                    ("date", text(&converted.date)),
                    ("accounts", count(converted.accounts)),
                    ("parts", count(converted.parts)),
                ])
            })),
        ),
        (
            "skipped",
            list(
                created
                    .skipped
                    .iter()
                    .map(|(what, number)| map([("what", text(what)), ("count", count(*number))])),
            ),
        ),
        ("transfers", count(created.transfers)),
        ("rules", count(created.rules)),
        (
            "patternsLeftOut",
            list(created.patterns_left_out.iter().map(|left_out| {
                map([
                    ("payee", text(&left_out.payee)),
                    ("pattern", text(&left_out.pattern)),
                    ("ruleMade", QVariant::from(&left_out.rule_made)),
                ])
            })),
        ),
    ])
}

/// The answers as QML sends them: a list of objects, every field as text.
fn read_answers(answers: &QVariant) -> Vec<Answer> {
    let Some(list) = answers.value::<QList<QVariant>>() else {
        return Vec::new();
    };
    list.iter()
        .map(|answer| {
            let object = answer
                .value::<QMap<QMapPair_QString_QVariant>>()
                .unwrap_or_default();
            let field = |key: &str| crate::register_model::field(&object, key);
            Answer {
                currency: field("currency"),
                successor: field("successor"),
                rate: field("rate"),
                date: field("date"),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book_from_file::PatternText;
    use crate::variant::read_back::{flag_of, keys, list_of, number_of, text_of};

    #[test]
    fn what_came_in_reads_back_as_qml_reads_it() {
        let created = Created {
            rules: 2,
            patterns_left_out: vec![
                PatternText {
                    payee: "Corner Grocery".into(),
                    pattern: "^(?!x)Corner$".into(),
                    rule_made: true,
                },
                PatternText {
                    payee: "Only Unusable".into(),
                    pattern: "^(?=a).*$".into(),
                    rule_made: false,
                },
            ],
            skipped: vec![("payee-rules", 3)],
            ..Created::default()
        };
        let object = created_object(&created, true, None);
        assert_eq!(
            keys(&object),
            [
                "accounts",
                "categories",
                "completed",
                "converted",
                "fileName",
                "folder",
                "leftOpen",
                "movedToTop",
                "newBook",
                "patternsLeftOut",
                "previousFileName",
                "rates",
                "renamed",
                "rules",
                "skipped",
                "transactions",
                "transfers",
                "uncategorized",
            ]
        );
        assert_eq!(number_of(&object, "rules"), 2);
        let left_out = list_of(&object, "patternsLeftOut");
        assert_eq!(left_out.len(), 2);
        assert_eq!(keys(&left_out[0]), ["pattern", "payee", "ruleMade"]);
        assert_eq!(text_of(&left_out[0], "payee"), "Corner Grocery");
        assert_eq!(text_of(&left_out[0], "pattern"), "^(?!x)Corner$");
        assert!(flag_of(&left_out[0], "ruleMade"));
        assert!(!flag_of(&left_out[1], "ruleMade"));
        let skipped = list_of(&object, "skipped");
        assert_eq!(text_of(&skipped[0], "what"), "payee-rules");
        assert_eq!(number_of(&skipped[0], "count"), 3);
    }
}
