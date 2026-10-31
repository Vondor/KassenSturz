// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `propose_view` (M6c.8, D-352 to D-354): its lists come from the real
//! catalog and schema, its checks refuse in the fixed order of D-353, and
//! what it prepares is the change-set entry the review page knows.

use kassensturz_core::ViewKind;
use kassensturz_core::ai_switch::Row;
use kassensturz_report::{CATALOG, ComponentType, Document, PERIODS};

use super::*;

/// The code a call gives, with the argument a bad one names.
fn view_code(result: Result<Proposal, ToolError>) -> String {
    match result {
        Ok(_) => "prepared".into(),
        Err(ToolError::BadArgument { argument }) => format!("bad-argument {argument}"),
        Err(ToolError::NotAllowed { argument }) => format!("not-allowed {argument}"),
        Err(error) => error.report().code.into(),
    }
}

fn call(book: &Book, text: &str) -> Result<Proposal, ToolError> {
    propose_view_text(book, text, Permissions::all())
}

/// The one chart a proposal holds.
fn chart(proposal: &Proposal) -> &NewView {
    match proposal.changes.changes() {
        [ProposedChange::NewView(new)] => new,
        other => panic!("one chart, not {other:?}"),
    }
}

/// The values of an `enum` of the offered schema.
fn words(value: &Value) -> Vec<&str> {
    value["enum"]
        .as_array()
        .expect("an enum")
        .iter()
        .map(|word| word.as_str().expect("a word"))
        .collect()
}

#[test]
fn the_lists_the_model_sees_are_the_catalogs_and_the_schemas() {
    let spec = propose_view_spec(Permissions::all()).expect("offered");
    assert_eq!(spec.name, "propose_view");
    let schema: Value = serde_json::from_str(&spec.parameters).expect("JSON");
    assert_eq!(schema["additionalProperties"], false);
    let properties = &schema["properties"];
    let for_ai: Vec<&str> = CATALOG
        .iter()
        .filter(|dataset| dataset.for_ai)
        .map(|dataset| dataset.id)
        .collect();
    assert_eq!(words(&properties["data"]), for_ai);
    // The financial datasets are the AI's since M6c.8 (D-354); the one with
    // My groups is not (D-306, Q-268 = A).
    for id in [
        "account_balances",
        "net_worth_over_time",
        "cash_flow_by_month",
    ] {
        assert!(for_ai.contains(&id), "{id}");
        assert!(spec.description.contains(id), "{id}");
    }
    assert!(!for_ai.contains(&"category_over_time"));
    // Every kind of chart that suits one of them, and auto.
    let shown = words(&properties["show_as"]);
    assert_eq!(shown[0], "auto");
    for word in [
        "line",
        "area",
        "bar",
        "stacked_bar",
        "pie",
        "donut",
        "figure",
        "progress",
        "table",
    ] {
        assert!(shown.contains(&word), "{word}");
    }
    let groupings = words(&properties["group_by"]);
    assert!(!groupings.contains(&kassensturz_report::GROUPS_GROUPING));
    for word in [
        "category",
        "top_level_category",
        "month",
        "account",
        "account_kind",
    ] {
        assert!(groupings.contains(&word), "{word}");
    }
    let measures = words(&properties["measure"]);
    assert!(!measures.contains(&kassensturz_report::FORMULA_MEASURE));
    for word in [
        "net",
        "savings-rate",
        "net_worth",
        "liabilities",
        "money_in",
    ] {
        assert!(measures.contains(&word), "{word}");
    }
    assert_eq!(
        words(&properties["compare"]),
        ["none", "previous_period", "same_last_year"]
    );
    let periods = words(&properties["period"]["anyOf"][0]);
    assert_eq!(periods[0], "page");
    assert_eq!(&periods[1..], PERIODS);
    // Every list is the one a refusal names (D-353).
    for argument in ["data", "show_as", "group_by", "measure", "compare"] {
        assert_eq!(
            Some(words(&properties[argument])),
            view_choices(argument),
            "{argument}"
        );
    }
    assert_eq!(view_choices("title"), None);
}

