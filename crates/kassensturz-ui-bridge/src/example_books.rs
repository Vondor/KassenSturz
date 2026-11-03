// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Example books (M7a.2): made-up households that come with the app, so a
//! person can see what KassenSturz does before entering their own money.
//!
//! The bundled files are KMyMoney SQLite books, made from synthetic data
//! only, and are read through the importer like any other file (D-101). A
//! pick never opens a bundled file and never touches a book that is open: it
//! always makes a new, clearly named book in the data folder, so the user's
//! own book stays as it is and the originals stay as they are (D-571).
//! Names and descriptions are written in QML, where
//! KI18n finds them (D-034); this file knows only the ids.

use std::collections::HashMap;

use kassensturz_pipeline::RuleScope;

use crate::book_from_file::{self, Created, FromFileError};
use crate::books::{Books, Filled, OpenBook, ProblemText};
use crate::format::Locale;

/// One example book that is bundled with the app.
pub struct Example {
    /// What QML calls it, e.g. `space-colony`.
    pub id: &'static str,
    /// The KMyMoney SQLite book.
    bytes: &'static [u8],
    /// The book as a complete export, if it has one: it holds what a
    /// KMyMoney file cannot, such as budgets and a saved Overview, and is
    /// then used instead of `bytes` (M6-B138).
    export: Option<&'static [u8]>,
}

/// The id of the book suggested to start with (D-540 plan, M7a.2): a
/// household most people recognize.
pub const SUGGESTED: &str = "suburban-family";

/// The bundled books, in the order the chooser shows them: the suggested
/// start first. The ids are the ones `ExampleBookChooser.qml` and the test
/// below know. The bank files that go with them are in the same folder
/// (`examples/statements/`, M7a.8).
static EXAMPLES: &[Example] = &[
    Example {
        id: "suburban-family",
        bytes: include_bytes!(
            "../../kassensturz-import/tests/books/examples/suburban-family-v15.sqlite"
        ),
        export: None,
    },
    Example {
        id: "flower-utopia",
        bytes: include_bytes!(
            "../../kassensturz-import/tests/books/examples/flower-utopia-v15.sqlite"
        ),
        export: Some(include_bytes!(
            "../../kassensturz-import/tests/books/examples/flower-utopia.json"
        )),
    },
    Example {
        id: "space-colony",
        bytes: include_bytes!("../../kassensturz-import/tests/books/space-colony-5y-v15.sqlite"),
        export: None,
    },
    Example {
        id: "udssr-agent",
        bytes: include_bytes!(
            "../../kassensturz-import/tests/books/examples/udssr-agent-v15.sqlite"
        ),
        export: None,
    },
];

/// The bundled books.
pub fn all() -> &'static [Example] {
    EXAMPLES
}

/// Why an example book wasn't made.
#[derive(Debug)]
pub enum ExampleError {
    /// No bundled book has this id.
    Unknown,
    /// The bundled file didn't read or the book refused it. A mistake in
    /// KassenSturz, not in anything the user did.
    Fill(FromFileError),
    /// The new book didn't open, e.g. there is no data folder.
    Problem(ProblemText),
}

fn prepare(id: &str, stem: &str) -> Result<book_from_file::Prepared, ExampleError> {
    let example = all()
        .iter()
        .find(|example| example.id == id)
        .ok_or(ExampleError::Unknown)?;
    match example.export {
        Some(export) => book_from_file::read_bundled_export(stem, export),
        None => book_from_file::read_bundled_kmy_sqlite(stem, example.bytes),
    }
    .map_err(ExampleError::Fill)
}

