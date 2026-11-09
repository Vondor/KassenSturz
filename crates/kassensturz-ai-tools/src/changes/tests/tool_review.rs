// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the US1 review of the AI tools found (M6-B200, #568): a Rule whose
//! conditions `list_rules` can't show exactly is marked and can't get its
//! conditions replaced, search results name the other party Rules test,
//! and what `propose_view` says each data takes is what it accepts.

use kassensturz_core::ImportedDetails;
use kassensturz_report::CATALOG;

use super::*;

fn today() -> Date {
    Date::constant(2026, 10, 31)
}

fn read(book: &Book, name: &str, arguments: Value) -> Value {
    crate::call(book, name, &arguments, today())
        .expect("answered")
        .value
}

fn text_rule(name: &str, value: &str, category: AccountId) -> Rule {
    Rule {
        id: RuleId::generate(),
        name: name.into(),
        match_mode: MatchMode::Any,
        conditions: vec![Condition::Text {
            field: TextField::OtherPartyName,
            operator: TextOperator::Matches,
            value: value.into(),
            case_sensitive: false,
        }],
        actions: Actions {
            book_to: Some(BookTo::Category(category)),
            payee: None,
        },
    }
}

/// The `conditions_complete` of the Rule named `name` in `list_rules`.
fn complete(book: &Book, name: &str) -> Value {
    let rules = read(book, "list_rules", json!({}));
    rules["rules"]
        .as_array()
        .expect("rules")
        .iter()
        .find(|rule| rule["name"] == json!(name))
        .unwrap_or_else(|| panic!("{name} in {rules}"))["conditions_complete"]
        .clone()
}

