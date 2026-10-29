// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use kassensturz_core::{ErrorParam, Report};
use serde_json::json;

use crate::*;

fn reason(error: ReportError) -> (&'static str, String) {
    match error {
        ReportError::DocumentInvalid { reason, path } => (reason, path),
        other => panic!("not a document error: {other:?}"),
    }
}

#[test]
fn ids_round_trip_and_every_dataset_has_a_component() {
    for kind in ComponentType::ALL {
        assert_eq!(ComponentType::from_id(kind.id()), Some(kind));
    }
    assert_eq!(ComponentType::from_id("chart.radar"), None);
    for dataset in CATALOG {
        assert!(
            ComponentType::ALL
                .into_iter()
                .any(|kind| kind.datasets().any(|d| d.id == dataset.id)),
            "{} suits no component",
            dataset.id
        );
    }
}

#[test]
fn every_component_has_a_title_and_unique_keys() {
    for kind in ComponentType::ALL {
        let keys: Vec<&str> = kind.properties().iter().map(|p| p.key).collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), keys.len(), "{}", kind.id());
        assert!(keys.contains(&"title"), "{}", kind.id());
    }
}

#[test]
fn a_new_component_is_valid_for_every_dataset_it_suits() {
    for kind in ComponentType::ALL {
        for dataset in kind.datasets() {
            let component = Component::new(kind, dataset.id).unwrap();
            for (property, setting) in component.settings() {
                property.check(dataset, setting).unwrap();
            }
            // A document written from it reads back to the same component.
            let again = Component::from_value(&component.to_value()).unwrap();
            assert_eq!(again, component, "{} {}", kind.id(), dataset.id);
        }
    }
}

#[test]
fn defaults_are_the_first_choice_and_the_datasets_first_grouping() {
    let chart = Component::new(ComponentType::ChartPie, SPENDING_BY_CATEGORY).unwrap();
    assert_eq!(
        chart.get("grouping"),
        Some(&Setting::Word("category".into()))
    );
    assert_eq!(chart.get("legend"), Some(&Setting::Flag(true)));
    assert_eq!(chart.get("title"), Some(&Setting::Text(String::new())));
    let card = Component::new(ComponentType::KpiCard, INCOME_EXPENSE_BY_MONTH).unwrap();
    assert_eq!(card.get("measure"), Some(&Setting::Word("income".into())));
    assert_eq!(
        card.get("compare"),
        Some(&Setting::Word(Compare::PreviousPeriod.id().into()))
    );
}

#[test]
fn compare_choices_are_the_ids_the_reporting_layer_knows() {
    for kind in [ComponentType::KpiCard, ComponentType::ProgressBar] {
        let compare = kind
            .properties()
            .iter()
            .find(|p| p.key == "compare")
            .unwrap();
        let words = compare.words(dataset(INCOME_EXPENSE_BY_MONTH).unwrap());
        assert!(!words.is_empty());
        for word in words {
            assert!(Compare::from_id(word).is_some(), "{word}");
        }
    }
}

#[test]
fn a_dataset_that_does_not_suit_is_refused() {
    let (why, path) =
        reason(Component::new(ComponentType::ChartPie, INCOME_EXPENSE_BY_MONTH).unwrap_err());
    assert_eq!((why, path.as_str()), ("dataset-not-suited", "dataset"));
    let (why, _) = reason(Component::new(ComponentType::ChartPie, "no_such").unwrap_err());
    assert_eq!(why, "unknown-dataset");
}

#[test]
fn set_refuses_what_the_schema_does_not_allow_and_changes_nothing() {
    let mut chart = Component::new(ComponentType::ChartBar, SPENDING_BY_CATEGORY).unwrap();
    let before = chart.clone();
    for (key, setting, why) in [
        ("grouping", Setting::Word("month".into()), "bad-choice"),
        ("grouping", Setting::Flag(true), "wrong-type"),
        ("legend", Setting::Word("yes".into()), "wrong-type"),
        ("title", Setting::Text("x".repeat(201)), "too-long"),
        ("title", Setting::Text("two\nlines".into()), "bad-text"),
        ("script", Setting::Text("x".into()), "unknown-property"),
    ] {
        let (got, _) = reason(chart.set(key, setting).unwrap_err());
        assert_eq!(got, why, "{key}");
        assert_eq!(chart, before);
    }
    // 200 characters is the limit, counted as characters, not bytes.
    chart.set("title", Setting::Text("é".repeat(200))).unwrap();
    chart
        .set("grouping", Setting::Word("top_level_category".into()))
        .unwrap();
}

#[test]
fn changing_the_dataset_keeps_what_still_fits_and_resets_what_does_not() {
    let mut table = Component::new(ComponentType::TableRows, SPENDING_BY_CATEGORY).unwrap();
    table.set("title", Setting::Text("Mine".into())).unwrap();
    table
        .set("grouping", Setting::Word("top_level_category".into()))
        .unwrap();
    table.use_dataset(INCOME_EXPENSE_BY_MONTH).unwrap();
    assert_eq!(table.get("title"), Some(&Setting::Text("Mine".into())));
    assert_eq!(table.get("grouping"), Some(&Setting::Word("month".into())));
    assert_eq!(table.dataset().id, INCOME_EXPENSE_BY_MONTH);
    // A refused change leaves the component as it was.
    let before = table.clone();
    let mut pie = Component::new(ComponentType::ChartPie, SPENDING_BY_CATEGORY).unwrap();
    assert!(pie.use_dataset(INCOME_EXPENSE_BY_MONTH).is_err());
    assert_eq!(pie.dataset().id, SPENDING_BY_CATEGORY);
    assert_eq!(table, before);
}

