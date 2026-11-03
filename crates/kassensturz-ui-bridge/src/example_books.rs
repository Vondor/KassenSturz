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
//!
//! The bundled books are made for one month, the reference month: they hold
//! about two years before it and some spending in it. A book made later
//! would start on an empty month, so every date moves by whole months until
//! the reference month is the running one, and what then lies after today
//! is left out (D-584, M6-B160). Last month's figures stay what the answer
//! sheets say.

use std::collections::HashMap;

use kassensturz_core::Date;
use kassensturz_import::ImportedBook;
use kassensturz_pipeline::RuleScope;
use serde_json::Value;

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

/// The month the bundled books were made for, as the generator wrote it
/// (`YYYY-MM`, `tools/example-books/generate.py`).
const REFERENCE_MONTH: &str =
    include_str!("../../kassensturz-import/tests/books/examples/reference-month.txt");

/// How an example book's dates move when it is made on `today` (D-584).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Dating {
    /// Whole months every date moves by; negative moves them back.
    months: i32,
    /// The last day a transaction may have: today.
    through: Date,
}

impl Dating {
    fn on(today: Date) -> Self {
        let (year, month) = REFERENCE_MONTH
            .trim()
            .split_once('-')
            .and_then(|(year, month)| Some((year.parse::<i32>().ok()?, month.parse::<i32>().ok()?)))
            .expect("reference-month.txt holds YYYY-MM");
        let months = month_number(i32::from(today.year()), i32::from(today.month()))
            - month_number(year, month);
        Self {
            months,
            through: today,
        }
    }

    /// `date` moved by the months. A day the new month doesn't have becomes
    /// its last day (31 October to 30 November), so dates keep their order
    /// and stay in their month.
    fn moved(self, date: Date) -> Date {
        if self.months == 0 {
            return date;
        }
        let number = month_number(i32::from(date.year()), i32::from(date.month())) + self.months;
        let year = i16::try_from(number.div_euclid(12)).unwrap_or(date.year());
        let month = i8::try_from(number.rem_euclid(12) + 1).expect("1 to 12");
        let Ok(first) = Date::new(year, month, 1) else {
            return date;
        };
        Date::new(year, month, date.day().min(first.days_in_month())).unwrap_or(date)
    }

    /// The KMyMoney book with its dates moved and nothing after today.
    fn apply_to_book(self, book: &mut ImportedBook) {
        for transaction in &mut book.transactions {
            transaction.date = self.moved(transaction.date);
            transaction.entered_on = transaction.entered_on.map(|day| self.moved(day));
            for part in &mut transaction.parts {
                if let Some(details) = &mut part.details {
                    details.booking_date = details.booking_date.map(|day| self.moved(day));
                    details.value_date = details.value_date.map(|day| self.moved(day));
                }
            }
        }
        book.transactions
            .retain(|transaction| transaction.date <= self.through);
        for rate in &mut book.rates {
            rate.date = self.moved(rate.date);
        }
    }

    /// The complete export with every date in it moved (they are all
    /// `YYYY-MM-DD` texts) and no transaction after today.
    fn apply_to_export(self, bytes: &[u8]) -> Option<Vec<u8>> {
        let mut data: Value = serde_json::from_slice(bytes).ok()?;
        self.move_dates_in(&mut data);
        if let Some(Value::Array(transactions)) = data.get_mut("transactions") {
            transactions.retain(|transaction| {
                transaction
                    .get("date")
                    .and_then(Value::as_str)
                    .and_then(|text| text.parse::<Date>().ok())
                    .is_none_or(|date| date <= self.through)
            });
        }
        serde_json::to_vec(&data).ok()
    }

    fn move_dates_in(self, value: &mut Value) {
        match value {
            Value::String(text) => {
                if let Some(date) = whole_date(text) {
                    *text = self.moved(date).to_string();
                }
            }
            Value::Array(items) => items.iter_mut().for_each(|item| self.move_dates_in(item)),
            Value::Object(fields) => fields
                .values_mut()
                .for_each(|field| self.move_dates_in(field)),
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
    }
}

/// Months counted from year 0, so two months can be subtracted.
fn month_number(year: i32, month: i32) -> i32 {
    year * 12 + month - 1
}

/// `text` if it is nothing but a date, `2026-10-30`.
fn whole_date(text: &str) -> Option<Date> {
    let bytes = text.as_bytes();
    let shape = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit());
    if shape { text.parse().ok() } else { None }
}

fn prepare(id: &str, stem: &str, today: Date) -> Result<book_from_file::Prepared, ExampleError> {
    let example = all()
        .iter()
        .find(|example| example.id == id)
        .ok_or(ExampleError::Unknown)?;
    let dating = Dating::on(today);
    match example.export {
        Some(export) => {
            let moved = dating.apply_to_export(export);
            book_from_file::read_bundled_export(stem, moved.as_deref().unwrap_or(export))
        }
        None => book_from_file::read_bundled_kmy_sqlite(stem, example.bytes, |book| {
            dating.apply_to_book(book);
        }),
    }
    .map_err(ExampleError::Fill)
}

