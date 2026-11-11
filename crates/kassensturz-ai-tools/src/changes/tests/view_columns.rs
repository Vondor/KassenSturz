// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `propose_view` with `columns` (US3.1b, #562, D-687): a chart of spending
//! made of named columns, each a list of categories or a formula with its
//! own period, as the Statistic Builder's board makes them (D-339, D-341).

use kassensturz_core::ViewKind;
use kassensturz_report::{ColumnSetting, Document, Item, MAX_FORMULA_CHARS, Setting};

use super::*;
use crate::functions::call_allowed;

fn today() -> kassensturz_core::Date {
    "2026-10-31".parse().expect("a day")
}

/// The code a call gives, with the argument and the column it names.
fn code(result: Result<Proposal, ToolError>) -> String {
    match result {
        Ok(_) => "prepared".into(),
        Err(error) => {
            let report = error.model_report();
            let mut text = report.code.to_owned();
            for key in ["argument", "list", "item", "reason"] {
                if let Some(param) = report.param(key) {
                    text.push_str(&format!(" {key}={param:?}"));
                }
            }
            text
        }
    }
}

fn bars(columns: Value) -> Value {
    json!({"title": "Salary and spending", "data": "spending_by_category",
        "show_as": "bar", "period": "this-month", "columns": columns})
}

fn call(book: &Book, arguments: &Value) -> Result<Proposal, ToolError> {
    propose_view(book, arguments, Permissions::all(), today())
}

#[test]
fn the_ai_makes_a_chart_of_a_formula_column_and_a_category_column() {
    let Sample { mut book, food, .. } = sample();
    // The example of #562: last month's salary next to this month's money
    // out, and a column of a category.
    let proposal = call(
        &book,
        &bars(json!([
            {"name": "Salary last month", "formula": "=Salary@\"last month\""},
            {"name": "Money out", "formula": "=Spending.ALL"},
            {"name": "Food", "categories": [food.to_string()]},
        ])),
    )
    .expect("prepared");
    let [ProposedChange::NewView(new)] = proposal.changes.changes() else {
        panic!("one chart, not {:?}", proposal.changes.changes());
    };
    let component: Value = serde_json::from_str(&new.component_json()).expect("JSON");
    assert_eq!(
        component["params"]["columns"],
        json!([
            {"name": "Salary last month", "members": [], "formula": "=Salary@\"last month\""},
            {"name": "Money out", "members": [], "formula": "=Spending.ALL"},
            {"name": "Food", "members": [food.to_string()]},
        ])
    );
    // A chart of many figures has no computed figure (D-685).
    assert!(proposal.output.value.get("computed").is_none());
    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("saved");
    let views = book.views(Some(ViewKind::View)).expect("views");
    let document = Document::read(&views[0].document).expect("readable");
    let Item::Component(saved) = &document.entries[0].item else {
        panic!("a component");
    };
    assert_eq!(
        saved.get("columns"),
        Some(&Setting::Columns(vec![
            ColumnSetting {
                name: "Salary last month".into(),
                members: Vec::new(),
                formula: "=Salary@\"last month\"".into(),
            },
            ColumnSetting {
                name: "Money out".into(),
                members: Vec::new(),
                formula: "=Spending.ALL".into(),
            },
            ColumnSetting {
                name: "Food".into(),
                members: vec![food.to_string()],
                formula: String::new(),
            },
        ]))
    );

    // list_views gives them back in the words of propose_view, and names
    // them only without the reading of categories.
    let charts = Permissions::NONE
        .with(Permission::ChartsRead, true)
        .with(Permission::CategoriesRead, true);
    let output = call_allowed(&book, "list_views", &Value::Null, today(), charts).expect("listed");
    let chart = &output.value["charts"][0];
    assert_eq!(chart["columns"][0]["name"], json!("Salary last month"));
    assert_eq!(
        chart["columns"][0]["formula"],
        json!("=Salary@\"last month\"")
    );
    assert_eq!(chart["columns"][2]["categories"], json!([food.to_string()]));
    let without = charts.with(Permission::CategoriesRead, false);
    let output = call_allowed(&book, "list_views", &Value::Null, today(), without).expect("listed");
    let chart = &output.value["charts"][0];
    assert!(chart.get("columns").is_none());
    assert_eq!(chart["not_shown"], json!(["columns"]));

    // A change takes them away with an empty list.
    let id = views[0].id.to_string();
    let proposal = call(&book, &json!({"view_id": id, "columns": []})).expect("prepared");
    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    let views = book.views(Some(ViewKind::View)).expect("views");
    let document = Document::read(&views[0].document).expect("readable");
    let Item::Component(changed) = &document.entries[0].item else {
        panic!("a component");
    };
    assert_eq!(changed.get("columns"), Some(&Setting::Columns(Vec::new())));
}

#[test]
fn columns_are_offered_only_with_the_reading_of_categories() {
    let Sample { book, food, .. } = sample();
    let spec = propose_view_spec(Permissions::all()).expect("offered");
    let schema: Value = serde_json::from_str(&spec.parameters).expect("JSON");
    let columns = &schema["properties"]["columns"];
    assert_eq!(columns["maxItems"], json!(kassensturz_report::MAX_COLUMNS));
    assert_eq!(columns["items"]["required"], json!(["name"]), "{columns}");
    // Without any cell of categories: changing them reads them too.
    let without = Permissions::NONE.with(Permission::NewViews, true);
    let spec = propose_view_spec(without).expect("offered");
    let schema: Value = serde_json::from_str(&spec.parameters).expect("JSON");
    assert!(schema["properties"].get("columns").is_none());
    let arguments = bars(json!([{"name": "Food", "categories": [food.to_string()]}]));
    assert_eq!(
        code(propose_view(&book, &arguments, without, today())),
        "ai-tools.unknown-argument"
    );
}

