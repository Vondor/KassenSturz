// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved charts through the AI (US2, #578, #599, D-681): `list_views` lists
//! them with their IDs, `propose_view` with `view_id` changes one and
//! `delete_views` deletes one, each as a change set the user confirms.
//! The charts here are saved the way a model's chart is, so the tests read
//! what `propose_view` really produces.

use kassensturz_core::ViewKind;
use kassensturz_core::ai_switch::Row;
use kassensturz_report::{Document, Item, Setting};

use super::*;
use kassensturz_core::ViewId;

use crate::changes::propose as propose_changes;
use crate::functions::call_allowed;

fn today() -> kassensturz_core::Date {
    "2026-10-31".parse().expect("a day")
}

/// Saves a chart of food spending limited to one category, as the user
/// confirms one the AI prepared, and gives its ID.
fn saved(book: &mut Book, food: AccountId, title: &str) -> String {
    let proposal = propose_view(
        book,
        &json!({
            "title": title,
            "data": "spending_by_category",
            "show_as": "bar",
            "group_by": "category",
            "period": "this-year",
            "categories": [food.to_string()],
        }),
        Permissions::all(),
        today(),
    )
    .expect("prepared");
    proposal
        .changes
        .apply(book, Confirmed::by_the_user())
        .expect("saved");
    let views = book.views(Some(ViewKind::View)).expect("views");
    let view = views
        .iter()
        .find(|view| view.name == title)
        .expect("the saved chart");
    view.id.to_string()
}

/// The one component of a saved chart.
fn component_of(book: &Book, id: &str) -> kassensturz_report::Component {
    let view = book
        .views(Some(ViewKind::View))
        .expect("views")
        .into_iter()
        .find(|view| view.id.to_string() == id)
        .expect("the chart");
    let document = Document::read(&view.document).expect("readable");
    match &document.entries[0].item {
        Item::Component(component) => component.clone(),
        Item::Unknown(_) => panic!("a known component"),
    }
}

fn code(result: Result<Proposal, ToolError>) -> String {
    match result.map_err(ToolError::without_item) {
        Ok(_) => "prepared".into(),
        Err(ToolError::BadArgument { argument }) => format!("bad-argument {argument}"),
        Err(ToolError::NotAllowed { argument }) => format!("not-allowed {argument}"),
        Err(ToolError::NotChangeable { argument, reason }) => {
            format!("not-changeable {argument} {reason}")
        }
        Err(error) => error.report().code.into(),
    }
}

#[test]
fn list_views_gives_each_saved_chart_with_its_id_in_the_words_of_propose_view() {
    let Sample { mut book, food, .. } = sample();
    let id = saved(&mut book, food, "Food this year");
    // A dashboard is not a saved chart: it isn't listed.
    let dashboards = book.views(Some(ViewKind::Dashboard)).expect("views").len();
    let charts = Permissions::NONE.with(Permission::ChartsRead, true);
    let output = call_allowed(&book, "list_views", &Value::Null, today(), charts).expect("listed");
    let listed = output.value["charts"].as_array().expect("charts");
    assert_eq!(listed.len(), 1, "{dashboards} dashboards are left out");
    let chart = &listed[0];
    assert_eq!(chart["view_id"], json!(id));
    assert_eq!(chart["title"], json!("Food this year"));
    assert_eq!(chart["data"], json!("spending_by_category"));
    assert_eq!(chart["show_as"], json!("bar"));
    assert_eq!(chart["group_by"], json!("category"));
    assert_eq!(chart["period"], json!("this-year"));
    // Without Categories › Read the category is not shown, only named.
    assert!(chart.get("categories").is_none());
    assert_eq!(chart["not_shown"], json!(["categories"]));
    let with_categories = charts.with(Permission::CategoriesRead, true);
    let output =
        call_allowed(&book, "list_views", &Value::Null, today(), with_categories).expect("listed");
    let chart = &output.value["charts"][0];
    assert_eq!(chart["categories"], json!([food.to_string()]));
    assert!(chart.get("not_shown").is_none());
    // Without Charts › Read the function is refused and not offered.
    // Changing or deleting a chart gives its reading, so those are off too.
    let without = Permissions::all()
        .with(Permission::ChartsRead, false)
        .with(Permission::EditViews, false)
        .with(Permission::DeleteViews, false);
    assert!(!without.reads(Row::Charts));
    assert!(matches!(
        call_allowed(&book, "list_views", &Value::Null, today(), without),
        Err(ToolError::NotAllowed {
            argument: "list_views"
        })
    ));
    assert!(!crate::readable_functions(without).any(|function| function.name == "list_views"));
    assert!(crate::readable_functions(charts).any(|function| function.name == "list_views"));
}

