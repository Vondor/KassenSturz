// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `chart.custom` component (M6e.3, D-504, D-505, D-508). Every
//! description in this file is written by hand (D-280).

use serde_json::json;

use crate::spec::{self, SPEC_DATASETS};
use crate::*;

fn reason(error: ReportError) -> (&'static str, String) {
    match error {
        ReportError::DocumentInvalid { reason, path } => (reason, path),
        other => panic!("not a document error: {other:?}"),
    }
}

fn text(component: &Component, key: &str) -> String {
    match component.get(key) {
        Some(Setting::Text(text)) => text.clone(),
        other => panic!("{key} is not text: {other:?}"),
    }
}

fn ids(component: &Component, key: &str) -> Vec<String> {
    match component.get(key) {
        Some(Setting::Ids(ids)) => ids.clone(),
        other => panic!("{key} is not a list: {other:?}"),
    }
}

/// A bar chart of the monthly dataset, written for these tests.
const MONTHLY: &str = r#"{"data":{"name":"income_expense_by_month"},"mark":"bar",
  "encoding":{"x":{"field":"month","type":"temporal","timeUnit":"yearmonth"},
  "y":{"field":"income","type":"quantitative"}}}"#;

/// A line chart over time with a color per series.
const OVER_TIME: &str = r#"{"data":{"name":"category_over_time"},"mark":"line",
  "encoding":{"x":{"field":"month","type":"temporal","timeUnit":"yearmonth"},
  "y":{"field":"total","type":"quantitative"},
  "color":{"field":"series","type":"nominal"}}}"#;

/// A description with a part this version does not read (`transform`).
const WITH_UNSUPPORTED: &str = r#"{"data":{"name":"spending_by_category"},"mark":"bar",
  "transform":[{"calculate":"datum.total * 2","as":"double"}],
  "encoding":{"x":{"field":"category","type":"nominal"},
  "y":{"field":"total","type":"quantitative"}}}"#;

fn custom(dataset: &str) -> Component {
    Component::new(ComponentType::CustomChart, dataset).unwrap()
}

#[test]
fn it_is_in_the_library_and_never_offered_to_the_ai() {
    let kind = ComponentType::CustomChart;
    assert_eq!(kind.id(), "chart.custom");
    assert_eq!(ComponentType::from_id("chart.custom"), Some(kind));
    assert!(ComponentType::LIBRARY.contains(&kind));
    // Q-352 = A: the AI neither writes nor sees a description.
    assert!(!ComponentType::ALL.contains(&kind));
    assert!(!kind.shows_no_data());
    let keys: Vec<&str> = kind.properties().iter().map(|p| p.key).collect();
    assert_eq!(
        keys,
        [
            "spec",
            "partial",
            "series",
            "categories",
            "hide_note",
            "period"
        ]
    );
    // It can show every dataset a description can name, no more and no less.
    let offered: Vec<&str> = kind.datasets().map(|dataset| dataset.id).collect();
    let named: Vec<&str> = SPEC_DATASETS.iter().map(|dataset| dataset.id()).collect();
    assert_eq!(offered.len(), named.len());
    for id in &named {
        assert!(offered.contains(id), "{id}");
    }
}

#[test]
fn a_new_chart_starts_with_a_description_the_parser_reads_in_full() {
    for dataset in SPEC_DATASETS {
        let component = custom(dataset.id());
        assert_eq!(component.dataset().id, dataset.id());
        let spec_text = text(&component, "spec");
        let read = spec::read(&spec_text).unwrap();
        assert!(read.unsupported().is_empty(), "{}", dataset.id());
        assert_eq!(
            read.chart().expect("a chart").dataset().id(),
            dataset.id(),
            "the starter names its own dataset"
        );
        assert_eq!(component.get("partial"), Some(&Setting::Flag(false)));
        assert_eq!(
            Component::from_value(&component.to_value()).unwrap(),
            component
        );
    }
}

#[test]
fn the_description_is_stored_exactly_as_it_was_written() {
    let mut chart = custom("spending_by_category");
    chart
        .set("spec", Setting::Text(MONTHLY.to_owned()))
        .unwrap();
    assert_eq!(text(&chart, "spec"), MONTHLY);
    let value = chart.to_value();
    assert_eq!(value["props"]["spec"], json!(MONTHLY));
    assert_eq!(Component::from_value(&value).unwrap(), chart);
}

