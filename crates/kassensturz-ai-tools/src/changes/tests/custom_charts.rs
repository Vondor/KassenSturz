// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Custom charts the AI describes (#700 part B, D-709, D-729): the cell
//! Chart descriptions, `propose_view` with `show_as` `custom` and
//! `description`, its checks, the change of a saved custom chart and what
//! `list_views` shows of one. The descriptions are part A's own examples
//! (`SPEC_EXAMPLES`), the ones the help page and the tool text show.

use kassensturz_core::ViewKind;
use kassensturz_report::spec::SPEC_EXAMPLES;
use kassensturz_report::{Component, ComponentType, Document, Item, Setting};

use super::*;
use crate::functions::call_allowed;

fn today() -> kassensturz_core::Date {
    "2026-10-31".parse().expect("a day")
}

/// Everything the AI may do with charts, and descriptions.
fn describing() -> Permissions {
    Permissions::NONE
        .with(Permission::NewViews, true)
        .with(Permission::EditViews, true)
        .with(Permission::CustomCharts, true)
}

fn example(key: &str) -> &'static str {
    SPEC_EXAMPLES
        .iter()
        .find(|example| example.key == key)
        .expect("an example")
        .text
}

fn call(description: &str) -> Value {
    json!({"title": "Money this year", "show_as": "custom", "description": description})
}

fn code(result: Result<Proposal, ToolError>) -> String {
    match result.map_err(ToolError::without_item) {
        Ok(_) => "prepared".into(),
        Err(ToolError::BadArgument { argument }) => format!("bad-argument {argument}"),
        Err(ToolError::NotAllowed { argument }) => format!("not-allowed {argument}"),
        Err(ToolError::NotChangeable { argument, reason }) => {
            format!("not-changeable {argument} {reason}")
        }
        Err(ToolError::BadDescription { reason, why, .. }) => {
            format!("bad-description {reason} {}", why.unwrap_or("-"))
        }
        Err(ToolError::NotForChart {
            argument,
            only_for_show_as,
            ..
        }) => format!("not-for-chart {argument} {}", only_for_show_as.join(",")),
        Err(error) => error.report().code.into(),
    }
}

/// Saves a custom chart the user wrote by hand, and gives its ID.
fn saved_by_hand(book: &mut Book, title: &str, spec: &str) -> String {
    let mut component =
        Component::new(ComponentType::CustomChart, "income_expense_by_month").expect("custom");
    component
        .set("spec", Setting::Text(spec.to_owned()))
        .expect("spec");
    let document = Document::view(title, Some("last-12-months"), component).to_json();
    book.add_view(&document).expect("saved").to_string()
}

fn component_of(book: &Book, id: &str) -> Component {
    let view = book
        .views(Some(ViewKind::View))
        .expect("views")
        .into_iter()
        .find(|view| view.id.to_string() == id)
        .expect("the chart");
    match &Document::read(&view.document).expect("readable").entries[0].item {
        Item::Component(component) => component.clone(),
        Item::Unknown(_) => panic!("a known component"),
    }
}

#[test]
fn every_example_of_the_help_becomes_a_chart_the_user_confirms() {
    let Sample { mut book, .. } = sample();
    for (index, example) in SPEC_EXAMPLES.iter().enumerate() {
        let title = format!("Example {index}");
        let proposal = propose_view(
            &book,
            &json!({"title": title, "show_as": "custom", "description": example.text}),
            describing(),
            today(),
        )
        .expect(example.key);
        let [ProposedChange::NewView(view)] = proposal.changes.changes() else {
            panic!("one new chart");
        };
        assert_eq!(view.type_id(), "chart.custom", "{}", example.key);
        assert_eq!(view.dataset_id(), example.dataset, "{}", example.key);
        assert_eq!(view.description(), Some(example.text));
        // The model gets no figure of a custom chart (D-685).
        assert!(proposal.output.value.get("computed").is_none());
        proposal
            .changes
            .apply(&mut book, Confirmed::by_the_user())
            .expect("saved");
    }
    let saved = book.views(Some(ViewKind::View)).expect("views");
    assert_eq!(saved.len(), SPEC_EXAMPLES.len());
}

