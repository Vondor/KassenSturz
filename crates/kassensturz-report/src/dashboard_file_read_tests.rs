// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of reading a dashboard file (M6d.4, D-441, D-442, T-51, T-53):
//! names looked up in a real synthetic book made through the store, and
//! hostile files, each refused whole with its reason.

use kassensturz_core::{GroupKind, GroupMember};
use proptest::prelude::*;
use serde_json::{Value, json};

use super::tests::{EUR, account, sample};
use super::*;

/// A file with the header of version 1, `references` and `dashboard`.
fn file(references: Value, dashboard: Value) -> String {
    json!({
        "format": "kassensturz-dashboard-file",
        "version": 1,
        "application": "KassenSturz 0.261101.8",
        "references": references,
        "dashboard": dashboard,
    })
    .to_string()
}

/// A dashboard of the format, with a filter bar and these components.
fn dashboard(filters: Value, components: Value) -> Value {
    json!({
        "format": "kassensturz-dashboard",
        "version": 2,
        "kind": "dashboard",
        "name": "Where the money goes",
        "filters": filters,
        "components": components,
    })
}

fn stacked(id: &str, title: &str, series: &[&str], row: u32) -> Value {
    json!({
        "id": id,
        "type": "chart.stacked_bar",
        "dataset": "category_over_time",
        "params": {"series": series},
        "props": {"title": title},
        "frame": {"col": 0, "row": row, "cols": 12, "rows": 4},
    })
}

/// The design's example (section 4.1): a category, a group and an account.
fn example() -> String {
    file(
        json!({
            "ref:1": {"kind": "category", "path": ["Food", "Groceries"]},
            "ref:2": {"kind": "group", "name": "Fixed costs"},
            "ref:3": {"kind": "account", "name": "Checking account"},
        }),
        dashboard(
            json!({"period": "last-12-months", "accounts": ["ref:3"]}),
            json!([stacked(
                "c1",
                "Groceries and fixed costs",
                &["ref:1", "ref:2"],
                0
            )]),
        ),
    )
}

fn refused(text: &str) -> (&'static str, String) {
    match IncomingDashboardFile::read(text) {
        Err(ReportError::DashboardFileInvalid { reason, path }) => (reason, path),
        other => panic!("not refused as a dashboard file: {other:?}"),
    }
}

