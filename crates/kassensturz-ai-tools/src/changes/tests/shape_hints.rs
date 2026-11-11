// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A change list of the wrong kind is refused with a hint at the shape it
//! expects (US3.9, D-696), for every list of a `propose_changes` call.

use super::*;

/// The `expected_shape` of the refusal of `arguments`, and the refusal's
/// argument and item place.
fn refused(book: &Book, arguments: &Value) -> (Option<&'static str>, String, Option<usize>) {
    // Every cell given and no field locked, so every list is open.
    let open = Field::ALL
        .iter()
        .fold(Permissions::all(), |p, field| p.with_lock(*field, false));
    let error =
        super::super::propose_with(book, arguments, open, Some(Date::constant(2026, 11, 14)))
            .expect_err("refused");
    let item = match &error {
        ToolError::InItem { item, .. } => Some(*item),
        _ => None,
    };
    (
        error.expected_shape(),
        error.kind().report().code.to_owned(),
        item,
    )
}

#[test]
fn every_change_list_that_is_not_a_list_gets_the_shape_it_expects() {
    let Sample { book, .. } = sample();
    for (name, _) in SECTIONS {
        // Text, an object and a number are all not a list of objects.
        for wrong in [json!("x"), json!({"a": 1}), json!(5)] {
            let (shape, code, item) = refused(&book, &json!({ name: wrong }));
            assert_eq!(shape, Some("a list of objects"), "{name}");
            assert_eq!(code, "ai-tools.bad-argument", "{name}");
            assert_eq!(item, None, "{name}");
        }
    }
}

#[test]
fn every_item_that_is_not_an_object_gets_the_shape_it_expects() {
    let Sample { book, .. } = sample();
    for (name, _) in SECTIONS {
        let (shape, code, item) = refused(&book, &json!({ name: ["x"] }));
        assert_eq!(shape, Some("an object"), "{name}: {code}");
        assert_eq!(code, "ai-tools.bad-argument", "{name}");
        assert_eq!(item, Some(1), "{name}");
    }
}

#[test]
fn the_whole_call_and_the_fields_of_an_item_say_what_they_expect() {
    let Sample { book, .. } = sample();
    let (shape, _, _) = refused(&book, &json!([]));
    assert_eq!(shape, Some("an object"));
    let (shape, _, item) = refused(&book, &json!({"new_categories": [{"name": 5}]}));
    assert_eq!(shape, Some("text"));
    assert_eq!(item, Some(1));
    let (shape, _, _) = refused(
        &book,
        &json!({"new_rules": [{"name": "A", "conditions": "x", "new_category": "k"}]}),
    );
    assert_eq!(shape, Some("a list of objects"));
    // The hint is one of the fixed words, never the model's text (X-8).
    let (shape, _, _) = refused(&book, &json!({"delete_rules": [{"rule_id": "mark-me"}]}));
    assert!(
        [
            "text that is an ID, as the read functions give it",
            "an object",
        ]
        .contains(&shape.unwrap_or("")),
        "{shape:?}"
    );
}
