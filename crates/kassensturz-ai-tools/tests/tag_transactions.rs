// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tags in the AI's read functions and in its change sets (US5.2 step b,
//! D-744), on the synthetic Space Colony book as the KMyMoney importer and
//! the pipeline fill it, with tags the store itself wrote: `list_tags`,
//! the search's tag filter and rows, the sum by tag of `category_totals`
//! (D-714: only the tagged category-side parts), and `tag_transactions`
//! behind its own cell, applied as one Undo and refused when the entry
//! changed in between. Synthetic data only.

#![cfg(feature = "ai")]

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use kassensturz_ai_tools::{Confirmed, ProposedChange, call, call_allowed, propose};
use kassensturz_core::ai_switch::{Permission, Permissions};
use kassensturz_core::{
    Account, AccountKind, Date, Money, Report as _, Split, Tag, TagId, Transaction, TransactionId,
};
use kassensturz_import::{Imported, KmySqlite, Limits, import};
use kassensturz_pipeline::fill_book_from;
use kassensturz_store::{Action, Book};
use serde_json::{Value, json};

const TODAY: Date = Date::constant(2027, 6, 30);
const DAY: Date = Date::constant(2027, 6, 15);

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

struct Fixture {
    book: Book,
    holiday: TagId,
    tax: TagId,
    /// Checking pays two categories; only the second part carries Holiday.
    split_tagged: TransactionId,
    /// Checking pays one category, the whole entry tagged Holiday.
    whole_tagged: TransactionId,
}

fn tag(book: &mut Book, name: &str) -> TagId {
    let id = TagId::generate();
    book.add_tag(&Tag {
        id,
        name: name.into(),
    })
    .expect("tag added");
    id
}

fn by_name(accounts: &[Account], name: &str) -> Account {
    accounts
        .iter()
        .find(|account| account.name == name)
        .unwrap_or_else(|| panic!("{name}"))
        .clone()
}

fn fixture() -> Fixture {
    let mut book = space_colony();
    let holiday = tag(&mut book, "Holiday");
    let tax = tag(&mut book, "Tax");
    let accounts = book.all_accounts().expect("accounts");
    let checking = by_name(&accounts, "Everyday Checking");
    let currency = checking.currency;
    let mut spending = accounts
        .iter()
        .filter(|account| account.kind == AccountKind::Expense && !account.closed)
        .filter(|account| account.currency == currency);
    let first = spending.next().expect("a spending category").id;
    let second = spending.next().expect("another spending category").id;
    let money = |minor| Money::from_minor_units(minor, currency);
    let split_tagged = TransactionId::generate();
    book.add_transaction(
        &Transaction::new(
            split_tagged,
            DAY,
            Some("Orbit Travel Desk".into()),
            vec![
                Split::new(checking.id, money(-9_000)),
                Split::new(first, money(6_000)),
                Split {
                    tags: vec![holiday],
                    ..Split::new(second, money(3_000))
                },
            ],
        )
        .expect("balanced"),
    )
    .expect("added");
    let whole_tagged = TransactionId::generate();
    book.add_transaction(
        &Transaction::new(
            whole_tagged,
            DAY,
            Some("Crater Lodge".into()),
            vec![
                Split::new(checking.id, money(-2_500)),
                Split::new(first, money(2_500)),
            ],
        )
        .expect("balanced")
        .with_tags(vec![holiday]),
    )
    .expect("added");
    Fixture {
        book,
        holiday,
        tax,
        split_tagged,
        whole_tagged,
    }
}

fn run(book: &Book, name: &str, arguments: Value) -> Value {
    call(book, name, &arguments, TODAY)
        .unwrap_or_else(|error| panic!("{name}: {error}"))
        .value
}

fn june(tag: Option<TagId>) -> Value {
    let mut arguments = json!({"from": "2027-06-01", "to": "2027-06-30"});
    if let Some(tag) = tag {
        arguments["tag_id"] = json!(tag.to_string());
    }
    arguments
}

#[test]
fn list_tags_names_each_tag_with_how_many_transactions_carry_it() {
    let f = fixture();
    let tags = run(&f.book, "list_tags", json!({}))["tags"].clone();
    assert_eq!(
        tags,
        json!([
            {"name": "Holiday", "tag_id": f.holiday.to_string(), "transactions": 2},
            {"name": "Tax", "tag_id": f.tax.to_string(), "transactions": 0},
        ])
    );
}

