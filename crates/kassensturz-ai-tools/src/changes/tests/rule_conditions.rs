// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Single conditions of a Rule added and taken away (#570, D-673): the
//! others stay exactly as stored, also those `list_rules` shows cut or
//! leaves out, so nothing a model can't see is written back.

use kassensturz_core::ErrorParam;

use super::*;

fn today() -> Date {
    Date::constant(2026, 10, 31)
}

/// What the model hears about a refused call, as in `rule_batches`.
fn heard(result: Result<Proposal, ToolError>) -> String {
    let report = result.expect_err("refused").model_report();
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

fn payee(text: &str) -> Condition {
    Condition::Text {
        field: TextField::Payee,
        operator: TextOperator::Contains,
        value: text.into(),
        case_sensitive: false,
    }
}

/// The conditions of the Rule named `name` as `list_rules` shows them.
fn listed(book: &Book, name: &str) -> Vec<Value> {
    let rules = crate::call(book, "list_rules", &json!({}), today())
        .expect("answered")
        .value;
    rules["rules"]
        .as_array()
        .expect("rules")
        .iter()
        .find(|rule| rule["name"] == json!(name))
        .unwrap_or_else(|| panic!("{name} in {rules}"))["conditions"]
        .as_array()
        .expect("conditions")
        .clone()
}

#[test]
fn a_condition_is_added_and_one_undo_takes_it_back() {
    let (Sample { mut book, .. }, rules) = with_rules();
    let call = json!({"edit_rules": [{
        "rule_id": rules[0].id.to_string(),
        "add_conditions": [{"field": "bank_text", "test": "contains", "text": "corner shop"}],
    }]});
    let proposal = propose(&book, &call).expect("added");
    match proposal.changes.changes() {
        [ProposedChange::EditRule(edit)] => {
            assert_eq!(edit.before.conditions, [payee("grocer")]);
            assert_eq!(edit.rule.conditions.len(), 2);
            assert_eq!(
                edit.rule.conditions[0],
                payee("grocer"),
                "the old one stays first"
            );
        }
        other => panic!("{other:?}"),
    }
    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    assert_eq!(book.rule(rules[0].id).expect("Rule").conditions.len(), 2);
    book.undo().expect("undone");
    assert_eq!(
        book.rule(rules[0].id).expect("Rule").conditions,
        [payee("grocer")]
    );
}

#[test]
fn a_condition_is_removed_by_its_place_in_list_rules() {
    let (Sample { mut book, .. }, rules) = with_rules();
    let mut rule = rules[1].clone();
    rule.conditions = vec![payee("pet"), payee("vet"), payee("zoo")];
    book.update_rule(&rule).expect("three conditions");
    // The model reads the places from list_rules: "vet" is the second.
    let shown = listed(&book, "Pets");
    assert_eq!(shown[1]["text"], json!("vet"));
    let call = json!({"edit_rules": [{
        "rule_id": rule.id.to_string(),
        "remove_conditions": [2],
        "add_conditions": [{"field": "payee", "test": "contains", "text": "animal"}],
    }]});
    propose(&book, &call)
        .expect("removed and added")
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    assert_eq!(
        book.rule(rule.id).expect("Rule").conditions,
        [payee("pet"), payee("zoo"), payee("animal")]
    );
}

#[test]
fn a_rule_shown_cut_keeps_what_the_model_cannot_see() {
    let (Sample { mut book, food, .. }, _) = with_rules();
    // A pattern list_rules cuts and an IBAN it leaves out (#568, D-186).
    let long = format!("^({})$", vec!["grocer"; 60].join("|"));
    let iban = Condition::Text {
        field: TextField::OtherPartyIban,
        operator: TextOperator::Equals,
        value: "DE02120300000000202051".into(),
        case_sensitive: false,
    };
    let cut = Condition::Text {
        field: TextField::OtherPartyName,
        operator: TextOperator::Matches,
        value: long.clone(),
        case_sensitive: false,
    };
    let rule = Rule {
        id: RuleId::generate(),
        name: "Imported".into(),
        match_mode: MatchMode::Any,
        conditions: vec![iban.clone(), cut.clone(), payee("old shop")],
        actions: Actions {
            book_to: Some(BookTo::Category(food)),
            payee: None,
        },
    };
    book.add_rule(&rule, None).expect("Rule");
    let shown = listed(&book, "Imported");
    assert_eq!(shown[0]["text_left_out"], json!(true));
    assert!(shown[1]["text"].as_str().expect("text").ends_with('…'));
    let call = json!({"edit_rules": [{
        "rule_id": rule.id.to_string(),
        "remove_conditions": [3],
        "add_conditions": [{"field": "payee", "test": "contains", "text": "new shop"}],
    }]});
    propose(&book, &call)
        .expect("single conditions change also where conditions_complete is false")
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    assert_eq!(
        book.rule(rule.id).expect("Rule").conditions,
        [iban, cut, payee("new shop")],
        "the IBAN and the long pattern are kept exactly as stored"
    );
}

#[test]
fn a_wrong_place_or_condition_is_refused_with_its_place() {
    let (Sample { book, .. }, rules) = with_rules();
    let id = rules[0].id.to_string();
    let grocer = json!({"field": "payee", "test": "contains", "text": "grocer"});
    let other = json!({"field": "payee", "test": "contains", "text": "x"});
    for (item, expected) in [
        (
            json!({"rule_id": id, "remove_conditions": [3]}),
            "ai-tools.bad-condition condition=3 reason=not-there list=edit_rules item=1",
        ),
        (
            json!({"rule_id": id, "remove_conditions": [0]}),
            "ai-tools.bad-condition condition=0 reason=not-there list=edit_rules item=1",
        ),
        (
            json!({"rule_id": id, "remove_conditions": [1]}),
            "ai-tools.bad-condition reason=none list=edit_rules item=1",
        ),
        (
            json!({"rule_id": id, "remove_conditions": [1, 1], "add_conditions": [other]}),
            "ai-tools.bad-argument argument=remove_conditions list=edit_rules item=1",
        ),
        (
            json!({"rule_id": id, "remove_conditions": ["1"]}),
            "ai-tools.bad-argument argument=remove_conditions list=edit_rules item=1",
        ),
        (
            json!({"rule_id": id, "add_conditions": [other, grocer]}),
            "ai-tools.bad-condition condition=2 reason=duplicate list=edit_rules item=1",
        ),
        (
            json!({"rule_id": id, "add_conditions": [{"field": "payee", "test": "like", "text": "x"}]}),
            "ai-tools.bad-condition condition=1 reason=test list=edit_rules item=1",
        ),
        (
            json!({"rule_id": id, "add_conditions": (0..MAX_CONDITIONS).map(|n| json!({"field": "payee", "test": "contains", "text": format!("shop {n}")})).collect::<Vec<_>>()}),
            "ai-tools.bad-condition reason=too-many list=edit_rules item=1",
        ),
        (
            json!({"rule_id": id, "conditions": [other], "add_conditions": [other]}),
            "ai-tools.bad-argument argument=conditions list=edit_rules item=1",
        ),
        (
            json!({"rule_id": id, "add_conditions": {"field": "payee"}}),
            "ai-tools.bad-argument argument=add_conditions list=edit_rules item=1",
        ),
    ] {
        let call = json!({"edit_rules": [item]});
        assert_eq!(heard(propose(&book, &call)), expected, "{call}");
    }
}

#[test]
fn the_schema_offers_both_forms_with_the_codes_bounds() {
    let spec = propose_spec(Permissions::all()).expect("offered");
    let parameters: Value = serde_json::from_str(&spec.parameters).expect("JSON");
    let item = &parameters["properties"]["edit_rules"]["items"]["properties"];
    assert_eq!(item["add_conditions"]["maxItems"], json!(MAX_CONDITIONS));
    assert_eq!(item["remove_conditions"]["maxItems"], json!(MAX_CONDITIONS));
    assert_eq!(
        item["add_conditions"]["items"]["properties"]["text"]["maxLength"],
        json!(MAX_PATTERN_CHARS)
    );
    assert!(spec.description.contains("remove_conditions"));
}

#[test]
fn a_check_names_the_conditions_remove_conditions_would_take_away() {
    let (Sample { mut book, .. }, rules) = with_rules();
    let long = format!("^({})$", vec!["grocer"; 60].join("|"));
    let iban = Condition::Text {
        field: TextField::OtherPartyIban,
        operator: TextOperator::Equals,
        value: "DE02120300000000202051".into(),
        case_sensitive: false,
    };
    let cut = Condition::Text {
        field: TextField::OtherPartyName,
        operator: TextOperator::Matches,
        value: long,
        case_sensitive: false,
    };
    let mut rule = rules[1].clone();
    rule.conditions = vec![payee("pet"), iban, payee("zoo"), cut];
    book.update_rule(&rule).expect("four conditions");
    let shown = listed(&book, "Pets");
    // The model asks to take away the 2nd (the IBAN) and the 4th (the cut
    // pattern), and the check says which ones those are.
    let call = json!({"edit_rules": [
        {"rule_id": rule.id.to_string(), "remove_conditions": [4, 2]},
    ]});
    let proposal = propose(&book, &call).expect("removed");
    let answer = crate::check_only_answer(&book, &proposal).expect("checked");
    let removed = &answer["removed_conditions"];
    assert_eq!(removed.as_array().map(Vec::len), Some(1), "{answer}");
    assert_eq!(removed[0]["list"], "edit_rules");
    assert_eq!(removed[0]["item"], 1);
    let conditions = removed[0]["conditions"].as_array().expect("conditions");
    assert_eq!(conditions.len(), 2);
    assert_eq!(conditions[0]["place"], 2);
    assert_eq!(
        conditions[0]["text_left_out"], true,
        "no IBAN reaches a model"
    );
    assert!(!answer.to_string().contains("DE02"));
    assert_eq!(conditions[1]["place"], 4);
    // Exactly as list_rules shows it, also when it is cut.
    for (condition, place) in conditions.iter().zip([2, 4]) {
        let mut as_listed = shown[place - 1].clone();
        as_listed["place"] = json!(place);
        assert_eq!(condition, &as_listed);
    }
    // Nothing is named for a call that removes nothing, or for another list.
    let adds = json!({"edit_rules": [{
        "rule_id": rule.id.to_string(),
        "add_conditions": [{"field": "payee", "test": "contains", "text": "fish"}],
    }]});
    let answer =
        crate::check_only_answer(&book, &propose(&book, &adds).expect("added")).expect("checked");
    assert!(answer.get("removed_conditions").is_none(), "{answer}");
}

#[test]
fn list_rules_shows_one_rule_by_its_id_with_its_sub_rules() {
    let (
        Sample {
            mut book,
            groceries,
            ..
        },
        rules,
    ) = with_rules();
    let sub = Rule {
        id: RuleId::generate(),
        name: "Corner".into(),
        match_mode: MatchMode::All,
        conditions: vec![payee("corner")],
        actions: Actions {
            book_to: Some(BookTo::Category(groceries)),
            payee: None,
        },
    };
    book.add_rule(&sub, Some(rules[0].id)).expect("sub-Rule");
    let all = crate::call(&book, "list_rules", &json!({}), today())
        .expect("answered")
        .value;
    assert_eq!(all["rules"].as_array().map(Vec::len), Some(3));
    // A top-level Rule comes with its sub-Rules, exactly as in the whole list.
    let one = crate::call(
        &book,
        "list_rules",
        &json!({"rule_id": rules[0].id.to_string()}),
        today(),
    )
    .expect("answered")
    .value;
    assert_eq!(one["rules"].as_array().map(Vec::len), Some(1));
    assert_eq!(one["rules"][0], all["rules"][0]);
    assert_eq!(one["rules"][0]["sub_rules"][0]["name"], "Corner");
    // A sub-Rule alone.
    let one = crate::call(
        &book,
        "list_rules",
        &json!({"rule_id": sub.id.to_string()}),
        today(),
    )
    .expect("answered")
    .value;
    assert_eq!(one["rules"][0]["name"], "Corner");
    assert!(one.get("left_out").is_none(), "{one}");
    // A Rule the book does not have, and an ID that is none.
    let missing = crate::call(
        &book,
        "list_rules",
        &json!({"rule_id": RuleId::generate().to_string()}),
        today(),
    )
    .expect_err("refused");
    assert_eq!(missing.report().code, "ai-tools.unknown-rule");
    let bad =
        crate::call(&book, "list_rules", &json!({"rule_id": "x"}), today()).expect_err("refused");
    assert_eq!(bad.report().code, "ai-tools.bad-argument");
}
