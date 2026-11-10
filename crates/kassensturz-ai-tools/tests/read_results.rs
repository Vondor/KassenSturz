// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the read functions return beyond their figures (US2, #593, D-670):
//! every name with its ID, a category's parent, each text once, paging of a
//! search, payees grouped by the bank's other party name, and a refused
//! period that names its arguments. The book is synthetic and filled
//! through the store's own operations.

#![cfg(feature = "ai")]

use kassensturz_ai_tools::{ToolError, call};
use kassensturz_core::{
    Account, AccountId, AccountKind, Actions, BookTo, Condition, CurrencyCode, Date, ErrorParam,
    ImportedDetails, MatchMode, Money, Report, Rule, RuleId, Split, Transaction, TransactionId,
};
use kassensturz_store::Book;
use serde_json::{Value, json};

const TODAY: Date = Date::constant(2026, 10, 25);

fn eur(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, CurrencyCode::new("EUR").expect("EUR"))
}

fn account(name: &str, kind: AccountKind, parent: Option<AccountId>) -> Account {
    Account {
        id: AccountId::generate(),
        name: name.to_owned(),
        kind,
        currency: CurrencyCode::new("EUR").expect("EUR"),
        account_type: None,
        parent,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    }
}

struct Ids {
    checking: AccountId,
    savings: AccountId,
    food: AccountId,
    groceries: AccountId,
    /// An income category with the same name as the spending one.
    groceries_refund: AccountId,
}

/// An entry of the checking account: `minor_units` out (negative) or in,
/// booked to `category`, with what the bank delivered and the memo.
struct Entry<'a> {
    day: i8,
    minor_units: i64,
    category: Option<AccountId>,
    payee: Option<&'a str>,
    other_party: Option<&'a str>,
    bank_text: Option<&'a str>,
    memo: Option<&'a str>,
}

