// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! B-272 (#968): a name another program wrote comes in as one visible line,
//! and a complete export with a look-alike name is refused. The KMyMoney
//! book is the real importer's reading of the synthetic schedules sample.

use std::collections::HashMap;

use kassensturz_export::ExportData;
use kassensturz_import::{Imported, ImportedBook, Kmy, Limits, import};
use kassensturz_pipeline::{export_book, fill_book, fill_book_from};
use kassensturz_store::{Book, MAX_ACCOUNT_NAME_CHARS, StoreError};

const XML: &[u8] = include_bytes!("../../kassensturz-import/tests/golden/kmy/schedules.kmy");

fn imported() -> ImportedBook {
    match import(&Kmy, XML, &Limits::default()).expect("the sample imports") {
        Imported::Book(book) => *book,
        _ => panic!("the sample is a book"),
    }
}

fn names(book: &Book) -> Vec<String> {
    book.all_accounts()
        .expect("read")
        .into_iter()
        .map(|account| account.name)
        .collect()
}

#[test]
fn a_kmymoney_name_comes_in_as_one_visible_line() {
    let mut file = imported();
    let original = file.accounts[0].name.clone();
    file.accounts[0].name = format!("{original}\u{200B}");
    file.accounts[1].name = "Line\nBreak".into();
    file.accounts[2].name = "y".repeat(5000);
    let mut book = Book::create_in_memory().expect("book created");
    let report = fill_book_from(&mut book, &file, &HashMap::new()).expect("filled");
    let names = names(&book);
    assert!(names.contains(&original), "{names:?}");
    assert!(names.contains(&"Line Break".to_owned()), "{names:?}");
    assert!(
        names.contains(&"y".repeat(MAX_ACCOUNT_NAME_CHARS)),
        "{names:?}"
    );
    // Each name that had to change is listed with the name in the file.
    let listed: Vec<&str> = report
        .renamed
        .iter()
        .map(|(_, name)| name.as_str())
        .collect();
    assert!(listed.contains(&format!("{original}\u{200B}").as_str()));
    assert!(listed.contains(&"Line\nBreak"));
    assert_eq!(listed.len(), 3, "{listed:?}");
}

#[test]
fn a_complete_export_with_a_look_alike_name_is_refused() {
    let mut book = Book::create_in_memory().expect("book created");
    fill_book_from(&mut book, &imported(), &HashMap::new()).expect("filled");
    let ExportData::Book(data) = export_book(&book).expect("exported") else {
        panic!("a book was gathered");
    };
    let mut broken = data.clone();
    let name = broken.accounts[0].name.clone();
    broken.accounts[0].name = format!("{name}\u{200B}");
    let mut new = Book::create_in_memory().expect("book created");
    let error = fill_book(&mut new, &broken).expect_err("refused");
    assert!(
        matches!(
            error,
            StoreError::AccountName {
                reason: "invisible",
                ..
            }
        ),
        "{error:?}"
    );
    assert!(new.is_empty().expect("read"));
    // The export as it was fills a book.
    fill_book(&mut Book::create_in_memory().expect("book created"), &data).expect("filled");
}