#[test]
fn a_change_by_view_id_keeps_what_it_does_not_name_and_waits_for_the_confirm() {
    let Sample { mut book, food, .. } = sample();
    let id = saved(&mut book, food, "Food this year");
    let before = component_of(&book, &id);
    let proposal = propose_view(
        &book,
        &json!({"view_id": id, "orientation": "horizontal", "title": "Food by row"}),
        Permissions::all(),
        today(),
    )
    .expect("prepared");
    let [ProposedChange::EditView(edit)] = proposal.changes.changes() else {
        panic!("one changed chart, not {:?}", proposal.changes.changes());
    };
    assert_eq!(edit.before, "Food this year");
    assert_eq!(edit.title, "Food by row");
    assert_eq!(edit.period, "this-year");
    assert_eq!(
        (edit.type_id(), edit.dataset_id()),
        ("chart.bar", "spending_by_category")
    );
    // The slot of the review page is its own list.
    assert!(proposal.changes.item(slot("edit_views/0")).is_some());
    // Nothing changed yet.
    assert_eq!(component_of(&book, &id), before);
    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    let after = component_of(&book, &id);
    assert_eq!(
        after.get("orientation"),
        Some(&Setting::Word("horizontal".into()))
    );
    // What the call didn't name stays: the category and the grouping.
    assert_eq!(after.get("categories"), before.get("categories"));
    assert_eq!(after.get("grouping"), before.get("grouping"));
    let views = book.views(Some(ViewKind::View)).expect("views");
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].name, "Food by row");
    assert_eq!(
        book.history().expect("history")[0].action,
        Action::ApplyAiChanges
    );
    // Undo brings the chart back as it was.
    book.undo().expect("undone");
    assert_eq!(component_of(&book, &id), before);
    assert_eq!(
        book.views(Some(ViewKind::View)).expect("views")[0].name,
        "Food this year"
    );
}

#[test]
fn a_default_word_puts_a_setting_back_and_another_kind_keeps_the_rest() {
    let Sample { mut book, food, .. } = sample();
    let id = saved(&mut book, food, "Food");
    let change = |book: &mut Book, call: Value| {
        propose_view(book, &call, Permissions::all(), today())
            .expect("prepared")
            .changes
            .apply(book, Confirmed::by_the_user())
            .expect("applied");
    };
    change(&mut book, json!({"view_id": id, "orientation": "vertical"}));
    change(&mut book, json!({"view_id": id, "orientation": "auto"}));
    assert_eq!(
        component_of(&book, &id).get("orientation"),
        Some(&Setting::Word("auto".into()))
    );
    // A pie has no orientation; it keeps the categories and the grouping.
    change(&mut book, json!({"view_id": id, "show_as": "pie"}));
    let pie = component_of(&book, &id);
    assert_eq!(pie.kind().id(), "chart.pie");
    assert_eq!(
        pie.get("categories"),
        Some(&Setting::Ids(vec![food.to_string()]))
    );
    // An empty list takes the limit away; page is the data's usual period.
    change(
        &mut book,
        json!({"view_id": id, "categories": [], "period": "page"}),
    );
    assert_eq!(
        component_of(&book, &id).get("categories"),
        Some(&Setting::Ids(Vec::new()))
    );
    let output = call_allowed(
        &book,
        "list_views",
        &Value::Null,
        today(),
        Permissions::all(),
    )
    .expect("listed");
    assert_eq!(output.value["charts"][0]["period"], json!("last-12-months"));
    assert!(output.value["charts"][0].get("categories").is_none());
}

