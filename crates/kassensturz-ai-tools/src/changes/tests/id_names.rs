// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A name next to an ID (US2.18, #598, D-675): a call may say which item
//! it means by name too, and is refused when the name doesn't fit the ID.

use super::*;

/// The code and the ID's key the model hears, without the item.
fn refused(result: Result<Proposal, ToolError>) -> (String, Option<String>) {
    let report = result.expect_err("refused").model_report();
    let argument = report.param("argument").and_then(|value| match value {
        kassensturz_core::ErrorParam::Keyword(word) => Some((*word).to_owned()),
        _ => None,
    });
    (report.code.to_owned(), argument)
}

#[test]
fn a_name_that_fits_its_id_is_taken_out_of_the_call() {
    let Sample {
        mut book,
        groceries,
        unsorted,
        ..
    } = sample();
    // The path as list_categories shows it, the category's own name, other
    // capitals, a plain ">" and spaces around all fit.
    for name in [
        "Food › Groceries",
        "Groceries",
        "food › groceries",
        "Food > Groceries",
        "  Groceries ",
    ] {
        let call = json!({"categorize": [{
            "transaction_id": unsorted.to_string(),
            "category_id": groceries.to_string(),
            "category_name": name,
        }]});
        let proposal = propose(&book, &call).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(proposal.changes.changes().len(), 1, "{name}");
    }
    // The set keeps the call without the names, so the user's edits and
    // the confirm never see them.
    let call = json!({"categorize": [{
        "transaction_id": unsorted.to_string(),
        "category_id": groceries.to_string(),
        "category_name": "Groceries",
    }]});
    let proposal = propose(&book, &call).expect("prepared");
    let slot = proposal.changes.slots()[0];
    let item = proposal.changes.item(slot).expect("the item");
    assert!(item.get("category_name").is_none(), "{item}");
    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
}

#[test]
fn a_name_that_names_another_item_is_refused_with_the_ids_key() {
    let (
        Sample {
            book,
            checking,
            food,
            groceries,
            unsorted,
            ..
        },
        rules,
    ) = with_rules();
    // A miscopied ID that names another category of the book: Food, not
    // Groceries.
    for (call, list, key) in [
        (
            json!({"categorize": [{"transaction_id": unsorted.to_string(), "category_id": food.to_string(), "category_name": "Groceries"}]}),
            "categorize",
            "category_id",
        ),
        (
            json!({"new_categories": [{"key": "p", "name": "Pets", "parent_id": groceries.to_string(), "parent_name": "Food"}]}),
            "new_categories",
            "parent_id",
        ),
        (
            json!({"edit_rules": [{"rule_id": rules[0].id.to_string(), "rule_name": "Pets", "name": "Shop"}]}),
            "edit_rules",
            "rule_id",
        ),
        (
            json!({"edit_accounts": [{"account_id": checking.to_string(), "account_name": "Savings", "name": "Main"}]}),
            "edit_accounts",
            "account_id",
        ),
        (
            json!({"new_rules": [{"name": "Vet", "parent_rule_id": rules[1].id.to_string(), "parent_rule_name": "Grocer",
                "conditions": [{"field": "payee", "test": "contains", "text": "vet"}], "category_id": food.to_string(), "category_name": "Food"}]}),
            "new_rules",
            "parent_rule_id",
        ),
    ] {
        let error = propose(&book, &call).expect_err("refused");
        match &error {
            ToolError::InItem {
                list: at, item: 1, ..
            } => assert_eq!(*at, list, "{call}"),
            other => panic!("{call}: {other:?}"),
        }
        assert_eq!(
            refused(Err(error)),
            ("ai-tools.name-mismatch".to_owned(), Some(key.to_owned())),
            "{call}"
        );
    }
    // The right names pass.
    propose(
        &book,
        &json!({"edit_rules": [{"rule_id": rules[0].id.to_string(), "rule_name": "grocer", "name": "Shop"}],
            "edit_accounts": [{"account_id": checking.to_string(), "account_name": "checking account", "name": "Main"}]}),
    )
    .expect("fitting names");
}

#[test]
fn a_name_without_its_id_or_not_text_is_a_bad_argument() {
    let Sample {
        book,
        groceries,
        unsorted,
        ..
    } = sample();
    for call in [
        json!({"categorize": [{"transaction_id": unsorted.to_string(), "category_name": "Groceries", "new_category": "x"}]}),
        json!({"categorize": [{"transaction_id": unsorted.to_string(), "category_id": groceries.to_string(), "category_name": 5}]}),
    ] {
        assert_eq!(
            refused(propose(&book, &call)),
            (
                "ai-tools.bad-argument".to_owned(),
                Some("category_name".to_owned())
            ),
            "{call}"
        );
    }
    // An ID that names nothing is refused as before, name or not.
    let call = json!({"categorize": [{"transaction_id": unsorted.to_string(), "category_id": AccountId::generate().to_string(), "category_name": "Groceries"}]});
    assert_eq!(
        refused(propose(&book, &call)).0,
        "ai-tools.unknown-category"
    );
}

/// How many IDs of the lists' items have their name next to them; panics
/// on a name offered without its ID or an ID without its name.
fn named_ids(parameters: &str) -> usize {
    let parameters: Value = serde_json::from_str(parameters).expect("JSON");
    let mut offered = 0;
    for (list, schema) in parameters["properties"].as_object().expect("lists") {
        let Some(item) = schema["items"]["properties"].as_object() else {
            continue;
        };
        for (id, name, _) in crate::changes::NAMED_IDS {
            assert_eq!(
                item.contains_key(id),
                item.contains_key(name),
                "{list}: {name}"
            );
            offered += usize::from(item.contains_key(name));
        }
    }
    offered
}

#[test]
fn the_schema_offers_a_name_next_to_each_id() {
    assert_eq!(named_ids(PROPOSE_CHANGES.parameters), 16);
    // A locked field is not offered, and neither is its name (D-257).
    for allowed in [Permissions::all(), additions()] {
        let spec = propose_spec(allowed).expect("offered");
        assert!(named_ids(&spec.parameters) < 16);
    }
}