#[test]
fn without_the_cell_custom_and_description_are_not_offered_or_taken() {
    let Sample { book, .. } = sample();
    let charts = Permissions::NONE.with(Permission::NewViews, true);
    let spec = propose_view_spec(charts).expect("offered");
    let parameters: Value = serde_json::from_str(&spec.parameters).expect("json");
    assert!(parameters["properties"].get("description").is_none());
    let words = parameters["properties"]["show_as"]["enum"]
        .as_array()
        .expect("enum");
    assert!(!words.contains(&json!("custom")));
    let text = example("left-each-month");
    assert_eq!(
        code(propose_view(&book, &call(text), charts, today())),
        "ai-tools.unknown-argument"
    );
    // The cell alone offers nothing: it widens Add and Change.
    let alone = Permissions::NONE.with(Permission::CustomCharts, true);
    assert!(propose_view_spec(alone).is_none());
    // With it, both are offered, and data is no longer required.
    let spec = propose_view_spec(describing().with(Permission::EditViews, false)).expect("offered");
    let parameters: Value = serde_json::from_str(&spec.parameters).expect("json");
    assert!(
        parameters["properties"]["description"]["description"]
            .as_str()
            .expect("help")
            .contains("cash_flow_by_month")
    );
    assert_eq!(parameters["required"], json!(["title", "show_as"]));
}

#[test]
fn a_set_with_a_description_is_refused_without_the_cell() {
    let Sample { book, .. } = sample();
    // As a request file or a later change of the grid could bring it.
    let set = json!({"new_views": [{
        "title": "By hand", "dataset": "income_expense_by_month", "chart": "chart.custom",
        "period": "last-12-months", "spec": example("left-each-month"),
    }]});
    let charts = Permissions::NONE.with(Permission::NewViews, true);
    assert_eq!(
        code(super::super::prepare(&book, &set, charts)),
        "not-allowed description"
    );
    assert_eq!(
        code(super::super::prepare(&book, &set, describing())),
        "prepared"
    );
    // A set prepared with the cell isn't applied once it is off.
    let prepared = super::super::prepare(&book, &set, describing()).expect("prepared");
    assert!(prepared.changes.check_allowed(charts).is_err());
    assert!(prepared.changes.check_allowed(describing()).is_ok());
}

#[test]
fn each_check_of_a_description_refuses_in_kassensturz_s_own_words() {
    let Sample { book, .. } = sample();
    let try_it = |text: &str| code(propose_view(&book, &call(text), describing(), today()));
    let ok = example("left-each-month");
    assert_eq!(try_it(ok), "prepared");
    // Too big for the AI, though a person may paste it.
    let padded = ok.replacen(
        "\"title\": \"What is left each month\"",
        &format!("\"description\": \"{}\"", "a".repeat(9 * 1024)),
        1,
    );
    assert_eq!(try_it(&padded), "bad-description too-big bytes");
    assert_eq!(try_it("not json"), "bad-description not-json -");
    // A hiding character in any text.
    let hidden = ok.replace("What is left", "What\u{202e} is left");
    assert_eq!(try_it(&hidden), "bad-description hidden-character -");
    // A part that isn't supported refuses; the AI can't draw without it.
    let calc = ok.replacen(
        "\"mark\": \"bar\"",
        "\"transform\": [{\"calculate\": \"datum.net * 2\", \"as\": \"x2\"}], \"mark\": \"bar\"",
        1,
    );
    assert_eq!(try_it(&calc), "bad-description not-supported expression");
    let url = ok.replacen(
        "{ \"name\": \"income_expense_by_month\" }",
        "{ \"name\": \"income_expense_by_month\", \"url\": \"https://example.org/x.json\" }",
        1,
    );
    assert_eq!(try_it(&url), "bad-description not-supported data-source");
    // Data the AI may not name in a description.
    let budgets = r#"{"data":{"name":"budget_utilization"},"mark":"bar",
        "encoding":{"x":{"field":"category","type":"nominal"},
        "y":{"field":"spent","type":"quantitative"}}}"#;
    let refused = propose_view(&book, &call(budgets), describing(), today()).expect_err("refused");
    assert_eq!(refused.allowed_fields(), Some(&DESCRIPTION_DATA[..]));
    // The answer never repeats the model's own key.
    let made_up = ok.replacen(
        "\"mark\": \"bar\"",
        "\"mark\": \"bar\", \"secret_word\": 1",
        1,
    );
    let refused = propose_view(&book, &call(&made_up), describing(), today()).expect_err("refused");
    let report = refused.model_report();
    assert!(!format!("{report:?}").contains("secret_word"), "{report:?}");
}