#[test]
fn a_change_is_refused_for_an_unknown_chart_a_taken_name_or_without_its_cell() {
    let Sample { mut book, food, .. } = sample();
    let id = saved(&mut book, food, "Food");
    saved(&mut book, food, "Other");
    // A dashboard's ID is not a chart's. The book checks only the header
    // of what it keeps, so a view's document marked as a dashboard will do.
    let dashboard = book
        .add_view(
            &book.views(Some(ViewKind::View)).expect("views")[0]
                .document
                .replace("\"kind\":\"view\"", "\"kind\":\"dashboard\""),
        )
        .expect("kept");
    let try_call =
        |call: Value, allowed: Permissions| code(propose_view(&book, &call, allowed, today()));
    let unknown = ViewId::generate().to_string();
    assert_eq!(
        try_call(
            json!({"view_id": unknown, "title": "X"}),
            Permissions::all()
        ),
        "ai-tools.unknown-view"
    );
    assert_eq!(
        try_call(
            json!({"view_id": dashboard.to_string(), "title": "X"}),
            Permissions::all()
        ),
        "ai-tools.unknown-view"
    );
    assert_eq!(
        try_call(json!({"view_id": "not an id"}), Permissions::all()),
        "bad-argument view_id"
    );
    assert_eq!(
        try_call(json!({"view_id": id, "title": "other"}), Permissions::all()),
        "ai-tools.name-taken"
    );
    // Its own name in other capitals is no clash.
    assert_eq!(
        try_call(json!({"view_id": id, "title": "FOOD"}), Permissions::all()),
        "prepared"
    );
    assert_eq!(
        try_call(
            json!({"view_id": id, "show_as": "figure", "group_by": "category"}),
            Permissions::all()
        ),
        "bad-argument group_by"
    );
    // Charts › Add alone doesn't change a saved chart, and Charts › Change
    // alone doesn't make a new one.
    let add_only = Permissions::NONE.with(Permission::NewViews, true);
    assert_eq!(
        try_call(json!({"view_id": id, "title": "X"}), add_only),
        "not-allowed view_id"
    );
    let change_only = Permissions::NONE.with(Permission::EditViews, true);
    assert_eq!(
        try_call(
            json!({"title": "X", "data": "spending_by_category", "show_as": "auto"}),
            change_only
        ),
        "not-allowed propose_view"
    );
    // A change needs no title, data or show_as; a new chart does.
    let spec = propose_view_spec(change_only).expect("offered");
    let schema: Value = serde_json::from_str(&spec.parameters).expect("JSON");
    assert_eq!(schema["required"], json!(["view_id"]));
    let spec = propose_view_spec(add_only).expect("offered");
    assert!(!spec.parameters.contains("view_id"));
    // propose_changes can't name the list of changed charts.
    assert_eq!(
        code(propose_changes(
            &book,
            &json!({"edit_views": [{"view_id": id}]}),
            Permissions::all()
        )),
        "ai-tools.unknown-argument"
    );
}

#[test]
fn a_chart_changed_by_the_user_since_makes_the_set_out_of_date() {
    let Sample { mut book, food, .. } = sample();
    let id = saved(&mut book, food, "Food");
    let edit = propose_view(
        &book,
        &json!({"view_id": id, "show_as": "pie"}),
        Permissions::all(),
        today(),
    )
    .expect("prepared");
    let delete = propose_changes(
        &book,
        &json!({"delete_views": [{"view_id": id}]}),
        Permissions::all(),
    )
    .expect("prepared");
    let view_id = book.views(Some(ViewKind::View)).expect("views")[0].id;
    book.rename_view(view_id, "Food by hand").expect("renamed");
    assert!(matches!(
        edit.changes.apply(&mut book, Confirmed::by_the_user()),
        Err(ChangeError::OutOfDate)
    ));
    assert!(matches!(
        delete.changes.apply(&mut book, Confirmed::by_the_user()),
        Err(ChangeError::OutOfDate)
    ));
    assert_eq!(book.views(Some(ViewKind::View)).expect("views").len(), 1);
}

#[test]
fn delete_views_removes_a_saved_chart_once_confirmed_and_undo_brings_it_back() {
    let Sample { mut book, food, .. } = sample();
    let id = saved(&mut book, food, "Food");
    let without = Permissions::all().with(Permission::DeleteViews, false);
    assert_eq!(
        code(propose_changes(
            &book,
            &json!({"delete_views": [{"view_id": id}]}),
            without
        )),
        "not-allowed delete_views"
    );
    assert!(
        !propose_spec(without)
            .expect("spec")
            .parameters
            .contains("delete_views")
    );
    assert_eq!(
        code(propose_changes(
            &book,
            &json!({"delete_views": [{"view_id": id}, {"view_id": id}]}),
            Permissions::all()
        )),
        "not-changeable view_id also-deleted"
    );
    let only = Permissions::NONE.with(Permission::DeleteViews, true);
    let spec = propose_spec(only).expect("offered with Charts › Delete alone");
    assert!(spec.parameters.contains("delete_views"));
    assert!(only.reads(Row::Charts));
    let proposal = propose_changes(&book, &json!({"delete_views": [{"view_id": id}]}), only)
        .expect("prepared");
    let [ProposedChange::DeleteView(delete)] = proposal.changes.changes() else {
        panic!("one deleted chart");
    };
    assert_eq!(delete.name, "Food");
    assert!(
        proposal
            .changes
            .summary()
            .contains("chart \"Food\" is deleted")
    );
    assert_eq!(book.views(Some(ViewKind::View)).expect("views").len(), 1);
    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    assert!(book.views(Some(ViewKind::View)).expect("views").is_empty());
    book.undo().expect("undone");
    let back = book.views(Some(ViewKind::View)).expect("views");
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].id.to_string(), id);
}
