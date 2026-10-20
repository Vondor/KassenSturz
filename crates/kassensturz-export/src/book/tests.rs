// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Unit tests for the complete export: amounts as text and back, the layout
//! of the JSON, and the size limit. Golden files and the reading of whole
//! files are in `tests/book.rs`.

use proptest::prelude::*;

use super::json::Json;
use super::read::{parse_amount, read_at_most};
use crate::decimal_text;
use crate::error::ExportError;

#[test]
fn amounts_are_read_exactly() {
    assert_eq!(parse_amount("12.34", 2), Some(1234));
    assert_eq!(parse_amount("-0.05", 2), Some(-5));
    assert_eq!(parse_amount("7", 2), Some(700));
    assert_eq!(parse_amount("7.5", 2), Some(750));
    assert_eq!(parse_amount("-980", 0), Some(-980));
    assert_eq!(parse_amount("-0", 2), Some(0));
    assert_eq!(parse_amount("-92233720368547758.08", 2), Some(i64::MIN));
    assert_eq!(parse_amount("92233720368547758.07", 2), Some(i64::MAX));
    // Never rounded, never guessed.
    for wrong in [
        "12.345",
        "92233720368547758.08",
        "",
        "-",
        ".5",
        "5.",
        "1,50",
        "+1",
        "1e2",
        " 1",
        "1 ",
        "--1",
        "١٢",
    ] {
        assert_eq!(parse_amount(wrong, 2), None, "{wrong:?}");
    }
    assert_eq!(parse_amount("1.5", 0), None);
}

proptest! {
    /// Every amount in every number of decimal places comes back as it was
    /// written (AGENTS.md §9).
    #[test]
    fn amounts_read_back_as_written(units in any::<i64>(), digits in 0_u8..=18) {
        let text = decimal_text(units, digits, '.').expect("at most 18 places");
        prop_assert_eq!(parse_amount(&text, digits), Some(units));
    }
}

fn layout(value: &Json) -> String {
    let mut out = Vec::new();
    value.write(&mut out).expect("written");
    String::from_utf8(out).expect("UTF-8")
}

#[test]
fn lists_and_objects_of_plain_values_stay_on_one_line() {
    let inner = Json::Object(vec![
        ("text", Json::text("a \"quote\"\n\u{1}")),
        ("none", Json::Null),
        ("yes", Json::Bool(true)),
        ("number", Json::Number(7)),
        ("empty", Json::List(vec![])),
    ]);
    assert_eq!(
        layout(&inner),
        r#"{"text": "a \"quote\"\n\u0001", "none": null, "yes": true, "number": 7, "empty": []}"#
    );
    let outer = Json::Object(vec![
        ("list", Json::List(vec![Json::Number(1), Json::Number(2)])),
        (
            "nested",
            Json::List(vec![inner.clone(), Json::Object(vec![])]),
        ),
    ]);
    assert_eq!(
        layout(&outer),
        "{\n  \"list\": [1, 2],\n  \"nested\": [\n    \
         {\"text\": \"a \\\"quote\\\"\\n\\u0001\", \"none\": null, \"yes\": true, \"number\": 7, \"empty\": []},\n    \
         {}\n  ]\n}"
    );
}

#[test]
fn a_file_larger_than_the_limit_is_not_read() {
    assert_eq!(
        read_at_most(b"{\"format\": \"kassensturz-book\"}", 10),
        Err(ExportError::TooLarge { max: 10 })
    );
    assert_eq!(read_at_most(b"[]", 2), Err(ExportError::NotABook));
}
