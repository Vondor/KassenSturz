// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! B-269 (#965): a KMyMoney book whose amounts the book couldn't total is
//! refused as a whole, and the new book stays empty. The book is the real
//! importer's reading of the synthetic schedules sample, with one amount
//! made too large, so the test starts where the importer hands over.

use std::collections::HashMap;

use kassensturz_core::Money;
use kassensturz_import::{Imported, ImportedBook, Kmy, Limits, import};
use kassensturz_pipeline::{PipelineError, fill_book_from};
use kassensturz_store::{Book, MAX_CURRENCY_TOTAL, MAX_PLAN_AMOUNT, StoreError};

const XML: &[u8] = include_bytes!("../../kassensturz-import/tests/golden/kmy/schedules.kmy");

fn imported() -> ImportedBook {
    match import(&Kmy, XML, &Limits::default()).expect("the sample imports") {
        Imported::Book(book) => *book,
        _ => panic!("the sample is a book"),
    }
}

/// Fills a new book from `imported`, expects it refused for amounts that
/// don't add up, and checks nothing of it stayed.
fn refused(imported: &ImportedBook) {
    let mut book = Book::create_in_memory().expect("book created");
    let error = fill_book_from(&mut book, imported, &HashMap::new()).expect_err("refused");
    assert!(
        matches!(error, PipelineError::Store(StoreError::AmountOverflow)),
        "{error:?}"
    );
    assert!(book.is_empty().expect("read"), "the book stays empty");
}

/// Sets the two parts of `parts` to `minor_units` out and in.
fn set(parts: &mut [kassensturz_import::BookPart], minor_units: i64) {
    assert_eq!(parts.len(), 2, "a payment from one account to one category");
    let currency = parts[0].amount.currency();
    parts[0].amount = Money::from_minor_units(-minor_units, currency);
    parts[1].amount = Money::from_minor_units(minor_units, currency);
}

#[test]
fn the_sample_fills_a_book_as_it_is() {
    let mut book = Book::create_in_memory().expect("book created");
    fill_book_from(&mut book, &imported(), &HashMap::new()).expect("filled");
}

#[test]
fn a_kmymoney_transaction_too_large_to_total_is_refused_and_the_book_stays_empty() {
    // Each transaction balances, but its two sides together are more than
    // the book can add up in one currency.
    let mut book = imported();
    set(&mut book.transactions[0].parts, MAX_CURRENCY_TOTAL / 2 + 1);
    refused(&book);

    // Two that fit on their own but not together.
    let mut book = imported();
    book.transactions.push(book.transactions[0].clone());
    for transaction in &mut book.transactions {
        set(&mut transaction.parts, MAX_CURRENCY_TOTAL / 3);
    }
    refused(&book);
}

#[test]
fn a_kmymoney_plan_too_large_to_forecast_is_refused() {
    let mut book = imported();
    set(&mut book.schedules[0].payment.parts, MAX_PLAN_AMOUNT + 1);
    refused(&book);

    // The largest a plan may be still comes in.
    let mut book = imported();
    set(&mut book.schedules[0].payment.parts, MAX_PLAN_AMOUNT);
    let mut filled = Book::create_in_memory().expect("book created");
    fill_book_from(&mut filled, &book, &HashMap::new()).expect("filled");
}
