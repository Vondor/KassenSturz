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
use kassensturz_store::{Book, Lock, PreparedRestore, StoreError};

use crate::backup_copies;
use crate::books::{Books, OpenBook, Problem};
use crate::locale::QtLocale;
use crate::platform;
use crate::session;
use crate::settings::Settings;
use crate::variant;

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
        #[qproperty(QString, version, READ, CONSTANT)]
        #[qproperty(bool, from_pull_request, cxx_name = "fromPullRequest", READ, CONSTANT)]
        #[qproperty(QVariant, problem, READ, NOTIFY)]
        #[qproperty(bool, use_folders, cxx_name = "useFolders", READ, CONSTANT)]
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

        /// Puts the backup at `path`, one `BackupCopies.backups()` lists,
        /// in place of the open book, after the user confirmed it (#806).
        /// The open book is backed up first, so the way back is in the
        /// list, then the book opens again. Returns `ok`, or `code` for why
        /// not; the book is as it was then.
        #[qinvokable]
        #[cxx_name = "restoreBackup"]
        fn restore_backup(self: Pin<&mut BookSession>, path: &QString) -> QVariant;
    }

    extern "RustQt" {
        /// The names of the books (`.ksz` files) in `folder`, a folder
        /// the folder chooser granted, sorted; empty if it can't be read.
        /// In the Flatpak "Open another book…" asks for a folder first
        /// (D-733).
        #[qinvokable]
        #[cxx_name = "booksIn"]
        fn books_in(self: &BookSession, folder: &QString) -> QVariant;

        /// Opens the book called `name` in `folder`, a folder the folder
        /// chooser granted, as `openFile` does. A name that isn't a plain
        /// file name is refused. Also for "Choose the folder again…" (D-732).
        #[qinvokable]
        #[cxx_name = "openIn"]
        fn open_in(self: Pin<&mut BookSession>, folder: &QString, name: &QString);

        /// Makes a new, empty book called `name` in `folder`, as `createFile`
        /// does. Returns `ok`, or `code` `name.invalid` for a name that isn't
        /// a plain file name; nothing is made then. A book that can't be made
        /// shows as `problem`.
        #[qinvokable]
        #[cxx_name = "createIn"]
        fn create_in(self: Pin<&mut BookSession>, folder: &QString, name: &QString) -> QVariant;

        /// If writing to the open book failed because of the drive, closes
        /// the book, so nothing more is written and its lock is released,
        /// and shows `problem` (`book.write-failed`). Returns whether it did
        /// (D-732). Cheap, so a timer may ask often.
        #[qinvokable]
        #[cxx_name = "checkWriteHealth"]
        fn check_write_health(self: Pin<&mut BookSession>) -> bool;
    }

    extern "RustQt" {
        /// Shows where the open book is now, after it moved into another
        /// folder (D-731): the same book, so no screen changes.
        #[qinvokable]
        #[cxx_name = "showMovedBook"]
        fn show_moved_book(self: Pin<&mut BookSession>);
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
    version: QString,
    from_pull_request: bool,
    problem: QVariant,
    /// Whether the book is chosen by its folder: in the Flatpak, where a
    /// save or open dialog hands over one file only (D-730, D-733).
    use_folders: bool,
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
        // Until a book is open, the file dialogs start in the app's own
        // folder: a book the Flatpak's file chooser hands over from anywhere
        // else can't be opened (book.sandboxed, M6-B159).
        let folder_url = books
            .app_folder()
            .filter(|folder| folder.is_dir())
            .map(|folder| folder_url(&display(folder)))
            .unwrap_or_default();
        let mut state = self.as_mut().rust_mut();
        state.backup_folder = QString::from(backup_folder.as_str());
        state.folder_url = folder_url;
        state.version = QString::from(version_label().as_str());
        state.from_pull_request = from_pull_request();
        state.use_folders = platform::in_flatpak_sandbox() && platform::can_choose_book_folder();
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

    fn books_in(&self, folder: &QString) -> QVariant {
        let names = book_names_in(Path::new(&folder.to_string()));
        variant::list(names.iter().map(|name| variant::text(name)))
    }

    fn open_in(self: Pin<&mut Self>, folder: &QString, name: &QString) {
        let Some(path) = file_in(Path::new(&folder.to_string()), &name.to_string()) else {
            return;
        };
        if session::path().as_deref() == Some(path.as_path()) {
            return;
        }
        self.attempt(Attempt::Open(path.clone()), |books| books.open(&path));
    }

    fn create_in(self: Pin<&mut Self>, folder: &QString, name: &QString) -> QVariant {
        let Some(path) = file_in(Path::new(&folder.to_string()), &name.to_string()) else {
            return variant::map([
                ("ok", QVariant::from(&false)),
                ("code", variant::text("name.invalid")),
            ]);
        };
        self.attempt(Attempt::Open(path.clone()), |books| books.create(&path));
        variant::map([("ok", QVariant::from(&true))])
    }

    fn check_write_health(mut self: Pin<&mut Self>) -> bool {
        if !session::with_book(Book::write_failed).unwrap_or(false) {
            return false;
        }
        let Some(path) = session::path() else {
            return false;
        };
        // Closing drops the lock; the change that failed was taken back, so
        // the file is as it was before it.
        session::close();
        let shown = self.problem_map(&Problem::WriteFailed { path: path.clone() });
        let mut state = self.as_mut().rust_mut();
        state.open = false;
        state.last_attempt = Some(Attempt::Open(path));
        self.as_mut().open_changed();
        self.as_mut().set_problem(shown);
        self.as_mut().failed();
        true
    }

    fn show_moved_book(self: Pin<&mut Self>) {
        self.show_open_book();
    }

    fn dismiss_problem(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().last_attempt = None;
        self.as_mut().set_problem(QVariant::default());
    }

    fn restore_backup(mut self: Pin<&mut Self>, path: &QString) -> QVariant {
        let refused = |code: &str| {
            variant::map([
                ("ok", QVariant::from(&false)),
                ("code", variant::text(code)),
            ])
        };
        let (Some(book), Some(backups)) = (
            session::path(),
            self.rust()
                .books
                .as_ref()
                .map(|books| books.backup_folder().to_owned()),
        ) else {
            return refused("restore.no-book");
        };
        // Only a backup the page lists, never any file QML names.
        let wanted = PathBuf::from(path.to_string());
        let chosen = backup_copies::chosen(&Settings::user()).map(|chosen| chosen.path);
        let (listed, _) = backup_copies::listed(&book, &backups, chosen.as_deref());
        if !listed.iter().any(|listed| listed.path == wanted) {
            return refused("restore.not-listed");
        }
        // Checked and copied next to the book first, so the backup below
        // can't prune the one being restored.
        let prepared = match PreparedRestore::new(&wanted, &book) {
            Ok(prepared) => prepared,
            Err(
                StoreError::NotABook
                | StoreError::UnsupportedLayout { .. }
                | StoreError::DevelopmentLayout { .. },
            ) => return refused("restore.not-usable"),
            Err(_) => return refused("restore.failed"),
        };
        if !matches!(
            session::with_book(|open| open.back_up_now(&book, &backups)),
            Some(Ok(_))
        ) {
            return refused("restore.no-backup-first");
        }
        session::close();
        let placed = prepared.put_in_place();
        // The book opens again either way: restored, or as it was.
        self.as_mut()
            .attempt(Attempt::Open(book.clone()), |books| books.open(&book));
        match placed {
            Ok(()) => variant::map([("ok", QVariant::from(&true))]),
            Err(_) => refused("restore.failed"),
        }
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
                let backups = books.backup_folder().to_owned();
                session::set(book);
                // A copy in the folder the user chose, at most once a day;
                // a missing or full folder never stops the book (#806).
                backup_copies::keep_copy_of_open_book(&Settings::user(), &backups);
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
        let folder_url = folder_url(&folder);
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

/// The names of the books in `folder`, sorted: regular files ending in
/// `.ksz`.
fn book_names_in(folder: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.to_lowercase().ends_with(".ksz"))
        .collect();
    names.sort();
    names
}

/// The file called `name` in `folder`, `.ksz` added unless it is there. `None`
/// for a name that isn't a plain file name (empty, a path, `.` or `..`, a
/// control character), so a name can never lead out of the folder.
pub(crate) fn file_in(folder: &Path, name: &str) -> Option<PathBuf> {
    let name = name.trim();
    let plain = !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\'])
        && !name.chars().any(char::is_control);
    if !plain || !folder.is_absolute() {
        return None;
    }
    let file = if name.to_lowercase().ends_with(".ksz") {
        name.to_owned()
    } else {
        format!("{name}.ksz")
    };
    Some(folder.join(file))
}

/// The local file `url` names; `None` for anything else.
pub(crate) fn local_path(url: &QUrl) -> Option<PathBuf> {
    let path = url.to_local_file()?.to_string();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// The version the app was built as (D-152, D-226): the build date and the
/// number of the merge, worked out by the build from git, so Settings never
/// shows a number typed by hand. `Cargo.toml` has the last prepared build.
pub(crate) fn app_version() -> &'static str {
    env!("KASSENSTURZ_BUILD_VERSION")
}

/// What Settings shows: the version, and the commit the build is made from
/// (empty when the build doesn't know it), since a build is numbered only when
/// it is prepared for users and many builds share one number (D-152).
fn version_label() -> String {
    match option_env!("KASSENSTURZ_BUILD_COMMIT") {
        Some(commit) if !commit.is_empty() => format!("{} ({commit})", app_version()),
        _ => app_version().to_owned(),
    }
}

/// Whether the commit in [`version_label`] is a pull request's last commit:
/// CI built the bundle in the pull request, and main reuses it for the
/// merge (D-427), so About says so and a tester looks for the merge of that
/// commit on main (#758).
fn from_pull_request() -> bool {
    commit_from_pull_request(
        option_env!("KASSENSTURZ_BUILD_COMMIT").unwrap_or(""),
        option_env!("KASSENSTURZ_BUILD_COMMIT_FROM").unwrap_or(""),
    )
}

fn commit_from_pull_request(commit: &str, from: &str) -> bool {
    !commit.is_empty() && from == "pull-request-head"
}

fn display(path: &Path) -> String {
    path.display().to_string()
}

/// A folder as the file dialogs' `currentFolder` takes it.
fn folder_url(folder: &str) -> QString {
    QUrl::from_user_input(&QString::from(folder), &QString::default()).to_qstring()
}

fn qtext(value: &str) -> QVariant {
    QVariant::from(&QString::from(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_label_names_the_version_and_the_commit_when_known() {
        let label = version_label();
        assert!(label.starts_with(app_version()), "{label}");
        let rest = label.strip_prefix(app_version()).unwrap();
        assert!(
            rest.is_empty()
                || (rest.starts_with(" (")
                    && rest.ends_with(')')
                    && rest[2..rest.len() - 1]
                        .chars()
                        .all(|c| c.is_ascii_hexdigit())),
            "{label}"
        );
    }

    #[test]
    fn only_a_known_commit_of_a_pull_request_build_is_marked() {
        assert!(commit_from_pull_request("abc1234", "pull-request-head"));
        assert!(!commit_from_pull_request("abc1234", ""));
        assert!(!commit_from_pull_request("", "pull-request-head"));
        assert!(!commit_from_pull_request("abc1234", "main"));
    }

    /// The build files carry one version (D-152), and the version Settings
    /// shows, which comes from git (D-226), has the form 0.YYMMDD.N.
    #[test]
    fn shows_the_version_of_the_build() {
        let files = env!("CARGO_PKG_VERSION");
        let cmake = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../CMakeLists.txt"),
        )
        .expect("CMakeLists.txt is in the repository");
        assert!(
            cmake.contains(&format!("project(kassensturz VERSION {files} ")),
            "CMakeLists.txt has another version than Cargo.toml's {files}"
        );
        let version = app_version();
        let parts: Vec<&str> = version.split('.').collect();
        assert_eq!(parts.len(), 3, "{version}");
        assert!(
            parts.iter().all(|part| part.parse::<u32>().is_ok()),
            "{version}"
        );
        assert_eq!(parts[1].len(), 6, "{version} has no YYMMDD date");
    }

    #[test]
    fn a_name_becomes_a_book_in_the_folder_and_never_leads_out_of_it() {
        let folder = Path::new("/data/Books");
        let file = |name| file_in(folder, name);
        assert_eq!(
            file("Household"),
            Some(PathBuf::from("/data/Books/Household.ksz"))
        );
        assert_eq!(
            file(" Household.KSZ "),
            Some(PathBuf::from("/data/Books/Household.KSZ"))
        );
        for refused in ["", "  ", ".", "..", "../x", "a/b", "a\\b", "x\u{1}y"] {
            assert_eq!(file(refused), None, "{refused:?}");
        }
        // A folder that isn't a full path is no folder.
        assert_eq!(file_in(Path::new("Books"), "a"), None);
    }

    #[test]
    fn the_books_in_a_folder_are_its_ksz_files_by_name() {
        let folder = crate::test_folder::TestFolder::new();
        for name in ["b.ksz", "A.KSZ", "notes.txt", "c.ksz-journal"] {
            std::fs::write(folder.join(name), b"").expect("file");
        }
        std::fs::create_dir(folder.join("d.ksz")).expect("a folder called like a book");
        assert_eq!(book_names_in(folder.path()), ["A.KSZ", "b.ksz"]);
        assert!(book_names_in(&folder.join("missing")).is_empty());
    }
}
