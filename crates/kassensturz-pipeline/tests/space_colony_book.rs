// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The five-year Space Colony book as a complete export (version 8), with
//! sample category groups and a small hide list (D-270, D-290).
//!
//! `tools/space-colony/generate.py` writes the KMyMoney file; this test
//! reads that file through the real importer into a book, adds the groups
//! and the hidden categories with the store's own operations, and writes the
//! book's complete export. The export is the file a person opens with
//! "Start a book from a file", because a KMyMoney file cannot hold groups.
//! IDs are numbered in the order they appear, so the same book always gives
//! the same bytes and the committed file can be compared.
//!
//! To write the file again after a change to the generator or to the groups:
//!
//! ```sh
//! KASSENSTURZ_BLESS=1 cargo test -p kassensturz-pipeline --test space_colony_book
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use kassensturz_core::{GroupKind, GroupMember};
use kassensturz_export::{BookExport, ExportData, export, read_book};
use kassensturz_import::{Imported, KmySqlite, Limits, import};
use kassensturz_pipeline::{export_book, fill_book, fill_book_from};
use kassensturz_store::Book;

fn books() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../kassensturz-import/tests/books")
}

/// The sample groups: a name, whether it counts spending or income, and its
/// members, each a category by its full name or another group by its name.
const GROUPS: [(&str, GroupKind, &[&str]); 5] = [
    (
        "Not needed spendings",
        GroupKind::Expense,
        &[
            "Meals on the Road",
            "Olympus Mons Tours",
            "Souvenirs",
            "Holodeck",
        ],
    ),
    (
        "Fixed costs",
        GroupKind::Expense,
        &[
            "Rent",
            "Air Recycling",
            "Fusion Power",
            "Oxygen and Water",
            "Subspace Communications",
        ],
    ),
    (
        "Food and household",
        GroupKind::Expense,
        &["Food", "Everyday Spending"],
    ),
    (
        "Obligations",
        GroupKind::Expense,
        &["group:Fixed costs", "Insurance", "Taxes and Fees"],
    ),
    (
        "Work income",
        GroupKind::Income,
        &[
            "Salary Hydroponics Engineer",
            "Low-Gravity Allowance",
            "Mine Shift Bonus",
        ],
    ),
];

/// Hidden from charts, though still counted in the groups they are in
/// (Souvenirs is in "Not needed spendings", Q-225).
const HIDDEN: [&str; 4] = [
    "Small Expenses",
    "Rover Parking",
    "Refund Radiation Shielding",
    "Souvenirs",
];

fn the_book() -> Book {
    let bytes = fs::read(books().join("space-colony-5y-v15.sqlite")).expect("the KMyMoney book");
    let Imported::Book(imported) =
        import(&KmySqlite, &bytes, &Limits::default()).expect("the file reads")
    else {
        panic!("a book");
    };
    let mut book = Book::create_in_memory().expect("a book in memory");
    fill_book_from(&mut book, &imported, &HashMap::new()).expect("filled");

    let categories: HashMap<String, _> = book
        .categories()
        .expect("categories")
        .into_iter()
        .map(|entry| (entry.category.name.clone(), entry.category.id))
        .collect();
    let mut groups = HashMap::new();
    for (name, kind, members) in GROUPS {
        let members: Vec<GroupMember> = members
            .iter()
            .map(|member| match member.strip_prefix("group:") {
                Some(group) => GroupMember::Group(groups[group]),
                None => GroupMember::Category(
                    *categories
                        .get(*member)
                        .unwrap_or_else(|| panic!("no category {member}")),
                ),
            })
            .collect();
        let id = book
            .add_category_group(name, kind, &members)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        groups.insert(name, id);
    }
    let hidden: Vec<_> = HIDDEN.iter().map(|name| categories[*name]).collect();
    book.set_hidden_categories(&hidden).expect("hidden");
    book
}

/// The book's complete export, with every ID numbered in the order it first
/// appears: the same book always gives the same file.
fn exported(book: &Book) -> String {
    let data = export_book(book).expect("gathered");
    let bytes = export(&BookExport, &data).expect("written");
    let text = String::from_utf8(bytes).expect("UTF-8");
    let mut numbers: HashMap<&str, usize> = HashMap::new();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        let rest = &text[at..];
        if rest.len() >= 36
            && is_id(&rest.as_bytes()[..36])
            && !rest[36..].starts_with(|c: char| c.is_ascii_hexdigit())
        {
            let next = numbers.len() + 1;
            let number = *numbers.entry(&rest[..36]).or_insert(next);
            out.push_str(&format!("019a0000-0000-7000-8000-{number:012x}"));
            at += 36;
        } else {
            let character = rest.chars().next().expect("text left");
            out.push(character);
            at += character.len_utf8();
        }
    }
    out
}

fn is_id(bytes: &[u8]) -> bool {
    bytes.iter().enumerate().all(|(place, &byte)| match place {
        8 | 13 | 18 | 23 => byte == b'-',
        _ => byte.is_ascii_hexdigit(),
    })
}

#[test]
fn the_committed_export_is_what_the_book_gives() {
    let made = exported(&the_book());
    let path = books().join("space-colony-5y.json");
    if std::env::var_os("KASSENSTURZ_BLESS").is_some() {
        fs::write(&path, &made).expect("written");
    }
    let committed = fs::read_to_string(&path).expect("the committed export");
    assert!(
        committed == made,
        "{} is not what the book gives; run with KASSENSTURZ_BLESS=1 and commit the result",
        path.display()
    );
}

#[test]
fn the_export_opens_as_a_book_with_its_groups_and_hide_list() {
    let text = fs::read_to_string(books().join("space-colony-5y.json")).expect("the export");
    let data = read_book(text.as_bytes()).expect("a version 8 book");
    assert_eq!(data.groups.len(), GROUPS.len());
    assert_eq!(data.hidden_categories.len(), HIDDEN.len());
    let mut book = Book::create_in_memory().expect("a book in memory");
    fill_book(&mut book, &data).expect("filled");
    let opened = ExportData::Book(data);
    // Opening and writing again changes nothing.
    assert_eq!(exported(&book), exported_from(&opened));
    let groups = book.category_groups().expect("groups");
    assert_eq!(
        groups
            .iter()
            .map(|group| group.name.as_str())
            .collect::<Vec<_>>(),
        GROUPS.map(|(name, _, _)| name)
    );
    let obligations = &groups[3];
    assert!(
        obligations
            .members
            .iter()
            .any(|member| matches!(member, GroupMember::Group(_)))
    );
    assert_eq!(
        book.hidden_categories().expect("hidden").len(),
        HIDDEN.len()
    );
}

fn exported_from(data: &ExportData) -> String {
    let ExportData::Book(data) = data else {
        panic!("a book")
    };
    let mut book = Book::create_in_memory().expect("a book in memory");
    fill_book(&mut book, data).expect("filled");
    exported(&book)
}
