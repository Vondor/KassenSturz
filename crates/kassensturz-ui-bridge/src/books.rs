// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Opening books (D-033, D-038, D-080). The app manages the book: at the
//! first start it makes one in KassenSturz's data folder, and from then on
//! it opens the book opened last. "Open another book…" and "Start a new
//! book…" are for people who want more than one.
//!
//! Nothing here is shown as it is: problems come back as codes with their
//! parameters as text, and the QML writes the message (D-034).

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use kassensturz_core::book_folder;
use kassensturz_core::{ErrorParam, Report};
use kassensturz_store::{Book, Computer, Lock, StoreError};

use crate::platform::{self, Drive};
use crate::settings::Settings;
use crate::test_profile::{self, allows_in};

/// The extension of book files (D-037).
pub const EXTENSION: &str = "ksz";

/// The name of the app's own book when the UI passes none that fits.
const FALLBACK_NAME: &str = "Household";

/// A book that is open, and where it is.
pub struct OpenBook {
    /// The book.
    pub book: Book,
    /// Where it is, as an absolute path.
    pub path: PathBuf,
    /// Whether it is on a network drive, so the UI warns about it (D-038).
    pub on_network_drive: bool,
}

/// Why a book didn't open.
#[derive(Debug)]
pub enum Problem {
    /// The book is open on another computer, or wasn't closed properly
    /// (D-038). The user can open it anyway after being warned about this
    /// lock (D-080).
    Locked {
        /// The book.
        path: PathBuf,
        /// Its lock, as the lock file says.
        lock: Lock,
    },
    /// The book opened last isn't there anymore, e.g. because it was moved
    /// or is on a drive that isn't connected. A new book is never made in
    /// its place without asking, so nothing seems lost.
    Missing {
        /// Where it was.
        path: PathBuf,
    },
    /// The book is in a folder the user chose in the Flatpak, but the
    /// sandbox may no longer reach that folder (the grant was taken back or
    /// the portal's data was reset). Never a new book in its place; the user
    /// chooses the folder again (D-732).
    NoAccess {
        /// Where the book was remembered.
        path: PathBuf,
    },
    /// The book's folder or file can't be changed (the lock file can't be
    /// made), so the book isn't opened (D-732).
    ReadOnly {
        /// The book.
        path: PathBuf,
    },
    /// Writing to the open book failed because of the drive: the change in
    /// progress was taken back, the book was closed and nothing more is
    /// written until it is opened again (D-732).
    WriteFailed {
        /// The book.
        path: PathBuf,
    },
    /// Another program replaced the book's file while it was open, e.g. a
    /// sync program putting another computer's copy over it: the change in
    /// progress was not written to the file now in the folder, the book was
    /// closed, and nothing more is written until it is opened again (B-289).
    Replaced {
        /// The book.
        path: PathBuf,
    },
    /// The file came through a sandbox's file chooser, e.g. in the Flatpak,
    /// so its lock file and SQLite's journal can't be made next to it.
    Sandboxed {
        /// The file.
        path: PathBuf,
    },
    /// The app runs in the test profile, and the book is outside its test
    /// folder, so it isn't opened or made there (Q-399).
    OutsideTest {
        /// The file.
        path: PathBuf,
    },
    /// KassenSturz found no data folder to keep its book in.
    NoAppFolder,
    /// The book refused to open, e.g. because the file isn't a book.
    Store {
        /// The file.
        path: PathBuf,
        /// Why.
        error: StoreError,
    },
}

/// A problem as the UI shows it: a code and its parameters as text.
#[derive(Debug, PartialEq, Eq)]
pub struct ProblemText {
    /// The code, e.g. `book.missing` or `store.not-a-book`.
    pub code: String,
    /// The parameters by name, e.g. the file's name.
    pub params: Vec<(String, String)>,
}

