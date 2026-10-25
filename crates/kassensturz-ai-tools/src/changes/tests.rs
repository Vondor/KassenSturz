// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use kassensturz_core::{AccountType, Date};
use serde_json::json;

use super::*;
use crate::FUNCTIONS;

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};

const USD: CurrencyCode = match CurrencyCode::new("USD") {
    Ok(code) => code,
    Err(_) => panic!("USD is a valid code"),
};

fn eur(cents: i64) -> Money {
    Money::from_minor_units(cents, EUR)
}

/// A made-up book: a checking and a savings account, the categories Food ›
/// Groceries, Salary and Travel (in dollars), and a purchase at the
/// grocer's, one without a category, a transfer and a split purchase.
struct Sample {
    book: Book,
    checking: AccountId,
    food: AccountId,
    groceries: AccountId,
    salary: AccountId,
    travel: AccountId,
    bought: TransactionId,
    unsorted: TransactionId,
    transfer: TransactionId,
    split: TransactionId,
}

fn account(name: &str, kind: AccountKind, currency: CurrencyCode) -> Account {
    Account {
        id: AccountId::generate(),
        name: name.into(),
        kind,
        currency,
        account_type: kind.is_account().then_some(AccountType::Checking),
        parent: None,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    }
}

fn add(book: &mut Book, account: Account) -> AccountId {
    book.use_currency(account.currency).expect("currency");
    book.add_account(&account).expect("added");
    account.id
}

fn bought(book: &mut Book, payee: &str, splits: Vec<Split>) -> TransactionId {
    let transaction = Transaction::new(
        TransactionId::generate(),
        Date::constant(2026, 10, 5),
        Some(payee.into()),
        splits,
    )
    .expect("balances");
    book.add_transaction(&transaction).expect("added");
    transaction.id()
}

fn sample() -> Sample {
    let mut book = Book::create_in_memory().expect("book");
    let checking = add(
        &mut book,
        account("Checking account", AccountKind::Asset, EUR),
    );
    let savings = add(&mut book, account("Savings", AccountKind::Asset, EUR));
    let food = add(&mut book, account("Food", AccountKind::Expense, EUR));
    let groceries = add(
        &mut book,
        Account {
            parent: Some(food),
            ..account("Groceries", AccountKind::Expense, EUR)
        },
    );
    let salary = add(&mut book, account("Salary", AccountKind::Income, EUR));
    let travel = add(&mut book, account("Travel", AccountKind::Expense, USD));
    let bought_at_grocer = bought(
        &mut book,
        "Corner Grocery",
        vec![
            Split::new(checking, eur(-5320)),
            Split::new(groceries, eur(5320)),
        ],
    );
    let unsorted = bought(
        &mut book,
        "Pet Shop",
        vec![
            Split::new(checking, eur(-1999)),
            Split::uncategorized(eur(1999)),
        ],
    );
    let transfer = bought(
        &mut book,
        "Savings",
        vec![
            Split::new(checking, eur(-10000)),
            Split::new(savings, eur(10000)),
        ],
    );
    let split = bought(
        &mut book,
        "Market",
        vec![
            Split::new(checking, eur(-3000)),
            Split::new(groceries, eur(2000)),
            Split::new(food, eur(1000)),
        ],
    );
    Sample {
        book,
        checking,
        food,
        groceries,
        salary,
        travel,
        bought: bought_at_grocer,
        unsorted,
        transfer,
        split,
    }
}

fn code(result: Result<Proposal, ToolError>) -> String {
    match result {
        Ok(_) => "prepared".into(),
        Err(ToolError::BadArgument { argument }) => format!("bad-argument {argument}"),
        Err(error) => error.report().code.into(),
    }
}

/// Everything in the book a change set could touch, to see that nothing
/// changed.
fn state(book: &Book) -> (Vec<Account>, Vec<Transaction>, usize, usize) {
    (
        book.all_accounts().expect("accounts"),
        book.transactions().expect("transactions"),
        book.rules().expect("rules").len(),
        book.history().expect("history").len(),
    )
}

