// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The AI's Rule tools match what the app can do (US1.6, #518, D-627,
//! D-628): as many conditions as a Rule needs, patterns, sub-Rules, and
//! batches big enough for an overhaul. Everything is still one change set
//! the user confirms.

use super::*;

fn text_condition(test: &str, text: &str) -> Value {
    json!({"field": "payee", "test": test, "text": text})
}

fn rule(name: &str, conditions: Vec<Value>, category: AccountId) -> Value {
    json!({"name": name, "conditions": conditions, "category_id": category.to_string()})
}

/// The names of a tree of Rules, each followed by its sub-Rules in
/// brackets.
fn tree(nodes: &[RuleNode]) -> Vec<String> {
    nodes
        .iter()
        .map(|node| {
            if node.sub_rules.is_empty() {
                node.rule.name.clone()
            } else {
                format!("{} [{}]", node.rule.name, tree(&node.sub_rules).join(", "))
            }
        })
        .collect()
}

#[test]
fn one_rule_holds_as_many_conditions_as_the_app_does() {
    let Sample {
        mut book,
        groceries,
        ..
    } = sample();
    // An imported Rule had 27; a hundred is the limit, one more is refused.
    let merchants = |count: usize| -> Vec<Value> {
        (0..count)
            .map(|n| text_condition("contains", &format!("merchant {n}")))
            .collect()
    };
    let mut any = rule("Food › Groceries", merchants(27), groceries);
    any["match"] = json!("any");
    let proposal = propose(&book, &json!({"new_rules": [any]})).expect("27 conditions");
    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    let stored = book.rules().expect("rules");
    assert_eq!(stored[0].rule.conditions.len(), 27);
    assert_eq!(stored[0].rule.match_mode, MatchMode::Any);

    let full = rule("Full", merchants(MAX_CONDITIONS), groceries);
    propose(&book, &json!({"new_rules": [full]})).expect("the most");
    let over = rule("Over", merchants(MAX_CONDITIONS + 1), groceries);
    assert_eq!(
        code(propose(&book, &json!({"new_rules": [over]}))),
        "bad-condition - too-many"
    );
}

#[test]
fn a_pattern_sorts_what_it_matches_and_a_bad_one_says_which_condition() {
    let Sample {
        mut book,
        groceries,
        unsorted,
        ..
    } = sample();
    // Capitals are ignored, as the tool text says: "Pet Shop" matches.
    let call = json!({"new_rules": [rule(
        "Pets",
        vec![text_condition("matches_pattern", "^pet\\s+(shop|store)$")],
        groceries
    )]});
    let proposal = propose(&book, &call).expect("a pattern");
    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    book.run_rules()
        .expect("the Rules run on what has no category");
    let sorted = book.transaction(unsorted).expect("transaction");
    assert!(
        sorted
            .splits()
            .iter()
            .any(|split| split.account == Some(groceries)),
        "the engine used the AI's pattern"
    );

    // A case-sensitive pattern does not.
    let mut sensitive = text_condition("matches_pattern", "^pet shop$");
    sensitive["case_sensitive"] = json!(true);
    assert!(
        propose(
            &book,
            &json!({"new_rules": [rule("Strict", vec![sensitive], groceries)]})
        )
        .is_ok()
    );

    for (pattern, expected) in [
        ("(?=shop)", "bad-condition 2 bad-pattern"),
        ("(shop", "bad-condition 2 bad-pattern"),
        ("\\1(a)", "bad-condition 2 bad-pattern"),
        ("  ", "bad-condition 2 text"),
        ("a\u{202e}b", "bad-condition 2 text"),
    ] {
        let call = json!({"new_rules": [rule(
            "Bad",
            vec![text_condition("contains", "x"), text_condition("matches_pattern", pattern)],
            groceries
        )]});
        assert_eq!(code(propose(&book, &call)), expected, "{pattern:?}");
    }
    // A pattern may be longer than other text, up to a point.
    let long = format!("^({})$", ["shop"; 150].join("|"));
    assert!(long.chars().count() > 100 && long.chars().count() <= 1000);
    let call = json!({"new_rules": [rule("Long", vec![text_condition("matches_pattern", &long)], groceries)]});
    propose(&book, &call).expect("a long pattern");
    let call =
        json!({"new_rules": [rule("Long", vec![text_condition("contains", &long)], groceries)]});
    assert_eq!(code(propose(&book, &call)), "bad-condition 1 text");
    let huge = "a".repeat(1001);
    let call = json!({"new_rules": [rule("Huge", vec![text_condition("matches_pattern", &huge)], groceries)]});
    assert_eq!(code(propose(&book, &call)), "bad-condition 1 text");
}

