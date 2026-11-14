// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `propose_view` with a daily balance and the lengths of its averages, and
//! the lengths of the monthly trends (US7, #1036, D-794): the AI only
//! chooses words from KassenSturz's lists, a refused word comes back with
//! the list, and a length the data doesn't have is refused, never dropped.

use kassensturz_report::{
    DAILY_PARTS, EMA_FAST_WORDS, EMA_SIGNAL_WORDS, EMA_SLOW_WORDS, TREND_LENGTH_WORDS,
};

use super::*;

fn today() -> kassensturz_core::Date {
    "2026-10-31".parse().expect("a day")
}

/// The component of the one chart a call prepares, as the dashboard
/// file keeps it.
fn chart_props(book: &Book, arguments: &Value) -> Value {
    let proposal = propose_view(book, arguments, Permissions::all(), today()).expect("prepared");
    let [ProposedChange::NewView(new)] = proposal.changes.changes() else {
        panic!("one chart");
    };
    serde_json::from_str(&new.component_json()).expect("JSON")
}

fn refuse(book: &Book, arguments: &Value) -> ToolError {
    propose_view(book, arguments, Permissions::all(), today())
        .expect_err("refused")
        .without_item()
}

#[test]
fn the_schema_offers_the_daily_balance_and_documents_every_length() {
    let spec = propose_view_spec(Permissions::all()).expect("offered");
    let schema: Value = serde_json::from_str(&spec.parameters).expect("JSON");
    let words = |key: &str| -> Vec<String> {
        schema["properties"][key]["enum"]
            .as_array()
            .expect("an enum")
            .iter()
            .map(|word| word.as_str().expect("a word").to_owned())
            .collect()
    };
    assert!(words("data").contains(&"daily_balance".to_owned()));
    for part in DAILY_PARTS {
        assert!(words("group_by").contains(&part.to_owned()), "{part}");
    }
    assert_eq!(words("ema_fast"), EMA_FAST_WORDS);
    assert_eq!(words("ema_slow"), EMA_SLOW_WORDS);
    assert_eq!(words("ema_signal"), EMA_SIGNAL_WORDS);
    assert_eq!(words("trend_length"), TREND_LENGTH_WORDS);
    let data = schema["properties"]["data"]["description"]
        .as_str()
        .expect("text");
    assert!(data.contains("- daily_balance:"));
    assert!(data.contains("stepped line"));
    let fast = schema["properties"]["ema_fast"]["description"]
        .as_str()
        .expect("text");
    assert!(fast.starts_with("Only for daily_balance"));
}

#[test]
fn a_macd_of_the_daily_balance_is_prepared_with_the_chosen_lengths() {
    let Sample { book, checking, .. } = sample();
    // The usual lengths are the chart's defaults.
    let props = chart_props(
        &book,
        &json!({
            "title": "Checking account day by day", "data": "daily_balance",
            "show_as": "line", "period": "last-6-months", "accounts": [checking.to_string()],
            "ema_fast": "12-days", "ema_slow": "26-days", "ema_signal": "9-days"
        }),
    );
    assert_eq!(props["dataset"], "daily_balance");
    assert_eq!(props["type"], "chart.line");
    assert_eq!(props["params"]["grouping"], "balance-averages-macd");
    assert_eq!(props["params"]["ema_fast"], "12-days", "{props}");
    assert_eq!(props["params"]["accounts"][0], checking.to_string());
    // Other lengths are kept.
    let props = chart_props(
        &book,
        &json!({
            "title": "Faster averages", "data": "daily_balance", "show_as": "line",
            "group_by": "macd", "ema_fast": "5-days", "ema_slow": "20-days",
            "ema_signal": "3-days"
        }),
    );
    assert_eq!(props["params"]["ema_fast"], "5-days", "{props}");
    assert_eq!(props["params"]["ema_slow"], "20-days");
    assert_eq!(props["params"]["ema_signal"], "3-days");
    assert_eq!(props["params"]["grouping"], "macd");
    // The monthly MACD takes its lengths too.
    let props = chart_props(
        &book,
        &json!({
            "title": "Saving trend", "data": "macd", "show_as": "line",
            "trend_length": "2-months", "trend_slow": "12-months"
        }),
    );
    assert_eq!(props["params"]["trend_length"], "2-months", "{props}");
    assert_eq!(props["params"]["trend_slow"], "12-months");
}

#[test]
fn a_length_the_data_does_not_have_is_refused_with_a_hint() {
    let Sample { book, .. } = sample();
    for (data, argument, word) in [
        ("income_expense_by_month", "ema_fast", "12-days"),
        ("macd", "ema_slow", "26-days"),
        ("daily_balance", "trend_length", "3-months"),
        ("moving_average", "trend_slow", "6-months"),
    ] {
        let error = refuse(
            &book,
            &json!({"title": "A chart", "data": data, "show_as": "line", argument: word}),
        );
        assert_eq!(
            error.model_report().param("argument"),
            Some(&ErrorParam::Keyword(argument)),
            "{data} {argument}"
        );
        assert_eq!(
            error.hint(),
            Some(crate::error::LENGTH_HINT),
            "{data} {argument}"
        );
        assert_eq!(error.allowed_fields(), None);
    }
}

#[test]
fn a_word_that_is_not_in_its_list_comes_back_with_the_list() {
    let Sample { book, .. } = sample();
    // A data that doesn't exist.
    let error = refuse(
        &book,
        &json!({"title": "A chart", "data": "daily_balances", "show_as": "line"}),
    );
    assert_eq!(
        error.model_report().param("argument"),
        Some(&ErrorParam::Keyword("data"))
    );
    assert_eq!(
        error.allowed_fields().map(<[_]>::to_vec),
        view_choices("data")
    );
    // A length of the wrong kind.
    let error = refuse(
        &book,
        &json!({"title": "A chart", "data": "daily_balance", "show_as": "line",
            "ema_fast": "12-months"}),
    );
    assert_eq!(error.allowed_fields(), Some(&EMA_FAST_WORDS[..]));
    assert_eq!(error.hint(), None, "the list says it all");
    // A grouping of another data: the words of this one.
    for show_as in ["line", "auto"] {
        let error = refuse(
            &book,
            &json!({"title": "A chart", "data": "daily_balance", "show_as": show_as,
                "group_by": "month"}),
        );
        assert_eq!(
            error.model_report().param("argument"),
            Some(&ErrorParam::Keyword("group_by")),
            "{show_as}"
        );
        assert_eq!(error.allowed_fields(), Some(&DAILY_PARTS[..]), "{show_as}");
    }
}