#[test]
fn a_wrong_column_is_refused_with_its_place() {
    let Sample { book, food, .. } = sample();
    let food = food.to_string();
    let column = |name: &str| json!({"name": name, "categories": [food]});
    for (columns, expected) in [
        (
            json!([]),
            "ai-tools.bad-argument argument=Keyword(\"columns\")",
        ),
        (
            json!([column("Food"), column("food")]),
            "ai-tools.bad-argument argument=Keyword(\"columns\") list=Keyword(\"columns\") item=Number(2)",
        ),
        (
            json!([{"name": "Both", "categories": [food], "formula": "=Food"}]),
            "ai-tools.bad-argument argument=Keyword(\"columns\") list=Keyword(\"columns\") item=Number(1)",
        ),
        (
            json!([{"name": "Neither"}]),
            "ai-tools.bad-argument argument=Keyword(\"columns\") list=Keyword(\"columns\") item=Number(1)",
        ),
        (
            json!([{"name": " ", "formula": "=Food"}]),
            "ai-tools.bad-argument argument=Keyword(\"columns\") list=Keyword(\"columns\") item=Number(1)",
        ),
        (
            json!([{"name": "Other", "formula": "=Food", "color": "red"}]),
            "ai-tools.bad-argument argument=Keyword(\"columns\") list=Keyword(\"columns\") item=Number(1)",
        ),
        (
            json!([column("Food"), {"name": "Pets", "formula": "=Pets"}]),
            "ai-tools.bad-formula argument=Keyword(\"columns\") list=Keyword(\"columns\") item=Number(2) reason=Keyword(\"unknown-name\")",
        ),
        // A column's figure is an amount: a plain number is refused.
        (
            json!([{"name": "Three", "formula": "=3"}]),
            "ai-tools.bad-formula argument=Keyword(\"columns\") list=Keyword(\"columns\") item=Number(1) reason=Keyword(\"kinds\")",
        ),
        (
            json!([{"name": "Gone", "categories": [AccountId::generate().to_string()]}]),
            "ai-tools.unknown-category list=Keyword(\"columns\") item=Number(1)",
        ),
    ] {
        assert_eq!(
            code(call(&book, &bars(columns.clone()))),
            expected,
            "{columns}"
        );
    }
    let thirteen: Vec<Value> = (0..13).map(|n| column(&format!("Food {n}"))).collect();
    assert_eq!(
        code(call(&book, &bars(json!(thirteen)))),
        "ai-tools.bad-argument argument=Keyword(\"columns\")"
    );
}

#[test]
fn columns_go_with_spending_and_without_a_comparison_or_an_average() {
    let Sample { book, food, .. } = sample();
    let columns = json!([{"name": "Food", "categories": [food.to_string()]}]);
    let with = |extra: Value| {
        let mut arguments = bars(columns.clone());
        for (key, value) in extra.as_object().expect("an object") {
            arguments[key] = value.clone();
        }
        code(call(&book, &arguments))
    };
    assert_eq!(
        with(json!({"compare": "previous_period"})),
        "ai-tools.bad-argument argument=Keyword(\"compare\")"
    );
    assert_eq!(
        with(json!({"average": "per-month"})),
        "ai-tools.bad-argument argument=Keyword(\"average\")"
    );
    assert_eq!(with(json!({"compare": "none"})), "prepared");
    assert_eq!(
        with(json!({"data": "income_expense_by_month"})),
        "ai-tools.bad-argument argument=Keyword(\"columns\")"
    );
    // Every way of drawing that takes columns makes one.
    for show_as in ["pie", "donut", "stacked_bar", "table"] {
        assert_eq!(with(json!({"show_as": show_as})), "prepared", "{show_as}");
    }
    // A key figure has none: the refusal names the ways that do.
    let figure = {
        let mut arguments = bars(columns.clone());
        arguments["show_as"] = json!("figure");
        call(&book, &arguments).expect_err("refused")
    };
    let ways = figure
        .only_for_show_as()
        .expect("the ways that take columns");
    assert!(ways.contains(&"bar") && ways.contains(&"table"), "{ways:?}");
}

#[test]
fn twelve_columns_with_long_formulas_fit_in_one_call() {
    let Sample { book, .. } = sample();
    // Twelve formulas of 1,000 characters: more than the old 8 KiB.
    let formula = format!("=Food{}", "+0".repeat((MAX_FORMULA_CHARS - 5) / 2));
    assert!(formula.chars().count() <= MAX_FORMULA_CHARS);
    let columns: Vec<Value> = (0..12)
        .map(|n| json!({"name": format!("Column {n}"), "formula": formula}))
        .collect();
    let arguments = bars(json!(columns));
    assert!(arguments.to_string().len() > 8 * 1024);
    assert!(arguments.to_string().len() <= MAX_VIEW_CALL_BYTES);
    assert_ne!(
        code(call(&book, &arguments)).split(' ').next(),
        Some("ai-tools.call-too-big")
    );
}
