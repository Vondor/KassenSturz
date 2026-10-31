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

#[test]
fn groups_series_and_the_hidden_flag_round_trip_through_a_document() {
    let mut chart = Component::new(ComponentType::ChartLine, "category_over_time").unwrap();
    assert_eq!(chart.get("groups"), Some(&Setting::Ids(Vec::new())));
    assert_eq!(chart.get("show_hidden"), Some(&Setting::Flag(false)));
    let ids = vec!["0197-aaa".to_owned(), "0197-bbb".to_owned()];
    chart.set("series", Setting::Ids(ids.clone())).unwrap();
    chart.set("show_hidden", Setting::Flag(true)).unwrap();
    let value = chart.to_value();
    assert_eq!(value["params"]["series"], json!(["0197-aaa", "0197-bbb"]));
    assert_eq!(value["props"]["show_hidden"], json!(true));
    assert_eq!(Component::from_value(&value).unwrap(), chart);
    // A table takes groups too, and spending has the `groups` grouping.
    let mut table = Component::new(ComponentType::TableRows, "spending_by_category").unwrap();
    table
        .set("grouping", Setting::Word("groups".into()))
        .unwrap();
    table.set("groups", Setting::Ids(ids)).unwrap();
}

#[test]
fn a_list_of_ids_is_checked_like_the_rest_of_a_document() {
    let mut chart = Component::new(ComponentType::ChartLine, "category_over_time").unwrap();
    let many = (0..=MAX_IDS).map(|n| n.to_string()).collect();
    assert_eq!(
        reason(chart.set("series", Setting::Ids(many)).unwrap_err()),
        ("too-many", "params.series".into())
    );
    assert_eq!(
        reason(
            chart
                .set("groups", Setting::Ids(vec!["a\nb".into()]))
                .unwrap_err()
        ),
        ("bad-text", "params.groups".into())
    );
    assert_eq!(
        reason(chart.set("series", Setting::Word("x".into())).unwrap_err()),
        ("wrong-type", "params.series".into())
    );
    for bad in [json!("x"), json!([1]), json!([["a"]])] {
        let value = json!({"type": "chart.line", "dataset": "category_over_time",
            "params": {"series": bad}});
        assert_eq!(
            reason(Component::from_value(&value).unwrap_err()),
            ("wrong-type", "params.series".into())
        );
    }
}

#[test]
fn a_chart_over_time_is_offered_for_lines_and_stacked_bars_only() {
    let offered = |kind: ComponentType| kind.datasets().any(|d| d.id == "category_over_time");
    assert!(offered(ComponentType::ChartLine));
    assert!(offered(ComponentType::ChartArea));
    assert!(offered(ComponentType::ChartStackedBar));
    assert!(!offered(ComponentType::ChartPie));
    assert!(!offered(ComponentType::KpiCard));
}

#[test]
fn a_formula_is_a_property_of_the_key_figure_and_the_progress_bar_only() {
    for kind in [ComponentType::KpiCard, ComponentType::ProgressBar] {
        let mut component = Component::new(kind, INCOME_EXPENSE_BY_MONTH).unwrap();
        assert_eq!(
            component.get("formula"),
            Some(&Setting::Text(String::new()))
        );
        component
            .set("formula", Setting::Text("=Pets - Travel".into()))
            .unwrap();
        if kind == ComponentType::KpiCard {
            // The key figure shows its formula when its measure says so.
            component
                .set("measure", Setting::Word(FORMULA_MEASURE.into()))
                .unwrap();
        }
        // It is a property, like the title (D-289).
        let value = component.to_value();
        assert_eq!(value["props"]["formula"], "=Pets - Travel");
        assert_eq!(Component::from_value(&value).unwrap(), component);
        // Only length and characters are the schema's business: a mistake
        // in the formula fails its own component when it is worked out.
        component
            .set("formula", Setting::Text("=Pets -".into()))
            .unwrap();
        for (setting, why) in [
            (Setting::Text("x".repeat(1_001)), "too-long"),
            (Setting::Text("=1\n+2".into()), "bad-text"),
            (Setting::Word("=1".into()), "wrong-type"),
        ] {
            let (found, path) = reason(component.set("formula", setting).unwrap_err());
            assert_eq!((found, path.as_str()), (why, "props.formula"));
        }
        assert_eq!(
            component.get("formula"),
            Some(&Setting::Text("=Pets -".into()))
        );
    }
    let mut chart = Component::new(ComponentType::ChartBar, SPENDING_BY_CATEGORY).unwrap();
    let (why, _) = reason(
        chart
            .set("formula", Setting::Text("=Pets".into()))
            .unwrap_err(),
    );
    assert_eq!(why, "unknown-property");
}

#[test]
fn a_formula_stays_when_the_card_becomes_a_progress_bar() {
    let mut card = Component::new(ComponentType::KpiCard, INCOME_EXPENSE_BY_MONTH).unwrap();
    card.set("formula", Setting::Text("=PERCENT_OF(Pets; 300)".into()))
        .unwrap();
    card.change_kind(ComponentType::ProgressBar).unwrap();
    assert_eq!(
        card.get("formula"),
        Some(&Setting::Text("=PERCENT_OF(Pets; 300)".into()))
    );
    card.change_kind(ComponentType::ChartBar).unwrap();
    assert!(card.get("formula").is_none());
}

