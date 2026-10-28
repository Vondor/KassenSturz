// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Changing the fields of a transaction (D-257) on the real Space Colony
//! book, with what the read functions give a model: the preview says what
//! the search said, the balance moves by exactly the change, undo takes
//! all of it back, and what is locked or isn't plain is refused.

#![cfg(feature = "ai")]

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use kassensturz_ai_tools::{Confirmed, FieldValue, ProposedChange, call, propose as propose_with};
use kassensturz_core::ai_switch::{Field, Permissions};
use kassensturz_core::{Date, Report};
use kassensturz_import::{Imported, KmySqlite, Limits, import};
use kassensturz_pipeline::fill_book_from;
use kassensturz_store::{Action, Book};
use serde_json::{Value, json};

/// Everything allowed, nothing locked.
fn open() -> Permissions {
    Field::ALL
        .iter()
        .fold(Permissions::all(), |p, field| p.with_lock(*field, false))
}

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

/// What the read functions say about the whole book.
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

/// The newest entry with one category, like a model finds it with
/// `search_transactions`. The Space Colony book was imported from a file,
/// so every part of it counts as delivered (D-067).
fn plain_row(book: &Book) -> Value {
    run(book, "search_transactions", json!({"limit": 50}))["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|row| row.get("category").is_some())
        .expect("an entry with one category")
        .clone()
}

fn find(book: &Book, id: &Value) -> Value {
    run(book, "search_transactions", json!({"limit": 50}))["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|row| row["transaction_id"] == *id)
        .expect("still there")
        .clone()
}

#[test]
fn an_edit_from_a_real_search_is_applied_and_undone() {
    let mut book = space_colony();
    let row = plain_row(&book);
    let before = seen(&book);

    let proposal = propose_with(
        &book,
        &json!({"edit_transactions": [{
            "transaction_id": row["transaction_id"],
            "note": "  Checked by the AI  ",
            "payee": "Colony Cafe",
            "date": "2027-04-07",
        }]}),
        open(),
    )
    .expect("prepared");
    assert_eq!(seen(&book), before, "proposing changes nothing");

    let ProposedChange::EditTransaction(edit) = &proposal.changes.changes()[0] else {
        panic!("an edit");
    };
    // The preview says what the search said.
    assert_eq!(edit.date.to_string(), row["date"].as_str().unwrap());
    assert_eq!(edit.account, row["account"].as_str().unwrap());
    assert_eq!(edit.payee.as_deref(), row["payee"].as_str());
    let fields: Vec<_> = edit.edits.iter().map(|line| line.field.name()).collect();
    assert_eq!(fields, ["date", "payee", "note"]);
    assert!(
        edit.edits
            .iter()
            .any(|line| line.after == FieldValue::Text(Some("Checked by the AI".into())))
    );

    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    let now = find(&book, &row["transaction_id"]);
    assert_eq!(now["payee"], "Colony Cafe");
    assert_eq!(now["date"], "2027-04-07");
    assert_eq!(now["amount"], row["amount"], "the amount stays");
    assert_eq!(seen(&book).0, before.0, "no account balance changes");
    assert_eq!(
        book.history().expect("history")[0].action,
        Action::ApplyAiChanges
    );

    book.undo().expect("undone").expect("a change");
    assert_eq!(seen(&book), before, "undo takes back all of it");
    assert_eq!(find(&book, &row["transaction_id"]), row);
    book.redo().expect("made again");
    assert_eq!(find(&book, &row["transaction_id"])["payee"], "Colony Cafe");
}

#[test]
fn a_part_a_file_delivered_keeps_its_amount_and_account() {
    let book = space_colony();
    let row = plain_row(&book);
    let other = run(&book, "list_accounts", json!({}))["accounts"]
        .as_array()
        .expect("accounts")
        .iter()
        .find(|account| account["name"] != row["account"])
        .expect("another account")["id"]
        .clone();
    for (key, value) in [("amount", json!("-12.34")), ("account_id", other)] {
        let error = propose_with(
            &book,
            &json!({"edit_transactions": [{"transaction_id": row["transaction_id"], key: value}]}),
            open(),
        )
        .expect_err("refused");
        assert_eq!(error.report().code, "ai-tools.not-editable", "{key}");
    }
}

#[test]
fn a_locked_field_is_refused_and_nothing_changes() {
    let book = space_colony();
    let row = plain_row(&book);
    let before = seen(&book);
    // The defaults lock the amount, the date and the payee.
    for (key, value) in [("amount", "-1.00"), ("date", "2026-10-01"), ("payee", "X")] {
        let error = propose_with(
            &book,
            &json!({"edit_transactions": [{"transaction_id": row["transaction_id"], key: value}]}),
            Permissions::all(),
        )
        .expect_err("refused");
        assert_eq!(error.report().code, "ai-tools.field-locked", "{key}");
    }
    // The note isn't locked by default.
    propose_with(
        &book,
        &json!({"edit_transactions": [{"transaction_id": row["transaction_id"], "note": "ok"}]}),
        Permissions::all(),
    )
    .expect("the note isn't locked");
    assert_eq!(seen(&book), before);
}

#[test]
fn an_edit_is_out_of_date_when_the_entry_changed_in_between() {
    let mut book = space_colony();
    let row = plain_row(&book);
    let first = propose_with(
        &book,
        &json!({"edit_transactions": [{"transaction_id": row["transaction_id"], "note": "one"}]}),
        open(),
    )
    .expect("prepared");
    let second = propose_with(
        &book,
        &json!({"edit_transactions": [{"transaction_id": row["transaction_id"], "note": "two"}]}),
        open(),
    )
    .expect("prepared");
    first
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    let error = second
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect_err("out of date");
    assert_eq!(error.report().code, "ai-changes.out-of-date");
}