impl Problem {
    /// The file it is about, if any.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Locked { path, .. }
            | Self::Missing { path }
            | Self::Sandboxed { path }
            | Self::NoAccess { path }
            | Self::ReadOnly { path }
            | Self::WriteFailed { path }
            | Self::Replaced { path }
            | Self::OutsideTest { path }
            | Self::Store { path, .. } => Some(path),
            Self::NoAppFolder => None,
        }
    }

    /// The problem's code, e.g. `book.missing` or `store.corrupt.bad-split`,
    /// the one [`Problem::text`] gives, for the developer log.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Locked { .. } => "store.locked",
            Self::Missing { .. } => "book.missing",
            Self::Sandboxed { .. } => "book.sandboxed",
            Self::NoAccess { .. } => "book.no-access",
            Self::ReadOnly { .. } => "book.read-only",
            Self::WriteFailed { .. } => "book.write-failed",
            Self::Replaced { .. } => "book.replaced",
            Self::OutsideTest { .. } => "book.outside-test",
            Self::NoAppFolder => "book.no-app-folder",
            Self::Store { error, .. } => error.report().code,
        }
    }

    /// The problem as the UI shows it. The file's name is always there as
    /// `file`, without its folder, which the UI shows separately.
    pub fn text(&self) -> ProblemText {
        let file = self
            .path()
            .and_then(Path::file_name)
            .map(|name| ("file".to_owned(), name.to_string_lossy().into_owned()));
        let (code, mut params) = match self {
            Self::Locked { .. } => ("store.locked".to_owned(), Vec::new()),
            Self::Missing { .. } => ("book.missing".to_owned(), Vec::new()),
            Self::Sandboxed { .. } => ("book.sandboxed".to_owned(), Vec::new()),
            Self::NoAccess { .. } => ("book.no-access".to_owned(), Vec::new()),
            Self::ReadOnly { .. } => ("book.read-only".to_owned(), Vec::new()),
            Self::WriteFailed { .. } => ("book.write-failed".to_owned(), Vec::new()),
            Self::Replaced { .. } => ("book.replaced".to_owned(), Vec::new()),
            Self::OutsideTest { .. } => ("book.outside-test".to_owned(), Vec::new()),
            Self::NoAppFolder => ("book.no-app-folder".to_owned(), Vec::new()),
            Self::Store { error, .. } => {
                let report = error.report();
                let mut params: Vec<(String, String)> = report
                    .params
                    .iter()
                    .filter_map(|(name, param)| {
                        let text = match param {
                            ErrorParam::Text(text) => text.clone(),
                            ErrorParam::Number(number) => number.to_string(),
                            ErrorParam::Keyword(keyword) => (*keyword).to_owned(),
                            _ => return None,
                        };
                        Some(((*name).to_owned(), text))
                    })
                    .filter(|(name, _)| name != "file")
                    .collect();
                if let Some(cause) = &report.cause {
                    params.push(("cause".to_owned(), cause.code.to_owned()));
                }
                (report.code.to_owned(), params)
            }
        };
        params.extend(file);
        ProblemText { code, params }
    }
}

/// Opens books on this computer and remembers the one opened last.
pub struct Books {
    settings: Settings,
    app_folder: Option<PathBuf>,
    computer: Computer,
    /// The test profile's folder: no book outside it opens (Q-399).
    test_root: Option<PathBuf>,
}

impl Books {
    /// Books on this computer, with the user's own settings.
    pub fn for_user() -> Self {
        Self {
            settings: Settings::user(),
            app_folder: platform::app_folder(),
            computer: platform::computer(),
            test_root: test_profile::root().map(Path::to_owned),
        }
    }

    /// Books that live in `folder` only: its own settings file and data
    /// folder, so a test never touches the user's.
    #[cfg(test)]
    pub(crate) fn in_folder(folder: &Path, computer_name: &str) -> Self {
        Self {
            settings: Settings::at(&folder.join("kassensturzrc")),
            app_folder: Some(folder.join("data")),
            computer: Computer {
                name: computer_name.to_owned(),
                backup_folder: folder.join("data/backups"),
            },
            test_root: None,
        }
    }

    /// Where automatic backups go (D-033).
    pub fn backup_folder(&self) -> &Path {
        &self.computer.backup_folder
    }

    /// The app's own folder, where its book lives and where the Flatpak may
    /// make the lock file next to a book (D-033, M6-B159).
    pub fn app_folder(&self) -> Option<&Path> {
        self.app_folder.as_deref()
    }

    /// The name of this computer, as lock files tell others (D-038).
    pub fn computer_name(&self) -> &str {
        &self.computer.name
    }

