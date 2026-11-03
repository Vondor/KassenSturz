// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Example books (M7a.2): made-up households that come with the app, so a
//! person can see what KassenSturz does before entering their own money.
//!
//! The bundled files are KMyMoney SQLite books, made from synthetic data
//! only, and are read through the importer like any other file (D-101). A
//! pick never opens a bundled file: it fills the open book if that is still
//! empty (the way a book started from a file is filled, M3.12), and otherwise
//! makes a new, clearly named book in the data folder, so a book with data is
//! never added to and the originals stay as they are.
//! Names and descriptions are written in QML, where
//! KI18n finds them (D-034); this file knows only the ids.

use std::collections::HashMap;

use kassensturz_pipeline::RuleScope;
use kassensturz_store::Book;

use crate::book_from_file::{self, Created, FromFileError};
use crate::books::{Books, Filled, OpenBook, ProblemText};
use crate::format::Locale;

/// One example book that is bundled with the app.
pub struct Example {
    /// What QML calls it, e.g. `space-colony`.
    pub id: &'static str,
    /// The KMyMoney SQLite book.
    bytes: &'static [u8],
}

/// The id of the book suggested to start with (D-540 plan, M7a.2): a
/// household most people recognize.
pub const SUGGESTED: &str = "suburban-family";

/// The bundled books, in the order the chooser shows them: the suggested
/// start first. The other three join when M7a.8 merges them: add a line
/// here, the name and description in `ExampleBookChooser.qml`, and the
/// ids to the test below.
static EXAMPLES: &[Example] = &[Example {
    id: "space-colony",
    bytes: include_bytes!("../../kassensturz-import/tests/books/space-colony-5y-v15.sqlite"),
}];

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
    book_from_file::read_bundled_kmy_sqlite(stem, example.bytes).map_err(ExampleError::Fill)
}

/// Fills `book`, which must be empty, with the example `id`, the way a book
/// started from a file is filled in place (M3.12). If the book refuses, it
/// stays empty. A book with something in it is refused, never added to.
pub fn fill_empty(
    book: &mut Book,
    id: &str,
    locale: &impl Locale,
) -> Result<Created, ExampleError> {
    let prepared = prepare(id, id)?;
    book_from_file::fill(
        book,
        &prepared,
        &HashMap::new(),
        RuleScope::default(),
        locale,
    )
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
        assert_eq!(before.as_slice(), all()[0].bytes);
    }

    #[test]
    fn an_empty_book_is_filled_in_place_and_a_full_one_is_refused() {
        let mut book = Book::create_in_memory().expect("a book");
        let created = fill_empty(&mut book, "space-colony", &TestLocale).expect("filled");
        assert!(created.transactions > 100);
        assert!(!book.is_empty().expect("asked"));
        let again = fill_empty(&mut book, "space-colony", &TestLocale);
        assert!(again.is_err());
        let mut other = Book::create_in_memory().expect("a book");
        assert!(matches!(
            fill_empty(&mut other, "no-such-book", &TestLocale),
            Err(ExampleError::Unknown)
        ));
        assert!(other.is_empty().expect("asked"));
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