#[test]
fn ids_are_offered_only_where_the_grid_lets_the_model_read_them() {
    let all = propose_view_spec(Permissions::all()).expect("offered");
    let properties = |spec: &ProposeSpec| -> Vec<String> {
        let schema: Value = serde_json::from_str(&spec.parameters).expect("JSON");
        schema["properties"]
            .as_object()
            .expect("properties")
            .keys()
            .cloned()
            .collect()
    };
    assert!(properties(&all).contains(&"categories".to_owned()));
    // A cell that changes categories lets the AI read them too, so only
    // Charts › Add.
    let no_categories = Permissions::NONE.with(Permission::NewViews, true);
    assert!(!no_categories.reads(Row::Categories));
    let spec = propose_view_spec(no_categories).expect("offered");
    assert!(!properties(&spec).contains(&"categories".to_owned()));
    // An argument not offered is one the function doesn't take.
    let Sample { book, food, .. } = sample();
    let text = json!({
        "title": "Food", "data": "spending_by_category", "show_as": "pie",
        "categories": [food.to_string()]
    })
    .to_string();
    assert_eq!(
        view_code(propose_view_text(&book, &text, no_categories)),
        "ai-tools.unknown-argument"
    );
    // The accounts follow the schema: offered once a component has
    // `params.accounts` (D-351, M6c.6).
    let has_accounts = ComponentType::ALL
        .iter()
        .flat_map(|kind| kind.properties())
        .any(|property| property.key == "accounts");
    assert_eq!(
        properties(&all).contains(&"accounts".to_owned()),
        has_accounts
    );
    // Without Charts › Add there is no function at all.
    let without = Permissions::all().with(Permission::NewViews, false);
    assert!(propose_view_spec(without).is_none());
    assert_eq!(
        view_code(propose_view_text(
            &book,
            r#"{"title":"A","data":"spending_by_category","show_as":"auto"}"#,
            without
        )),
        "not-allowed propose_view"
    );
}

#[test]
fn a_call_becomes_the_chart_the_review_page_shows_and_saves_only_on_confirm() {
    let Sample { mut book, food, .. } = sample();
    let proposal = call(
        &book,
        &json!({
            "title": "Food this year",
            "data": "spending_by_category",
            "show_as": "donut",
            "group_by": "category",
            "period": "this-year",
            "compare": null,
            "categories": [food.to_string()],
        })
        .to_string(),
    )
    .expect("prepared");
    let new = chart(&proposal);
    assert_eq!(
        (
            new.title.as_str(),
            new.type_id(),
            new.dataset_id(),
            new.period.as_str()
        ),
        (
            "Food this year",
            "chart.donut",
            "spending_by_category",
            "this-year"
        )
    );
    // The categories are in the component as a hand-made one has them.
    let component: Value = serde_json::from_str(&new.component_json()).expect("JSON");
    assert_eq!(component["params"]["categories"], json!([food.to_string()]));
    // The slot the review page edits is the same as before (D-297).
    assert!(proposal.changes.item(slot("new_views/0")).is_some());
    assert!(book.views(Some(ViewKind::View)).expect("views").is_empty());
    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    let saved = book.views(Some(ViewKind::View)).expect("views");
    assert_eq!(saved.len(), 1);
    let document = Document::read(&saved[0].document).expect("a valid document");
    assert_eq!(
        document.to_json(),
        Document::read(&document.to_json())
            .expect("again")
            .to_json()
    );
    assert_eq!(
        book.history().expect("history")[0].action,
        Action::ApplyAiChanges
    );
}