    /// Opens the book opened last. At the first start, or when no book was
    /// opened yet, it opens the app's own book, `name`.ksz in KassenSturz's
    /// data folder, and makes it if it isn't there. `name` comes from the
    /// UI, in the user's language.
    pub fn open_last(&self, name: &str) -> Result<OpenBook, Problem> {
        // In the test profile a book outside the test folder is never
        // opened, not even the one named last (Q-399).
        if let Some(path) = self
            .settings
            .last_book()
            .filter(|path| allows_in(self.test_root.as_deref(), path))
        {
            return self.open(&path);
        }
        let folder = self.app_folder.as_deref().ok_or(Problem::NoAppFolder)?;
        let path = folder.join(file_name(name));
        if path.exists() {
            return self.open(&path);
        }
        std::fs::create_dir_all(folder).map_err(|error| Problem::Store {
            path: path.clone(),
            error: error.into(),
        })?;
        self.create(&path)
    }

    /// Opens the book at `path` and remembers it as the one opened last.
    pub fn open(&self, path: &Path) -> Result<OpenBook, Problem> {
        self.opened(path, |path| Book::open(path, &self.computer))
    }

    /// Opens the book at `path` although it is locked, after the user was
    /// warned about `seen` (D-080).
    pub fn open_anyway(&self, path: &Path, seen: &Lock) -> Result<OpenBook, Problem> {
        self.opened(path, |path| Book::open_anyway(path, &self.computer, seen))
    }

    /// Makes a new, empty book at `path` and opens it. The extension
    /// `.ksz` is added if `path` lacks it. An existing file is never
    /// replaced, even if the file dialog asked whether it should be.
    pub fn create(&self, path: &Path) -> Result<OpenBook, Problem> {
        let path = with_extension(path);
        self.opened(&path, |path| Book::create(path, &self.computer))
    }

    /// Makes a new book in KassenSturz's data folder, called `stem` (the
    /// name of the file it is made from, say), filled by `fill`, and opens it.
    /// A name that is taken gets a number after it. If `fill` fails, the book
    /// is removed again and the book opened last stays the one opened last
    /// (D-125).
    pub fn create_filled<T, E>(
        &self,
        stem: &str,
        fill: impl FnOnce(&mut Book) -> Result<T, E>,
    ) -> Result<(OpenBook, T), Filled<E>> {
        let path = self.free_path(stem).map_err(Filled::Problem)?;
        self.create_filled_at(&path, fill)
    }

    /// Like [`Books::create_filled`], but at `path` the user chose: the
    /// extension `.ksz` is added if it lacks it, and an existing file is
    /// never replaced (D-125).
    pub fn create_filled_at<T, E>(
        &self,
        path: &Path,
        fill: impl FnOnce(&mut Book) -> Result<T, E>,
    ) -> Result<(OpenBook, T), Filled<E>> {
        let path = with_extension(path);
        let mut open = self
            .opened_unremembered(&path, |path| Book::create(path, &self.computer))
            .map_err(Filled::Problem)?;
        match fill(&mut open.book) {
            Ok(value) => {
                self.remember(&open.path);
                Ok((open, value))
            }
            Err(error) => {
                let path = open.path.clone();
                // Closing it first lets go of its lock. The book is only
                // just made and the fill takes back what it did, so nothing
                // is lost.
                drop(open);
                let _ = std::fs::remove_file(&path);
                Err(Filled::Fill(error))
            }
        }
    }

    /// A path in the data folder for a new book called `stem`: no book is
    /// there yet.
    fn free_path(&self, stem: &str) -> Result<PathBuf, Problem> {
        let folder = self.app_folder.as_deref().ok_or(Problem::NoAppFolder)?;
        std::fs::create_dir_all(folder).map_err(|error| Problem::Store {
            path: folder.to_owned(),
            error: error.into(),
        })?;
        let name = file_name(stem);
        let base = name.strip_suffix(&format!(".{EXTENSION}")).unwrap_or(&name);
        (1..)
            .map(|number| match number {
                1 => folder.join(&name),
                number => folder.join(format!("{base} ({number}).{EXTENSION}")),
            })
            .find(|path| !path.exists())
            .ok_or(Problem::NoAppFolder)
    }

    fn opened(
        &self,
        path: &Path,
        open: impl FnOnce(&Path) -> Result<Book, StoreError>,
    ) -> Result<OpenBook, Problem> {
        let book = self.opened_unremembered(path, open)?;
        self.remember(&book.path);
        Ok(book)
    }