#[test]
fn an_imported_rule_with_a_pattern_can_be_edited_with_its_conditions() {
    let Sample {
        mut book,
        groceries,
        ..
    } = sample();
    let imported = Rule {
        id: RuleId::generate(),
        name: "Imported".into(),
        match_mode: MatchMode::Any,
        conditions: vec![
            Condition::Text {
                field: TextField::OtherPartyName,
                operator: TextOperator::Matches,
                value: "^grocer(y)?$".into(),
                case_sensitive: false,
            },
            Condition::Text {
                field: TextField::Payee,
                operator: TextOperator::Contains,
                value: "market".into(),
                case_sensitive: false,
            },
        ],
        actions: Actions {
            book_to: Some(BookTo::Category(groceries)),
            payee: None,
        },
    };
    book.add_rule(&imported, None).expect("Rule");
    // The model hears the pattern in full from list_rules, so it can write
    // it back (it could not before US1.6).
    let call = json!({"edit_rules": [{
        "rule_id": imported.id.to_string(),
        "conditions": [
            {"field": "other_party_name", "test": "matches_pattern", "text": "^grocer(y|s)?$"},
            {"field": "payee", "test": "contains", "text": "market"},
            {"field": "payee", "test": "contains", "text": "bakery"},
        ],
    }]});
    let proposal = propose(&book, &call).expect("edited");
    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    assert_eq!(book.rule(imported.id).expect("Rule").conditions.len(), 3);
}

#[test]
fn a_rule_can_sit_inside_one_of_the_book_or_one_made_in_the_same_call() {
    let (
        Sample {
            mut book,
            groceries,
            food,
            ..
        },
        rules,
    ) = with_rules();
    let mut parent = rule("Food", vec![text_condition("contains", "food")], food);
    parent["key"] = json!("food");
    let mut sub = rule(
        "Groceries",
        vec![text_condition("contains", "grocer")],
        groceries,
    );
    sub["new_parent_rule"] = json!("food");
    let mut same_name_elsewhere = rule("Pets", vec![text_condition("contains", "pet")], food);
    same_name_elsewhere["new_parent_rule"] = json!("food");
    // "Pets" exists at the top level; inside "Food" it is another place.
    let mut under_existing = rule(
        "Corner shops",
        vec![text_condition("contains", "corner")],
        groceries,
    );
    under_existing["parent_rule_id"] = json!(rules[0].id.to_string());
    let before = state(&book);
    let proposal = propose(
        &book,
        &json!({"new_rules": [parent, sub, same_name_elsewhere, under_existing]}),
    )
    .expect("sub-Rules");
    assert_eq!(state(&book), before, "proposing changes nothing");
    let changes = proposal.changes.changes();
    let inside: Vec<Option<&str>> = changes
        .iter()
        .map(|change| match change {
            ProposedChange::NewRule(new) => new.inside.as_deref(),
            _ => panic!("only new Rules"),
        })
        .collect();
    assert_eq!(inside, [None, Some("Food"), Some("Food"), Some("Grocer")]);
    assert!(proposal.changes.summary().contains("inside \"Food\""));

    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied in order, parents first");
    assert_eq!(
        tree(&book.rules().expect("rules")),
        [
            "Grocer [Corner shops]",
            "Pets",
            "Market",
            "Food [Groceries, Pets]"
        ]
    );
    // The top-level order is for top-level Rules only.
    book.undo().expect("one undo takes the whole set back");
    assert_eq!(names(&book), ["Grocer", "Pets", "Market"]);
    assert!(book.rules().expect("rules")[0].sub_rules.is_empty());
}

