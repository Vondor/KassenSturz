// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use kassensturz_core::{Report, ViewKind};
use kassensturz_store::Book;
use serde_json::json;

use crate::*;

const DASHBOARD: &str = r#"{
  "format": "kassensturz-dashboard", "version": 1, "kind": "dashboard",
  "name": "Overview", "filters": {"period": "last-12-months"},
  "components": [
    {"id": "c1", "type": "chart.line", "dataset": "income_expense_by_month",
     "params": {}, "props": {"title": "In and out", "legend": true},
     "frame": {"col": 0, "row": 0, "cols": 6, "rows": 4}},
    {"id": "c2", "type": "chart.pie", "dataset": "spending_by_category",
     "frame": {"col": 6, "row": 0, "cols": 6, "rows": 4}}
  ]
}"#;

fn reason(text: &str) -> (&'static str, String) {
    match Document::read(text).unwrap_err() {
        ReportError::DocumentInvalid { reason, path } => (reason, path),
        other => panic!("not a document error: {other:?}"),
    }
}

fn with(patch: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut value: serde_json::Value = serde_json::from_str(DASHBOARD).unwrap();
    patch(&mut value);
    value.to_string()
}

#[test]
fn a_dashboard_is_read_and_written_back_the_same() {
    let document = Document::read(DASHBOARD).unwrap();
    assert_eq!(document.kind, ViewKind::Dashboard);
    assert_eq!(document.name, "Overview");
    assert_eq!(document.period.as_deref(), Some("last-12-months"));
    assert_eq!(document.entries.len(), 2);
    assert_eq!(
        document.entries[1].frame,
        Some(Frame {
            col: 6,
            row: 0,
            cols: 6,
            rows: 4
        })
    );
    let Item::Component(first) = &document.entries[0].item else {
        panic!("a known component");
    };
    assert_eq!(
        first.get("title"),
        Some(&Setting::Text("In and out".into()))
    );
    // What is written reads back to the same document, and writing again
    // gives the same text.
    let written = document.to_json();
    assert_eq!(Document::read(&written).unwrap(), document);
    assert_eq!(Document::read(&written).unwrap().to_json(), written);
}