#[test]
fn a_custom_chart_takes_only_title_period_and_its_own_data() {
    let Sample { book, .. } = sample();
    let text = example("left-each-month");
    let with = |extra: Value| {
        let mut arguments = call(text);
        for (key, value) in extra.as_object().expect("object") {
            arguments[key] = value.clone();
        }
        code(propose_view(&book, &arguments, describing(), today()))
    };
    assert_eq!(with(json!({"period": "this-year"})), "prepared");
    assert_eq!(with(json!({"data": "income_expense_by_month"})), "prepared");
    assert_eq!(
        with(json!({"data": "net_worth_over_time"})),
        "bad-argument data"
    );
    assert_eq!(
        with(json!({"compare": "previous_period"})),
        "bad-argument compare"
    );
    assert_eq!(
        with(json!({"show_as": "bar", "data": "income_expense_by_month"})),
        "not-for-chart description custom"
    );
    assert_eq!(
        code(propose_view(
            &book,
            &json!({"title": "No text", "show_as": "custom"}),
            describing(),
            today()
        )),
        "bad-argument description"
    );
}

#[test]
fn a_saved_custom_chart_gets_a_new_description_and_nothing_else_changes_kind() {
    let Sample { mut book, food, .. } = sample();
    let first = example("left-each-month");
    let id = saved_by_hand(&mut book, "Left over", first);
    // The user had chosen to draw without some parts; a new text takes that
    // choice back (D-519).
    {
        let view = book
            .views(Some(ViewKind::View))
            .expect("views")
            .into_iter()
            .find(|view| view.id.to_string() == id)
            .expect("chart");
        let mut document = Document::read(&view.document).expect("readable");
        if let Item::Component(component) = &mut document.entries[0].item {
            component
                .set("partial", Setting::Flag(true))
                .expect("partial");
        }
        book.edit_view(view.id, &document.to_json())
            .expect("edited");
    }
    let second = example("worth-and-average");
    let proposal = propose_view(
        &book,
        &json!({"view_id": id, "description": second}),
        describing(),
        today(),
    )
    .expect("prepared");
    let [ProposedChange::EditView(edit)] = proposal.changes.changes() else {
        panic!("one changed chart");
    };
    assert!(edit.writes_description());
    assert_eq!(edit.dataset_id(), "net_worth_over_time");
    assert_eq!(edit.title, "Left over");
    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("saved");
    let after = component_of(&book, &id);
    assert_eq!(after.get("spec"), Some(&Setting::Text(second.to_owned())));
    assert_eq!(after.get("partial"), Some(&Setting::Flag(false)));
    // Without Change, a saved chart's description isn't changed.
    let add_only = describing().with(Permission::EditViews, false);
    assert_eq!(
        code(propose_view(
            &book,
            &json!({"view_id": id, "description": first}),
            add_only,
            today()
        )),
        "not-allowed view_id"
    );
    // A custom chart doesn't become a built-in one, nor the other way.
    assert_eq!(
        code(propose_view(
            &book,
            &json!({"view_id": id, "show_as": "bar"}),
            describing(),
            today()
        )),
        "not-changeable show_as custom-kind"
    );
    let built_in = propose_view(
        &book,
        &json!({"title": "Food", "data": "spending_by_category", "show_as": "bar",
                "categories": [food.to_string()]}),
        Permissions::all(),
        today(),
    )
    .expect("prepared");
    built_in
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("saved");
    let food_id = book
        .views(Some(ViewKind::View))
        .expect("views")
        .into_iter()
        .find(|view| view.name == "Food")
        .expect("chart")
        .id
        .to_string();
    for change in [
        json!({"view_id": food_id, "show_as": "custom"}),
        json!({"view_id": food_id, "description": first}),
    ] {
        assert!(
            code(propose_view(&book, &change, describing(), today())).ends_with("custom-kind"),
            "{change}"
        );
    }
}

