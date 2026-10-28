// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The reading of each row of the book is the user's to allow (D-252), on
//! the synthetic Space Colony book as the importer and the store really
//! fill it (AGENTS.md §9.4: tests where steps meet).

#![cfg(feature = "ai")]

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use kassensturz_ai_tools::{
    FUNCTIONS, ToolError, call, call_allowed, overview_allowed, readable_functions,
};
use kassensturz_core::Date;
use kassensturz_core::ai_switch::{Permission, Permissions};
use kassensturz_import::{Imported, KmySqlite, Limits, import};
use kassensturz_pipeline::fill_book_from;
use kassensturz_store::Book;
use serde_json::json;

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

fn names(permissions: Permissions) -> Vec<&'static str> {
    readable_functions(permissions)
        .map(|function| function.name)
        .collect()
}

#[test]
fn nothing_is_read_until_the_user_allows_it() {
    let book = space_colony();
    assert!(names(Permissions::NONE).is_empty());
    for function in FUNCTIONS {
        let refused = call_allowed(&book, function.name, &json!({}), TODAY, Permissions::NONE);
        assert!(
            matches!(refused, Err(ToolError::NotAllowed { argument }) if argument == function.name),
            "{}",
            function.name
        );
    }
}

#[test]
fn each_row_opens_only_the_functions_that_read_it() {
    assert_eq!(
        names(Permissions::NONE.with(Permission::AccountsRead, true)),
        ["list_accounts", "account_balance"]
    );
    assert_eq!(
        names(Permissions::NONE.with(Permission::RulesRead, true)),
        ["list_rules"]
    );
    assert_eq!(
        names(Permissions::NONE.with(Permission::TransactionsRead, true)),
        ["search_transactions"]
    );
    // Totals per category read both rows.
    let both = Permissions::NONE
        .with(Permission::CategoriesRead, true)
        .with(Permission::TransactionsRead, true);
    assert_eq!(
        names(both),
        ["category_totals", "search_transactions", "list_categories"]
    );
    assert_eq!(names(Permissions::all()).len(), FUNCTIONS.len());
}

#[test]
fn a_change_gives_the_reading_of_its_row_and_an_addition_does_not() {
    assert_eq!(
        names(Permissions::NONE.with(Permission::DeleteRules, true)),
        ["list_rules"]
    );
    assert!(names(Permissions::NONE.with(Permission::NewRules, true)).is_empty());
}

#[test]
fn an_allowed_read_gives_the_same_answer_as_the_plain_call() {
    let book = space_colony();
    let accounts = Permissions::NONE.with(Permission::AccountsRead, true);
    let allowed = call_allowed(&book, "list_accounts", &json!({}), TODAY, accounts).expect("read");
    let plain = call(&book, "list_accounts", &json!({}), TODAY).expect("read");
    assert_eq!(allowed.value, plain.value);
}

#[test]
fn the_overview_holds_only_what_the_user_allows_reading() {
    let book = space_colony();
    let none = overview_allowed(&book, TODAY, Permissions::NONE).expect("overview");
    assert_eq!(none.value["today"], "2026-10-25");
    assert!(none.value.get("accounts").is_none() && none.value.get("this_month").is_none());
    let accounts = Permissions::NONE.with(Permission::AccountsRead, true);
    let some = overview_allowed(&book, TODAY, accounts).expect("overview");
    assert!(some.value.get("this_month").is_none());
    assert!(some.value.get("accounts").is_some(), "{}", some.value);
    let all = overview_allowed(&book, TODAY, Permissions::all()).expect("overview");
    assert!(all.value.get("this_month").is_some());
}