#[test]
fn auto_and_the_default_period_suit_the_financial_datasets() {
    let Sample { book, .. } = sample();
    let prepared = |data: &str, period: Value| {
        let mut call = json!({"title": data, "data": data, "show_as": "auto"});
        if !period.is_null() {
            call["period"] = period;
        }
        let proposal = propose_view_text(&book, &call.to_string(), Permissions::all())
            .unwrap_or_else(|error| panic!("{data}: {}", error.report().code));
        let new = chart(&proposal);
        (new.type_id(), new.period.clone())
    };
    // What you are worth over time is a line, cash flow bars, today's
    // balances a key figure of this month (D-352).
    assert_eq!(
        prepared("net_worth_over_time", Value::Null),
        ("chart.line", "last-12-months".to_owned())
    );
    assert_eq!(
        prepared("cash_flow_by_month", Value::Null),
        ("chart.bar", "last-12-months".to_owned())
    );
    assert_eq!(
        prepared("account_balances", Value::Null),
        ("kpi.card", "this-month".to_owned())
    );
    assert_eq!(
        prepared("account_balances", json!("page")),
        ("kpi.card", "this-month".to_owned())
    );
    assert_eq!(
        prepared("spending_by_category", Value::Null),
        ("chart.pie", "last-12-months".to_owned())
    );
    // A range is kept as the word a typed range has (D-332).
    assert_eq!(
        prepared(
            "income_expense_by_month",
            json!({"from": "2026-01-01", "to": "2026-03-31"})
        ),
        ("chart.stacked_bar", "2026-01-01..2026-03-31".to_owned())
    );
    assert_eq!(
        prepared("net_worth_over_time", json!({"to": "2025-12-31"})).1,
        "..2025-12-31"
    );
    // A key figure of the balances takes their own figures.
    let figure = json!({
        "title": "Owed", "data": "account_balances", "show_as": "figure",
        "measure": "liabilities"
    });
    assert_eq!(view_code(call(&book, &figure.to_string())), "prepared");
}