/// Makes a new book called `name` in the data folder from the example
/// `id`, and opens it. If a book of that name is there, the new one gets a
/// number after it; nothing is replaced. If anything fails nothing is left
/// behind and the book opened last stays the one opened last. The book's
/// dates follow the locale's today (D-584).
pub fn create(
    books: &Books,
    id: &str,
    name: &str,
    locale: &impl Locale,
) -> Result<(OpenBook, Created), ExampleError> {
    create_on(books, id, name, locale, locale.today())
}

/// [`create`] as if today were `today`.
fn create_on(
    books: &Books,
    id: &str,
    name: &str,
    locale: &impl Locale,
    today: Date,
) -> Result<(OpenBook, Created), ExampleError> {
    let prepared = prepare(id, name, today)?;
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
    fn dates_move_by_whole_months_and_keep_their_month() {
        let later = Dating {
            months: 1,
            through: Date::constant(2026, 12, 1),
        };
        assert_eq!(
            later.moved(Date::constant(2026, 10, 31)),
            Date::constant(2026, 11, 30)
        );
        assert_eq!(
            later.moved(Date::constant(2026, 1, 31)),
            Date::constant(2026, 2, 28)
        );
        assert_eq!(
            later.moved(Date::constant(2026, 12, 5)),
            Date::constant(2027, 1, 5)
        );
        let earlier = Dating {
            months: -13,
            through: Date::constant(2025, 10, 1),
        };
        assert_eq!(
            earlier.moved(Date::constant(2026, 11, 3)),
            Date::constant(2025, 10, 3)
        );
        let none = Dating::on(Date::constant(2026, 11, 3));
        assert_eq!(none.months, 0);
        assert_eq!(
            none.moved(Date::constant(2025, 2, 28)),
            Date::constant(2025, 2, 28)
        );
        assert_eq!(Dating::on(Date::constant(2027, 3, 2)).months, 4);
        assert_eq!(Dating::on(Date::constant(2026, 10, 19)).months, -1);
    }

    #[test]
    fn only_whole_dates_count_as_dates_in_an_export() {
        assert_eq!(whole_date("2026-10-30"), Some(Date::constant(2026, 10, 30)));
        for text in [
            "2026-10-30T08:00:00",
            "Beiträge 2026-10-30",
            "2026-1-30",
            "20261030ab",
            "",
        ] {
            assert_eq!(whole_date(text), None, "{text}");
        }
    }

    /// M6-B160: whenever an example book is made, its running month has
    /// bookings, last month is full, nothing lies after today, and the book
    /// reaches back about two years. Each book on the first day of the
    /// reference month (nothing moves), mid-month a few months later, and
    /// on the last day of a month before it.
    #[test]
    fn every_example_book_has_bookings_this_month_and_none_after_today() {
        for today in [
            Date::constant(2026, 11, 1),
            Date::constant(2027, 3, 15),
            Date::constant(2026, 9, 30),
        ] {
            for example in all() {
                let folder = TestFolder::new();
                let books = Books::in_folder(folder.path(), "Desk");
                let (open, _) = create_on(&books, example.id, "Example", &TestLocale, today)
                    .unwrap_or_else(|_| panic!("{} did not make a book", example.id));
                let dates: Vec<Date> = open
                    .book
                    .transactions()
                    .expect("transactions")
                    .iter()
                    .map(|transaction| transaction.booking_date())
                    .collect();
                let what = format!("{} on {today}", example.id);
                let newest = dates.iter().max().copied().expect("transactions");
                let oldest = dates.iter().min().copied().expect("transactions");
                assert!(newest <= today, "{what}: {newest}");
                let this_month = today.first_of_month();
                assert!(
                    dates.iter().any(|date| date.first_of_month() == this_month),
                    "{what}: nothing this month"
                );
                let last_month = this_month.yesterday().expect("a day").first_of_month();
                let in_last_month = dates
                    .iter()
                    .filter(|date| date.first_of_month() == last_month)
                    .count();
                assert!(in_last_month >= 10, "{what}: {in_last_month} last month");
                assert!(
                    oldest
                        <= Dating {
                            months: -22,
                            through: today
                        }
                        .moved(this_month),
                    "{what}: starts {oldest}"
                );
                drop(open);
            }
        }
    }

    /// Flower Utopia's budgets move with its bookings, so they have
    /// figures for last month whenever the book is made.
    #[test]
    fn flower_utopias_budgets_move_with_its_dates() {
        let folder = TestFolder::new();
        let books = Books::in_folder(folder.path(), "Desk");
        let today = Date::constant(2027, 3, 15);
        let (open, _) =
            create_on(&books, "flower-utopia", "Example", &TestLocale, today).expect("made");
        let budgets = open.book.budgets().expect("budgets");
        assert!(!budgets.is_empty());
        for budget in budgets {
            assert_eq!(budget.first_day, Date::constant(2025, 3, 1));
        }
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
