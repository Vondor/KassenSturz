// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! B-283: KMyMoney allows two accounts of one name, e.g. two "Daily
//! money" at two banks, each grouped under its bank's account. A book
//! doesn't, so the second gets its bank's name after it, or a number, and
//! both come in. The book is the real importer's reading of the synthetic
//! schedules sample, with the names and banks changed; the bank names are
//! made up.

use std::collections::HashMap;

use kassensturz_export::ExportData;
use kassensturz_import::{BookInstitution, Imported, ImportedBook, Kmy, Limits, import};
use kassensturz_pipeline::{export_book, fill_book, fill_book_from};
use kassensturz_store::{Book, StoreError};

const XML: &[u8] = include_bytes!("../../kassensturz-import/tests/golden/kmy/schedules.kmy");

fn imported() -> ImportedBook {
    match import(&Kmy, XML, &Limits::default()).expect("the sample imports") {
        Imported::Book(book) => *book,
        _ => panic!("the sample is a book"),
    }
}

/// The sample with its two checking accounts both called "Daily money",
/// the second grouped under the first, as KMyMoney allows.
fn two_of_one_name(banks: bool) -> ImportedBook {
    let mut file = imported();
    let keys: Vec<String> = file
        .accounts
        .iter()
        .filter(|account| account.kind.is_account())
        .map(|account| account.key.clone())
        .collect();
    let [first, second, ..] = keys.as_slice() else {
        panic!("two accounts in the sample: {keys:?}");
    };
    if banks {
        for (key, name) in [("I1", "Lumen Bank"), ("I2", "Harbor Savings")] {
            file.institutions.push(BookInstitution {
                key: key.into(),
                name: name.into(),
                bank_code: None,
                contact: None,
                address: None,
                phone: None,
            });
        }
    }
    for account in &mut file.accounts {
        if &account.key == first {
            account.name = "Daily money".into();
            account.institution = banks.then(|| "I1".into());
        } else if &account.key == second {
            account.name = "Daily money".into();
            account.parent = Some(first.clone());
            account.institution = banks.then(|| "I2".into());
        }
    }
    file
}

fn account_names(book: &Book) -> Vec<String> {
    book.accounts(true)
        .expect("read")
        .into_iter()
        .map(|account| account.name)
        .collect()
}

#[test]
fn two_kmymoney_accounts_of_one_name_both_come_in() {
    let mut book = Book::create_in_memory().expect("book created");
    let report =
        fill_book_from(&mut book, &two_of_one_name(true), &HashMap::new()).expect("both come in");
    let names = account_names(&book);
    assert!(names.contains(&"Daily money".to_owned()), "{names:?}");
    assert!(
        names.contains(&"Daily money (Harbor Savings)".to_owned()),
        "{names:?}"
    );
    assert_eq!(
        report
            .renamed
            .iter()
            .map(|(_, name)| name.as_str())
            .collect::<Vec<_>>(),
        ["Daily money"]
    );

    // Without banks, a number.
    let mut book = Book::create_in_memory().expect("book created");
    fill_book_from(&mut book, &two_of_one_name(false), &HashMap::new()).expect("both come in");
    let names = account_names(&book);
    assert!(names.contains(&"Daily money".to_owned()), "{names:?}");
    assert!(names.contains(&"Daily money (2)".to_owned()), "{names:?}");
}

#[test]
fn a_complete_export_with_two_accounts_of_one_name_names_them_in_the_error() {
    let mut book = Book::create_in_memory().expect("book created");
    fill_book_from(&mut book, &two_of_one_name(false), &HashMap::new()).expect("filled");
    let ExportData::Book(mut data) = export_book(&book).expect("exported") else {
        panic!("a book was gathered");
    };
    for account in &mut data.accounts {
        if account.name == "Daily money (2)" {
            account.name = "Daily money".into();
        }
    }
    let mut new = Book::create_in_memory().expect("book created");
    let error = fill_book(&mut new, &data).expect_err("a book can't hold both");
    let StoreError::DuplicateAccountName { name, .. } = &error else {
        panic!("{error:?}");
    };
    assert_eq!(name, "Daily money");
    let report = kassensturz_core::Report::report(&error);
    assert!(
        report.params.iter().any(|(key, value)| *key == "name"
            && *value == kassensturz_core::ErrorParam::Text("Daily money".into())),
        "{report:?}"
    );
    assert!(new.is_empty().expect("read"));
}

/// B-272 meets B-283: a name that only differs by a character that shows
/// as nothing is the same name, and a bank's name is cleaned like an
/// account's before it goes after one.
#[test]
fn a_look_alike_name_gets_a_clean_bank_name_after_it() {
    let mut file = two_of_one_name(true);
    for institution in &mut file.institutions {
        if institution.key == "I2" {
            institution.name = "Harbor\u{200B} Savings".into();
        }
    }
    let mut second = true;
    for account in file.accounts.iter_mut().rev() {
        if account.name == "Daily money" && second {
            account.name = "Daily\u{200B} money".into();
            second = false;
        }
    }
    let mut book = Book::create_in_memory().expect("book created");
    fill_book_from(&mut book, &file, &HashMap::new()).expect("both come in");
    let mut names: Vec<String> = account_names(&book)
        .into_iter()
        .filter(|name| name.starts_with("Daily"))
        .collect();
    names.sort();
    assert_eq!(names, ["Daily money", "Daily money (Harbor Savings)"]);
}
