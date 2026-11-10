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