#[test]
fn a_document_component_is_read_strictly() {
    let ok = json!({
        "id": "c1", "type": "chart.donut", "dataset": "spending_by_category",
        "params": {"grouping": "top_level_category"},
        "props": {"title": "Where it went", "legend": false},
        "frame": {"col": 0, "row": 0, "cols": 6, "rows": 4}
    });
    let component = Component::from_value(&ok).unwrap();
    assert_eq!(component.kind(), ComponentType::ChartDonut);
    assert_eq!(component.get("legend"), Some(&Setting::Flag(false)));

    for (value, why, path) in [
        (json!([]), "wrong-type", "component"),
        (
            json!({"type": "chart.radar", "dataset": "spending_by_category"}),
            "unknown-type",
            "type",
        ),
        (json!({"type": "chart.pie"}), "wrong-type", "dataset"),
        (
            json!({"type": "chart.pie", "dataset": "spending_by_category", "url": "https://x"}),
            "unknown-property",
            "url",
        ),
        (
            json!({"type": "chart.pie", "dataset": "spending_by_category", "props": {"onclick": "x"}}),
            "unknown-property",
            "props.onclick",
        ),
        // A property in the wrong place is not the property.
        (
            json!({"type": "chart.pie", "dataset": "spending_by_category", "params": {"title": "x"}}),
            "unknown-property",
            "params.title",
        ),
        (
            json!({"type": "chart.pie", "dataset": "spending_by_category", "props": {"legend": "yes"}}),
            "wrong-type",
            "props.legend",
        ),
        (
            json!({"type": "chart.pie", "dataset": "spending_by_category", "params": {"grouping": "month"}}),
            "bad-choice",
            "params.grouping",
        ),
        (
            json!({"type": "chart.pie", "dataset": "spending_by_category", "props": 3}),
            "wrong-type",
            "props",
        ),
    ] {
        let (got, got_path) = reason(Component::from_value(&value).unwrap_err());
        assert_eq!((got, got_path.as_str()), (why, path), "{value}");
    }
}

#[test]
fn the_error_report_names_the_path_as_text() {
    let report = ReportError::DocumentInvalid {
        reason: "bad-choice",
        path: "params.grouping".into(),
    }
    .report();
    assert_eq!(report.code, "report.document-invalid");
    assert_eq!(
        report.param("path"),
        Some(&ErrorParam::Text("params.grouping".into()))
    );
}

#[test]
fn text_that_is_not_json_is_a_wrong_type_not_a_panic() {
    for text in ["", "{", "null", "[1]", "\"chart.pie\""] {
        let (why, path) = reason(Component::from_json(text).unwrap_err());
        assert_eq!((why, path.as_str()), ("wrong-type", "component"), "{text}");
    }
}

#[test]
fn changing_the_type_keeps_the_title_and_what_still_fits() {
    let mut chart = Component::new(ComponentType::ChartBar, SPENDING_BY_CATEGORY).unwrap();
    chart.set("title", Setting::Text("Mine".into())).unwrap();
    chart.set("legend", Setting::Flag(false)).unwrap();
    chart
        .set("grouping", Setting::Word("top_level_category".into()))
        .unwrap();
    // A pie shows the same dataset: everything stays.
    chart.change_kind(ComponentType::ChartPie).unwrap();
    assert_eq!(chart.kind(), ComponentType::ChartPie);
    assert_eq!(chart.dataset().id, SPENDING_BY_CATEGORY);
    assert_eq!(chart.get("title"), Some(&Setting::Text("Mine".into())));
    assert_eq!(chart.get("legend"), Some(&Setting::Flag(false)));
    assert_eq!(
        chart.get("grouping"),
        Some(&Setting::Word("top_level_category".into()))
    );
    // A line can't show spending: it takes the monthly dataset, the title
    // stays and the grouping, which that dataset doesn't have, resets.
    chart.change_kind(ComponentType::ChartLine).unwrap();
    assert_eq!(chart.dataset().id, INCOME_EXPENSE_BY_MONTH);
    assert_eq!(chart.get("title"), Some(&Setting::Text("Mine".into())));
    assert_eq!(chart.get("grouping"), Some(&Setting::Word("month".into())));
    // A key figure has no legend, and a new component of its type is valid.
    chart.change_kind(ComponentType::KpiCard).unwrap();
    assert_eq!(chart.get("legend"), None);
    assert_eq!(chart.get("measure"), Some(&Setting::Word("income".into())));
    for (property, setting) in chart.settings() {
        property.check(chart.dataset(), setting).unwrap();
    }
    assert_eq!(Component::from_value(&chart.to_value()).unwrap(), chart);
}
