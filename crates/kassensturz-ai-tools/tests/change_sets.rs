// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A change set built from what the read functions really return, applied
//! to the synthetic Space Colony book as the importer, the pipeline and the
//! store really fill it, and undone with the store's real history
//! (AGENTS.md §9.4: tests where steps meet; threat T-27).

#![cfg(feature = "ai")]

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use kassensturz_ai_tools::{Confirmed, ProposedChange, call, propose};
use kassensturz_core::Date;
use kassensturz_import::{Imported, KmySqlite, Limits, import};
use kassensturz_pipeline::fill_book_from;
use kassensturz_store::{Action, Book};
use serde_json::{Value, json};

const TODAY: Date = Date::constant(2026, 10, 25);

fn space_colony() -> Book {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../kassensturz-import/tests/golden/kmy-sqlite/space-colony-v15.sqlite");
    let bytes = fs::read(&path).expect("the Space Colony book is in the repository");
    let Imported::Book(imported) =
        import(&KmySqlite, &bytes, &Limits::default()).expect("the file reads")
    else {
        panic!("a book");
    };
    let mut book = Book::create_in_memory().expect("a book in memory");
    fill_book_from(&mut book, &imported, &HashMap::new()).expect("filled");
    book
}

fn run(book: &Book, name: &str, arguments: Value) -> Value {
    call(book, name, &arguments, TODAY)
        .unwrap_or_else(|error| panic!("{name}: {error}"))
        .value
}

/// What the read functions say about the whole book: what a change set
/// may change, and what it must not.
fn seen(book: &Book) -> (Value, Value, Value, Value) {
    let (from, to) = book
        .booking_date_range()
        .expect("dates")
        .expect("transactions");
    (
        run(book, "list_accounts", json!({"include_closed": true})),
        run(
            book,
            "category_totals",
            json!({"from": from.to_string(), "to": to.to_string()}),
        ),
        run(book, "list_categories", json!({})),
        run(book, "list_rules", json!({})),
    )
}

#[test]
fn a_change_set_from_a_real_search_is_applied_and_undone() {
    let mut book = space_colony();
    let before = seen(&book);
    let history = book.history().expect("history").len();

    // The model searched, and picks the newest entry that has one category.
    let found = run(&book, "search_transactions", json!({"limit": 50}));
    let row = found["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|row| row.get("category").is_some())
        .expect("an entry with one category")
        .clone();
    let currency = row["currency"].as_str().expect("currency");
    let proposal = propose(
        &book,
        &json!({
            "new_categories": [{"key": "new", "name": "Sorted by the AI", "currency": currency}],
            "new_rules": [{
                "name": "Sorted by the AI",
                "conditions": [{"field": "payee", "test": "equals", "text": "No such payee"}],
                "new_category": "new",
            }],
            "categorize": [{"transaction_id": row["transaction_id"], "new_category": "new"}],
        }),
    )
    .expect("prepared");
    assert_eq!(seen(&book), before, "proposing changes nothing");

    // The preview says what the search said.
    let ProposedChange::Categorize(moved) = &proposal.changes.changes()[2] else {
        panic!("the transaction comes last");
    };
    assert_eq!(moved.date.to_string(), row["date"].as_str().unwrap());
    assert_eq!(moved.account, row["account"].as_str().unwrap());
    assert_eq!(moved.payee.as_deref(), row["payee"].as_str());
    let shown = moved
        .before
        .as_ref()
        .expect("a category before")
        .join(" › ");
    assert_eq!(shown, row["category"].as_str().unwrap());
    assert_eq!(moved.after, ["Sorted by the AI"]);
    let new_category = moved.category;

    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    let after = seen(&book);
    assert_eq!(after.0, before.0, "no account balance changes");
    assert_ne!(after.1, before.1, "the totals per category do");
    let totals = after.1["currencies"]
        .as_array()
        .expect("currencies")
        .iter()
        .flat_map(|currency| {
            ["income", "spending"]
                .iter()
                .filter_map(|key| currency[*key].as_array().cloned())
                .flatten()
                .collect::<Vec<_>>()
        })
        .find(|entry| entry["category_id"] == new_category.to_string().as_str());
    assert!(
        totals.is_some(),
        "the new category has the entry: {}",
        after.1
    );
    let changes = book.history().expect("history");
    assert_eq!(changes.len(), history + 1, "one change");
    assert_eq!(changes[0].action, Action::ApplyAiChanges);

    let undone = book.undo().expect("undone").expect("a change");
    assert_eq!(undone.action, Action::ApplyAiChanges);
    assert_eq!(seen(&book), before, "undo takes back all of it");
    book.redo().expect("made again");
    assert_eq!(seen(&book), after);
}