#[test]
fn a_sub_rule_needs_a_parent_that_is_there_and_not_too_deep() {
    let (Sample { book, food, .. }, rules) = with_rules();
    let child = |parent: (&str, Value)| {
        let mut item = rule("Child", vec![text_condition("contains", "x")], food);
        item[parent.0] = parent.1;
        item
    };
    let keyed = |key: &str, name: &str| {
        let mut item = rule(name, vec![text_condition("contains", "x")], food);
        item["key"] = json!(key);
        item
    };
    for (call, expected) in [
        // A key that no earlier Rule has, or a later one.
        (
            json!({"new_rules": [child(("new_parent_rule", json!("nope")))]}),
            "bad-argument new_parent_rule",
        ),
        (
            json!({"new_rules": [child(("new_parent_rule", json!("later"))), keyed("later", "Later")]}),
            "bad-argument new_parent_rule",
        ),
        // An ID that is no Rule, or one the same call deletes.
        (
            json!({"new_rules": [child(("parent_rule_id", json!(RuleId::generate().to_string())))]}),
            "ai-tools.unknown-rule",
        ),
        (
            json!({
                "delete_rules": [{"rule_id": rules[1].id.to_string()}],
                "new_rules": [child(("parent_rule_id", json!(rules[1].id.to_string())))],
            }),
            "ai-tools.unknown-rule",
        ),
        // A category's ID is not a Rule's, and the refusal says so (#860).
        (
            json!({"new_rules": [child(("parent_rule_id", json!(food.to_string())))]}),
            "ai-tools.category-not-a-rule",
        ),
        // Both ways to name a parent.
        (
            json!({"new_rules": [keyed("k", "K"), {
                "name": "Both", "conditions": [text_condition("contains", "x")],
                "category_id": food.to_string(), "parent_rule_id": rules[0].id.to_string(),
                "new_parent_rule": "k",
            }]}),
            "bad-argument parent_rule_id",
        ),
        // The same key twice, and the same name inside one parent.
        (
            json!({"new_rules": [keyed("k", "A"), keyed("k", "B")]}),
            "bad-argument key",
        ),
        (
            json!({"new_rules": [keyed("k", "A"), child(("new_parent_rule", json!("k"))), {
                "name": "CHILD", "conditions": [text_condition("contains", "y")],
                "category_id": food.to_string(), "new_parent_rule": "k",
            }]}),
            "ai-tools.name-taken",
        ),
    ] {
        assert_eq!(code(propose(&book, &call)), expected, "{call}");
    }
    // Five levels are allowed, a sixth is not.
    let mut list = Vec::new();
    for level in 0..6 {
        let mut item = keyed(&format!("l{level}"), &format!("Level {level}"));
        if level > 0 {
            item["new_parent_rule"] = json!(format!("l{}", level - 1));
        }
        list.push(item);
    }
    assert_eq!(
        code(propose(&book, &json!({"new_rules": list}))),
        "bad-argument new_parent_rule"
    );
    list.pop();
    propose(&book, &json!({"new_rules": list})).expect("five levels");
}

#[test]
fn leaving_out_a_parent_the_set_uses_is_refused_for_the_user_and_the_rest_stays() {
    let Sample {
        book,
        food,
        groceries,
        ..
    } = sample();
    let mut parent = rule("Food", vec![text_condition("contains", "food")], food);
    parent["key"] = json!("food");
    let mut sub = rule(
        "Groceries",
        vec![text_condition("contains", "grocer")],
        groceries,
    );
    sub["new_parent_rule"] = json!("food");
    let proposal = propose(&book, &json!({"new_rules": [parent, sub]})).expect("prepared");
    let set = proposal.changes;
    let slots = set.slots().to_vec();
    let error = set
        .leave_out(&book, slots[0], Permissions::all())
        .expect_err("the sub-Rule needs its parent");
    assert_eq!(error.report().code, "ai-changes.edit-in-use");
    // The sub-Rule can go alone, and the parent can be renamed.
    let alone = set
        .leave_out(&book, slots[1], Permissions::all())
        .expect("left out");
    assert_eq!(alone.changes().len(), 1);
    let renamed = set
        .edit(
            &book,
            slots[0],
            &json!({"name": "Eating"}),
            Permissions::all(),
        )
        .expect("renamed");
    assert!(matches!(
        &renamed.changes()[1],
        ProposedChange::NewRule(new) if new.inside.as_deref() == Some("Eating")
    ));
}