fn book() -> (Book, Ids) {
    let mut book = Book::create_in_memory().expect("a book in memory");
    book.use_currency(CurrencyCode::new("EUR").expect("EUR"))
        .expect("euros");
    let checking = account("Everyday Checking", AccountKind::Asset, None);
    let savings = account("Rainy Day Savings", AccountKind::Asset, None);
    let food = account("Food", AccountKind::Expense, None);
    let groceries = account("Groceries", AccountKind::Expense, Some(food.id));
    let refund_parent = account("Food", AccountKind::Income, None);
    let groceries_refund = account("Groceries", AccountKind::Income, Some(refund_parent.id));
    for a in [
        &checking,
        &savings,
        &food,
        &groceries,
        &refund_parent,
        &groceries_refund,
    ] {
        book.add_account(a).expect("an account");
    }
    let ids = Ids {
        checking: checking.id,
        savings: savings.id,
        food: food.id,
        groceries: groceries.id,
        groceries_refund: groceries_refund.id,
    };
    let entries = [
        // The memo is part of the bank text: sent once, as the bank text.
        Entry {
            day: 1,
            minor_units: -2_000,
            category: Some(groceries.id),
            payee: Some("Supermarket"),
            other_party: Some("Corner Grocer North"),
            bank_text: Some("CARD PAYMENT  Corner Grocer   North 0815"),
            memo: Some("Corner Grocer North"),
        },
        // The bank text is part of the memo: sent once, as the memo.
        Entry {
            day: 2,
            minor_units: -3_000,
            category: Some(groceries.id),
            payee: Some("Supermarket"),
            other_party: Some("Corner Grocer South"),
            bank_text: Some("Grocer South"),
            memo: Some("Weekly shop at Corner Grocer South"),
        },
        // Two different texts: both are sent.
        Entry {
            day: 3,
            minor_units: -1_500,
            category: Some(food.id),
            payee: Some("Supermarket"),
            other_party: Some("Corner Grocer North"),
            bank_text: Some("CARD PAYMENT 0816"),
            memo: Some("Party snacks"),
        },
        // A refund booked to the income category of the same name.
        Entry {
            day: 4,
            minor_units: 500,
            category: Some(groceries_refund.id),
            payee: None,
            other_party: Some("Corner Grocer North"),
            bank_text: None,
            memo: None,
        },
        // Entered by hand: no bank details, only the payee.
        Entry {
            day: 5,
            minor_units: -700,
            category: None,
            payee: Some("Farmers market"),
            other_party: None,
            bank_text: None,
            memo: None,
        },
    ];
    for entry in entries {
        let mut paid = Split::new(checking.id, eur(entry.minor_units));
        if entry.other_party.is_some() || entry.bank_text.is_some() {
            paid.imported = Some(ImportedDetails {
                other_party_name: entry.other_party.map(str::to_owned),
                remittance_information: entry.bank_text.map(str::to_owned),
                ..ImportedDetails::default()
            });
        }
        let other = match entry.category {
            Some(category) => Split::new(category, eur(-entry.minor_units)),
            None => Split::uncategorized(eur(-entry.minor_units)),
        };
        let transaction = Transaction::new(
            TransactionId::generate(),
            Date::constant(2026, 10, entry.day),
            entry.payee.map(str::to_owned),
            vec![paid, other],
        )
        .expect("a transaction")
        .with_note(entry.memo.map(str::to_owned));
        book.add_transaction(&transaction).expect("added");
    }
    // A joined Transfer to savings.
    let transfer = Transaction::new(
        TransactionId::generate(),
        Date::constant(2026, 10, 6),
        None,
        vec![
            Split::new(checking.id, eur(-10_000)),
            Split::new(savings.id, eur(10_000)),
        ],
    )
    .expect("a transfer");
    book.add_transaction(&transfer).expect("added");

    let rule = Rule {
        id: RuleId::generate(),
        name: "Card payments".to_owned(),
        match_mode: MatchMode::All,
        conditions: vec![Condition::Account(checking.id)],
        actions: Actions {
            book_to: Some(BookTo::Category(groceries.id)),
            payee: Some("Supermarket".to_owned()),
        },
    };
    book.add_rule(&rule, None).expect("a Rule");
    let to_savings = Rule {
        id: RuleId::generate(),
        name: "Saving".to_owned(),
        match_mode: MatchMode::All,
        conditions: vec![Condition::Account(checking.id)],
        actions: Actions {
            book_to: Some(BookTo::Transfer(savings.id)),
            payee: None,
        },
    };
    book.add_rule(&to_savings, None).expect("a Rule");
    (book, ids)
}

fn run(book: &Book, name: &str, arguments: Value) -> Value {
    call(book, name, &arguments, TODAY)
        .unwrap_or_else(|error| panic!("{name}: {error}"))
        .value
}

fn rows(value: &Value) -> &Vec<Value> {
    value["rows"].as_array().expect("rows")
}

fn row_of_day<'a>(value: &'a Value, day: &str) -> &'a Value {
    rows(value)
        .iter()
        .find(|row| row["date"] == day && row["account"] == "Everyday Checking")
        .unwrap_or_else(|| panic!("no row on {day}"))
}

#[test]
fn a_category_row_names_the_category_it_is_in() {
    let (book, ids) = book();
    let value = run(
        &book,
        "category_totals",
        json!({ "from": "2026-10-01", "to": "2026-10-31" }),
    );
    let totals = &value["currencies"][0];
    let row = |side: &str, id: AccountId| -> Value {
        totals[side]
            .as_array()
            .expect("a list")
            .iter()
            .find(|row| row["category_id"] == id.to_string())
            .cloned()
            .unwrap_or_else(|| panic!("no row {id}"))
    };
    assert_eq!(
        row("spending", ids.groceries)["parent_id"],
        json!(ids.food.to_string())
    );
    // A top-level category has no parent_id.
    assert!(row("spending", ids.food).get("parent_id").is_none());
    // The income category of the same name sits in its own parent.
    let refund = row("income", ids.groceries_refund);
    assert!(refund["parent_id"].is_string());
    assert_ne!(refund["parent_id"], json!(ids.food.to_string()));
    // Money not sorted yet has no parent either.
    let unsorted = totals["spending"]
        .as_array()
        .expect("a list")
        .iter()
        .find(|row| row["unsorted"] == true)
        .expect("unsorted money");
    assert!(unsorted.get("parent_id").is_none());
}