    /// If this isn't saved, the app opens its own book next time, which is
    /// safe: the user can open this one again.
    fn remember(&self, path: &Path) {
        if !self.settings.set_last_book(path) {
            eprintln!("kassensturz: couldn't remember which book is open");
        }
    }

    fn opened_unremembered(
        &self,
        path: &Path,
        open: impl FnOnce(&Path) -> Result<Book, StoreError>,
    ) -> Result<OpenBook, Problem> {
        let path = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
        if !allows_in(self.test_root.as_deref(), &path) {
            return Err(Problem::OutsideTest { path });
        }
        let drive = platform::drive(&path);
        if drive == Drive::Sandboxed {
            return Err(Problem::Sandboxed { path });
        }
        if platform::runtime_dir().is_some_and(|runtime| grant_is_gone(&path, &runtime)) {
            return Err(Problem::NoAccess { path });
        }
        let book = open(&path).map_err(|error| match error {
            StoreError::Locked(lock) => Problem::Locked {
                path: path.clone(),
                lock,
            },
            StoreError::NotFound => Problem::Missing { path: path.clone() },
            // The lock file (or the book) can't be made or changed there.
            StoreError::Io(ErrorKind::PermissionDenied | ErrorKind::ReadOnlyFilesystem) => {
                Problem::ReadOnly { path: path.clone() }
            }
            error => Problem::Store {
                path: path.clone(),
                error,
            },
        })?;
        Ok(OpenBook {
            book,
            path,
            on_network_drive: drive == Drive::Network,
        })
    }
}

/// Whether `path` is in a folder the user exported through the document
/// portal and that export is gone: the Flatpak may no longer reach it
/// (D-732). A book that is missing from a folder that is still there is
/// `book.missing`, not this.
fn grant_is_gone(path: &Path, runtime_dir: &Path) -> bool {
    book_folder::export_root(path, runtime_dir).is_some_and(|root| !root.is_dir())
}

/// Whether `previous`, the book open before `next`, is the app's own book
/// in `app_folder` with nothing ever put in it (no currency, account or
/// change in its history), so closing it can remove its file without losing
/// anything (M6-B148).
pub fn unused_own_book(previous: &OpenBook, next: &Path, app_folder: Option<&Path>) -> bool {
    app_folder.is_some_and(|folder| previous.path.parent() == Some(folder))
        && previous.path != next
        && previous.book.is_empty().unwrap_or(false)
}

/// Why a book made from a file wasn't made: the book couldn't be made, or
/// what was to go into it was refused.
#[derive(Debug)]
pub enum Filled<E> {
    /// The new book didn't open.
    Problem(Problem),
    /// The new book was made and opened, but refused what the file holds.
    Fill(E),
}

/// The app's own book's file name, from the name the UI passes: without
/// anything that would make it a path or hide it.
fn file_name(name: &str) -> String {
    let name: String = name
        .chars()
        .filter(|&c| !c.is_control() && !matches!(c, '/' | '\\' | ':'))
        .collect();
    let name = name.trim().trim_start_matches('.').trim();
    let name = if name.is_empty() { FALLBACK_NAME } else { name };
    format!("{name}.{EXTENSION}")
}