#[test]
fn a_proposal_changes_nothing_and_applying_it_is_one_change_that_undo_takes_back() {
    let Sample {
        mut book,
        food,
        unsorted,
        bought,
        ..
    } = sample();
    let before = state(&book);
    let proposal = propose(
        &book,
        &json!({
            "new_categories": [{"key": "pets", "name": " Pets ", "parent_id": food.to_string()}],
            "new_rules": [{
                "name": "Pet shop",
                "conditions": [
                    {"field": "payee", "test": "contains", "text": "pet"},
                    {"money_out": true},
                ],
                "new_category": "pets",
            }],
            "categorize": [{"transaction_id": unsorted.to_string(), "new_category": "pets"}],
        }),
    )
    .expect("prepared");
    assert_eq!(state(&book), before, "proposing changes nothing");
    assert_eq!(proposal.output.value["prepared"], 3);
    assert_eq!(
        proposal.output.value["note"],
        crate::DATA_NOTE,
        "the answer says what all results say"
    );

    let changes = proposal.changes.changes();
    let ProposedChange::NewCategory(pets) = &changes[0] else {
        panic!("a new category first: {changes:?}");
    };
    assert_eq!(pets.path, ["Food", "Pets"]);
    assert_eq!((pets.income, pets.currency), (false, EUR));
    let ProposedChange::NewRule(rule) = &changes[1] else {
        panic!("then the Rule");
    };
    assert_eq!(rule.category, ["Food", "Pets"]);
    assert_eq!(rule.rule.actions.book_to, Some(BookTo::Category(pets.id)));
    let ProposedChange::Categorize(moved) = &changes[2] else {
        panic!("then the transaction");
    };
    assert_eq!(moved.account, "Checking account");
    assert_eq!(moved.amount, eur(-1999));
    assert_eq!(moved.payee.as_deref(), Some("Pet Shop"));
    assert_eq!(moved.before, None);
    assert_eq!(moved.after, ["Food", "Pets"]);
    let pets_id = pets.id;

    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    let history = book.history().expect("history");
    assert_eq!(history.len(), before.3 + 1, "one change in the history");
    assert_eq!(history[0].action, Action::ApplyAiChanges, "newest first");
    assert_eq!(book.account(pets_id).expect("added").name, "Pets");
    let rules = book.rules().expect("rules");
    assert_eq!(rules.last().expect("a Rule").rule.name, "Pet shop");
    let parts = book.transaction(unsorted).expect("there").splits().to_vec();
    assert_eq!(parts[1].account, Some(pets_id));
    assert_eq!(parts[1].amount, eur(1999));
    assert_eq!(
        book.transaction(bought).expect("there").splits()[1].amount,
        eur(5320),
        "the others stay"
    );

    book.undo().expect("undone");
    assert_eq!(state(&book).0, before.0);
    assert_eq!(state(&book).1, before.1);
    assert_eq!(state(&book).2, before.2);
    book.redo().expect("made again");
    assert_eq!(
        book.transaction(unsorted).expect("there").splits()[1].account,
        Some(pets_id)
    );
}

#[test]
fn a_transaction_can_move_from_one_category_to_another() {
    let Sample {
        mut book,
        bought,
        food,
        groceries,
        ..
    } = sample();
    let proposal = propose(
        &book,
        &json!({"categorize": [{"transaction_id": bought.to_string(), "category_id": food.to_string()}]}),
    )
    .expect("prepared");
    let ProposedChange::Categorize(moved) = &proposal.changes.changes()[0] else {
        panic!("a move");
    };
    assert_eq!(
        moved.before.as_deref(),
        Some(&["Food".to_owned(), "Groceries".to_owned()][..])
    );
    assert_eq!(moved.after, ["Food"]);
    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    let parts = book.transaction(bought).expect("there").splits().to_vec();
    assert_eq!(parts[1].account, Some(food));
    assert_ne!(parts[1].account, Some(groceries));
}

#[test]
fn the_book_changed_since_the_review_so_nothing_is_applied() {
    let Sample {
        mut book,
        checking,
        groceries,
        unsorted,
        ..
    } = sample();
    let proposal = propose(
        &book,
        &json!({
            "new_categories": [{"key": "pets", "name": "Pets"}],
            "categorize": [{"transaction_id": unsorted.to_string(), "new_category": "pets"}],
        }),
    )
    .expect("prepared");
    // The user books it themselves before confirming.
    let edited = Transaction::new(
        unsorted,
        Date::constant(2026, 10, 5),
        Some("Pet Shop".into()),
        vec![
            Split::new(checking, eur(-1999)),
            Split::new(groceries, eur(1999)),
        ],
    )
    .expect("balances");
    book.update_transaction(&edited).expect("edited");
    let before = state(&book);
    let error = proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect_err("out of date");
    assert!(matches!(error, ChangeError::OutOfDate), "{error:?}");
    assert_eq!(error.report().code, "ai-changes.out-of-date");
    assert_eq!(state(&book), before, "not even the new category is added");

    // A transaction removed since is out of date too.
    let proposal = propose(
        &book,
        &json!({"categorize": [{"transaction_id": unsorted.to_string(), "category_id": book_food(&book)}]}),
    )
    .expect("prepared");
    book.remove_transaction(unsorted).expect("removed");
    assert!(matches!(
        proposal.changes.apply(&mut book, Confirmed::by_the_user()),
        Err(ChangeError::OutOfDate)
    ));
}

fn book_food(book: &Book) -> String {
    book.all_accounts()
        .expect("accounts")
        .into_iter()
        .find(|account| account.name == "Food")
        .expect("Food")
        .id
        .to_string()
}