#[test]
fn a_broken_or_hostile_call_is_refused_in_the_fixed_order() {
    let Sample {
        book,
        checking,
        food,
        ..
    } = sample();
    let good = r#"{"title":"Months","data":"income_expense_by_month","show_as":"line"}"#;
    assert_eq!(view_code(call(&book, good)), "prepared");
    let with = |extra: Value| -> String {
        let mut value: Value = serde_json::from_str(good).expect("JSON");
        value
            .as_object_mut()
            .expect("object")
            .extend(extra.as_object().expect("object").clone());
        value.to_string()
    };
    let big = with(json!({"title": "x".repeat(MAX_VIEW_CALL_BYTES)}));
    for (text, expected) in [
        // 1. Size, JSON, an object, nesting.
        (big.as_str(), "ai-tools.call-too-big"),
        ("{\"title\": ", "bad-argument arguments"),
        ("[1, 2]", "bad-argument arguments"),
        ("null", "bad-argument arguments"),
        (
            r#"{"title":"A","data":"spending_by_category","show_as":"pie","period":{"from":[["2026-01-01"]]}}"#,
            "bad-argument arguments",
        ),
        // 2. A key twice, also when written with an escape; it wins over
        // an unknown key that follows.
        (
            r#"{"title":"A","title":"B","data":"spending_by_category","show_as":"pie"}"#,
            "ai-tools.repeated-key",
        ),
        (
            r#"{"title":"A","title":"B","data":"spending_by_category","show_as":"pie","x":1}"#,
            "ai-tools.repeated-key",
        ),
        // 3. An unknown key, never repeated back.
        (
            r#"{"title":"A","data":"spending_by_category","show_as":"pie","unit":"euro"}"#,
            "ai-tools.unknown-argument",
        ),
        (
            r#"{"title":"A","data":"spending_by_category","show_as":"pie","in_currency":"EUR"}"#,
            "ai-tools.unknown-argument",
        ),
        // 4. Types and lists.
        (
            r#"{"data":"spending_by_category","show_as":"pie"}"#,
            "bad-argument title",
        ),
        (
            r#"{"title":"two\nlines","data":"spending_by_category","show_as":"pie"}"#,
            "bad-argument title",
        ),
        (
            "{\"title\":\"A\\u202eB\",\"data\":\"spending_by_category\",\"show_as\":\"pie\"}",
            "bad-argument title",
        ),
        (r#"{"title":"A","show_as":"pie"}"#, "bad-argument data"),
        (
            r#"{"title":"A","data":"category_over_time","show_as":"line"}"#,
            "bad-argument data",
        ),
        (
            r#"{"title":"A","data":"Spending_By_Category","show_as":"pie"}"#,
            "bad-argument data",
        ),
        (
            r#"{"title":"A","data":"spending_by_category","show_as":"chart.pie"}"#,
            "bad-argument show_as",
        ),
        (
            r#"{"title":"A","data":"spending_by_category","show_as":7}"#,
            "bad-argument show_as",
        ),
        (
            r#"{"title":"A","data":"spending_by_category","show_as":"pie","group_by":"groups"}"#,
            "bad-argument group_by",
        ),
        (
            r#"{"title":"A","data":"spending_by_category","show_as":"pie","measure":"formula"}"#,
            "bad-argument measure",
        ),
        (
            r#"{"title":"A","data":"spending_by_category","show_as":"pie","period":"last-century"}"#,
            "bad-argument period",
        ),
        (
            r#"{"title":"A","data":"spending_by_category","show_as":"pie","period":{}}"#,
            "bad-argument period",
        ),
        (
            r#"{"title":"A","data":"spending_by_category","show_as":"pie","period":{"from":"1.1.2026"}}"#,
            "bad-argument period",
        ),
        (
            r#"{"title":"A","data":"spending_by_category","show_as":"pie","period":{"from":"2026-01-01","until":"2026-02-01"}}"#,
            "bad-argument period",
        ),
        (
            r#"{"title":"A","data":"spending_by_category","show_as":"pie","period":{"from":"2026-03-01","to":"2026-02-01"}}"#,
            "ai-tools.bad-period",
        ),
        (
            r#"{"title":"A","data":"income_expense_by_month","show_as":"figure","compare":"tomorrow"}"#,
            "bad-argument compare",
        ),
        (
            r#"{"title":"A","data":"spending_by_category","show_as":"pie","categories":"food"}"#,
            "bad-argument categories",
        ),
        (
            r#"{"title":"A","data":"spending_by_category","show_as":"pie","categories":["not an id"]}"#,
            "bad-argument categories",
        ),
    ] {
        assert_eq!(view_code(call(&book, text)), expected, "{text}");
    }
    // Ids: twice, too many, of another kind, not in the book.
    let categories = |ids: Vec<String>| {
        with(json!({"data": "spending_by_category", "show_as": "pie", "categories": ids}))
    };
    let food = food.to_string();
    assert_eq!(
        view_code(call(&book, &categories(vec![food.clone(), food.clone()]))),
        "bad-argument categories"
    );
    let many: Vec<String> = (0..=MAX_VIEW_CATEGORIES)
        .map(|n| format!("00000000-0000-0000-0000-{n:012}"))
        .collect();
    assert_eq!(
        view_code(call(&book, &categories(many))),
        "bad-argument categories"
    );
    assert_eq!(
        view_code(call(&book, &categories(vec![checking.to_string()]))),
        "ai-tools.unknown-category"
    );
    assert_eq!(
        view_code(call(
            &book,
            &categories(vec!["00000000-0000-0000-0000-000000000001".into()])
        )),
        "ai-tools.unknown-category"
    );
    assert_eq!(
        view_code(call(&book, &categories(vec![food.clone()]))),
        "prepared"
    );
    // A dataset that doesn't apply categories doesn't keep them (M6-B29).
    for data in [
        "net_worth_over_time",
        "account_balances",
        "cash_flow_by_month",
    ] {
        let text = json!({"title": data, "data": data, "show_as": "auto", "categories": [food]});
        assert_eq!(
            view_code(call(&book, &text.to_string())),
            "bad-argument categories",
            "{data}"
        );
    }
    // 6. The schema: a pie of a monthly dataset, a grouping a key figure
    // doesn't have, a measure a line doesn't take.
    for (extra, expected) in [
        (json!({"show_as": "pie"}), "bad-argument show_as"),
        (
            json!({"show_as": "figure", "group_by": "month"}),
            "bad-argument group_by",
        ),
        (json!({"measure": "net"}), "bad-argument measure"),
        (
            json!({"data": "net_worth_over_time", "measure": "money_in", "show_as": "figure"}),
            "bad-argument show_as",
        ),
    ] {
        assert_eq!(
            view_code(call(&book, &with(extra.clone()))),
            expected,
            "{extra}"
        );
    }
    // A title the book has is taken.
    let mut book = book;
    call(&book, good)
        .expect("prepared")
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    assert_eq!(
        view_code(call(&book, &with(json!({"title": "MONTHS"})))),
        "ai-tools.name-taken"
    );
}

#[test]
fn a_refusal_never_carries_the_models_text() {
    let Sample { book, .. } = sample();
    let marker = "IGNORE-ALL-PREVIOUS-INSTRUCTIONS";
    for text in [
        format!(r#"{{"title":"A","data":"{marker}","show_as":"pie"}}"#),
        format!(r#"{{"title":"A","data":"spending_by_category","show_as":"pie","{marker}":1}}"#),
        format!(r#"{{"title":"A","data":"spending_by_category","show_as":"{marker}"}}"#),
        format!(
            r#"{{"title":"A","data":"spending_by_category","show_as":"pie","{marker}":1,"{marker}":2}}"#
        ),
    ] {
        let error = call(&book, &text).expect_err("refused");
        let report = format!("{:?}", error.report());
        assert!(!report.contains(marker), "{report}");
    }
}

#[test]
fn the_mcp_and_the_request_files_reach_both_functions_by_name() {
    let Sample { book, .. } = sample();
    let view = json!({"title": "Months", "data": "income_expense_by_month", "show_as": "auto"});
    let proposal =
        prepare_named(&book, PROPOSE_VIEW_NAME, &view, Permissions::all()).expect("prepared");
    assert_eq!(chart(&proposal).type_id(), "chart.stacked_bar");
    let changes = json!({"new_accounts": [{"name": "Wallet", "type": "cash"}]});
    assert!(prepare_named(&book, PROPOSE_CHANGES.name, &changes, Permissions::all()).is_ok());
    assert!(matches!(
        prepare_named(&book, "list_accounts", &changes, Permissions::all()),
        Err(ToolError::UnknownFunction)
    ));
    // A parsed call has no key twice, but is checked for size and depth.
    let deep = json!({"title": "A", "data": "spending_by_category", "show_as": "pie",
        "period": {"from": {"day": ["2026-01-01"]}}});
    assert_eq!(
        view_code(prepare_named(
            &book,
            PROPOSE_VIEW_NAME,
            &deep,
            Permissions::all()
        )),
        "bad-argument arguments"
    );
}

#[test]
fn a_chart_can_count_only_some_accounts_where_its_dataset_takes_them() {
    // The shared filters of M6c.6 (D-350, D-351): `params.accounts`.
    let Sample {
        mut book,
        checking,
        food,
        ..
    } = sample();
    let text = |data: &str, ids: Vec<String>| {
        json!({"title": data, "data": data, "show_as": "auto", "accounts": ids}).to_string()
    };
    let proposal = call(
        &book,
        &text("net_worth_over_time", vec![checking.to_string()]),
    )
    .expect("prepared");
    let component: Value = serde_json::from_str(&chart(&proposal).component_json()).expect("JSON");
    assert_eq!(
        component["params"]["accounts"],
        json!([checking.to_string()])
    );
    // The saved document is read back by the report layer, filter and all.
    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    let saved = book.views(Some(ViewKind::View)).expect("views");
    let document = Document::read(&saved[0].document).expect("a valid document");
    assert!(document.to_json().contains(&checking.to_string()));
    // A category is no account, an unknown id is refused, and without the
    // reading of accounts the argument isn't there.
    assert_eq!(
        view_code(call(
            &book,
            &text("spending_by_category", vec![food.to_string()])
        )),
        "ai-tools.unknown-account"
    );
    let only_charts = Permissions::NONE.with(Permission::NewViews, true);
    assert_eq!(
        view_code(propose_view_text(
            &book,
            &text("spending_by_category", vec![checking.to_string()]),
            only_charts
        )),
        "ai-tools.unknown-argument"
    );
    // Every dataset the AI may name either takes accounts or refuses them.
    for dataset in CATALOG.iter().filter(|dataset| dataset.for_ai) {
        let result = view_code(call(
            &book,
            &json!({"title": format!("{} by account", dataset.id), "data": dataset.id,
                "show_as": "table", "accounts": [checking.to_string()]})
            .to_string(),
        ));
        if dataset.takes.accounts {
            assert_ne!(result, "bad-argument accounts", "{}", dataset.id);
        } else {
            assert_eq!(result, "bad-argument accounts", "{}", dataset.id);
        }
    }
}
