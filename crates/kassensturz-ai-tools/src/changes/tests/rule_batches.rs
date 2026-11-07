// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Rules changed in one call (M6-B191, D-626): a Rule replaced by a new one
//! of the same name, a Rule the AI can't rewrite says why, and every refusal
//! names the item and the reason.

use kassensturz_core::ErrorParam;

use super::*;

/// What the model hears about a refused call: the code, and the words and
/// numbers that come with it, as `name=value` in order.
fn heard(result: Result<Proposal, ToolError>) -> String {
    let error = result.expect_err("refused");
    let report = error.model_report();
    let mut words = vec![report.code.to_owned()];
    for (name, value) in &report.params {
        match value {
            ErrorParam::Keyword(word) => words.push(format!("{name}={word}")),
            ErrorParam::Number(number) => words.push(format!("{name}={number}")),
            other => panic!("only fixed words and counts go to the model: {other:?}"),
        }
    }
    words.join(" ")
}

fn new_rule(name: &str, text: &str, category: AccountId) -> Value {
    json!({
        "name": name,
        "conditions": [{"field": "payee", "test": "contains", "text": text}],
        "category_id": category.to_string(),
    })
}

#[test]
fn a_rule_is_replaced_by_a_new_one_of_the_same_name_in_one_call() {
    let (Sample { mut book, food, .. }, rules) = with_rules();
    let before = state(&book);
    let call = json!({
        "delete_rules": [{"rule_id": rules[1].id.to_string()}],
        "new_rules": [new_rule("pets", "pet shop", food)],
    });
    let proposal = propose(&book, &call).expect("the name is free once the old Rule goes");
    assert_eq!(state(&book), before, "proposing changes nothing");
    // The delete comes first, in the set and when it is applied.
    assert!(matches!(
        proposal.changes.changes(),
        [ProposedChange::DeleteRule(_), ProposedChange::NewRule(_)]
    ));
    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("the store accepts the new Rule after the deletion");
    assert_eq!(names(&book), ["Grocer", "Market", "pets"]);
    book.undo().expect("one undo takes both back");
    assert_eq!(names(&book), ["Grocer", "Pets", "Market"]);
}

#[test]
fn a_rename_frees_the_old_name_and_takes_the_new_one() {
    let (Sample { mut book, food, .. }, rules) = with_rules();
    let call = json!({
        "edit_rules": [{"rule_id": rules[0].id.to_string(), "name": "Supermarket"}],
        "new_rules": [new_rule("Grocer", "grocer's market", food)],
    });
    let proposal = propose(&book, &call).expect("Grocer is free after the rename");
    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    assert_eq!(names(&book), ["Supermarket", "Pets", "Market", "Grocer"]);

    // A name still in use after the call is refused, naming the item.
    let (Sample { book, food, .. }, rules) = with_rules();
    for (call, expected) in [
        (
            json!({"new_rules": [new_rule("grocer", "x", food)]}),
            "ai-tools.name-taken list=new_rules item=1",
        ),
        (
            json!({"edit_rules": [
                {"rule_id": rules[0].id.to_string(), "name": "Fresh"},
                {"rule_id": rules[1].id.to_string(), "name": "MARKET"},
            ]}),
            "ai-tools.name-taken list=edit_rules item=2",
        ),
        (
            json!({
                "delete_rules": [{"rule_id": rules[1].id.to_string()}],
                "new_rules": [new_rule("Market", "x", food), new_rule("Pets", "y", food), new_rule("pets", "z", food)],
            }),
            "ai-tools.name-taken list=new_rules item=1",
        ),
    ] {
        assert_eq!(heard(propose(&book, &call)), expected, "{call}");
    }
    // Replacing a Rule while keeping it too is the same name twice.
    let call = json!({
        "delete_rules": [{"rule_id": rules[1].id.to_string()}],
        "new_rules": [new_rule("Pets", "y", food), new_rule("pets", "z", food)],
    });
    assert_eq!(
        heard(propose(&book, &call)),
        "ai-tools.name-taken list=new_rules item=2"
    );
}