#[test]
fn a_change_the_book_refuses_takes_back_the_whole_set() {
    let Sample {
        mut book, unsorted, ..
    } = sample();
    let proposal = propose(
        &book,
        &json!({
            "new_rules": [{"name": "Pets", "conditions": [{"money_out": true}], "category_id": book_food(&book)}],
            "new_categories": [{"key": "pets", "name": "Pets"}],
            "categorize": [{"transaction_id": unsorted.to_string(), "new_category": "pets"}],
        }),
    )
    .expect("prepared");
    // The user adds a category of the same name in the meantime.
    add(&mut book, account("pets", AccountKind::Expense, EUR));
    let before = state(&book);
    let error = proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect_err("refused");
    assert_eq!(error.report().code, "ai-changes.not-applied");
    assert!(matches!(
        error,
        ChangeError::NotApplied(StoreError::DuplicateAccountName { .. })
    ));
    assert_eq!(state(&book), before);
}

#[test]
fn only_simple_transactions_get_a_category() {
    let s = sample();
    let categorize = |id: TransactionId, category: AccountId| {
        code(propose(
            &s.book,
            &json!({"categorize": [{"transaction_id": id.to_string(), "category_id": category.to_string()}]}),
        ))
    };
    assert_eq!(categorize(s.unsorted, s.food), "prepared");
    assert_eq!(categorize(s.unsorted, s.salary), "prepared");
    assert_eq!(categorize(s.transfer, s.food), "ai-tools.not-categorizable");
    assert_eq!(categorize(s.split, s.food), "ai-tools.not-categorizable");
    assert_eq!(categorize(s.unsorted, s.travel), "ai-tools.other-currency");
    assert_eq!(
        categorize(s.unsorted, s.checking),
        "ai-tools.unknown-category"
    );
    assert_eq!(
        categorize(s.bought, s.groceries),
        "bad-argument category_id",
        "already there"
    );
    assert_eq!(
        categorize(TransactionId::generate(), s.food),
        "ai-tools.unknown-transaction"
    );
    let twice = json!({"categorize": [
        {"transaction_id": s.unsorted.to_string(), "category_id": s.food.to_string()},
        {"transaction_id": s.unsorted.to_string(), "category_id": s.salary.to_string()},
    ]});
    assert_eq!(
        code(propose(&s.book, &twice)),
        "bad-argument transaction_id"
    );
}

#[test]
fn calls_that_are_not_what_the_function_takes_are_refused() {
    let s = sample();
    let food = s.food.to_string();
    let salary = s.salary.to_string();
    let cases = [
        (json!({}), "bad-argument arguments"),
        (json!(null), "bad-argument arguments"),
        (json!({"delete": []}), "bad-argument arguments"),
        (json!({"new_categories": {}}), "bad-argument new_categories"),
        (
            json!({"new_categories": [{"key": "a", "name": "A", "sql": "x"}]}),
            "bad-argument new_categories",
        ),
        (
            json!({"new_categories": [{"key": "a", "name": ""}]}),
            "bad-argument name",
        ),
        (
            json!({"new_categories": [{"key": "a", "name": "Pets\u{202E}"}]}),
            "bad-argument name",
        ),
        (
            json!({"new_categories": [{"key": "a", "name": "Line\nbreak"}]}),
            "bad-argument name",
        ),
        (
            json!({"new_categories": [{"key": "a", "name": "x".repeat(101)}]}),
            "bad-argument name",
        ),
        (
            json!({"new_categories": [{"key": "a", "name": "A"}, {"key": "a", "name": "B"}]}),
            "bad-argument key",
        ),
        (
            json!({"new_categories": [{"key": "a", "name": "groceries", "parent_id": food}]}),
            "ai-tools.name-taken",
        ),
        (
            json!({"new_categories": [{"key": "a", "name": "A"}, {"key": "b", "name": "a"}]}),
            "ai-tools.name-taken",
        ),
        (
            json!({"new_categories": [{"key": "a", "name": "Bonus", "parent_id": salary, "income": false}]}),
            "bad-argument income",
        ),
        (
            json!({"new_categories": [{"key": "a", "name": "A", "currency": "DEM"}]}),
            "bad-argument currency",
        ),
        (
            json!({"new_categories": [{"key": "a", "name": "A", "parent_id": food, "currency": "USD"}]}),
            "bad-argument currency",
        ),
        (
            json!({"new_rules": [{"name": "A", "conditions": [], "category_id": food}]}),
            "bad-argument conditions",
        ),
        (
            json!({"new_rules": [{"name": "A", "conditions": [{"field": "other_party_iban", "test": "contains", "text": "DE"}], "category_id": food}]}),
            "bad-argument field",
        ),
        (
            json!({"new_rules": [{"name": "A", "conditions": [{"field": "payee", "test": "matches_pattern", "text": ".*"}], "category_id": food}]}),
            "bad-argument test",
        ),
        (
            json!({"new_rules": [{"name": "A", "conditions": [{"money_in": true, "money_out": true}], "category_id": food}]}),
            "bad-argument conditions",
        ),
        (
            json!({"new_rules": [{"name": "A", "conditions": [{"money_in": false}], "category_id": food}]}),
            "bad-argument conditions",
        ),
        (
            json!({"new_rules": [{"name": "A", "conditions": [{"money_out": true}]}]}),
            "bad-argument category_id",
        ),
        (
            json!({"new_rules": [{"name": "A", "conditions": [{"money_out": true}], "new_category": "nope"}]}),
            "bad-argument new_category",
        ),
        (
            json!({"new_rules": [
                {"name": "A", "conditions": [{"money_out": true}], "category_id": food},
                {"name": "a", "conditions": [{"money_in": true}], "category_id": food},
            ]}),
            "ai-tools.name-taken",
        ),
        (
            json!({"categorize": [{"transaction_id": "not an ID", "category_id": food}]}),
            "bad-argument transaction_id",
        ),
    ];
    for (call, expected) in cases {
        assert_eq!(code(propose(&s.book, &call)), expected, "{call}");
    }

    let many: Vec<Value> = (0..=MAX_CHANGES)
        .map(|n| json!({"key": format!("k{n}"), "name": format!("Category {n}")}))
        .collect();
    assert_eq!(
        code(propose(&s.book, &json!({"new_categories": many}))),
        "ai-tools.too-many-changes"
    );
}