#[test]
fn saving_a_description_changes_the_dataset_and_drops_the_choices_it_does_not_take() {
    let mut chart = custom("spending_by_category");
    chart
        .set("spec", Setting::Text(OVER_TIME.to_owned()))
        .unwrap();
    assert_eq!(chart.dataset().id, CATEGORY_OVER_TIME);
    chart
        .set("series", Setting::Ids(vec!["a".into(), "b".into()]))
        .unwrap();
    assert_eq!(ids(&chart, "series"), ["a", "b"]);
    // The categories belong to the budgets, not to a chart over time.
    let refused = chart
        .set("categories", Setting::Ids(vec!["c".into()]))
        .unwrap_err();
    assert_eq!(reason(refused), ("bad-choice", "params.categories".into()));
    // Another dataset in the same edit takes the series away.
    chart
        .set("spec", Setting::Text(MONTHLY.to_owned()))
        .unwrap();
    assert_eq!(chart.dataset().id, INCOME_EXPENSE_BY_MONTH);
    assert!(ids(&chart, "series").is_empty());
    // And the document does not write what is not used.
    let params = &chart.to_value()["params"];
    assert!(params.as_object().unwrap().is_empty(), "{params}");
}

#[test]
fn the_budget_categories_are_kept_only_for_budgets() {
    let budgets = r#"{"data":{"name":"budget_utilization"},"mark":"bar",
      "encoding":{"x":{"field":"category","type":"nominal"},
      "y":{"field":"spent","type":"quantitative"}}}"#;
    let mut chart = custom("spending_by_category");
    chart
        .set("spec", Setting::Text(budgets.to_owned()))
        .unwrap();
    chart
        .set("categories", Setting::Ids(vec!["c".into()]))
        .unwrap();
    let value = chart.to_value();
    assert_eq!(value["params"], json!({"categories": ["c"]}));
    assert_eq!(Component::from_value(&value).unwrap(), chart);
    chart
        .set("spec", Setting::Text(MONTHLY.to_owned()))
        .unwrap();
    assert!(ids(&chart, "categories").is_empty());
}

