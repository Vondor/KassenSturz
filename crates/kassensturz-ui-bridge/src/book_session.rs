// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Opening and switching books, as a QML object (D-033, D-038, D-080).
//!
//! The window has one `BookSession`. It opens the book through `books.rs`
//! and hands it to `session.rs`, where every screen's object finds it. When
//! a book doesn't open, `problem` says why, and a book that was open stays
//! open.

use core::pin::Pin;
use std::path::{Path, PathBuf};

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QMap, QMapPair_QString_QVariant, QString, QUrl, QVariant};
use kassensturz_store::Lock;

use crate::books::{Books, OpenBook, Problem};
use crate::locale::QtLocale;
use crate::session;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt's string type.
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qurl.h");
        /// A URL, such as a file a file dialog chose.
        type QUrl = cxx_qt_lib::QUrl;

        include!("cxx-qt-lib/qvariant.h");
        /// A value of any Qt type.
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// The book the app has open.
        #[qobject]
        #[qml_element]
        #[qproperty(bool, open, READ, NOTIFY)]
        #[qproperty(QString, file_name, cxx_name = "fileName", READ, NOTIFY)]
        #[qproperty(QString, folder, READ, NOTIFY)]
        #[qproperty(QString, folder_url, cxx_name = "folderUrl", READ, NOTIFY)]
        #[qproperty(bool, network_drive, cxx_name = "networkDrive", READ, NOTIFY)]
        #[qproperty(QString, backup_folder, cxx_name = "backupFolder", READ, CONSTANT)]
        #[qproperty(QVariant, problem, READ, NOTIFY)]
        type BookSession = super::BookSessionRust;
    }

    extern "RustQt" {
        /// Opens the book opened last, or at the first start the app's own
        /// book, `name` in the user's language.
        #[qinvokable]
        #[cxx_name = "openLast"]
        fn open_last(self: Pin<&mut BookSession>, name: &QString);

        /// Opens the book a file dialog chose.
        #[qinvokable]
        #[cxx_name = "openFile"]
        fn open_file(self: Pin<&mut BookSession>, file: &QUrl);

        /// Makes a new, empty book where a file dialog chose, and opens it.
        #[qinvokable]
        #[cxx_name = "createFile"]
        fn create_file(self: Pin<&mut BookSession>, file: &QUrl);

        /// Shows the book that another screen just made and opened, e.g. a
        /// new book started from a file (M3.12), as `openFile` shows one.
        #[qinvokable]
        #[cxx_name = "adoptOpenBook"]
        fn adopt_open_book(self: Pin<&mut BookSession>);

        /// Opens the book `problem` says is locked, after the user was
        /// warned about that lock (D-080).
        #[qinvokable]
        #[cxx_name = "openAnyway"]
        fn open_anyway(self: Pin<&mut BookSession>);

        /// Tries to open the book `problem` is about again.
        #[qinvokable]
        fn retry(self: Pin<&mut BookSession>);

        /// Forgets `problem`, e.g. when the user goes back to the book that
        /// is open.
        #[qinvokable]
        #[cxx_name = "dismissProblem"]
        fn dismiss_problem(self: Pin<&mut BookSession>);
    }

    extern "RustQt" {
        /// A book was opened: every screen reads it again.
        #[qsignal]
        fn opened(self: Pin<&mut BookSession>);

        /// A book didn't open; `problem` says why.
        #[qsignal]
        fn failed(self: Pin<&mut BookSession>);
    }

    impl cxx_qt::Initialize for BookSession {}
}

/// The Rust side of [`qobject::BookSession`].
#[derive(Default)]
pub struct BookSessionRust {
    open: bool,
    file_name: QString,
    folder: QString,
    folder_url: QString,
    network_drive: bool,
    backup_folder: QString,
    problem: QVariant,
    books: Option<Books>,
    /// What `problem` is about: the book to try again, the lock the user
    /// was warned about, and the name of the app's own book.
    last_attempt: Option<Attempt>,
}

enum Attempt {
    OpenLast(String),
    Open(PathBuf),
    Locked(PathBuf, Lock),
}

impl cxx_qt::Initialize for qobject::BookSession {
    fn initialize(mut self: Pin<&mut Self>) {
        let books = Books::for_user();
        let backup_folder = display(books.backup_folder());
        let mut state = self.as_mut().rust_mut();
        state.backup_folder = QString::from(backup_folder.as_str());
        state.books = Some(books);
    }
}

impl qobject::BookSession {
    fn open_last(self: Pin<&mut Self>, name: &QString) {
        let name = name.to_string();
        self.attempt(Attempt::OpenLast(name.clone()), |books| {
            books.open_last(&name)
        });
    }

    fn open_file(self: Pin<&mut Self>, file: &QUrl) {
        let Some(path) = local_path(file) else {
            return;
        };
        if session::path().as_deref() == Some(path.as_path()) {
            // It is the book that is open.
            return;
        }
        self.attempt(Attempt::Open(path.clone()), |books| books.open(&path));
    }

