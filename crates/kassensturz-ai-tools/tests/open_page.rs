// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `open_page` with the IDs the read functions really give, on the synthetic
//! Space Colony book as the importer, the pipeline and the store really fill
//! it (AGENTS.md §9.4: tests where steps meet; threat T-36).

#![cfg(feature = "ai")]

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use kassensturz_ai_tools::{PageTarget, ToolError, call, open_page};
use kassensturz_core::Date;
use kassensturz_import::{Imported, KmySqlite, Limits, import};
use kassensturz_pipeline::fill_book_from;
use kassensturz_store::Book;
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

fn run(book: &Book, name: &str) -> Value {
    call(book, name, &Value::Null, TODAY)
        .unwrap_or_else(|error| panic!("{name}: {error}"))
        .value
}

fn first_id(value: &Value, list: &str) -> String {
    value[list][0]["id"].as_str().expect("an ID").to_owned()
}

fn code(result: Result<impl Sized, ToolError>) -> &'static str {
    use kassensturz_core::Report;
    result.err().expect("an error").report().code
}

#[test]
fn a_rule_from_list_rules_opens_the_rules_page_with_it_selected() {
    let book = space_colony();
    let id = first_id(&run(&book, "list_rules"), "rules");
    let (target, output) =
        open_page(&book, &json!({"page": "rule", "id": id})).expect("the Rule exists");
    assert_eq!(
        target,
        PageTarget::Rules {
            select: Some(id.clone())
        }
    );
    assert_eq!(target.id(), Some(id.as_str()));
    assert_eq!(output.value["opened"], "rules");
    assert!(
        output.value["note"].is_string(),
        "the result says it is data"
    );
}

#[test]
fn an_account_from_list_accounts_opens_its_transactions() {
    let book = space_colony();
    let id = first_id(&run(&book, "list_accounts"), "accounts");
    let (target, _) = open_page(&book, &json!({"page": "account", "id": id})).expect("exists");
    assert_eq!(target, PageTarget::Account { id });
}

#[test]
fn the_plain_pages_need_no_id_and_take_none() {
    let book = space_colony();
    for (name, page) in [
        ("overview", PageTarget::Overview),
        ("accounts", PageTarget::Accounts),
        ("categories", PageTarget::Categories),
        ("rules", PageTarget::Rules { select: None }),
    ] {
        let (target, _) = open_page(&book, &json!({"page": name})).expect(name);
        assert_eq!(target, page);
        let id = first_id(&run(&book, "list_rules"), "rules");
        assert_eq!(
            code(open_page(&book, &json!({"page": name, "id": id}))),
            "ai-tools.bad-argument"
        );
    }
}

#[test]
fn an_id_that_is_not_the_right_kind_is_refused() {
    let book = space_colony();
    let rule = first_id(&run(&book, "list_rules"), "rules");
    let category = first_id(&run(&book, "list_categories"), "categories");
    let account = first_id(&run(&book, "list_accounts"), "accounts");
    // A category is not an account, a Rule is not an account, and the
    // reverse.
    for arguments in [
        json!({"page": "account", "id": category}),
        json!({"page": "account", "id": rule}),
    ] {
        assert_eq!(
            code(open_page(&book, &arguments)),
            "ai-tools.unknown-account"
        );
    }
    assert_eq!(
        code(open_page(&book, &json!({"page": "rule", "id": account}))),
        "ai-tools.unknown-rule"
    );
}

#[test]
fn a_call_that_is_not_plain_is_refused_with_the_name_of_its_argument() {
    let book = space_colony();
    for (arguments, argument) in [
        (json!({}), "page"),
        (json!({"page": "settings"}), "page"),
        (json!({"page": "https://example.org"}), "page"),
        (json!({"page": 3}), "page"),
        (json!({"page": "rule"}), "id"),
        (json!({"page": "rule", "id": "not an id"}), "id"),
        (json!({"page": "rule", "id": 7}), "id"),
        (json!({"page": "rules", "path": "/"}), "arguments"),
        (json!("rules"), "arguments"),
    ] {
        let error = open_page(&book, &arguments).expect_err("refused");
        let report = kassensturz_core::Report::report(&error);
        assert_eq!(report.code, "ai-tools.bad-argument", "{arguments}");
        assert!(
            format!("{report:?}").contains(argument),
            "{arguments}: {report:?}"
        );
    }
}

#[test]
fn opening_a_page_changes_nothing_in_the_book() {
    let book = space_colony();
    let read = |book: &Book| {
        [
            run(book, "list_accounts"),
            run(book, "list_categories"),
            run(book, "list_rules"),
        ]
    };
    let before = read(&book);
    let rule = first_id(&before[2], "rules");
    open_page(&book, &json!({"page": "rule", "id": rule})).expect("opens");
    open_page(&book, &json!({"page": "categories"})).expect("opens");
    assert_eq!(read(&book), before);
}