#[test]
fn a_description_that_is_refused_is_not_kept() {
    let mut chart = custom("income_expense_by_month");
    let before = chart.clone();
    for bad in [
        "not json",
        "[]",
        r#"{"data":{"name":"nope"},"mark":"bar"}"#,
        // A repeated key is refused, so the checks and the drawing can never
        // see different descriptions (D-505).
        r#"{"mark":"bar","mark":"line"}"#,
    ] {
        let refused = chart
            .set("spec", Setting::Text(bad.to_owned()))
            .unwrap_err();
        assert_eq!(reason(refused), ("bad-spec", "props.spec".into()), "{bad}");
        assert_eq!(chart, before);
    }
    // Over 32 KiB is refused before anything is parsed.
    let big = format!(r#"{{"title":"{}"}}"#, "x".repeat(spec::MAX_SPEC_BYTES));
    assert!(chart.set("spec", Setting::Text(big)).is_err());
    assert_eq!(chart, before);
}

#[test]
fn unsupported_parts_are_stored_and_the_choice_to_leave_them_out_is_kept() {
    let mut chart = custom("spending_by_category");
    chart
        .set("spec", Setting::Text(WITH_UNSUPPORTED.to_owned()))
        .unwrap();
    chart.set("partial", Setting::Flag(true)).unwrap();
    let read = spec::read(&text(&chart, "spec")).unwrap();
    assert_eq!(read.unsupported().len(), 1);
    assert_eq!(read.unsupported()[0].reason(), "expression");
    let again = Component::from_value(&chart.to_value()).unwrap();
    assert_eq!(again, chart);
    assert_eq!(again.get("partial"), Some(&Setting::Flag(true)));
}

#[test]
fn a_description_without_a_dataset_leaves_the_dataset_as_it_was() {
    let only_url = r#"{"data":{"url":"https://example.invalid/x.json"},"mark":"bar"}"#;
    let mut chart = custom("cash_flow_by_month");
    chart
        .set("spec", Setting::Text(only_url.to_owned()))
        .unwrap();
    assert_eq!(chart.dataset().id, CASH_FLOW_BY_MONTH);
    assert_eq!(text(&chart, "spec"), only_url);
}

#[test]
fn a_document_must_agree_with_its_description() {
    let component = |dataset: &str, spec: &str, params: serde_json::Value| {
        json!({"type": "chart.custom", "dataset": dataset,
               "params": params, "props": {"spec": spec, "partial": false}})
    };
    let ok =
        Component::from_value(&component("income_expense_by_month", MONTHLY, json!({}))).unwrap();
    assert_eq!(ok.dataset().id, INCOME_EXPENSE_BY_MONTH);
    // The dataset differs from the description's.
    let error =
        Component::from_value(&component("spending_by_category", MONTHLY, json!({}))).unwrap_err();
    assert_eq!(reason(error), ("spec-dataset", "dataset".into()));
    // The description is refused.
    let error =
        Component::from_value(&component("income_expense_by_month", "{", json!({}))).unwrap_err();
    assert_eq!(reason(error), ("bad-spec", "props.spec".into()));
    // A choice for a dataset the description doesn't name.
    let error = Component::from_value(&component(
        "income_expense_by_month",
        MONTHLY,
        json!({"series": ["a"]}),
    ))
    .unwrap_err();
    assert_eq!(reason(error), ("unknown-property", "params.series".into()));
    // Not a text.
    let error = Component::from_value(
        &json!({"type": "chart.custom", "dataset": "income_expense_by_month",
                "props": {"spec": {"data": {}}}}),
    )
    .unwrap_err();
    assert_eq!(reason(error), ("wrong-type", "props.spec".into()));
}

#[test]
fn the_dataset_follows_the_description_and_cannot_be_chosen_on_its_own() {
    let mut chart = custom("income_expense_by_month");
    chart.use_dataset("income_expense_by_month").unwrap();
    let error = chart.use_dataset("spending_by_category").unwrap_err();
    assert_eq!(reason(error), ("dataset-not-suited", "dataset".into()));
    assert_eq!(chart.dataset().id, INCOME_EXPENSE_BY_MONTH);
}

#[test]
fn changing_the_type_to_and_from_a_custom_chart_keeps_nothing_unused() {
    let mut bar = Component::new(ComponentType::ChartLine, CATEGORY_OVER_TIME).unwrap();
    bar.set("series", Setting::Ids(vec!["a".into()])).unwrap();
    bar.set("title", Setting::Text("Mine".into())).unwrap();
    // To a custom chart: it starts with a description of the same data.
    bar.change_kind(ComponentType::CustomChart).unwrap();
    assert_eq!(bar.dataset().id, CATEGORY_OVER_TIME);
    assert_eq!(ids(&bar, "series"), ["a"]);
    assert!(bar.get("title").is_none());
    // A chart that cannot show the dataset moves to one it can; the choice goes.
    let mut over_time = custom("category_over_time");
    over_time
        .set("series", Setting::Ids(vec!["a".into()]))
        .unwrap();
    over_time.change_kind(ComponentType::ChartBar).unwrap();
    assert_eq!(over_time.kind(), ComponentType::ChartBar);
    // And a plain chart becomes a custom chart of another dataset's choices.
    let mut pie = Component::new(ComponentType::ChartPie, SPENDING_BY_CATEGORY).unwrap();
    pie.set("categories", Setting::Ids(vec!["c".into()]))
        .unwrap();
    pie.change_kind(ComponentType::CustomChart).unwrap();
    assert!(ids(&pie, "categories").is_empty(), "spending takes none");
}

#[test]
fn the_error_of_unsupported_parts_has_its_count() {
    use kassensturz_core::{ErrorParam, Report};
    let report = ReportError::SpecUnsupported { count: 3 }.report();
    assert_eq!(report.code, "report.spec-unsupported");
    assert_eq!(report.param("count"), Some(&ErrorParam::Number(3)));
}

#[test]
fn a_new_description_takes_back_the_choice_to_leave_parts_out() {
    let mut chart = custom("spending_by_category");
    chart
        .set("spec", Setting::Text(WITH_UNSUPPORTED.to_owned()))
        .unwrap();
    chart.set("partial", Setting::Flag(true)).unwrap();
    // The same text again changes nothing, the choice stays.
    chart
        .set("spec", Setting::Text(WITH_UNSUPPORTED.to_owned()))
        .unwrap();
    assert_eq!(chart.get("partial"), Some(&Setting::Flag(true)));
    // Another text is drawn only as far as it fully says (D-508).
    chart
        .set("spec", Setting::Text(MONTHLY.to_owned()))
        .unwrap();
    assert_eq!(chart.get("partial"), Some(&Setting::Flag(false)));
}
