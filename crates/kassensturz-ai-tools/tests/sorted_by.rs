// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `search_transactions` says who sorted an entry (US2.10, #569, D-672): a
//! Rule, by its ID, while the store still marks the part the Rule booked,
//! or the user by hand. The marks come from a real run of the Rules
//! ([`Book::run_rules`]), not from values built here. The book is
//! synthetic.

#![cfg(feature = "ai")]

use kassensturz_ai_tools::{call, call_allowed};
use kassensturz_core::ai_switch::{Permission, Permissions};
use kassensturz_core::{
    Account, AccountKind, Actions, BookTo, Condition, CurrencyCode, Date, MatchMode, Money, Rule,
    RuleId, Split, TextField, TextOperator, Transaction, TransactionId,
};
use kassensturz_store::Book;
use serde_json::{Value, json};

const TODAY: Date = Date::constant(2026, 10, 25);

fn eur(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, CurrencyCode::new("EUR").expect("EUR"))
}

fn account(name: &str, kind: AccountKind) -> Account {
    Account {
        id: kassensturz_core::AccountId::generate(),
        name: name.to_owned(),
        kind,
        currency: CurrencyCode::new("EUR").expect("EUR"),
        account_type: None,
        parent: None,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    }
}

/// A book whose Rules sorted one entry, the user sorted another by hand,
/// and one is still unsorted; and the Rule's ID.
fn book() -> (Book, RuleId) {
    let mut book = Book::create_in_memory().expect("a book in memory");
    book.use_currency(CurrencyCode::new("EUR").expect("EUR"))
        .expect("euros");
    let checking = account("Everyday Checking", AccountKind::Asset);
    let groceries = account("Groceries", AccountKind::Expense);
    let bakery = account("Bakery", AccountKind::Expense);
    for a in [&checking, &groceries, &bakery] {
        book.add_account(a).expect("an account");
    }
    let rule = Rule {
        id: RuleId::generate(),
        name: "Grocer".to_owned(),
        match_mode: MatchMode::All,
        conditions: vec![Condition::Text {
            field: TextField::Payee,
            operator: TextOperator::Contains,
            value: "grocer".to_owned(),
            case_sensitive: false,
        }],
        actions: Actions {
            book_to: Some(BookTo::Category(groceries.id)),
            payee: None,
        },
    };
    book.add_rule(&rule, None).expect("a Rule");
    let entries = [
        (1, "Corner Grocer", None),
        (2, "Village Bakery", Some(bakery.id)),
        (3, "Somewhere else", None),
    ];
    for (day, payee, category) in entries {
        let other = match category {
            Some(category) => Split::new(category, eur(1_000)),
            None => Split::uncategorized(eur(1_000)),
        };
        let transaction = Transaction::new(
            TransactionId::generate(),
            Date::constant(2026, 10, day),
            Some(payee.to_owned()),
            vec![Split::new(checking.id, eur(-1_000)), other],
        )
        .expect("a transaction");
        book.add_transaction(&transaction).expect("added");
    }
    let run = book.run_rules().expect("the Rules run");
    assert_eq!(run.changed.len(), 1, "the Rule sorted the grocer");
    (book, rule.id)
}

fn row<'a>(value: &'a Value, payee: &str) -> &'a Value {
    value["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|row| row["payee"] == payee)
        .unwrap_or_else(|| panic!("no row {payee}"))
}

#[test]
fn a_row_says_whether_a_rule_or_the_user_sorted_it() {
    let (book, rule) = book();
    let value = call(&book, "search_transactions", &json!({}), TODAY)
        .expect("a search")
        .value;
    let by_rule = row(&value, "Corner Grocer");
    assert_eq!(by_rule["category"], json!("Groceries"));
    assert_eq!(by_rule["sorted_by"], json!("rule"));
    assert_eq!(by_rule["rule_id"], json!(rule.to_string()));
    // The ID is the one list_rules gives.
    let rules = call(&book, "list_rules", &Value::Null, TODAY)
        .expect("the Rules")
        .value;
    assert_eq!(rules["rules"][0]["id"], by_rule["rule_id"]);

    let by_hand = row(&value, "Village Bakery");
    assert_eq!(by_hand["sorted_by"], json!("by_hand"));
    assert!(by_hand.get("rule_id").is_none());

    // Nothing sorted it yet: no sorted_by.
    let unsorted = row(&value, "Somewhere else");
    assert_eq!(unsorted["unsorted"], json!(true));
    assert!(unsorted.get("sorted_by").is_none());
}

#[test]
fn a_rule_removed_since_is_still_a_rule_without_an_id() {
    let (mut book, rule) = book();
    book.remove_rule(rule).expect("removed");
    let value = call(&book, "search_transactions", &json!({}), TODAY)
        .expect("a search")
        .value;
    let by_rule = row(&value, "Corner Grocer");
    assert_eq!(by_rule["sorted_by"], json!("rule"));
    assert!(by_rule.get("rule_id").is_none());
}

#[test]
fn who_sorted_an_entry_needs_the_reading_of_rules() {
    let (book, _) = book();
    let transactions = Permissions::NONE.with(Permission::TransactionsRead, true);
    let value = call_allowed(
        &book,
        "search_transactions",
        &json!({}),
        TODAY,
        transactions,
    )
    .expect("a search")
    .value;
    for row in value["rows"].as_array().expect("rows") {
        assert!(row.get("sorted_by").is_none(), "{row}");
        assert!(row.get("rule_id").is_none(), "{row}");
    }
    let with_rules = transactions.with(Permission::RulesRead, true);
    let value = call_allowed(&book, "search_transactions", &json!({}), TODAY, with_rules)
        .expect("a search")
        .value;
    assert_eq!(row(&value, "Corner Grocer")["sorted_by"], json!("rule"));
}