#[test]
fn every_name_in_a_search_row_comes_with_its_id() {
    let (book, ids) = book();
    let value = run(&book, "search_transactions", json!({ "limit": 50 }));
    let groceries = row_of_day(&value, "2026-10-01");
    assert_eq!(groceries["account_id"], json!(ids.checking.to_string()));
    assert_eq!(groceries["category"], json!("Food › Groceries"));
    assert_eq!(groceries["category_id"], json!(ids.groceries.to_string()));
    // The refund has the same name but the other category's ID.
    let refund = row_of_day(&value, "2026-10-04");
    assert_eq!(refund["category"], json!("Food › Groceries"));
    assert_eq!(
        refund["category_id"],
        json!(ids.groceries_refund.to_string())
    );
    // A transfer names the other account and its ID, on both sides.
    let out = row_of_day(&value, "2026-10-06");
    assert_eq!(out["transfer"], json!("Rainy Day Savings"));
    assert_eq!(out["transfer_account_id"], json!(ids.savings.to_string()));
    let into = rows(&value)
        .iter()
        .find(|row| row["account"] == "Rainy Day Savings")
        .expect("the savings side");
    assert_eq!(into["account_id"], json!(ids.savings.to_string()));
    assert_eq!(into["transfer_account_id"], json!(ids.checking.to_string()));
    // Every ID is one list_accounts or list_categories gives with that name.
    let accounts = run(&book, "list_accounts", Value::Null);
    let categories = run(&book, "list_categories", Value::Null);
    for row in rows(&value) {
        let listed = |list: &Value, key: &str, id: &Value| -> Value {
            list[key]
                .as_array()
                .expect("a list")
                .iter()
                .find(|item| item["id"] == *id)
                .map(|item| item["name"].clone())
                .unwrap_or_else(|| panic!("{id} isn't listed"))
        };
        assert_eq!(
            listed(&accounts, "accounts", &row["account_id"]),
            row["account"]
        );
        if let Some(id) = row.get("category_id") {
            assert_eq!(listed(&categories, "categories", id), row["category"]);
        }
        if let Some(id) = row.get("transfer_account_id") {
            assert_eq!(listed(&accounts, "accounts", id), row["transfer"]);
        }
    }
}

#[test]
fn a_text_the_other_text_holds_is_sent_once() {
    let (book, _) = book();
    let value = run(&book, "search_transactions", json!({ "limit": 50 }));
    // The memo is in the bank text (whitespace aside): only the bank text,
    // which Rules test.
    let first = row_of_day(&value, "2026-10-01");
    assert!(first.get("note").is_none(), "{first}");
    assert_eq!(
        first["bank_text"],
        json!("CARD PAYMENT  Corner Grocer   North 0815")
    );
    // The bank text is in the memo: only the memo.
    let second = row_of_day(&value, "2026-10-02");
    assert_eq!(second["note"], json!("Weekly shop at Corner Grocer South"));
    assert!(second.get("bank_text").is_none(), "{second}");
    // Different texts: both.
    let third = row_of_day(&value, "2026-10-03");
    assert_eq!(third["note"], json!("Party snacks"));
    assert_eq!(third["bank_text"], json!("CARD PAYMENT 0816"));
    // The search still finds a row by the text left out.
    let found = run(
        &book,
        "search_transactions",
        json!({ "text": "weekly shop" }),
    );
    assert_eq!(found["matches"], json!(1));
}