/// `path` with the book extension, unless it has it already.
fn with_extension(path: &Path) -> PathBuf {
    let has_it = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(EXTENSION));
    if has_it {
        path.to_owned()
    } else {
        let mut name = path.as_os_str().to_owned();
        name.push(".");
        name.push(EXTENSION);
        name.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_folder::TestFolder;

    #[test]
    fn a_folder_whose_grant_is_gone_is_told_from_a_book_missing_in_it() {
        let folder = TestFolder::new();
        let runtime = folder.path();
        let book = |id: &str, name: &str| runtime.join("doc").join(id).join("Books").join(name);
        // The export is there, the book isn't: `book.missing`, not this.
        std::fs::create_dir_all(runtime.join("doc/ab12/Books")).expect("export");
        assert!(!grant_is_gone(&book("ab12", "Household.ksz"), runtime));
        // The export is gone, as after `flatpak permission-reset`.
        assert!(grant_is_gone(&book("cd34", "Household.ksz"), runtime));
        // A book outside the portal's mount is never this.
        assert!(!grant_is_gone(&runtime.join("Household.ksz"), runtime));
    }

    struct Setup {
        folder: TestFolder,
    }

    impl Setup {
        fn new() -> Self {
            Self {
                folder: TestFolder::new(),
            }
        }

        /// KassenSturz on a computer called `name`, sharing this setup's
        /// settings file and data folder, as two programs on one computer do.
        fn books(&self, name: &str) -> Books {
            Books {
                settings: Settings::at(&self.folder.join("kassensturzrc")),
                app_folder: Some(self.folder.join("data")),
                computer: Computer {
                    name: name.to_owned(),
                    backup_folder: self.folder.join("data/backups"),
                },
                test_root: None,
            }
        }

        /// KassenSturz in the test profile, with its test folder in this
        /// setup's folder (Q-399).
        fn test_books(&self) -> Books {
            let root = self.folder.join("kassensturz-test");
            Books {
                settings: Settings::at(&root.join("config/kassensturzrc")),
                app_folder: Some(root.join("data/app")),
                computer: Computer {
                    name: "test".to_owned(),
                    backup_folder: root.join("data/app/backups"),
                },
                test_root: Some(root),
            }
        }
    }

    #[test]
    fn only_an_empty_book_of_the_apps_own_counts_as_unused() {
        let setup = Setup::new();
        let books = setup.books("here");
        let app_folder = setup.folder.join("data");
        std::fs::create_dir_all(&app_folder).expect("folder made");
        let own = books.open_last("Haushalt").expect("own book made");
        assert_eq!(own.path.parent(), Some(app_folder.as_path()));
        let next = app_folder.join("Example.ksz");
        assert!(unused_own_book(&own, &next, Some(&app_folder)));
        // Not when it is the book that opens next, nor outside the app's
        // folder.
        assert!(!unused_own_book(&own, &own.path, Some(&app_folder)));
        assert!(!unused_own_book(
            &own,
            &next,
            Some(&setup.folder.join("other"))
        ));
        assert!(!unused_own_book(&own, &next, None));
        drop(own);
        // Once anything is in it, it stays.
        let mut used = books.open_last("Haushalt").expect("own book again");
        used.book
            .use_currency(kassensturz_core::CurrencyCode::new("EUR").expect("a code"))
            .expect("currency set");
        assert!(!unused_own_book(&used, &next, Some(&app_folder)));
    }

    #[test]
    fn the_test_profile_never_opens_or_makes_a_book_outside_its_folder() {
        let setup = Setup::new();
        // The real book, as a user's own setup has it.
        let real = setup.folder.join("Household.ksz");
        let opened = setup.books("here").create(&real).expect("real book made");
        drop(opened);
        let before = std::fs::read(&real).expect("real book read");
        let books = setup.test_books();
        // Named last in the test settings, it is passed over: the test
        // profile opens its own book instead (Q-399).
        assert!(books.settings.set_last_book(&real));
        let own = books.open_last("Haushaltsbuch").expect("own book opens");
        assert!(own.path.starts_with(setup.folder.join("kassensturz-test")));
        drop(own);
        assert_eq!(
            code(books.open(&real).err().expect("refused")),
            "book.outside-test"
        );
        assert_eq!(
            code(
                books
                    .create(&setup.folder.join("Other.ksz"))
                    .err()
                    .expect("refused")
            ),
            "book.outside-test"
        );
        assert!(!setup.folder.join("Other.ksz").exists());
        // The real book is unchanged and has no lock file beside it.
        assert_eq!(std::fs::read(&real).expect("real book read"), before);
        let lock_files = std::fs::read_dir(setup.folder.path())
            .expect("folder listed")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("lock"))
            .count();
        assert_eq!(lock_files, 0);
    }

    fn code(problem: Problem) -> String {
        let shown = problem.text().code;
        assert_eq!(problem.code(), shown, "the log names what the page shows");
        shown
    }

    // B-289: a replaced book file has a page of its own, not the drive's.
    #[test]
    fn a_replaced_book_file_is_told_from_a_lost_drive() {
        let path = PathBuf::from("/data/Household.ksz");
        assert_eq!(
            code(Problem::Replaced { path: path.clone() }),
            "book.replaced"
        );
        assert_eq!(
            code(Problem::WriteFailed { path: path.clone() }),
            "book.write-failed"
        );
        let shown = Problem::Replaced { path }.text();
        assert!(
            shown
                .params
                .contains(&("file".to_owned(), "Household.ksz".to_owned()))
        );
    }

    #[test]
    fn a_book_with_foreign_parts_is_refused_under_its_own_code() {
        let refused = Problem::Store {
            path: PathBuf::from("/data/Household.ksz"),
            error: StoreError::Corrupt(kassensturz_store::Corruption::UnexpectedLayout),
        };
        // The page words it apart from other damage and the log names it
        // (B-262).
        assert_eq!(refused.code(), "store.corrupt.unexpected-layout");
        assert_eq!(refused.text().code, "store.corrupt.unexpected-layout");
    }

    #[test]
    fn makes_the_apps_own_book_at_the_first_start_and_opens_it_again() {
        let setup = Setup::new();
        let books = setup.books("Desk");
        let open = books.open_last("Haushalt").expect("book made");
        let expected = setup.folder.join("data/Haushalt.ksz");
        assert_eq!(open.path, expected);
        assert!(!open.on_network_drive);
        assert!(expected.with_extension("ksz.lock").exists());
        drop(open);
        // Closing it removed the lock, so the next start opens it again.
        assert!(!expected.with_extension("ksz.lock").exists());
        let again = books.open_last("Household").expect("book opened");
        assert_eq!(again.path, expected);
    }

    #[test]
    fn opens_the_book_opened_last() {
        let setup = Setup::new();
        let books = setup.books("Desk");
        let other = setup.folder.join("Holiday.ksz");
        drop(books.create(&other).expect("book made"));
        let open = books.open_last("Household").expect("book opened");
        assert_eq!(open.path, other);
        assert!(!setup.folder.join("data/Household.ksz").exists());
    }

    #[test]
    fn never_makes_a_new_book_in_place_of_a_missing_one() {
        let setup = Setup::new();
        let books = setup.books("Desk");
        let other = setup.folder.join("Holiday.ksz");
        drop(books.create(&other).expect("book made"));
        std::fs::remove_file(&other).expect("book removed");

        let problem = books.open_last("Household").err().expect("refused");
        assert_eq!(problem.path(), Some(other.as_path()));
        assert_eq!(
            problem.text(),
            ProblemText {
                code: "book.missing".into(),
                params: vec![("file".into(), "Holiday.ksz".into())],
            }
        );
        assert!(!other.exists());
        assert!(!setup.folder.join("data/Household.ksz").exists());
    }

    #[test]
    fn says_where_a_locked_book_is_open_and_opens_it_anyway_when_asked() {
        let setup = Setup::new();
        let laptop = setup.books("Laptop");
        let desk = setup.books("Desk");
        let path = setup.folder.join("Household.ksz");
        let on_laptop = laptop.create(&path).expect("book made");

        let Err(Problem::Locked { lock, .. }) = desk.open(&path) else {
            panic!("the book is open on the laptop");
        };
        assert_eq!(lock.computer.as_deref(), Some("Laptop"));
        let open = desk.open_anyway(&path, &lock).expect("opened anyway");
        // The laptop no longer holds the lock, so closing there leaves the
        // desk's lock alone (D-080).
        drop(on_laptop);
        assert!(path.with_extension("ksz.lock").exists());
        drop(open);
        assert!(!path.with_extension("ksz.lock").exists());
    }

    #[test]
    fn makes_new_books_with_the_extension_and_never_replaces_a_file() {
        let setup = Setup::new();
        let books = setup.books("Desk");
        let open = books
            .create(&setup.folder.join("Holiday"))
            .expect("book made");
        assert_eq!(open.path, setup.folder.join("Holiday.ksz"));
        drop(open);

        let existing = setup.folder.join("Notes.ksz");
        std::fs::write(&existing, "not a book").expect("file written");
        let problem = books.create(&existing).err().expect("refused");
        assert_eq!(
            problem.text(),
            ProblemText {
                code: "store.already-exists".into(),
                params: vec![("file".into(), "Notes.ksz".into())],
            }
        );
        assert_eq!(
            std::fs::read_to_string(&existing).expect("file kept"),
            "not a book"
        );
        assert_eq!(
            code(books.open(&existing).err().expect("refused")),
            "store.not-a-book"
        );
    }

    #[test]
    fn the_apps_own_book_gets_a_plain_file_name() {
        assert_eq!(file_name("Haushalt"), "Haushalt.ksz");
        assert_eq!(file_name(" ../Haus/halt "), "Haushalt.ksz");
        assert_eq!(file_name("..."), "Household.ksz");
        assert_eq!(file_name("\n"), "Household.ksz");
        assert_eq!(
            with_extension(Path::new("a/Book.KSZ")),
            Path::new("a/Book.KSZ")
        );
        assert_eq!(
            with_extension(Path::new("a/Book.v2")),
            Path::new("a/Book.v2.ksz")
        );
    }

    #[test]
    fn a_book_made_from_a_file_is_filled_remembered_and_never_replaces_another() {
        let setup = Setup::new();
        let books = setup.books("Desk");
        let first = books
            .create_filled("Old KMyMoney book", |book| {
                book.is_empty().map(|empty| assert!(empty))
            })
            .expect("book made");
        assert_eq!(
            first.0.path,
            setup.folder.join("data/Old KMyMoney book.ksz")
        );
        drop(first);
        // Opened last, so the next start opens it.
        let open = books.open_last("Household").expect("opened");
        assert_eq!(open.path, setup.folder.join("data/Old KMyMoney book.ksz"));
        drop(open);

        // The name is taken: the new book gets a number, the old one stays.
        let second = books
            .create_filled::<(), StoreError>("Old KMyMoney book", |_| Ok(()))
            .expect("book made");
        assert_eq!(
            second.0.path,
            setup.folder.join("data/Old KMyMoney book (2).ksz")
        );
        drop(second);
        assert!(setup.folder.join("data/Old KMyMoney book.ksz").exists());
    }

    #[test]
    fn a_refused_file_leaves_no_book_and_the_last_book_stays_the_last() {
        let setup = Setup::new();
        let books = setup.books("Desk");
        drop(books.open_last("Household").expect("the app's own book"));
        let home = setup.folder.join("data/Household.ksz");

        let refused = books.create_filled::<(), _>("From KMyMoney", |_| Err(StoreError::NotEmpty));
        assert!(matches!(refused, Err(Filled::Fill(StoreError::NotEmpty))));
        assert!(!setup.folder.join("data/From KMyMoney.ksz").exists());
        assert!(!setup.folder.join("data/From KMyMoney.ksz.lock").exists());
        let open = books.open_last("Household").expect("still the last");
        assert_eq!(open.path, home);
    }

    #[test]
    fn a_book_from_a_file_can_go_where_the_user_chose_and_never_replaces_a_file() {
        let setup = Setup::new();
        let books = setup.books("Desk");
        let chosen = setup.folder.join("chosen");
        std::fs::create_dir_all(&chosen).expect("a folder");

        // The extension is added, as for any new book.
        let made = books
            .create_filled_at::<(), StoreError>(&chosen.join("Savings"), |_| Ok(()))
            .expect("book made");
        assert_eq!(made.0.path, chosen.join("Savings.ksz"));
        drop(made);
        let open = books.open_last("Household").expect("remembered");
        assert_eq!(open.path, chosen.join("Savings.ksz"));
        drop(open);

        // A file that is there stays as it is, whatever the dialog asked.
        std::fs::write(chosen.join("Mine.ksz"), b"mine").expect("a file");
        let refused =
            books.create_filled_at::<(), StoreError>(&chosen.join("Mine.ksz"), |_| Ok(()));
        assert!(matches!(refused, Err(Filled::Problem(_))));
        assert_eq!(
            std::fs::read(chosen.join("Mine.ksz")).expect("kept"),
            b"mine"
        );

        // A refused file leaves no book at the chosen place either.
        let refused = books
            .create_filled_at::<(), _>(&chosen.join("Gone.ksz"), |_| Err(StoreError::NotEmpty));
        assert!(matches!(refused, Err(Filled::Fill(StoreError::NotEmpty))));
        assert!(!chosen.join("Gone.ksz").exists());
        assert!(!chosen.join("Gone.ksz.lock").exists());
    }

    #[test]
    fn a_book_from_a_file_needs_a_folder_to_live_in() {
        let setup = Setup::new();
        let mut books = setup.books("Desk");
        books.app_folder = None;
        let refused = books.create_filled::<(), StoreError>("From KMyMoney", |_| Ok(()));
        assert!(matches!(
            refused,
            Err(Filled::Problem(Problem::NoAppFolder))
        ));
    }
}