    fn create_file(self: Pin<&mut Self>, file: &QUrl) {
        let Some(path) = local_path(file) else {
            return;
        };
        self.attempt(Attempt::Open(path.clone()), |books| books.create(&path));
    }

    fn adopt_open_book(mut self: Pin<&mut Self>) {
        if session::path().is_none() {
            return;
        }
        self.as_mut().rust_mut().last_attempt = None;
        self.as_mut().set_problem(QVariant::default());
        self.as_mut().show_open_book();
        self.as_mut().opened();
    }

    fn open_anyway(self: Pin<&mut Self>) {
        let Some(Attempt::Locked(path, lock)) = self.rust().last_attempt.as_ref() else {
            return;
        };
        let (path, lock) = (path.clone(), lock.clone());
        self.attempt(Attempt::Open(path.clone()), |books| {
            books.open_anyway(&path, &lock)
        });
    }

    fn retry(self: Pin<&mut Self>) {
        match self.rust().last_attempt.as_ref() {
            Some(Attempt::OpenLast(name)) => {
                let name = QString::from(name.as_str());
                self.open_last(&name);
            }
            Some(Attempt::Open(path) | Attempt::Locked(path, _)) => {
                let path = path.clone();
                self.attempt(Attempt::Open(path.clone()), |books| books.open(&path));
            }
            None => {}
        }
    }

    fn dismiss_problem(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().last_attempt = None;
        self.as_mut().set_problem(QVariant::default());
    }

    /// Opens a book with `open`. If that fails, the book that was open stays
    /// open, and `problem` says why.
    fn attempt(
        mut self: Pin<&mut Self>,
        attempt: Attempt,
        open: impl FnOnce(&Books) -> Result<OpenBook, Problem>,
    ) {
        let Some(books) = self.rust().books.as_ref() else {
            return;
        };
        match open(books) {
            Ok(book) => {
                session::set(book);
                self.as_mut().rust_mut().last_attempt = None;
                self.as_mut().set_problem(QVariant::default());
                self.as_mut().show_open_book();
                self.as_mut().opened();
            }
            Err(problem) => {
                let attempt = match (&problem, attempt) {
                    (Problem::Locked { path, lock }, _) => {
                        Attempt::Locked(path.clone(), lock.clone())
                    }
                    (_, attempt) => attempt,
                };
                let shown = self.problem_map(&problem);
                self.as_mut().rust_mut().last_attempt = Some(attempt);
                self.as_mut().set_problem(shown);
                self.as_mut().failed();
            }
        }
    }

    /// `problem` for QML: `code` and its parameters as text, `folder` the
    /// file's folder, and for a locked book `computer` and `since` as the
    /// lock file says, and `here` if it names this computer.
    fn problem_map(&self, problem: &Problem) -> QVariant {
        let text = problem.text();
        let mut entries: Vec<(String, QVariant)> = vec![("code".into(), qtext(&text.code))];
        entries.extend(
            text.params
                .iter()
                .map(|(name, value)| (name.clone(), qtext(value))),
        );
        if let Some(folder) = problem.path().and_then(Path::parent) {
            entries.push(("folder".into(), qtext(&display(folder))));
        }
        if let Problem::Locked { lock, .. } = problem {
            if let Some(computer) = &lock.computer {
                entries.push(("computer".into(), qtext(computer)));
                let here = self
                    .rust()
                    .books
                    .as_ref()
                    .is_some_and(|books| books.computer_name() == computer);
                entries.push(("here".into(), QVariant::from(&here)));
            }
            if let Some(since) = lock.since {
                entries.push(("since".into(), qtext(&QtLocale::date_time(since))));
            }
        }
        let mut map = QMap::<QMapPair_QString_QVariant>::default();
        for (key, value) in entries {
            map.insert(QString::from(key.as_str()), value);
        }
        QVariant::from(&map)
    }

    /// Shows where the open book is.
    fn show_open_book(mut self: Pin<&mut Self>) {
        let Some(path) = session::path() else {
            return;
        };
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let folder = path.parent().map(display).unwrap_or_default();
        let folder_url =
            QUrl::from_user_input(&QString::from(folder.as_str()), &QString::default())
                .to_qstring();
        let network_drive = session::on_network_drive();
        let mut state = self.as_mut().rust_mut();
        state.open = true;
        state.file_name = QString::from(file_name.as_str());
        state.folder = QString::from(folder.as_str());
        state.folder_url = folder_url;
        state.network_drive = network_drive;
        self.as_mut().open_changed();
        self.as_mut().file_name_changed();
        self.as_mut().folder_changed();
        self.as_mut().folder_url_changed();
        self.as_mut().network_drive_changed();
    }

    fn set_problem(mut self: Pin<&mut Self>, problem: QVariant) {
        self.as_mut().rust_mut().problem = problem;
        self.as_mut().problem_changed();
    }
}

/// The local file `url` names; `None` for anything else.
pub(crate) fn local_path(url: &QUrl) -> Option<PathBuf> {
    let path = url.to_local_file()?.to_string();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

fn display(path: &Path) -> String {
    path.display().to_string()
}

fn qtext(value: &str) -> QVariant {
    QVariant::from(&QString::from(value))
}