#[test]
fn list_views_shows_a_description_only_with_the_cell_and_both_readings() {
    let Sample { mut book, .. } = sample();
    let text = example("left-each-month");
    saved_by_hand(&mut book, "Left over", text);
    let list = |allowed: Permissions| {
        call_allowed(&book, "list_views", &Value::Null, today(), allowed)
            .expect("listed")
            .value["charts"][0]
            .clone()
    };
    let read = Permissions::NONE.with(Permission::ChartsRead, true);
    let chart = list(read);
    assert_eq!(chart["show_as"], json!("other"));
    assert!(chart.get("description").is_none());
    let with_cell = read.with(Permission::CustomCharts, true);
    let chart = list(with_cell);
    assert_eq!(chart["show_as"], json!("custom"));
    assert_eq!(chart["not_shown"], json!(["description"]));
    let with_both = with_cell
        .with(Permission::CategoriesRead, true)
        .with(Permission::AccountsRead, true);
    let chart = list(with_both);
    let shown = chart["description"].as_str().expect("shown");
    // Cleaned for the model (one line), still the same description.
    let a: Value = serde_json::from_str(shown).expect("json");
    let b: Value = serde_json::from_str(text).expect("json");
    assert_eq!(a, b);
    assert!(chart.get("not_shown").is_none());
}

#[test]
fn the_texts_of_a_description_are_listed_for_the_number_check() {
    let text = example("balance-and-flows");
    let titles = description_titles(text);
    assert!(titles.contains(&"Balance, money in and money out".to_owned()));
    assert!(titles.contains(&"Money in".to_owned()));
}

#[test]
fn a_description_gets_the_same_answer_on_any_book() {
    // Security look of #749: a filter that names a category or an account
    // must not tell the model whether the book has one, so a description
    // is answered the same on a book that has it and on an empty one.
    let Sample { book: full, .. } = sample();
    let empty = Book::create_in_memory().expect("book");
    let names = |name: &str, field: &str, data: &str, y: &str| {
        format!(
            r#"{{"data":{{"name":"{data}"}},"transform":[{{"filter":{{"field":"{field}","equal":"{name}"}}}}],
            "mark":"bar","encoding":{{"x":{{"field":"{field}","type":"nominal"}},
            "y":{{"field":"{y}","type":"quantitative"}}}}}}"#
        )
    };
    let descriptions = [
        names("Groceries", "category", "spending_by_category", "total"),
        names(
            "No such category",
            "category",
            "spending_by_category",
            "total",
        ),
        names("Checking account", "account", "account_balances", "balance"),
        names("No such account", "account", "account_balances", "balance"),
        // And refusals: unsupported, unknown field, data not offered.
        names("Groceries", "category", "spending_by_category", "total").replacen(
            "\"mark\"",
            "\"params\":[],\"mark\"",
            1,
        ),
        names("Groceries", "no_field", "spending_by_category", "total"),
        names("Groceries", "category", "budget_utilization", "spent"),
    ];
    for description in &descriptions {
        let answer = |book: &Book| {
            let result = propose_view(book, &call(description), describing(), today());
            let report = result
                .as_ref()
                .err()
                .map(|error| format!("{:?}", error.model_report()));
            (code(result), report)
        };
        assert_eq!(answer(&full), answer(&empty), "{description}");
    }
    // The test means something: names prepare, the broken ones are refused.
    let on_empty = |index: usize| {
        code(propose_view(
            &empty,
            &call(&descriptions[index]),
            describing(),
            today(),
        ))
    };
    for index in 0..4 {
        assert_eq!(on_empty(index), "prepared", "{index}");
    }
    for index in 4..7 {
        assert_ne!(on_empty(index), "prepared", "{index}");
    }
}