#[test]
fn a_fifty_rule_overhaul_is_one_set() {
    let Sample {
        mut book,
        groceries,
        ..
    } = sample();
    let mut old = Vec::new();
    for n in 0..100 {
        let rule = Rule {
            id: RuleId::generate(),
            name: format!("Old {n}"),
            match_mode: MatchMode::All,
            conditions: vec![Condition::Text {
                field: TextField::Payee,
                operator: TextOperator::Contains,
                value: format!("old {n}"),
                case_sensitive: false,
            }],
            actions: Actions {
                book_to: Some(BookTo::Category(groceries)),
                payee: None,
            },
        };
        book.add_rule(&rule, None).expect("Rule");
        old.push(rule);
    }
    // Every old Rule is replaced by a new one of the same name: 100
    // deletions and 100 new Rules, the biggest set.
    let deletions: Vec<Value> = old
        .iter()
        .map(|rule| json!({"rule_id": rule.id.to_string()}))
        .collect();
    let rules: Vec<Value> = (0..100)
        .map(|n| {
            rule(
                &format!("old {n}"),
                vec![text_condition("contains", &format!("new {n}"))],
                groceries,
            )
        })
        .collect();
    let call = json!({"delete_rules": deletions, "new_rules": rules});
    let proposal = propose(&book, &call).expect("one set");
    assert_eq!(proposal.changes.changes().len(), MAX_CHANGES);
    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    assert_eq!(book.rules().expect("rules").len(), 100);
    assert_eq!(names(&book)[0], "old 0");
    book.undo().expect("one undo");
    assert_eq!(names(&book)[0], "Old 0");

    // One change more is too many to review.
    let mut more = call.clone();
    more["new_categories"] = json!([{"key": "k", "name": "One more"}]);
    assert_eq!(code(propose(&book, &more)), "ai-tools.too-many-changes");
}

#[test]
fn the_model_is_told_what_a_rule_can_hold() {
    let spec = propose_spec(Permissions::all()).expect("offered");
    let parameters: Value = serde_json::from_str(&spec.parameters).expect("JSON");
    let item = &parameters["properties"]["new_rules"]["items"]["properties"];
    for name in ["key", "parent_rule_id", "new_parent_rule", "conditions"] {
        assert!(item.get(name).is_some(), "{name}");
    }
    assert_eq!(item["conditions"]["maxItems"], json!(MAX_CONDITIONS));
    let test = &item["conditions"]["items"]["properties"]["test"];
    assert!(
        test["enum"]
            .as_array()
            .is_some_and(|tests| tests.contains(&json!("matches_pattern")))
    );
    let help = test["description"].as_str().expect("help");
    assert!(
        help.contains("lookahead") && help.contains("regex"),
        "{help}"
    );
    let case = item["conditions"]["items"]["properties"]["case_sensitive"]["description"]
        .as_str()
        .expect("help");
    assert!(case.contains("capitals are ignored"), "{case}");
    assert_eq!(
        parameters["properties"]["new_rules"]["maxItems"],
        json!(100)
    );
    assert_eq!(
        parameters["properties"]["delete_rules"]["maxItems"],
        json!(100)
    );
    assert!(spec.description.contains("parent_rule_id"));
    // The schema's bounds are the code's.
    let edit = &parameters["properties"]["edit_rules"]["items"]["properties"]["conditions"];
    assert_eq!(edit["maxItems"], json!(MAX_CONDITIONS));
    assert_eq!(
        item["conditions"]["items"]["properties"]["text"]["maxLength"],
        json!(MAX_PATTERN_CHARS)
    );
}

#[test]
fn a_rule_card_counts_what_it_catches_and_the_rule_stays_marked_as_the_ais() {
    let Sample {
        mut book,
        groceries,
        ..
    } = sample();
    // "o" is in "Corner Grocery" and "Pet Shop", not in "Market".
    let call = json!({"new_rules": [rule(
        "Shops",
        vec![text_condition("contains", "o")],
        groceries
    )]});
    let proposal = propose(&book, &call).expect("prepared");
    let ProposedChange::NewRule(new) = &proposal.changes.changes()[0] else {
        panic!("a new Rule");
    };
    let made = new.rule.id;
    assert_eq!(
        new.catches,
        book.rule_preview(&new.rule).expect("fits").len()
    );
    assert_eq!(new.catches, 2);
    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    assert_eq!(
        book.unchecked_ai_rules().expect("marks"),
        HashSet::from([made])
    );
    // The user opens it once: the mark goes. A change by the AI marks it
    // again, and the card counts what the changed Rule catches.
    book.mark_rule_checked(made).expect("opened");
    let edit = json!({"edit_rules": [{
        "rule_id": made.to_string(),
        "conditions": [text_condition("contains", "Pet")]
    }]});
    let proposal = propose(&book, &edit).expect("prepared");
    let ProposedChange::EditRule(changed) = &proposal.changes.changes()[0] else {
        panic!("a changed Rule");
    };
    assert_eq!(changed.catches, 1);
    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    assert_eq!(
        book.unchecked_ai_rules().expect("marks"),
        HashSet::from([made])
    );
}
