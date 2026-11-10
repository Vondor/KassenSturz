// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The read tools on the example book Space Colony as the app makes it
//! (US2 Phase 3, card C): the KMyMoney file filled by the pipeline, then
//! its bank statement imported into the checking account, as card 5 of the
//! tester kit does. Only the import runs the Rules, so the book alone has no
//! entry sorted by a Rule (AGENTS.md §9.4: the importer's and the
//! pipeline's real output, no values built here). The book is synthetic.

#![cfg(feature = "ai")]

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use kassensturz_ai_tools::call;
use kassensturz_core::Date;
use kassensturz_import::{Camt053, Imported, KmySqlite, Limits, import};
use kassensturz_pipeline::{Choices, fill_book_from, import_statements};
use kassensturz_store::Book;
use serde_json::{Value, json};

const TODAY: Date = Date::constant(2026, 11, 10);

fn books() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../kassensturz-import/tests/books")
}

/// The example book, with its statement imported when `statement`.
fn space_colony(statement: bool) -> Book {
    let bytes = fs::read(books().join("space-colony-5y-v15.sqlite")).expect("the example book");
    let Imported::Book(imported) =
        import(&KmySqlite, &bytes, &Limits::default()).expect("the file reads")
    else {
        panic!("a book");
    };
    let mut book = Book::create_in_memory().expect("a book in memory");
    fill_book_from(&mut book, &imported, &HashMap::new()).expect("filled");
    if statement {
        let checking = book
            .all_accounts()
            .expect("accounts")
            .into_iter()
            .find(|account| account.name == "Everyday Checking")
            .expect("the checking account")
            .id;
        let bytes = fs::read(books().join("examples/statements/space-colony.camt053.xml"))
            .expect("the statement");
        let Imported::Statements(statements) =
            import(&Camt053, &bytes, &Limits::default()).expect("the statement reads")
        else {
            panic!("a statement");
        };
        import_statements(&mut book, checking, &statements, &Choices::default(), None)
            .expect("imported");
    }
    book
}

/// Every row of `search_transactions` over the whole book, page by page.
fn all_rows(book: &Book) -> Vec<Value> {
    let mut rows = Vec::new();
    let mut offset = 0;
    loop {
        let page = call(
            book,
            "search_transactions",
            &json!({ "limit": 50, "offset": offset }),
            TODAY,
        )
        .expect("a search")
        .value;
        rows.extend(page["rows"].as_array().expect("rows").iter().cloned());
        match page.get("next_offset") {
            Some(next) => offset = next.as_u64().expect("a number"),
            None => return rows,
        }
    }
}

fn sorted_by_rule(rows: &[Value]) -> usize {
    rows.iter().filter(|row| row["sorted_by"] == "rule").count()
}

#[test]
fn the_example_book_has_entries_sorted_by_a_rule_once_its_statement_is_in() {
    // As the chooser opens it: every category came with the file.
    assert_eq!(sorted_by_rule(&all_rows(&space_colony(false))), 0);
    let book = space_colony(true);
    let rows = all_rows(&book);
    assert!(
        sorted_by_rule(&rows) > 0,
        "the statement's entries are sorted by Rules"
    );
    let rules = call(&book, "list_rules", &Value::Null, TODAY)
        .expect("the Rules")
        .value;
    let ids: Vec<&Value> = rules["rules"]
        .as_array()
        .expect("rules")
        .iter()
        .map(|rule| &rule["id"])
        .collect();
    for row in rows.iter().filter(|row| row["sorted_by"] == "rule") {
        assert!(ids.contains(&&row["rule_id"]), "{row}");
    }
}

#[test]
fn payee_totals_always_says_how_it_grouped() {
    let book = space_colony(true);
    for group in ["payee", "other_party_name"] {
        let value = call(
            &book,
            "payee_totals",
            &json!({ "from": "2026-01-01", "to": "2026-12-31", "group_by": group }),
            TODAY,
        )
        .expect("totals")
        .value;
        assert_eq!(value["group_by"], json!(group));
    }
    let value = call(
        &book,
        "payee_totals",
        &json!({ "from": "2026-01-01", "to": "2026-12-31" }),
        TODAY,
    )
    .expect("totals")
    .value;
    assert_eq!(value["group_by"], json!("payee"));
}
