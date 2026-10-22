// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Opening books (D-033, D-038, D-080). The app manages the book: at the
//! first start it makes one in KassenSturz's data folder, and from then on
//! it opens the book opened last. "Open another book…" and "Start a new
//! book…" are for people who want more than one.
//!
//! Nothing here is shown as it is: problems come back as codes with their
//! parameters as text, and the QML writes the message (D-034).

use std::path::{Path, PathBuf};

use kassensturz_core::{ErrorParam, Report};
use kassensturz_store::{Book, Computer, Lock, StoreError};

use crate::platform::{self, Drive};
use crate::settings::Settings;

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
    /// The file came through a sandbox's file chooser, e.g. in the Flatpak,
    /// so its lock file and SQLite's journal can't be made next to it.
    Sandboxed {
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
            | Self::Store { path, .. } => Some(path),
            Self::NoAppFolder => None,
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
}

impl Books {
    /// Books on this computer, with the user's own settings.
    pub fn for_user() -> Self {
        Self {
            settings: Settings::user(),
            app_folder: platform::app_folder(),
            computer: platform::computer(),
        }
    }

    /// Where automatic backups go (D-033).
    pub fn backup_folder(&self) -> &Path {
        &self.computer.backup_folder
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
        if let Some(path) = self.settings.last_book() {
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
        let drive = platform::drive(&path);
        if drive == Drive::Sandboxed {
            return Err(Problem::Sandboxed { path });
        }
        let book = open(&path).map_err(|error| match error {
            StoreError::Locked(lock) => Problem::Locked {
                path: path.clone(),
                lock,
            },
            StoreError::NotFound => Problem::Missing { path: path.clone() },
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
            }
        }
    }

    fn code(problem: Problem) -> String {
        problem.text().code
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