#[test]
fn a_rule_shown_cut_is_marked_and_its_conditions_are_not_replaced() {
    let Sample {
        mut book,
        groceries,
        ..
    } = sample();
    // A pattern may be 1000 characters long; list_rules shows 200 of them.
    let long = format!("^({})$", vec!["grocer"; 60].join("|"));
    assert!(long.chars().count() > crate::MAX_TEXT_CHARS);
    let cut = text_rule("Long pattern", &long, groceries);
    let iban = text_rule("Names an IBAN", "DE89370400440532013000", groceries);
    let short = text_rule("Short pattern", "^grocer(y)?$", groceries);
    for rule in [&cut, &iban, &short] {
        book.add_rule(rule, None).expect("Rule");
    }
    assert_eq!(complete(&book, "Long pattern"), json!(false));
    assert_eq!(complete(&book, "Names an IBAN"), json!(false));
    assert_eq!(complete(&book, "Short pattern"), json!(true));

    let shown = read(&book, "list_rules", json!({}));
    let shown_text = shown["rules"]
        .as_array()
        .expect("rules")
        .iter()
        .find(|rule| rule["name"] == json!("Long pattern"))
        .expect("listed")["conditions"][0]["text"]
        .as_str()
        .expect("text")
        .to_owned();
    assert!(shown_text.ends_with('…'), "{shown_text}");
    for rule in [&cut, &iban] {
        // What a model would send back: the text as it was shown.
        let call = json!({"edit_rules": [{
            "rule_id": rule.id.to_string(),
            "conditions": [{"field": "other_party_name", "test": "matches_pattern", "text": shown_text}],
        }]});
        match propose(&book, &call).map_err(ToolError::without_item) {
            Err(ToolError::NotChangeable {
                argument: "conditions",
                reason: "conditions-hidden",
            }) => {}
            other => panic!("{}: {other:?}", rule.name),
        }
        // Its name and category can still change.
        let rename = json!({"edit_rules": [{"rule_id": rule.id.to_string(), "name": format!("{} 2", rule.name)}]});
        propose(&book, &rename).expect("renamed");
    }
    let call = json!({"edit_rules": [{
        "rule_id": short.id.to_string(),
        "conditions": [{"field": "other_party_name", "test": "matches_pattern", "text": "^grocer(y|s)?$"}],
    }]});
    let proposal = propose(&book, &call).expect("a Rule shown in full");
    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    match &book.rule(cut.id).expect("Rule").conditions[0] {
        Condition::Text { value, .. } => assert_eq!(*value, long),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_search_row_names_the_other_party_when_the_payee_says_something_else() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let mut paid = Split::new(checking, eur(-850));
    paid.imported = Some(ImportedDetails {
        other_party_name: Some("Hillside Bakery Ltd".into()),
        ..ImportedDetails::default()
    });
    let mut same = Split::new(checking, eur(-120));
    same.imported = Some(ImportedDetails {
        other_party_name: Some("Kiosk ".into()),
        ..ImportedDetails::default()
    });
    bought(
        &mut book,
        "Cash",
        vec![paid, Split::new(groceries, eur(850))],
    );
    bought(
        &mut book,
        "Kiosk",
        vec![same, Split::new(groceries, eur(120))],
    );

    // The text filter finds the other party's name, as it always did.
    let found = read(&book, "search_transactions", json!({"text": "hillside"}));
    assert_eq!(found["matches"], json!(1), "{found}");
    let row = &found["rows"][0];
    assert_eq!(row["payee"], json!("Cash"));
    assert_eq!(row["other_party_name"], json!("Hillside Bakery Ltd"));

    let found = read(&book, "search_transactions", json!({"text": "kiosk"}));
    let row = &found["rows"][0];
    assert_eq!(row["payee"], json!("Kiosk"));
    assert!(row.get("other_party_name").is_none(), "{row}");

    let search = FUNCTIONS
        .iter()
        .find(|function| function.name == "search_transactions")
        .expect("offered");
    assert!(search.description.contains("other_party_name"));
    assert!(search.parameters.contains("the other party's name"));
}

/// Prepares a chart; the code it gives, "prepared" when it is.
fn chart_code(book: &Book, arguments: &Value) -> String {
    match propose_view(book, arguments, Permissions::all(), today())
        .map_err(ToolError::without_item)
    {
        Ok(_) => "prepared".into(),
        Err(error) => error.report().code.into(),
    }
}

#[test]
fn what_the_description_lists_per_data_is_what_propose_view_takes() {
    let Sample { book, .. } = sample();
    let spec = propose_view_spec(Permissions::all()).expect("offered");
    // The list per data is the description of `data` since M6-B201.
    let schema: Value = serde_json::from_str(&spec.parameters).expect("JSON");
    let help = schema["properties"]["data"]["description"]
        .as_str()
        .expect("text");
    let all_ways: Vec<&str> = view_choices("show_as").expect("a choice");
    let mut checked = 0;
    for dataset in CATALOG.iter().filter(|dataset| dataset.for_ai) {
        let fits = fits(dataset);
        let line = help
            .lines()
            .find(|line| line.starts_with(&format!("- {}: ", dataset.id)))
            .unwrap_or_else(|| panic!("{} has a line", dataset.id));
        assert!(
            line.contains(&format!("show_as: auto, {}.", fits.show_as.join(", "))),
            "{line}"
        );
        for way in &all_ways {
            let arguments = json!({"title": "T", "data": dataset.id, "show_as": way});
            let expected = if *way == "auto" || fits.show_as.contains(way) {
                "prepared"
            } else {
                "ai-tools.bad-argument"
            };
            assert_eq!(
                chart_code(&book, &arguments),
                expected,
                "{} as {way}",
                dataset.id
            );
            checked += 1;
        }
        for way in &fits.show_as {
            for grouping in &fits.group_by {
                let arguments =
                    json!({"title": "T", "data": dataset.id, "show_as": way, "group_by": grouping});
                let expected = if fits.grouped_as.contains(way) {
                    "prepared"
                } else {
                    "ai-tools.bad-argument"
                };
                assert_eq!(
                    chart_code(&book, &arguments),
                    expected,
                    "{} as {way} by {grouping}",
                    dataset.id
                );
            }
        }
        if fits.show_as.contains(&"figure") {
            for measure in &fits.measure {
                let arguments = json!({"title": "T", "data": dataset.id, "show_as": "figure", "measure": measure});
                assert_eq!(
                    chart_code(&book, &arguments),
                    "prepared",
                    "{} measures {measure}",
                    dataset.id
                );
            }
        }
        assert!(
            line.contains(&format!("Usual period: {}.", default_period(dataset.id))),
            "{line}"
        );
    }
    assert!(checked > 0);
}

#[test]
fn every_period_the_formula_help_names_after_at_is_taken() {
    let Sample { book, .. } = sample();
    let help = formula_help();
    for words in [
        "this month",
        "last month",
        "this quarter",
        "last quarter",
        "this year",
        "last year",
        "last N months",
        "last N complete months",
    ] {
        assert!(help.contains(&format!("\"{words}\"")), "{words}");
    }
    let at = |period: &str| {
        json!({
            "title": "T",
            "data": "income_expense_by_month",
            "show_as": "figure",
            "formula": format!("=Groceries@{period} + Spending.ALL@{period}"),
        })
    };
    for period in [
        "\"this month\"",
        "\"last month\"",
        "\"this quarter\"",
        "\"last quarter\"",
        "\"this year\"",
        "\"last year\"",
        "\"last 3 months\"",
        "\"last 120 complete months\"",
        "2026",
        "2026-09",
        "2026-09-15",
        "2026-01..2026-03",
    ] {
        assert_eq!(chart_code(&book, &at(period)), "prepared", "{period}");
    }
    // No salary month after @, as the help says.
    assert_eq!(
        chart_code(&book, &at("\"this salary month 25\"")),
        "ai-tools.bad-formula"
    );
}