#[test]
fn a_progress_bar_has_a_target_to_measure_its_formula_against() {
    let mut bar = Component::new(ComponentType::ProgressBar, INCOME_EXPENSE_BY_MONTH).unwrap();
    assert_eq!(bar.get("target"), Some(&Setting::Text(String::new())));
    bar.set("target", Setting::Text("=400".into())).unwrap();
    assert_eq!(bar.to_value()["props"]["target"], "=400");
    let (why, path) = reason(
        bar.set("target", Setting::Text("x".repeat(1_001)))
            .unwrap_err(),
    );
    assert_eq!((why, path.as_str()), ("too-long", "props.target"));
    // The key figure has a formula but no target.
    let card = Component::new(ComponentType::KpiCard, INCOME_EXPENSE_BY_MONTH).unwrap();
    assert!(card.get("target").is_none());
}

#[test]
fn a_key_figure_can_show_a_formula_as_its_measure() {
    let card = Component::new(ComponentType::KpiCard, INCOME_EXPENSE_BY_MONTH).unwrap();
    let measure = card
        .settings()
        .find(|(property, _)| property.key == "measure")
        .map(|(property, _)| property.words(card.dataset()))
        .unwrap();
    assert_eq!(measure.last(), Some(&FORMULA_MEASURE));
}

fn column(name: &str, members: &[&str]) -> ColumnSetting {
    ColumnSetting {
        name: name.to_owned(),
        members: members.iter().map(|id| (*id).to_owned()).collect(),
        formula: String::new(),
    }
}

#[test]
fn columns_round_trip_through_a_document_and_are_checked() {
    let mut pie = Component::new(ComponentType::ChartPie, "spending_by_category").unwrap();
    assert_eq!(pie.get("columns"), Some(&Setting::Columns(Vec::new())));
    let columns = vec![
        column("Work food", &["0197-aaa", "0197-bbb"]),
        column("Freelancer food", &["0197-aaa"]),
    ];
    pie.set("columns", Setting::Columns(columns)).unwrap();
    let value = pie.to_value();
    assert_eq!(
        value["params"]["columns"],
        json!([
            {"name": "Work food", "members": ["0197-aaa", "0197-bbb"]},
            {"name": "Freelancer food", "members": ["0197-aaa"]},
        ])
    );
    assert_eq!(Component::from_value(&value).unwrap(), pie);
    // A table and a chart over time take columns too.
    for kind in [ComponentType::TableRows, ComponentType::ChartLine] {
        let dataset = if kind == ComponentType::TableRows {
            "spending_by_category"
        } else {
            "category_over_time"
        };
        let mut other = Component::new(kind, dataset).unwrap();
        other
            .set("columns", Setting::Columns(vec![column("A", &[])]))
            .unwrap();
    }
    let mut set = |columns| pie.set("columns", Setting::Columns(columns)).unwrap_err();
    assert_eq!(
        reason(set(vec![column("  ", &[])])),
        ("bad-text", "params.columns".into())
    );
    assert_eq!(
        reason(set(vec![column("Food", &[]), column(" food ", &[])])),
        ("duplicate-name", "params.columns".into())
    );
    assert_eq!(
        reason(set((0..=MAX_COLUMNS)
            .map(|n| column(&n.to_string(), &[]))
            .collect())),
        ("too-many", "params.columns".into())
    );
    assert_eq!(
        reason(set(vec![column("A", &["x\ny"])])),
        ("bad-text", "params.columns".into())
    );
    // A document whose column has an unknown key is refused as a whole.
    let mut bad = pie.to_value();
    bad["params"]["columns"] = json!([{"name": "A", "members": [], "extra": 1}]);
    assert_eq!(
        reason(Component::from_value(&bad).unwrap_err()),
        ("wrong-type", "params.columns".into())
    );
}

#[test]
fn a_column_may_be_a_formula_instead_of_members() {
    let mut pie = Component::new(ComponentType::ChartPie, "spending_by_category").unwrap();
    let share = ColumnSetting {
        name: "Left over".to_owned(),
        members: Vec::new(),
        formula: "Pay - Food".to_owned(),
    };
    pie.set(
        "columns",
        Setting::Columns(vec![column("Food", &["0197-aaa"]), share.clone()]),
    )
    .unwrap();
    let value = pie.to_value();
    assert_eq!(
        value["params"]["columns"],
        json!([
            {"name": "Food", "members": ["0197-aaa"]},
            {"name": "Left over", "members": [], "formula": "Pay - Food"},
        ])
    );
    assert_eq!(Component::from_value(&value).unwrap(), pie);
    // Members and a formula together are refused, and so is a formula with
    // a control character.
    let mut both = share.clone();
    both.members = vec!["0197-aaa".to_owned()];
    assert_eq!(
        reason(
            pie.set("columns", Setting::Columns(vec![both]))
                .unwrap_err()
        ),
        ("wrong-type", "params.columns".into())
    );
    let mut odd = share;
    odd.formula = "Pay\n- Food".to_owned();
    assert_eq!(
        reason(pie.set("columns", Setting::Columns(vec![odd])).unwrap_err()),
        ("bad-text", "params.columns".into())
    );
}

#[test]
fn columns_survive_changing_between_charts_and_tables_and_back() {
    let mut chart = Component::new(ComponentType::ChartBar, "spending_by_category").unwrap();
    let columns = Setting::Columns(vec![column("Fast food", &["0197-aaa"])]);
    chart.set("columns", columns.clone()).unwrap();
    for kind in [
        ComponentType::ChartLine,
        ComponentType::ChartPie,
        ComponentType::ChartStackedBar,
        ComponentType::TableRows,
        ComponentType::ChartDonut,
        ComponentType::ChartArea,
        ComponentType::ChartBar,
    ] {
        chart.change_kind(kind).unwrap();
        assert_eq!(chart.get("columns"), Some(&columns), "after {kind:?}");
    }
}