/// Makes a new book called `name` in the data folder from the example
/// `id`, and opens it. If a book of that name is there, the new one gets a
/// number after it; nothing is replaced. If anything fails nothing is left
/// behind and the book opened last stays the one opened last.
pub fn create(
    books: &Books,
    id: &str,
    name: &str,
    locale: &impl Locale,
) -> Result<(OpenBook, Created), ExampleError> {
    let prepared = prepare(id, name)?;
    books
        .create_filled(name, |book| {
            book_from_file::fill(
                book,
                &prepared,
                &HashMap::new(),
                RuleScope::default(),
                locale,
            )
        })
        .map_err(|filled| match filled {
            Filled::Fill(error) => ExampleError::Fill(error),
            Filled::Problem(problem) => ExampleError::Problem(problem.text()),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::register::tests::TestLocale;
    use crate::test_folder::TestFolder;

    /// The ids the chooser's QML knows. A bundled book without a name and a
    /// description there would show as an empty row.
    const KNOWN: &[&str] = &[
        "suburban-family",
        "flower-utopia",
        "space-colony",
        "udssr-agent",
    ];

    #[test]
    fn every_bundled_book_has_a_known_id_and_the_suggested_one_is_known() {
        assert!(!all().is_empty());
        for example in all() {
            assert!(KNOWN.contains(&example.id), "{}", example.id);
        }
        assert!(KNOWN.contains(&SUGGESTED));
    }

    #[test]
    fn the_space_colony_book_comes_out_as_a_new_named_book_and_the_original_is_untouched() {
        // The seam: the real bundled file through the real importer and the
        // real fill, into a book file in a scratch folder.
        let folder = TestFolder::new();
        let books = Books::in_folder(folder.path(), "Desk");
        let before = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../kassensturz-import/tests/books/space-colony-5y-v15.sqlite"),
        )
        .expect("the bundled book");

        let (open, created) = create(
            &books,
            "space-colony",
            "Example – Space Colony",
            &TestLocale,
        )
        .expect("made");
        assert_eq!(open.path, folder.join("data/Example – Space Colony.ksz"));
        assert!(created.accounts > 0 && created.transactions > 100);
        assert_eq!(
            open.book.transactions().expect("transactions").len(),
            created.transactions
        );
        assert!(!open.book.is_empty().expect("asked"));
        drop(open);

        // The same pick again makes a second copy and replaces nothing.
        let (again, _) = create(
            &books,
            "space-colony",
            "Example – Space Colony",
            &TestLocale,
        )
        .expect("made");
        assert_eq!(
            again.path,
            folder.join("data/Example – Space Colony (2).ksz")
        );
        let bundled = all()
            .iter()
            .find(|example| example.id == "space-colony")
            .expect("bundled");
        assert_eq!(before.as_slice(), bundled.bytes);
    }

    /// The seam with M7a.8: every bundled book, as the generator wrote it,
    /// goes through the real importer and fill into a book with accounts,
    /// and transactions.
    #[test]
    fn every_bundled_book_makes_a_book() {
        for example in all() {
            let folder = TestFolder::new();
            let books = Books::in_folder(folder.path(), "Desk");
            let (open, created) = create(&books, example.id, "Example", &TestLocale)
                .unwrap_or_else(|_| panic!("{} did not make a book", example.id));
            assert!(created.accounts > 0, "{}", example.id);
            assert!(created.transactions > 30, "{}", example.id);
            drop(open);
        }
    }

    /// M6-B138: Flower Utopia comes with its budgets and its saved
    /// Overview, which only its complete export holds.
    #[test]
    fn flower_utopia_comes_with_its_budgets_and_overview() {
        let folder = TestFolder::new();
        let books = Books::in_folder(folder.path(), "Desk");
        let (open, created) =
            create(&books, "flower-utopia", "Example", &TestLocale).expect("made");
        assert!(created.transactions > 30);
        assert!(open.book.budgets().expect("budgets").len() >= 5);
        assert_eq!(open.book.views(None).expect("views").len(), 1);
        // The other books are plain KMyMoney books: no budgets, no views.
        drop(open);
        let folder = TestFolder::new();
        let books = Books::in_folder(folder.path(), "Desk");
        let (plain, _) = create(&books, "suburban-family", "Example", &TestLocale).expect("made");
        assert!(plain.book.budgets().expect("budgets").is_empty());
    }

    #[test]
    fn an_unknown_id_makes_nothing() {
        let folder = TestFolder::new();
        let books = Books::in_folder(folder.path(), "Desk");
        let error = create(&books, "no-such-book", "Example", &TestLocale)
            .err()
            .expect("refused");
        assert!(matches!(error, ExampleError::Unknown));
        assert!(!folder.join("data/Example.ksz").exists());
    }
}