#[test]
fn a_rule_the_ai_cannot_rewrite_says_which_argument_and_why() {
    let (
        Sample {
            mut book,
            food,
            checking,
            ..
        },
        rules,
    ) = with_rules();
    // A Rule with an IBAN, as an import makes them, and one with a sub-Rule.
    let imported = Rule {
        id: RuleId::generate(),
        name: "Imported".into(),
        match_mode: MatchMode::All,
        conditions: vec![Condition::Text {
            field: TextField::OtherPartyIban,
            operator: TextOperator::Equals,
            value: "DE02120300000000202051".into(),
            case_sensitive: false,
        }],
        actions: Actions {
            book_to: Some(BookTo::Category(food)),
            payee: None,
        },
    };
    book.add_rule(&imported, None).expect("Rule");
    let sub = Rule {
        id: RuleId::generate(),
        name: "Sub".into(),
        ..rules[0].clone()
    };
    book.add_rule(&sub, Some(rules[0].id)).expect("sub-Rule");
    let transfer = Rule {
        id: RuleId::generate(),
        name: "Moves money".into(),
        actions: Actions {
            book_to: Some(BookTo::Transfer(checking)),
            payee: None,
        },
        ..rules[1].clone()
    };
    book.add_rule(&transfer, None).expect("Transfer Rule");
    let shop = [json!({"field": "payee", "test": "contains", "text": "x"})];
    for (call, expected) in [
        (
            json!({"edit_rules": [{"rule_id": imported.id.to_string(), "conditions": shop}]}),
            "ai-tools.not-changeable argument=conditions reason=conditions-hidden list=edit_rules item=1",
        ),
        (
            json!({"delete_rules": [{"rule_id": rules[2].id.to_string()}, {"rule_id": rules[0].id.to_string()}]}),
            "ai-tools.not-changeable argument=rule_id reason=has-sub-rules list=delete_rules item=2",
        ),
        (
            json!({"move_rules": [{"rule_id": sub.id.to_string(), "position": 1}]}),
            "ai-tools.not-changeable argument=rule_id reason=is-sub-rule list=move_rules item=1",
        ),
        (
            json!({"edit_rules": [{"rule_id": transfer.id.to_string(), "category_id": food.to_string()}]}),
            "ai-tools.not-changeable argument=category_id reason=makes-transfer list=edit_rules item=1",
        ),
    ] {
        assert_eq!(heard(propose(&book, &call)), expected, "{call}");
    }
    // What the AI can write about the imported Rule is still allowed.
    propose(
        &book,
        &json!({"edit_rules": [{"rule_id": imported.id.to_string(), "name": "Landlord", "category_id": food.to_string()}]}),
    )
    .expect("its name and its category are the AI's to change");
}

#[test]
fn every_refusal_names_the_item_and_a_condition_names_its_place() {
    let (Sample { book, food, .. }, rules) = with_rules();
    let ok = json!({"field": "payee", "test": "contains", "text": "x"});
    for (call, expected) in [
        (
            json!({"new_categories": [{"key": "a", "name": "A"}, {"key": "b"}]}),
            "ai-tools.bad-argument argument=name list=new_categories item=2",
        ),
        (
            json!({"new_rules": [
                new_rule("One", "a", food),
                {"name": "Two", "conditions": [ok, ok, {"field": "payee", "test": "like", "text": "x"}], "category_id": food.to_string()},
            ]}),
            "ai-tools.bad-condition condition=3 reason=test list=new_rules item=2",
        ),
        (
            json!({"new_rules": [{"name": "Two", "conditions": [], "category_id": food.to_string()}]}),
            "ai-tools.bad-condition reason=none list=new_rules item=1",
        ),
        (
            json!({"new_rules": [{"name": "Two", "conditions": [{"field": "payee", "test": "contains", "text": " "}], "category_id": food.to_string()}]}),
            "ai-tools.bad-condition condition=1 reason=text list=new_rules item=1",
        ),
        (
            json!({"edit_rules": [{"rule_id": rules[0].id.to_string(), "conditions": (0..=MAX_CONDITIONS).map(|_| ok.clone()).collect::<Vec<_>>()}]}),
            "ai-tools.bad-condition reason=too-many list=edit_rules item=1",
        ),
        (
            json!({"delete_rules": [{"rule_id": rules[0].id.to_string()}, {"rule_id": RuleId::generate().to_string()}]}),
            "ai-tools.unknown-rule list=delete_rules item=2",
        ),
        (
            json!({"categorize": [{"transaction_id": "nope", "category_id": food.to_string()}]}),
            "ai-tools.bad-argument argument=transaction_id list=categorize item=1",
        ),
    ] {
        assert_eq!(heard(propose(&book, &call)), expected, "{call}");
    }
    // The call as a whole is not an item: it names no list.
    let many: Vec<Value> = (0..=MAX_CHANGES)
        .map(|n| json!({"key": format!("k{n}"), "name": format!("C{n}")}))
        .collect();
    assert_eq!(
        heard(propose(&book, &json!({"new_categories": many}))),
        "ai-tools.too-many-changes"
    );
    // Beneath the words, the nested report keeps the cause apart.
    let error =
        propose(&book, &json!({"delete_rules": [{"rule_id": "nope"}]})).expect_err("refused");
    assert_eq!(error.report().code, "ai-tools.in-item");
    assert_eq!(
        error.report().cause.as_ref().map(|cause| cause.code),
        Some("ai-tools.bad-argument")
    );
}