#[test]
fn the_sum_by_tag_counts_only_the_tagged_category_parts() {
    let f = fixture();
    let all = run(&f.book, "category_totals", june(None));
    let tagged = run(&f.book, "category_totals", june(Some(f.holiday)));
    assert_eq!(tagged["tag"], "Holiday");
    assert!(all.get("tag").is_none());
    // 25.00 tagged on the whole entry and 30.00 tagged on one part; the
    // other part's 60.00 and the account side never count (D-714).
    let currency = &tagged["currencies"][0];
    assert_eq!(currency["money_out"], "55.00");
    assert_eq!(currency["money_in"], "0.00");
    assert_eq!(tagged["transfers"], 0);
    let empty = run(&f.book, "category_totals", june(Some(f.tax)));
    assert_eq!(empty["tag"], "Tax");
    assert_eq!(empty["currencies"], json!([]));
}

#[test]
fn the_search_keeps_tagged_transactions_and_names_their_tags() {
    let f = fixture();
    let rows = run(
        &f.book,
        "search_transactions",
        json!({"tag_id": f.holiday.to_string()}),
    )["rows"]
        .clone();
    let mut ids: Vec<String> = rows
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| row["transaction_id"].as_str().expect("id").to_owned())
        .collect();
    ids.sort();
    let mut expected = vec![f.split_tagged.to_string(), f.whole_tagged.to_string()];
    expected.sort();
    assert_eq!(ids, expected);
    for row in rows.as_array().expect("rows") {
        assert_eq!(row["tags"], json!(["Holiday"]));
        assert_eq!(row["tag_ids"], json!([f.holiday.to_string()]));
    }
    // An untagged row has no tag keys at all.
    let all = run(&f.book, "search_transactions", json!({"limit": 50}));
    assert!(
        all["rows"]
            .as_array()
            .expect("rows")
            .iter()
            .filter(|row| row["payee"] != "Orbit Travel Desk" && row["payee"] != "Crater Lodge")
            .all(|row| row.get("tags").is_none() && row.get("tag_ids").is_none())
    );
}

#[test]
fn an_unknown_tag_is_refused_in_the_read_functions() {
    let f = fixture();
    let other = TagId::generate().to_string();
    for (name, arguments) in [
        (
            "category_totals",
            json!({"from": "2027-06-01", "to": "2027-06-30", "tag_id": other}),
        ),
        ("search_transactions", json!({"tag_id": other})),
    ] {
        let error = call(&f.book, name, &arguments, TODAY).expect_err("unknown");
        assert_eq!(error.model_report().code, "ai-tools.unknown-tag", "{name}");
    }
    let error = call(
        &f.book,
        "search_transactions",
        &json!({"tag_id": "not an id"}),
        TODAY,
    )
    .expect_err("not an id");
    assert_eq!(error.model_report().code, "ai-tools.bad-argument");
}

#[test]
fn list_tags_needs_the_reading_of_transactions() {
    let f = fixture();
    let error = call_allowed(&f.book, "list_tags", &json!({}), TODAY, Permissions::NONE)
        .expect_err("not allowed");
    assert_eq!(error.model_report().code, "ai-tools.not-allowed");
    call_allowed(
        &f.book,
        "list_tags",
        &json!({}),
        TODAY,
        Permissions::NONE.with(Permission::TransactionsRead, true),
    )
    .expect("allowed");
}

fn change(add: &[TagId], remove: &[TagId], transaction: TransactionId) -> Value {
    let ids = |tags: &[TagId]| tags.iter().map(ToString::to_string).collect::<Vec<_>>();
    json!({"tag_transactions": [{
        "transaction_id": transaction.to_string(),
        "add_tag_ids": ids(add),
        "remove_tag_ids": ids(remove),
    }]})
}

#[test]
fn tagging_is_off_until_its_own_cell_is_on() {
    let f = fixture();
    let call = change(&[f.tax], &[], f.whole_tagged);
    let off = Permissions::all().with(Permission::TagTransactions, false);
    let error = propose(&f.book, &call, off).expect_err("not allowed");
    assert_eq!(error.model_report().code, "ai-tools.not-allowed");
    // The grid starts with it off (AGENTS.md §1: defaults never lower
    // protection).
    assert!(!Permissions::defaults().allows(Permission::TagTransactions));
}