#[test]
fn a_rule_with_the_name_of_one_in_the_book_is_refused() {
    let mut s = sample();
    let first = propose(
        &s.book,
        &json!({"new_rules": [{"name": "Groceries", "conditions": [{"field": "payee", "test": "contains", "text": "grocery"}], "category_id": s.groceries.to_string()}]}),
    )
    .expect("prepared");
    first
        .changes
        .apply(&mut s.book, Confirmed::by_the_user())
        .expect("applied");
    let again = json!({"new_rules": [{"name": " groceries", "conditions": [{"money_out": true}], "category_id": s.food.to_string()}]});
    assert_eq!(code(propose(&s.book, &again)), "ai-tools.name-taken");
}

#[test]
fn a_new_category_takes_the_usual_currency_or_has_to_name_one() {
    let mut s = sample();
    let call = json!({"new_categories": [{"key": "a", "name": "Gifts", "income": true}]});
    let proposal = propose(&s.book, &call).expect("prepared");
    let ProposedChange::NewCategory(gifts) = &proposal.changes.changes()[0] else {
        panic!("a category");
    };
    assert_eq!(
        (gifts.income, gifts.currency, gifts.parent),
        (true, EUR, None)
    );

    // Accounts in two currencies: the model has to say which.
    add(
        &mut s.book,
        account("Dollar account", AccountKind::Asset, USD),
    );
    assert_eq!(code(propose(&s.book, &call)), "bad-argument currency");
    let named = json!({"new_categories": [{"key": "a", "name": "Gifts", "currency": "USD"}]});
    assert_eq!(code(propose(&s.book, &named)), "prepared");
}

#[test]
fn the_function_is_offered_apart_from_the_read_functions() {
    assert!(
        FUNCTIONS
            .iter()
            .all(|function| function.name != PROPOSE_CHANGES.name),
        "the read functions stay read-only, for the MCP server too"
    );
    let schema: Value = serde_json::from_str(PROPOSE_CHANGES.parameters).expect("JSON");
    // Every name at any depth is a keyword of `ai-tools.bad-argument`.
    fn names(value: &Value, found: &mut Vec<String>) {
        if let Some(properties) = value.get("properties").and_then(Value::as_object) {
            for (name, inner) in properties {
                found.push(name.clone());
                names(inner, found);
            }
        }
        if let Some(items) = value.get("items") {
            names(items, found);
        }
        if value.get("type").and_then(Value::as_str) == Some("object") {
            assert_eq!(
                value["additionalProperties"], false,
                "every object refuses other names: {value}"
            );
        }
    }
    let mut found = Vec::new();
    names(&schema, &mut found);
    for name in &found {
        assert!(ARGUMENTS.contains(&name.as_str()), "{name}");
    }
    for name in ARGUMENTS {
        assert!(found.iter().any(|found| found == name), "{name} is unused");
    }
    // Named in the threat model, like every function (T-23).
    let threats = include_str!("../../../../docs/THREAT_MODEL.md");
    let row = threats
        .lines()
        .find(|line| line.starts_with("| T-23 |"))
        .expect("T-23");
    assert!(row.contains("`propose_changes`"));
}