#[test]
fn pages_of_a_search_follow_on_without_gaps() {
    let (book, _) = book();
    let all = run(&book, "search_transactions", json!({ "limit": 50 }));
    assert_eq!(all["matches"], json!(7));
    assert!(all.get("next_offset").is_none());
    assert!(all.get("rows_left_out").is_none());
    let mut paged = Vec::new();
    let mut offset = 0;
    loop {
        let page = run(
            &book,
            "search_transactions",
            json!({ "limit": 3, "offset": offset }),
        );
        assert_eq!(page["matches"], json!(7));
        paged.extend(rows(&page).iter().cloned());
        match page.get("next_offset") {
            Some(next) => {
                offset = next.as_u64().expect("a number");
                assert_eq!(
                    page["rows_left_out"].as_u64().expect("a number"),
                    7 - offset
                );
            }
            None => {
                assert!(page.get("rows_left_out").is_none());
                break;
            }
        }
    }
    assert_eq!(&paged, rows(&all));
    // Past the last match: no rows, and nothing left out.
    let past = run(&book, "search_transactions", json!({ "offset": 7 }));
    assert!(rows(&past).is_empty());
    assert_eq!(past["offset"], json!(7));
    assert!(past.get("next_offset").is_none());
}

#[test]
fn payees_can_be_grouped_by_the_banks_other_party_name() {
    let (book, _) = book();
    let period = json!({ "from": "2026-10-01", "to": "2026-10-31" });
    let paid = |value: &Value| -> Vec<(String, String)> {
        value["currencies"][0]["paid"]
            .as_array()
            .expect("a list")
            .iter()
            .map(|row| {
                let name = row
                    .get("payee")
                    .or_else(|| row.get("other_party_name"))
                    .and_then(Value::as_str)
                    .unwrap_or("-")
                    .to_owned();
                (name, row["amount"].as_str().expect("an amount").to_owned())
            })
            .collect()
    };
    // By the payee shown, the Rule's general name joins both shops; the
    // refund has no payee, so it falls back to the bank's name.
    let by_payee = run(&book, "payee_totals", period.clone());
    assert_eq!(by_payee["group_by"], json!("payee"));
    assert_eq!(
        paid(&by_payee),
        [
            ("Supermarket".to_owned(), "65.00".to_owned()),
            ("Farmers market".to_owned(), "7.00".to_owned()),
        ]
    );
    let mut arguments = period;
    arguments["group_by"] = json!("other_party_name");
    let by_bank = run(&book, "payee_totals", arguments);
    assert_eq!(by_bank["group_by"], json!("other_party_name"));
    // North: 20.00 + 15.00 - 5.00 refund, a tie with South keeps the
    // order of the names; an entry without a bank name falls back to its
    // payee.
    assert_eq!(
        paid(&by_bank),
        [
            ("Corner Grocer North".to_owned(), "30.00".to_owned()),
            ("Corner Grocer South".to_owned(), "30.00".to_owned()),
            ("Farmers market".to_owned(), "7.00".to_owned()),
        ]
    );
    let rows = by_bank["currencies"][0]["paid"].as_array().expect("a list");
    assert!(rows.iter().all(|row| row.get("payee").is_none()));
}

#[test]
fn a_rule_names_what_it_books_to_with_its_id() {
    let (book, ids) = book();
    let value = run(&book, "list_rules", Value::Null);
    let rules = value["rules"].as_array().expect("rules");
    assert_eq!(rules[0]["category"], json!("Food › Groceries"));
    assert_eq!(rules[0]["category_id"], json!(ids.groceries.to_string()));
    assert_eq!(
        rules[0]["conditions"][0],
        json!({ "account": "Everyday Checking", "account_id": ids.checking.to_string() })
    );
    assert_eq!(rules[1]["transfer"], json!("Rainy Day Savings"));
    assert_eq!(
        rules[1]["transfer_account_id"],
        json!(ids.savings.to_string())
    );
}

#[test]
fn a_period_that_ends_before_it_starts_names_its_arguments_only() {
    let (book, _) = book();
    for name in ["category_totals", "payee_totals", "search_transactions"] {
        let error = call(
            &book,
            name,
            &json!({ "from": "2026-10-31", "to": "2026-10-01" }),
            TODAY,
        )
        .expect_err(name);
        assert!(matches!(error, ToolError::BadPeriod), "{name}");
        let report = error.report();
        assert_eq!(report.code, "ai-tools.bad-period");
        assert_eq!(
            report.params,
            vec![
                ("argument", ErrorParam::Keyword("to")),
                ("not_before", ErrorParam::Keyword("from")),
            ],
            "{name}"
        );
    }
}