#[test]
fn a_tag_change_is_reviewed_applied_as_one_undo_and_keeps_a_part_s_own_tags() {
    let mut f = fixture();
    let before_split = f.book.transaction(f.split_tagged).expect("stored");
    let before_whole = f.book.transaction(f.whole_tagged).expect("stored");
    let call = json!({"tag_transactions": [
        {"transaction_id": f.split_tagged.to_string(), "add_tag_ids": [f.tax.to_string()]},
        {"transaction_id": f.whole_tagged.to_string(),
         "add_tag_ids": [f.tax.to_string()], "remove_tag_ids": [f.holiday.to_string()]},
    ]});
    let proposal = propose(&f.book, &call, Permissions::all()).expect("prepared");
    let [
        ProposedChange::TagTransaction(first),
        ProposedChange::TagTransaction(second),
    ] = proposal.changes.changes()
    else {
        panic!("{:?}", proposal.changes.changes());
    };
    assert_eq!(first.transaction, f.split_tagged);
    assert_eq!(first.payee.as_deref(), Some("Orbit Travel Desk"));
    assert_eq!(first.account.as_deref(), Some("Everyday Checking"));
    assert_eq!(first.amount.minor_units(), -9_000);
    assert_eq!(first.added, ["Tax"]);
    assert!(first.removed.is_empty());
    assert_eq!(second.added, ["Tax"]);
    assert_eq!(second.removed, ["Holiday"]);
    assert_eq!(second.date, DAY);
    // Nothing is written before the user confirms.
    assert_eq!(
        f.book.transaction(f.whole_tagged).expect("stored"),
        before_whole
    );

    let history = f.book.history().expect("history").len();
    proposal
        .changes
        .apply(&mut f.book, Confirmed::by_the_user())
        .expect("applied");
    let split = f.book.transaction(f.split_tagged).expect("stored");
    assert_eq!(split.tags(), [f.tax]);
    // The part's own Holiday stays where the user put it.
    assert_eq!(split.splits()[2].tags, [f.holiday]);
    assert_eq!(split.splits(), before_split.splits());
    assert_eq!(
        f.book.transaction(f.whole_tagged).expect("stored").tags(),
        [f.tax]
    );
    let entries = f.book.history().expect("history");
    assert_eq!(entries.len(), history + 1, "one entry for the whole set");
    assert_eq!(entries[0].action, Action::ApplyAiChanges);

    f.book.undo().expect("undone").expect("a change");
    assert_eq!(
        f.book.transaction(f.split_tagged).expect("stored"),
        before_split
    );
    assert_eq!(
        f.book.transaction(f.whole_tagged).expect("stored"),
        before_whole
    );
}

#[test]
fn a_tag_change_is_out_of_date_when_the_entry_changed_in_between() {
    let mut f = fixture();
    let call = change(&[f.tax], &[], f.whole_tagged);
    let first = propose(&f.book, &call, Permissions::all()).expect("prepared");
    let second = propose(&f.book, &call, Permissions::all()).expect("prepared");
    first
        .changes
        .apply(&mut f.book, Confirmed::by_the_user())
        .expect("applied");
    let error = second
        .changes
        .apply(&mut f.book, Confirmed::by_the_user())
        .expect_err("out of date");
    assert_eq!(error.report().code, "ai-changes.out-of-date");
}

#[test]
fn what_the_model_cannot_tag_is_refused_and_nothing_is_prepared() {
    let f = fixture();
    let unknown = TagId::generate();
    let cases: Vec<(Value, &str)> = vec![
        // A tag the book doesn't have: the model never makes one.
        (
            change(&[unknown], &[], f.whole_tagged),
            "ai-tools.unknown-tag",
        ),
        // A transaction the book doesn't have.
        (
            change(&[f.tax], &[], TransactionId::generate()),
            "ai-tools.unknown-transaction",
        ),
        // Nothing to change.
        (change(&[], &[], f.whole_tagged), "ai-tools.bad-argument"),
        // Added though it is already on the whole entry.
        (
            change(&[f.holiday], &[], f.whole_tagged),
            "ai-tools.bad-argument",
        ),
        // Removed though only a part carries it: a part's tags stay.
        (
            change(&[], &[f.holiday], f.split_tagged),
            "ai-tools.bad-argument",
        ),
        // The same tag twice.
        (
            change(&[f.tax, f.tax], &[], f.whole_tagged),
            "ai-tools.bad-argument",
        ),
    ];
    for (call, code) in cases {
        let error = propose(&f.book, &call, Permissions::all()).expect_err("refused");
        assert_eq!(error.model_report().code, code, "{call}");
    }
    let twice = json!({"tag_transactions": [
        {"transaction_id": f.whole_tagged.to_string(), "add_tag_ids": [f.tax.to_string()]},
        {"transaction_id": f.whole_tagged.to_string(), "remove_tag_ids": [f.holiday.to_string()]},
    ]});
    let error = propose(&f.book, &twice, Permissions::all()).expect_err("twice");
    let report = error.model_report();
    assert_eq!(report.code, "ai-tools.not-changeable");
    assert!(format!("{report:?}").contains("also-changed"), "{report:?}");
}
