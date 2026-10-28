// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use kassensturz_core::ai_switch::Field;
use kassensturz_core::{AccountType, Date, ErrorParam};
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

/// Prepares changes with every kind of change allowed; the permissions
/// have their own tests below.
fn propose(book: &Book, arguments: &Value) -> Result<Proposal, ToolError> {
    super::propose(book, arguments, Permissions::all())
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
            json!({"new_categories": [{"key": "a", "name": "Games", "parent_id": salary}]}),
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

/// A book with three top-level Rules in a known order, a Rule with a
/// sub-Rule and one that books a Transfer.
fn with_rules() -> (Sample, Vec<Rule>) {
    let mut sample = sample();
    let mut rules = Vec::new();
    for (name, text) in [("Grocer", "grocer"), ("Pets", "pet"), ("Market", "market")] {
        let rule = Rule {
            id: RuleId::generate(),
            name: name.into(),
            match_mode: MatchMode::All,
            conditions: vec![Condition::Text {
                field: TextField::Payee,
                operator: TextOperator::Contains,
                value: text.into(),
                case_sensitive: false,
            }],
            actions: Actions {
                book_to: Some(BookTo::Category(sample.groceries)),
                payee: None,
            },
        };
        sample.book.add_rule(&rule, None).expect("Rule");
        rules.push(rule);
    }
    (sample, rules)
}

fn names(book: &Book) -> Vec<String> {
    book.rules()
        .expect("rules")
        .into_iter()
        .map(|node| node.rule.name)
        .collect()
}

#[test]
fn categories_are_renamed_and_moved_with_what_is_inside_them() {
    let Sample {
        mut book,
        food,
        groceries,
        salary,
        travel,
        ..
    } = sample();
    let before = state(&book);
    let proposal = propose(
        &book,
        &json!({
            "new_categories": [{"key": "survival", "name": "Survival"}],
            "edit_categories": [
                {"category_id": food.to_string(), "name": "Essentials", "new_parent": "survival"},
                {"category_id": groceries.to_string(), "name": "Supermarket"},
            ],
        }),
    )
    .expect("prepared");
    assert_eq!(state(&book), before, "proposing changes nothing");
    let changes = proposal.changes.changes();
    let ProposedChange::NewCategory(survival) = &changes[0] else {
        panic!("the new category first");
    };
    assert_eq!(survival.path, ["Survival"]);
    let survival_id = survival.id;
    let ProposedChange::EditCategory(edit) = &changes[1] else {
        panic!("then the move");
    };
    assert_eq!(edit.before, ["Food"]);
    assert_eq!(edit.after, ["Survival", "Essentials"]);
    let ProposedChange::EditCategory(edit) = &changes[2] else {
        panic!("then the rename");
    };
    assert_eq!(edit.before, ["Food", "Groceries"]);
    assert_eq!(
        edit.after,
        ["Survival", "Essentials", "Supermarket"],
        "the path after holds the earlier move"
    );
    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    assert_eq!(book.account(food).expect("there").name, "Essentials");
    assert_eq!(book.account(food).expect("there").parent, Some(survival_id));
    assert_eq!(book.account(groceries).expect("there").name, "Supermarket");
    assert_eq!(
        book.history().expect("history")[0].action,
        Action::ApplyAiChanges
    );
    book.undo().expect("one undo takes all back");
    assert_eq!(state(&book).0, before.0);

    // Refused: into itself or below itself, another kind or currency,
    // a name that is taken, nothing to change, twice, a category made by
    // the same call, and an unknown one.
    for (call, expected) in [
        (
            json!({"edit_categories": [{"category_id": food.to_string(), "parent_id": groceries.to_string()}]}),
            "bad-argument parent_id",
        ),
        (
            json!({"edit_categories": [{"category_id": groceries.to_string(), "parent_id": salary.to_string()}]}),
            "bad-argument parent_id",
        ),
        (
            json!({"edit_categories": [{"category_id": groceries.to_string(), "parent_id": travel.to_string()}]}),
            "bad-argument parent_id",
        ),
        (
            json!({"edit_categories": [{"category_id": groceries.to_string(), "name": " food ", "to_top": true}]}),
            "ai-tools.name-taken",
        ),
        (
            json!({"edit_categories": [{"category_id": food.to_string()}]}),
            "bad-argument category_id",
        ),
        (
            json!({"edit_categories": [{"category_id": food.to_string(), "name": "Food"}]}),
            "bad-argument category_id",
        ),
        (
            json!({"edit_categories": [
                {"category_id": food.to_string(), "name": "A"},
                {"category_id": food.to_string(), "name": "B"},
            ]}),
            "bad-argument category_id",
        ),
        (
            json!({"edit_categories": [{"category_id": food.to_string(), "to_top": true, "parent_id": salary.to_string()}]}),
            "bad-argument parent_id",
        ),
        (
            json!({"edit_categories": [{"category_id": AccountId::generate().to_string(), "name": "X"}]}),
            "ai-tools.unknown-category",
        ),
    ] {
        assert_eq!(code(propose(&book, &call)), expected, "{call}");
    }
}

#[test]
fn the_summary_names_each_change_in_plain_words() {
    let (Sample { book, food, .. }, rules) = with_rules();
    let proposal = propose(
        &book,
        &json!({
            "new_categories": [{"key": "p", "name": "Pets", "parent_id": food.to_string()}],
            "edit_rules": [{"rule_id": rules[0].id.to_string(), "name": "Supermarket"}],
            "delete_rules": [{"rule_id": rules[1].id.to_string()}],
            "move_rules": [{"rule_id": rules[2].id.to_string(), "position": 1}],
        }),
    )
    .expect("prepared");
    assert_eq!(
        proposal.changes.summary(),
        "new spending category \"Food › Pets\"; \
         Rule \"Grocer\" becomes \"Supermarket\" to Food › Groceries; \
         delete Rule \"Pets\"; \
         Rule \"Market\" moves from place 2 to 1"
    );
    assert!(!proposal.changes.summary().contains("ChangeSet"));
}

#[test]
fn rules_are_edited_moved_and_deleted_in_the_order_they_are_listed() {
    let (
        Sample {
            mut book,
            food,
            groceries,
            ..
        },
        rules,
    ) = with_rules();
    let [grocer, pets, market] = [rules[0].id, rules[1].id, rules[2].id];
    let before = state(&book);
    let proposal = propose(
        &book,
        &json!({
            "new_rules": [{
                "name": "Survival essentials",
                "conditions": [{"field": "bank_text", "test": "contains", "text": "water"}],
                "category_id": food.to_string(),
            }],
            "edit_rules": [{
                "rule_id": grocer.to_string(),
                "name": "Supermarket",
                "match": "any",
                "conditions": [{"field": "payee", "test": "contains", "text": "market"}],
                "category_id": food.to_string(),
            }],
            "delete_rules": [{"rule_id": pets.to_string()}],
            "move_rules": [{"rule_id": market.to_string(), "position": 1}],
        }),
    )
    .expect("prepared");
    assert_eq!(state(&book), before, "proposing changes nothing");
    let changes = proposal.changes.changes();
    let ProposedChange::EditRule(edit) = &changes[1] else {
        panic!("the edit after the new Rule: {changes:?}");
    };
    assert_eq!(edit.before.name, "Grocer");
    assert_eq!(
        edit.before_category.as_deref(),
        Some(&["Food".to_owned(), "Groceries".to_owned()][..])
    );
    assert_eq!(edit.category.as_deref(), Some(&["Food".to_owned()][..]));
    assert_eq!(edit.rule.match_mode, MatchMode::Any);
    let ProposedChange::MoveRule(moved) = &changes[3] else {
        panic!("the move last");
    };
    // After the delete, "Market" is the second of three: Grocer, Market,
    // Survival essentials.
    assert_eq!((moved.from, moved.to), (2, 1));

    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    assert_eq!(
        names(&book),
        ["Market", "Supermarket", "Survival essentials"]
    );
    let edited = book.rule(grocer).expect("kept its ID");
    assert_eq!(edited.actions.book_to, Some(BookTo::Category(food)));
    assert_eq!(edited.actions.payee, None);
    assert_eq!(
        book.history().expect("history")[0].action,
        Action::ApplyAiChanges
    );
    book.undo().expect("one undo takes all back");
    assert_eq!(names(&book), ["Grocer", "Pets", "Market"]);
    assert_eq!(
        book.rule(grocer).expect("back").actions.book_to,
        Some(BookTo::Category(groceries))
    );
}

#[test]
fn a_rule_change_is_refused_when_it_cannot_be_reviewed_or_made_right() {
    let (
        Sample {
            mut book,
            checking,
            food,
            ..
        },
        rules,
    ) = with_rules();
    let grocer = rules[0].id;
    // Rules a model can't write back in full, and one that books a Transfer.
    let iban = Rule {
        id: RuleId::generate(),
        name: "By IBAN".into(),
        match_mode: MatchMode::All,
        conditions: vec![Condition::Text {
            field: TextField::OtherPartyIban,
            operator: TextOperator::Equals,
            value: "DE89370400440532013000".into(),
            case_sensitive: false,
        }],
        actions: Actions {
            book_to: Some(BookTo::Transfer(checking)),
            payee: None,
        },
    };
    book.add_rule(&iban, None).expect("Rule");
    let parent = Rule {
        id: RuleId::generate(),
        name: "Parent".into(),
        ..rules[0].clone()
    };
    book.add_rule(&parent, None).expect("Rule");
    let child = Rule {
        id: RuleId::generate(),
        name: "Child".into(),
        ..rules[0].clone()
    };
    book.add_rule(&child, Some(parent.id)).expect("sub-Rule");
    for (call, expected) in [
        (
            json!({"edit_rules": [{"rule_id": iban.id.to_string(), "conditions": [{"money_in": true}]}]}),
            "ai-tools.not-changeable",
        ),
        (
            json!({"edit_rules": [{"rule_id": iban.id.to_string(), "category_id": food.to_string()}]}),
            "ai-tools.not-changeable",
        ),
        (
            json!({"delete_rules": [{"rule_id": parent.id.to_string()}]}),
            "ai-tools.not-changeable",
        ),
        (
            json!({"move_rules": [{"rule_id": child.id.to_string(), "position": 1}]}),
            "ai-tools.not-changeable",
        ),
        (
            json!({"edit_rules": [{"rule_id": RuleId::generate().to_string(), "name": "X"}]}),
            "ai-tools.unknown-rule",
        ),
        (
            json!({"edit_rules": [{"rule_id": grocer.to_string(), "name": "pets"}]}),
            "ai-tools.name-taken",
        ),
        (
            json!({"edit_rules": [{"rule_id": grocer.to_string()}]}),
            "bad-argument rule_id",
        ),
        (
            json!({"edit_rules": [{"rule_id": grocer.to_string(), "name": "A"}], "delete_rules": [{"rule_id": grocer.to_string()}]}),
            "bad-argument rule_id",
        ),
        (
            json!({"move_rules": [{"rule_id": grocer.to_string(), "position": 1}]}),
            "bad-argument position",
        ),
        (
            json!({"move_rules": [{"rule_id": grocer.to_string(), "position": 0}]}),
            "bad-argument position",
        ),
        (
            json!({"move_rules": [{"rule_id": grocer.to_string(), "position": 99}]}),
            "bad-argument position",
        ),
    ] {
        assert_eq!(code(propose(&book, &call)), expected, "{call}");
    }
    // A name change alone keeps the conditions a model can't see.
    let proposal = propose(
        &book,
        &json!({"edit_rules": [{"rule_id": iban.id.to_string(), "name": "By bank"}]}),
    )
    .expect("prepared");
    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    assert_eq!(
        book.rule(iban.id).expect("there").conditions,
        iban.conditions
    );
}

#[test]
fn rule_changes_are_out_of_date_when_the_user_changed_the_rules_since() {
    let (Sample { mut book, .. }, rules) = with_rules();
    let [grocer, pets, market] = [rules[0].id, rules[1].id, rules[2].id];
    for call in [
        json!({"edit_rules": [{"rule_id": grocer.to_string(), "name": "Supermarket"}]}),
        json!({"delete_rules": [{"rule_id": grocer.to_string()}]}),
        json!({"move_rules": [{"rule_id": market.to_string(), "position": 1}]}),
    ] {
        let proposal = propose(&book, &call).expect("prepared");
        // The user moves a Rule and renames another while the set waits.
        book.move_rule(pets, None, 0).expect("moved");
        let mut changed = book.rule(grocer).expect("there");
        changed.name = "Grocer, edited".into();
        book.update_rule(&changed).expect("edited");
        let result = proposal.changes.apply(&mut book, Confirmed::by_the_user());
        assert!(
            matches!(result, Err(ChangeError::OutOfDate)),
            "{call}: {result:?}"
        );
        // Back to how it was for the next call.
        book.move_rule(pets, None, 1).expect("moved back");
        changed.name = "Grocer".into();
        book.update_rule(&changed).expect("edited back");
    }
    assert_eq!(names(&book), ["Grocer", "Pets", "Market"]);
}

#[test]
fn a_list_the_user_does_not_allow_is_refused_and_one_they_allow_is_prepared() {
    let s = sample();
    let call = json!({
        "new_categories": [{"key": "pets", "name": "Pets"}],
        "delete_rules": [{"rule_id": "x"}],
    });
    let only_additions = additions();
    assert_eq!(
        super::propose(&s.book, &call, only_additions)
            .err()
            .map(|e| e.report().code),
        Some("ai-tools.not-allowed")
    );
    let not_new_categories = Permissions::all().with(Permission::NewCategories, false);
    let error = super::propose(
        &s.book,
        &json!({"new_categories": [{"key": "a", "name": "A"}]}),
        not_new_categories,
    )
    .expect_err("refused");
    let report = error.report();
    assert_eq!(report.code, "ai-tools.not-allowed");
    assert!(matches!(
        report.params.as_slice(),
        [("argument", ErrorParam::Keyword("new_categories"))]
    ));
    // Read only allows nothing; an empty list of a forbidden kind is fine.
    assert!(
        super::propose(
            &s.book,
            &json!({"new_categories": [{"key": "a", "name": "A"}]}),
            Permissions::NONE
        )
        .is_err()
    );
    assert!(
        super::propose(
            &s.book,
            &json!({"new_categories": [{"key": "a", "name": "A"}], "edit_rules": []}),
            only_additions
        )
        .is_ok()
    );
}

#[test]
fn each_list_needs_its_own_permission() {
    let s = sample();
    let calls = [
        (
            json!({"new_categories": [{"key": "a", "name": "A"}]}),
            Permission::NewCategories,
        ),
        (
            json!({"edit_categories": [{"category_id": s.groceries.to_string(), "name": "Food shop"}]}),
            Permission::EditCategories,
        ),
        (
            json!({"new_rules": [{"name": "R", "conditions": [{"money_out": true}], "category_id": s.groceries.to_string()}]}),
            Permission::NewRules,
        ),
        (
            json!({"categorize": [{"transaction_id": s.unsorted.to_string(), "category_id": s.groceries.to_string()}]}),
            Permission::SortTransactions,
        ),
        (
            json!({"move_rules": [{"rule_id": "x", "position": 1}]}),
            Permission::EditRules,
        ),
        (
            json!({"edit_rules": [{"rule_id": "x", "name": "n"}]}),
            Permission::EditRules,
        ),
        (
            json!({"delete_rules": [{"rule_id": "x"}]}),
            Permission::DeleteRules,
        ),
    ];
    for (call, needed) in calls {
        let without = Permissions::all().with(needed, false);
        assert_eq!(
            code(super::propose(&s.book, &call, without)),
            "ai-tools.not-allowed",
            "{call}"
        );
        // With only that permission the refusal is gone (the call may still
        // fail on its made-up IDs, but not for this reason).
        let only = Permissions::NONE.with(needed, true);
        assert_ne!(
            code(super::propose(&s.book, &call, only)),
            "ai-tools.not-allowed",
            "{call}"
        );
    }
}

#[test]
fn a_set_is_checked_again_against_the_permissions_of_the_moment_it_is_confirmed() {
    let s = sample();
    let proposal = propose(
        &s.book,
        &json!({
            "new_categories": [{"key": "a", "name": "A"}],
            "categorize": [{"transaction_id": s.unsorted.to_string(), "new_category": "a"}],
        }),
    )
    .expect("prepared");
    let set = proposal.changes;
    assert!(set.check_allowed(additions()).is_ok());
    let later = additions().with(Permission::SortTransactions, false);
    let error = set.check_allowed(later).expect_err("turned off meanwhile");
    assert_eq!(error.report().code, "ai-changes.not-allowed");
    assert!(set.check_allowed(Permissions::NONE).is_err());
}

#[test]
fn a_model_is_offered_only_the_lists_the_user_allows() {
    assert_eq!(propose_spec(Permissions::NONE), None);
    // Reading alone prepares nothing.
    assert_eq!(
        propose_spec(Permissions::NONE.with(Permission::RulesRead, true)),
        None
    );
    // With every field unlocked, the model is offered the whole function.
    let unlocked = Field::ALL
        .into_iter()
        .fold(Permissions::all(), |permissions, field| {
            permissions.with_lock(field, false)
        });
    let all = propose_spec(unlocked).expect("offered");
    assert_eq!(all.name, PROPOSE_CHANGES.name);
    let full: Value = serde_json::from_str(PROPOSE_CHANGES.parameters).expect("json");
    assert_eq!(
        serde_json::from_str::<Value>(&all.parameters).expect("json"),
        full
    );
    assert!(all.description.contains("move_rules"));

    let additions = propose_spec(additions()).expect("offered");
    let parameters: Value = serde_json::from_str(&additions.parameters).expect("json");
    let lists: Vec<&str> = parameters["properties"]
        .as_object()
        .expect("properties")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(lists.len(), 4, "{lists:?}");
    for list in [
        "new_categories",
        "new_rules",
        "categorize",
        "edit_transactions",
    ] {
        assert!(lists.contains(&list), "{list}");
    }
    assert!(!additions.description.contains("move_rules"));
    assert!(!additions.description.contains("deleting"));

    // Without new categories nothing points at their keys.
    let none_new =
        propose_spec(Permissions::all().with(Permission::NewCategories, false)).expect("offered");
    assert!(!none_new.parameters.contains("new_category"));
    assert!(!none_new.parameters.contains("new_parent"));
    assert!(!none_new.parameters.contains("new_categories"));

    // One kind alone is still a complete, valid tool.
    for permission in Permission::ALL.into_iter().filter(|p| !p.is_read()) {
        let spec = propose_spec(Permissions::NONE.with(permission, true)).expect("offered");
        let value: Value = serde_json::from_str(&spec.parameters).expect("json");
        assert!(
            !value["properties"]
                .as_object()
                .expect("properties")
                .is_empty()
        );
    }
}

/// What the chat could prepare before the grid: the additions and sorting.
fn additions() -> Permissions {
    Permissions::NONE
        .with(Permission::NewCategories, true)
        .with(Permission::NewRules, true)
        .with(Permission::SortTransactions, true)
}

#[test]
fn sorting_transactions_needs_the_category_field_not_locked() {
    let s = sample();
    let call = json!({"categorize": [{"transaction_id": s.unsorted.to_string(), "category_id": s.groceries.to_string()}]});
    let locked = additions().with_lock(Field::Category, true);
    assert_eq!(
        code(super::propose(&s.book, &call, locked)),
        "ai-tools.not-allowed"
    );
    let unlocked = additions().with_lock(Field::Category, false);
    assert!(super::propose(&s.book, &call, unlocked).is_ok());
    // A locked Category field leaves the list out of what a model is offered.
    let offered = propose_spec(locked).expect("the additions are still offered");
    assert!(!offered.parameters.contains("categorize"));
    // And a set prepared earlier is refused when the user locks it meanwhile.
    let set = super::propose(&s.book, &call, unlocked)
        .expect("prepared")
        .changes;
    assert!(set.check_allowed(unlocked).is_ok());
    assert!(set.check_allowed(locked).is_err());
}

/// The properties of one item of `edit_transactions` a model is offered.
fn edit_properties(allowed: Permissions) -> Option<Vec<String>> {
    let spec = propose_spec(allowed)?;
    let parameters: Value = serde_json::from_str(&spec.parameters).expect("json");
    let item = parameters["properties"].get("edit_transactions")?;
    let mut names: Vec<String> = item["items"]["properties"]
        .as_object()
        .expect("properties")
        .keys()
        .cloned()
        .collect();
    names.sort();
    Some(names)
}

#[test]
fn a_locked_field_is_not_offered_and_a_list_with_every_field_locked_is_left_out() {
    // The amount, the date and the payee are locked until the user chooses.
    let by_default = additions();
    assert_eq!(
        edit_properties(by_default),
        Some(vec![
            "account_id".into(),
            "note".into(),
            "transaction_id".into()
        ])
    );
    let amount_free = by_default.with_lock(Field::Amount, false);
    assert!(
        edit_properties(amount_free)
            .unwrap()
            .contains(&"amount".to_owned())
    );
    let all_locked = Field::ALL
        .into_iter()
        .fold(by_default, |permissions, field| {
            permissions.with_lock(field, true)
        });
    assert_eq!(edit_properties(all_locked), None);
    // Locking the category stops sorting, not editing the other fields.
    let spec = propose_spec(all_locked.with_lock(Field::Note, false)).expect("offered");
    assert!(!spec.parameters.contains("categorize"));
    assert!(spec.parameters.contains("edit_transactions"));
}

/// Everything allowed and no field locked.
fn open() -> Permissions {
    Field::ALL
        .iter()
        .fold(Permissions::all(), |p, field| p.with_lock(*field, false))
}

fn edit(book: &Book, arguments: &Value) -> Result<Proposal, ToolError> {
    super::propose(book, &json!({"edit_transactions": [arguments]}), open())
}

#[test]
fn a_transaction_gets_new_fields_and_still_balances() {
    let Sample {
        mut book,
        bought,
        checking,
        ..
    } = sample();
    let before = state(&book);
    let proposal = edit(
        &book,
        &json!({
            "transaction_id": bought.to_string(),
            "amount": "-60.00",
            "date": "2026-10-06",
            "payee": " Market Hall ",
            "note": "weekly",
        }),
    )
    .expect("prepared");
    assert_eq!(state(&book), before, "proposing changes nothing");
    let ProposedChange::EditTransaction(change) = &proposal.changes.changes()[0] else {
        panic!("an edit");
    };
    let fields: Vec<_> = change.edits.iter().map(|line| line.field).collect();
    assert_eq!(
        fields,
        [Field::Amount, Field::Date, Field::Payee, Field::Note]
    );
    assert_eq!(
        change.amount,
        eur(-5320),
        "the preview starts from the book"
    );
    assert_eq!(change.account, "Checking account");
    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    let now = book.transaction(bought).expect("there");
    assert_eq!(now.booking_date(), Date::constant(2026, 10, 6));
    assert_eq!(now.payee(), Some("Market Hall"));
    assert_eq!(now.note(), Some("weekly"));
    let parts = now.splits();
    assert_eq!(parts[0].account, Some(checking));
    assert_eq!(parts[0].account_amount(), eur(-6000));
    assert_eq!(parts[1].account_amount(), eur(6000), "both parts move");
    assert_eq!(book.history().unwrap()[0].action, Action::ApplyAiChanges);

    book.undo().expect("undone").expect("a change");
    let undone = state(&book);
    assert_eq!((&undone.0, &undone.1), (&before.0, &before.1));
}

#[test]
fn only_what_the_model_names_changes() {
    let Sample {
        mut book, bought, ..
    } = sample();
    let stored = book.transaction(bought).expect("there");
    edit(
        &book,
        &json!({"transaction_id": bought.to_string(), "note": "x"}),
    )
    .expect("prepared")
    .changes
    .apply(&mut book, Confirmed::by_the_user())
    .expect("applied");
    let now = book.transaction(bought).expect("there");
    assert_eq!(now.booking_date(), stored.booking_date());
    assert_eq!(now.payee(), stored.payee());
    assert_eq!(now.splits(), stored.splits());
    assert_eq!(now.note(), Some("x"));
}

#[test]
fn the_account_changes_to_another_open_one_in_the_same_currency() {
    let Sample {
        mut book, bought, ..
    } = sample();
    let savings = book
        .all_accounts()
        .unwrap()
        .into_iter()
        .find(|account| account.name == "Savings")
        .unwrap();
    edit(
        &book,
        &json!({"transaction_id": bought.to_string(), "account_id": savings.id.to_string()}),
    )
    .expect("prepared")
    .changes
    .apply(&mut book, Confirmed::by_the_user())
    .expect("applied");
    let parts = book.transaction(bought).unwrap().splits().to_vec();
    assert_eq!(parts[0].account, Some(savings.id));
    assert_eq!(parts[0].account_amount(), eur(-5320));
}

#[test]
fn what_is_not_plain_or_not_allowed_is_refused() {
    let Sample {
        book,
        bought,
        transfer,
        split,
        travel,
        salary,
        ..
    } = sample();
    let id = |id: TransactionId| id.to_string();
    // A transfer and a split keep their amount and account; their date,
    // payee and note can change.
    for (transaction, key, value) in [
        (transfer, "amount", "-5.00"),
        (split, "amount", "-5.00"),
        (transfer, "account_id", &salary.to_string()[..]),
    ] {
        assert_eq!(
            code(edit(
                &book,
                &json!({"transaction_id": id(transaction), key: value})
            )),
            "ai-tools.not-editable",
            "{key}"
        );
    }
    for transaction in [transfer, split] {
        assert_eq!(
            code(edit(
                &book,
                &json!({"transaction_id": id(transaction), "note": "ok"})
            )),
            "prepared"
        );
    }
    // Another currency, a category or an unknown account as the new account.
    assert_eq!(
        code(edit(
            &book,
            &json!({"transaction_id": id(bought), "account_id": travel.to_string()})
        )),
        "ai-tools.unknown-account"
    );
    assert_eq!(
        code(edit(
            &book,
            &json!({"transaction_id": id(bought), "account_id": AccountId::generate().to_string()})
        )),
        "ai-tools.unknown-account"
    );
    // Amounts: unchanged, zero, too many decimals, a float's notation.
    for amount in ["-53.20", "0", "-1.005", "1e2", "-5,5"] {
        assert_eq!(
            code(edit(
                &book,
                &json!({"transaction_id": id(bought), "amount": amount})
            )),
            "bad-argument amount",
            "{amount}"
        );
    }
    // Nothing to change, an unknown key and a day that isn't one.
    assert_eq!(
        code(edit(&book, &json!({"transaction_id": id(bought)}))),
        "bad-argument edit_transactions"
    );
    assert_eq!(
        code(edit(
            &book,
            &json!({"transaction_id": id(bought), "category_id": "x"})
        )),
        "bad-argument edit_transactions"
    );
    assert_eq!(
        code(edit(
            &book,
            &json!({"transaction_id": id(bought), "date": "06.10.2026"})
        )),
        "bad-argument date"
    );
}

#[test]
fn a_locked_field_is_refused_and_an_unlocked_one_is_not() {
    let Sample { book, bought, .. } = sample();
    let call = |allowed: Permissions, key: &str, value: &str| {
        code(super::propose(
            &book,
            &json!({"edit_transactions": [{"transaction_id": bought.to_string(), key: value}]}),
            allowed,
        ))
    };
    // The defaults lock the amount, the date and the payee.
    for (key, value) in [("amount", "-1.00"), ("date", "2026-10-06"), ("payee", "X")] {
        assert_eq!(
            call(Permissions::all(), key, value),
            "ai-tools.field-locked"
        );
        assert_eq!(call(open(), key, value), "prepared");
    }
    assert_eq!(call(Permissions::all(), "note", "ok"), "prepared");
    let note_locked = Permissions::all().with_lock(Field::Note, true);
    assert_eq!(call(note_locked, "note", "ok"), "ai-tools.field-locked");
    // The Change cell of Transactions is the switch for all of it.
    assert_eq!(
        call(Permissions::defaults(), "note", "ok"),
        "ai-tools.not-allowed"
    );
}

#[test]
fn a_lock_switched_on_after_the_review_stops_the_set() {
    let Sample { book, bought, .. } = sample();
    let proposal = edit(
        &book,
        &json!({"transaction_id": bought.to_string(), "amount": "-1.00"}),
    )
    .expect("prepared");
    assert!(proposal.changes.check_allowed(open()).is_ok());
    assert!(proposal.changes.check_allowed(Permissions::all()).is_err());
}

#[test]
fn the_entry_changed_after_the_review_so_the_edit_is_not_applied() {
    let Sample {
        mut book, bought, ..
    } = sample();
    let first = edit(
        &book,
        &json!({"transaction_id": bought.to_string(), "note": "one"}),
    )
    .expect("prepared");
    let second = edit(
        &book,
        &json!({"transaction_id": bought.to_string(), "note": "two"}),
    )
    .expect("prepared");
    first
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    let before = state(&book);
    assert!(matches!(
        second.changes.apply(&mut book, Confirmed::by_the_user()),
        Err(ChangeError::OutOfDate)
    ));
    assert_eq!(state(&book), before);
}

#[test]
fn the_same_transaction_cannot_be_named_twice_in_one_set() {
    let Sample { book, bought, .. } = sample();
    let id = bought.to_string();
    assert_eq!(
        code(super::propose(
            &book,
            &json!({
                "categorize": [{"transaction_id": id, "category_id": book_food(&book)}],
                "edit_transactions": [{"transaction_id": id, "note": "x"}],
            }),
            open(),
        )),
        "bad-argument transaction_id"
    );
}