#[test]
fn two_equal_keys_are_refused_wherever_they_are() {
    for (text, key) in [
        (r#"{"format":"a","format":"b"}"#, "format"),
        (
            r#"{"format":"kassensturz-dashboard","version":1,"kind":"dashboard","name":"A","name":"B","components":[]}"#,
            "name",
        ),
        // Inside a component, in a list, behind an escape.
        (
            r#"{"format":"kassensturz-dashboard","version":1,"kind":"dashboard","name":"A","components":[{"id":"c1","type":"chart.pie","dataset":"spending_by_category","props":{"title":"x","title":"y"}}]}"#,
            "title",
        ),
        (
            r#"{"format":"kassensturz-dashboard","version":1,"kind":"dashboard","name":"A","name":"B","components":[]}"#,
            "name",
        ),
    ] {
        let (why, path) = reason(text);
        assert_eq!((why, path.as_str()), ("duplicate-key", key), "{text}");
    }
    // The same key in different objects is not a duplicate, and a key's text
    // inside a string value is not a key.
    let fine = with(|value| {
        value["components"][1]["props"] = json!({"title": "title"});
    });
    Document::read(&fine).unwrap();
}

#[test]
fn nesting_size_and_counts_have_limits() {
    let deep = format!(
        r#"{{"format":"kassensturz-dashboard","version":1,"kind":"dashboard","name":"A","components":[],"x":{}1{}}}"#,
        "[".repeat(8),
        "]".repeat(8)
    );
    assert_eq!(reason(&deep).0, "too-deep");
    let many = with(|value| {
        let one = value["components"][1].clone();
        value["components"] = (0..=MAX_COMPONENTS)
            .map(|index| {
                let mut component = one.clone();
                component["id"] = format!("c{index}").into();
                component
            })
            .collect();
    });
    assert_eq!(reason(&many).0, "too-many-components");
    let big = with(|value| value["name"] = "x".repeat(MAX_BYTES).into());
    match Document::read(&big).unwrap_err() {
        ReportError::DocumentTooBig { max_bytes } => assert_eq!(max_bytes, MAX_BYTES),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_header_is_checked() {
    for (patch, why, path) in [
        (json!({"format": "other"}), "wrong-format", "format"),
        (json!({"version": 0}), "bad-version", "version"),
        (json!({"version": "1"}), "bad-version", "version"),
        (json!({"kind": "page"}), "bad-kind", "kind"),
        (json!({"name": ""}), "bad-name", "name"),
        (json!({"name": " padded"}), "bad-name", "name"),
        (json!({"name": "x".repeat(201)}), "bad-name", "name"),
        (json!({"extra": 1}), "unknown-property", "extra"),
        (
            json!({"filters": {"period": "forever"}}),
            "bad-choice",
            "filters.period",
        ),
        (
            json!({"filters": {"period": "2025-01-01..2000-01-01"}}),
            "bad-choice",
            "filters.period",
        ),
        (
            json!({"filters": {"url": "x"}}),
            "unknown-property",
            "filters.url",
        ),
        (json!({"components": {}}), "wrong-type", "components"),
    ] {
        let text = with(|value| {
            for (key, new) in patch.as_object().unwrap() {
                value[key] = new.clone();
            }
        });
        let (got, got_path) = reason(&text);
        assert_eq!((got, got_path.as_str()), (why, path), "{patch}");
    }
    assert_eq!(reason("{").0, "not-json");
    assert_eq!(reason("[]").0, "not-an-object");
}

#[test]
fn a_newer_version_is_refused_by_its_own_code() {
    let text = with(|value| value["version"] = (VERSION + 1).into());
    let error = Document::read(&text).unwrap_err();
    assert_eq!(error.report().code, "report.unsupported-document-version");
}

#[test]
fn components_are_checked_with_their_path() {
    for (patch, why, path) in [
        (
            json!({"id": "has space", "type": "chart.pie", "dataset": "spending_by_category"}),
            "bad-id",
            "components.1.id",
        ),
        (
            json!({"id": "c1", "type": "chart.pie", "dataset": "spending_by_category"}),
            "duplicate-id",
            "components.1.id",
        ),
        (
            json!({"id": "c3", "type": "chart.pie", "dataset": "income_expense_by_month"}),
            "dataset-not-suited",
            "dataset",
        ),
        (
            json!({"id": "c3", "type": "chart.pie", "dataset": "spending_by_category", "props": {"onclick": "x"}}),
            "unknown-property",
            "props.onclick",
        ),
        (
            json!({"id": "c3", "type": "chart.pie", "dataset": "spending_by_category", "frame": {"col": 0, "row": 0, "cols": 0, "rows": 1}}),
            "bad-frame",
            "components.1.frame.cols",
        ),
        (
            json!({"id": "c3", "type": "chart.pie", "dataset": "spending_by_category", "frame": {"col": 0, "row": 0, "cols": 1}}),
            "wrong-type",
            "components.1.frame.rows",
        ),
    ] {
        let text = with(|value| value["components"][1] = patch.clone());
        let (got, got_path) = reason(&text);
        assert_eq!((got, got_path.as_str()), (why, path), "{patch}");
    }
}

#[test]
fn a_component_of_an_unknown_type_is_kept_and_the_rest_opens() {
    let text = with(|value| {
        value["components"][1] = json!({"id": "c9", "type": "chart.radar", "dataset": "x", "props": {"a": 1},
                "frame": {"col": 6, "row": 0, "cols": 6, "rows": 4}});
    });
    let document = Document::read(&text).unwrap();
    assert!(matches!(document.entries[0].item, Item::Component(_)));
    assert!(matches!(document.entries[1].item, Item::Unknown(_)));
    // Saving the document again keeps what this version can't show.
    let again = Document::read(&document.to_json()).unwrap();
    assert_eq!(again, document);
}

#[test]
fn a_view_has_one_component_and_no_frame() {
    let view = |components: serde_json::Value| {
        with(|value| {
            value["kind"] = "view".into();
            value["components"] = components;
        })
    };
    let one = json!([{"id": "c1", "type": "chart.pie", "dataset": "spending_by_category"}]);
    assert_eq!(Document::read(&view(one)).unwrap().kind, ViewKind::View);
    assert_eq!(reason(&view(json!([]))).0, "view-needs-one-component");
    let framed = json!([{"id": "c1", "type": "chart.pie", "dataset": "spending_by_category",
        "frame": {"col": 0, "row": 0, "cols": 1, "rows": 1}}]);
    assert_eq!(reason(&view(framed)).0, "view-has-frame");
}

#[test]
fn the_book_keeps_what_is_written_and_the_reader_catches_what_the_book_cannot() {
    let mut book = Book::create_in_memory().unwrap();
    let mut document = Document::read(DASHBOARD).unwrap();
    document.kind = ViewKind::View;
    document.entries.truncate(1);
    document.entries[0].frame = None;
    let id = book.add_view(&document.to_json()).unwrap();
    let stored = book.view(id).unwrap();
    assert_eq!(Document::read(&stored.document).unwrap(), document);
    // The book checks only its header, so it keeps a document with two equal
    // keys in a component; reading it is where that is refused.
    let hostile = r#"{"format":"kassensturz-dashboard","version":1,"kind":"view","name":"Hostile","components":[{"id":"c1","type":"chart.pie","dataset":"spending_by_category","props":{"title":"a","title":"b"}}]}"#;
    let id = book.add_view(hostile).unwrap();
    assert_eq!(
        reason(&book.view(id).unwrap().document),
        ("duplicate-key", "title".to_owned())
    );
}