/// The series of the first chart: a version 2 dashboard gets the period
/// header and the filter bar in front of it when read (D-476).
fn series_of(document: &Document) -> Vec<String> {
    let Some(component) = document.entries.iter().find_map(|entry| match &entry.item {
        Item::Component(component) if !component.kind().one_per_page() => Some(component),
        _ => None,
    }) else {
        panic!("a known component");
    };
    match component.get("series") {
        Some(Setting::Ids(ids)) => ids.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_file_finds_its_names_in_the_book_and_gets_the_books_ids() {
    let mut sample = sample();
    let fixed = sample
        .book
        .add_category_group(
            "Fixed costs",
            GroupKind::Expense,
            &[GroupMember::Category(sample.rent)],
        )
        .expect("group added");
    let read = IncomingDashboardFile::read(&example()).expect("read");
    assert_eq!(read.name(), "Where the money goes");
    assert_eq!(read.application.as_deref(), Some("KassenSturz 0.261101.8"));
    let names = BookNames::read(&sample.book).expect("names");
    let resolved = read.resolve(&names).expect("resolved");
    assert_eq!(
        resolved.found,
        [
            Reference::Category(vec!["Food".into(), "Groceries".into()]),
            Reference::Group("Fixed costs".into()),
            Reference::Account("Checking account".into()),
        ]
    );
    assert!(resolved.missing.is_empty());
    assert!(resolved.shows_more.is_empty());
    assert!(resolved.formula_names.is_empty());
    assert!(resolved.unknown.is_empty());
    assert_eq!(
        series_of(&resolved.document),
        [sample.groceries.to_string(), fixed.to_string()]
    );
    assert_eq!(resolved.document.accounts, [sample.checking.to_string()]);
    assert_eq!(resolved.document.period.as_deref(), Some("last-12-months"));
    // What would be stored reads back as the same document.
    let stored = resolved.document.to_json();
    assert_eq!(
        Document::read(&stored).expect("read back"),
        resolved.document
    );
}

#[test]
fn names_the_book_lacks_are_left_out_and_listed_with_their_widgets() {
    // The sample book has no group "Fixed costs" and no "Colony Checking".
    let sample = sample();
    let text = file(
        json!({
            "ref:1": {"kind": "category", "path": ["Food", "Groceries"]},
            "ref:2": {"kind": "group", "name": "Fixed costs"},
            "ref:3": {"kind": "account", "name": "Colony Checking"},
        }),
        dashboard(
            json!({"accounts": ["ref:3"]}),
            json!([
                stacked("c1", "Groceries and fixed costs", &["ref:1", "ref:2"], 0),
                stacked("c2", "Fixed costs", &["ref:2"], 4),
            ]),
        ),
    );
    let names = BookNames::read(&sample.book).expect("names");
    let resolved = IncomingDashboardFile::read(&text)
        .expect("read")
        .resolve(&names)
        .expect("resolved");
    let widget = |id: &str, title: &str| Widget {
        id: id.into(),
        kind: "chart.stacked_bar".into(),
        title: title.into(),
    };
    assert_eq!(
        resolved.missing,
        [
            MissingName {
                reference: Reference::Group("Fixed costs".into()),
                several: false,
                filter_bar: false,
                widgets: vec![
                    widget("c1", "Groceries and fixed costs"),
                    widget("c2", "Fixed costs")
                ],
            },
            MissingName {
                reference: Reference::Account("Colony Checking".into()),
                several: false,
                filter_bar: true,
                widgets: vec![],
            },
        ]
    );
    // The filter bar and c2 named things, none of which are here: they now
    // narrow nothing and show more. c1 keeps Groceries.
    assert_eq!(
        resolved.shows_more,
        [
            ShowsMore {
                widget: None,
                list: "accounts",
                named: 1,
            },
            ShowsMore {
                widget: Some(widget("c2", "Fixed costs")),
                list: "series",
                named: 1,
            },
        ]
    );
    assert_eq!(
        series_of(&resolved.document),
        [sample.groceries.to_string()]
    );
    assert!(resolved.document.accounts.is_empty());
}

#[test]
fn a_category_is_found_only_by_its_whole_path_ignoring_capitals() {
    let sample = sample();
    let names = BookNames::read(&sample.book).expect("names");
    let resolve = |path: Value| {
        let text = file(
            json!({"ref:1": {"kind": "category", "path": path}}),
            dashboard(json!({}), json!([stacked("c1", "", &["ref:1"], 0)])),
        );
        IncomingDashboardFile::read(&text)
            .expect("read")
            .resolve(&names)
            .expect("resolved")
    };
    // Its last name alone is not enough: never guessed.
    assert_eq!(resolve(json!(["Groceries"])).missing.len(), 1);
    assert_eq!(resolve(json!(["Groceries", "Food"])).missing.len(), 1);
    let found = resolve(json!(["FOOD", "groceries"]));
    assert!(found.missing.is_empty());
    assert_eq!(series_of(&found.document), [sample.groceries.to_string()]);
}

#[test]
fn an_account_is_found_open_or_closed_and_two_names_for_one_thing_count_once() {
    let mut sample = sample();
    let old = account(
        &mut sample.book,
        "Old savings",
        kassensturz_core::AccountKind::Asset,
        EUR,
        None,
    );
    sample.book.close_account(old).expect("closed");
    let names = BookNames::read(&sample.book).expect("names");
    let text = file(
        json!({
            "ref:1": {"kind": "account", "name": "old SAVINGS"},
            "ref:2": {"kind": "account", "name": "Checking account"},
            "ref:3": {"kind": "account", "name": "CHECKING ACCOUNT"},
        }),
        dashboard(json!({"accounts": ["ref:1", "ref:2", "ref:3"]}), json!([])),
    );
    let resolved = IncomingDashboardFile::read(&text)
        .expect("read")
        .resolve(&names)
        .expect("resolved");
    assert!(resolved.missing.is_empty());
    assert_eq!(
        resolved.document.accounts,
        [old.to_string(), sample.checking.to_string()]
    );
}

#[test]
fn formula_names_the_book_lacks_and_unknown_widgets_are_listed() {
    let sample = sample();
    let names = BookNames::read(&sample.book).expect("names");
    let text = file(
        json!({}),
        dashboard(
            json!({}),
            json!([
                {
                    "id": "k1", "type": "kpi.card", "dataset": "income_expense_by_month",
                    "params": {"measure": "formula"},
                    "props": {"formula": "SUM(Food; Pets; 'Food > Groceries'; Rent)", "title": "Mine"},
                    "frame": {"col": 0, "row": 0, "cols": 3, "rows": 2},
                },
                {
                    "id": "x1", "type": "chart.from_the_future",
                    "props": {"title": "Later"},
                    "frame": {"col": 3, "row": 0, "cols": 3, "rows": 2},
                },
            ]),
        ),
    );
    let resolved = IncomingDashboardFile::read(&text)
        .expect("read")
        .resolve(&names)
        .expect("resolved");
    assert_eq!(
        resolved.formula_names,
        [FormulaName {
            widget: Widget {
                id: "k1".into(),
                kind: "kpi.card".into(),
                title: "Mine".into(),
            },
            name: "Pets".into(),
        }]
    );
    assert_eq!(
        resolved.unknown,
        [Widget {
            id: "x1".into(),
            kind: "chart.from_the_future".into(),
            title: "Later".into(),
        }]
    );
}

#[test]
fn a_file_without_references_reads_and_the_built_in_overview_fits_in_one() {
    let overview: Value = serde_json::from_str(DEFAULT_OVERVIEW).expect("json");
    let read = IncomingDashboardFile::read(&file(json!({}), overview)).expect("read");
    assert_eq!(read.name(), OVERVIEW_NAME);
    let mut text: Value = serde_json::from_str(&example()).expect("json");
    let object = text.as_object_mut().expect("object");
    object.remove("application");
    // Without references nothing can be named.
    object.remove("references");
    assert_eq!(
        refused(&text.to_string()),
        ("bad-reference", "dashboard.filters.accounts.0".into())
    );
}

#[test]
fn a_taken_name_is_offered_with_the_next_free_number() {
    let taken = [
        "Overview".to_owned(),
        "Spending".into(),
        "spending (2)".into(),
    ];
    assert_eq!(free_dashboard_name("Travel", &taken), "Travel");
    assert_eq!(free_dashboard_name("SPENDING", &taken), "SPENDING (3)");
    let long = "a".repeat(200);
    let free = free_dashboard_name(&long, std::slice::from_ref(&long));
    assert_eq!(free.chars().count(), 200);
    assert!(free.ends_with(" (2)"));
}

/// The example with one change made to its JSON.
fn changed(change: impl FnOnce(&mut Value)) -> String {
    let mut value: Value = serde_json::from_str(&example()).expect("json");
    change(&mut value);
    value.to_string()
}

#[test]
fn hostile_and_broken_files_are_refused_whole_with_the_reason() {
    let uuid = "0190d2a4-7a8c-7cc3-9d2e-6f1a2b3c4d5e";
    let cases: Vec<(String, &str, &str)> = vec![
        ("not json".into(), "not-json", ""),
        ("[1, 2]".into(), "not-an-object", ""),
        (
            example().replacen('{', "{\"version\":1,", 1),
            "duplicate-key",
            "version",
        ),
        (
            changed(|v| v["format"] = json!("kassensturz-dashboard")),
            "wrong-format",
            "format",
        ),
        (
            changed(|v| v["version"] = json!(0)),
            "bad-version",
            "version",
        ),
        (
            changed(|v| v["version"] = json!("1")),
            "bad-version",
            "version",
        ),
        (
            changed(|v| v["extra"] = json!(true)),
            "unknown-property",
            "extra",
        ),
        (
            changed(|v| v["application"] = json!("x".repeat(101))),
            "too-long",
            "application",
        ),
        (
            changed(|v| v["application"] = json!("a\nb")),
            "bad-text",
            "application",
        ),
        (
            changed(|v| v["dashboard"]["filters"]["accounts"][0] = json!(uuid)),
            "id-in-file",
            "dashboard.filters.accounts.0",
        ),
        (
            changed(|v| v["dashboard"]["components"][0]["params"]["series"][1] = json!(uuid)),
            "id-in-file",
            "dashboard.components.0.params.series.1",
        ),
        (
            changed(|v| v["dashboard"]["components"][0]["params"]["series"][1] = json!(7)),
            "bad-reference",
            "dashboard.components.0.params.series.1",
        ),
        (
            changed(|v| v["dashboard"]["filters"]["accounts"][0] = json!("ref:9")),
            "bad-reference",
            "dashboard.filters.accounts.0",
        ),
        (
            changed(|v| v["dashboard"]["filters"]["accounts"][0] = json!("ref:03")),
            "bad-reference",
            "dashboard.filters.accounts.0",
        ),
        // An account where categories belong, and the other way round.
        (
            changed(|v| v["dashboard"]["components"][0]["params"]["series"][0] = json!("ref:3")),
            "bad-reference",
            "dashboard.components.0.params.series.0",
        ),
        (
            changed(|v| v["dashboard"]["filters"]["accounts"][0] = json!("ref:1")),
            "bad-reference",
            "dashboard.filters.accounts.0",
        ),
        (
            changed(|v| v["references"]["ref:4"] = json!({"kind": "group", "name": "Unused"})),
            "unused-reference",
            "references.ref:4",
        ),
        (
            changed(|v| v["references"]["ref:0"] = json!({"kind": "group", "name": "Zero"})),
            "bad-reference",
            "references.ref:0",
        ),
        (
            changed(|v| v["references"]["ref:2"] = json!({"kind": "payee", "name": "Shop"})),
            "bad-reference",
            "references.ref:2.kind",
        ),
        (
            changed(|v| v["references"]["ref:2"]["id"] = json!(uuid)),
            "unknown-property",
            "references.ref:2.id",
        ),
        (
            changed(|v| v["references"]["ref:1"]["path"] = json!([])),
            "bad-reference",
            "references.ref:1.path",
        ),
        (
            changed(|v| v["references"]["ref:1"]["path"] = json!(vec!["a"; 17])),
            "path-too-long",
            "references.ref:1.path",
        ),
        (
            changed(|v| v["references"]["ref:2"]["name"] = json!("")),
            "bad-text",
            "references.ref:2.name",
        ),
        (
            changed(|v| v["references"]["ref:2"]["name"] = json!(" Fixed costs")),
            "bad-text",
            "references.ref:2.name",
        ),
        (
            changed(|v| v["references"]["ref:1"]["path"][1] = json!("Gro\u{7}ceries")),
            "bad-text",
            "references.ref:1.path.1",
        ),
        (
            changed(|v| v["references"]["ref:3"]["name"] = json!("n".repeat(201))),
            "too-long",
            "references.ref:3.name",
        ),
        (
            changed(|v| v["references"] = json!(["ref:1"])),
            "bad-reference",
            "references",
        ),
        (
            changed(|v| {
                v.as_object_mut().expect("object").remove("dashboard");
            }),
            "not-an-object",
            "dashboard",
        ),
        // A file can't change which dashboard opens (D-448).
        (
            changed(|v| v["dashboard"]["default"] = json!(true)),
            "unknown-property",
            "dashboard.default",
        ),
    ];
    for (text, reason, path) in cases {
        assert_eq!(refused(&text), (reason, path.to_owned()), "{text}");
    }
}

#[test]
fn too_many_references_and_too_deep_files_are_refused() {
    let many = changed(|v| {
        let references = v["references"].as_object_mut().expect("object");
        for number in 4..=501 {
            references.insert(
                format!("ref:{number}"),
                json!({"kind": "group", "name": format!("G{number}")}),
            );
        }
    });
    assert_eq!(
        refused(&many),
        ("too-many-references", "references".to_owned())
    );
    let deep = changed(|v| {
        v["dashboard"]["components"][0]["props"]["title"] = json!([[[[[[[[1]]]]]]]]);
    });
    assert_eq!(refused(&deep), ("too-deep", String::new()));
}

#[test]
fn the_dashboard_inside_is_checked_as_a_document_with_paths_from_the_top() {
    let wrong = changed(|v| v["dashboard"]["components"][0]["params"]["colour"] = json!("red"));
    assert!(matches!(
        IncomingDashboardFile::read(&wrong),
        Err(ReportError::DocumentInvalid { reason: "unknown-property", path })
            if path.starts_with("dashboard.") && path.ends_with("params.colour")
    ));
    let view = changed(|v| v["dashboard"]["kind"] = json!("view"));
    assert!(matches!(
        IncomingDashboardFile::read(&view),
        Err(ReportError::DocumentInvalid { path, .. }) if path.starts_with("dashboard")
    ));
}

#[test]
fn a_file_too_big_or_from_a_newer_version_says_so() {
    let big = format!("{}{}", " ".repeat(MAX_DASHBOARD_FILE_BYTES), example());
    assert!(matches!(
        IncomingDashboardFile::read(&big),
        Err(ReportError::DashboardFileTooBig {
            max_bytes: MAX_DASHBOARD_FILE_BYTES
        })
    ));
    // A newer file may have keys this one doesn't know: it says it's newer.
    let newer = changed(|v| {
        v["version"] = json!(2);
        v["signature"] = json!("abc");
    });
    assert!(matches!(
        IncomingDashboardFile::read(&newer),
        Err(ReportError::UnsupportedDashboardFileVersion { version: 2 })
    ));
}

#[test]
fn every_refusal_has_a_listed_error_code() {
    use kassensturz_core::Report;
    for text in [
        "x".to_owned(),
        changed(|v| v["version"] = json!(9)),
        " ".repeat(MAX_DASHBOARD_FILE_BYTES + 1),
    ] {
        let error = IncomingDashboardFile::read(&text).expect_err("refused");
        assert!(
            error.report().code.contains("dashboard-file"),
            "{}",
            error.report().code
        );
    }
}

proptest! {
    /// However a file is cut or changed, reading it never panics, and a file
    /// it accepts gives a document the book can read back.
    #[test]
    fn a_broken_file_never_panics(cut in 0usize..2000, at in 0usize..2000, byte in 0u8..128) {
        let text = example();
        let cut = cut.min(text.len());
        let mut bytes = text.as_bytes()[..cut].to_vec();
        if !bytes.is_empty() {
            let at = at % bytes.len();
            bytes[at] = byte;
        }
        if let Ok(text) = String::from_utf8(bytes)
            && let Ok(read) = IncomingDashboardFile::read(&text)
        {
            let resolved = read.resolve(&BookNames::default()).expect("resolved");
            prop_assert!(Document::read(&resolved.document.to_json()).is_ok());
        }
    }
}
