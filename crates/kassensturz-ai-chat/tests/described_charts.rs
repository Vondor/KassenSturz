// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The fake model's described charts (M6c.10b, D-356, D-411): every fixed
//! description, asked through the chat on the synthetic Space Colony book,
//! prepares one chart through the real `propose_view`, its schema, the
//! catalog's datasets and the book's own ids; together they cover every
//! dataset the AI may name, `auto`, a range, a comparison, category and
//! account ids and one refused call that is corrected.

#![cfg(feature = "ai")]

mod common;

use common::{TODAY, settings, space_colony};
use kassensturz_ai::KeyUse;
use kassensturz_ai_chat::{
    Answer, CHART_PREPARED, ChatSession, DEMO_LABEL, DESCRIBED_CHARTS, DemoModel, DescribedChart,
    LogEntry, NoHooks, demo_provider,
};
use kassensturz_ai_tools::{PROPOSE_VIEW_NAME, ProposedChange, view_choices};
use kassensturz_core::ai_switch::{Permission, Permissions};
use kassensturz_store::Book;
use serde_json::Value;

/// The calls of `propose_view` in the log: the arguments and the error.
fn view_calls(chat: &ChatSession) -> Vec<(Value, Option<&'static str>)> {
    chat.log()
        .iter()
        .filter_map(|entry| match entry {
            LogEntry::Called {
                function,
                arguments,
                error,
                ..
            } if function == PROPOSE_VIEW_NAME => {
                Some((serde_json::from_str(arguments).unwrap(), *error))
            }
            _ => None,
        })
        .collect()
}

fn functions_called(chat: &ChatSession) -> Vec<String> {
    chat.log()
        .iter()
        .filter_map(|entry| match entry {
            LogEntry::Called { function, .. } => Some(function.clone()),
            _ => None,
        })
        .collect()
}

fn ask(book: &Book, allowed: Permissions, question: &str) -> (ChatSession, Answer) {
    let provider = demo_provider(settings(true, None, KeyUse::None), DemoModel::Functions);
    let mut chat = ChatSession::new(kassensturz_ai_chat::BuildInfo::new("0.261031.0", "0000000"))
        .with_permissions(allowed);
    let answer = chat
        .ask(book, &provider, None, question, TODAY, &mut NoHooks)
        .expect("an answer");
    (chat, answer)
}

fn fixed(call: &str) -> Value {
    serde_json::from_str(call).unwrap()
}

#[test]
fn every_description_prepares_its_chart_through_the_real_function() {
    let book = space_colony();
    for chart in &DESCRIBED_CHARTS {
        let (chat, answer) = ask(&book, Permissions::all(), chart.description);
        let calls = view_calls(&chat);
        let tries = if chart.refused_first.is_some() { 2 } else { 1 };
        assert_eq!(calls.len(), tries, "{}: {calls:?}", chart.description);
        assert_eq!(calls.last().unwrap().1, None, "{}", chart.description);
        assert!(answer.chart_failed.is_none(), "{}", chart.description);
        assert_eq!(
            answer.text,
            format!("{DEMO_LABEL} {CHART_PREPARED}"),
            "{}",
            chart.description
        );
        let [set] = <[_; 1]>::try_from(answer.changes).expect(chart.description);
        let [ProposedChange::NewView(view)] = set.changes() else {
            panic!("{}: one new chart", chart.description);
        };
        let call = fixed(chart.call);
        assert_eq!(view.dataset_id(), call["data"], "{}", chart.description);
        assert_eq!(view.title, call["title"], "{}", chart.description);
        // The ids came from the read function, never from the fake's text.
        if let Some(ids) = chart.ids {
            assert_eq!(
                functions_called(&chat).first().map(String::as_str),
                Some(ids.function)
            );
            let sent = &calls.last().unwrap().0[ids.list];
            assert_eq!(sent.as_array().map(Vec::len), Some(1), "{sent}");
        }
    }
}

#[test]
fn the_ids_are_the_books_food_category_and_checking_account() {
    let book = space_colony();
    let names: Vec<(String, String)> = book
        .all_accounts()
        .unwrap()
        .into_iter()
        .map(|account| (account.id.to_string(), account.name))
        .collect();
    for chart in DESCRIBED_CHARTS.iter().filter(|chart| chart.ids.is_some()) {
        let ids = chart.ids.unwrap();
        let (chat, _) = ask(&book, Permissions::all(), chart.description);
        let sent = view_calls(&chat).last().unwrap().0[ids.list][0].clone();
        let name = names
            .iter()
            .find(|(id, _)| sent == id.as_str())
            .map(|(_, name)| name.to_lowercase())
            .expect("an id of the book");
        assert!(name.contains(ids.named), "{name}");
    }
}

#[test]
fn the_refused_call_is_refused_by_the_schema_and_then_corrected() {
    let book = space_colony();
    let chart = DESCRIBED_CHARTS
        .iter()
        .find(|chart| chart.refused_first.is_some())
        .expect("one refused call");
    let (chat, _) = ask(&book, Permissions::all(), chart.description);
    let calls = view_calls(&chat);
    assert_eq!(calls[0].0, fixed(chart.refused_first.unwrap()));
    assert_eq!(calls[0].1, Some("ai-tools.bad-argument"));
    assert_eq!(calls[1].0, fixed(chart.call));
}

#[test]
fn the_descriptions_cover_every_dataset_and_each_kind_of_argument() {
    let calls: Vec<Value> = DESCRIBED_CHARTS
        .iter()
        .map(|chart| fixed(chart.call))
        .collect();
    // Every dataset the AI may name, read from the real catalog, so a new
    // one needs a description of its own.
    for dataset in view_choices("data").unwrap() {
        assert!(
            calls.iter().any(|call| call["data"] == dataset),
            "no description names {dataset}"
        );
    }
    let has = |test: &dyn Fn(&Value) -> bool| calls.iter().any(test);
    assert!(has(&|call| call["show_as"] == "auto"));
    assert!(has(&|call| call["period"].is_object()));
    assert!(has(
        &|call| call["compare"].is_string() && call["compare"] != "none"
    ));
    let lists: Vec<&str> = DESCRIBED_CHARTS
        .iter()
        .filter_map(|chart| chart.ids.map(|ids| ids.list))
        .collect();
    assert!(lists.contains(&"categories") && lists.contains(&"accounts"));
    assert_eq!(
        DESCRIBED_CHARTS
            .iter()
            .filter(|chart| chart.refused_first.is_some())
            .count(),
        1
    );
}

#[test]
fn without_reading_categories_the_chart_is_prepared_without_them() {
    let book = space_colony();
    let chart: &DescribedChart = &DESCRIBED_CHARTS[0];
    let allowed = Permissions::defaults().with(Permission::NewViews, true);
    let (chat, answer) = ask(&book, allowed, chart.description);
    assert!(!functions_called(&chat).contains(&"list_categories".to_owned()));
    let calls = view_calls(&chat);
    assert_eq!(calls.len(), 1);
    assert!(calls[0].0.get("categories").is_none(), "{:?}", calls[0].0);
    assert_eq!(answer.changes.len(), 1);
}

#[test]
fn the_german_example_of_the_button_prepares_the_same_chart() {
    let book = space_colony();
    let chart = &DESCRIBED_CHARTS[0];
    let (chat, answer) = ask(&book, Permissions::all(), chart.also[0]);
    assert_eq!(answer.changes.len(), 1);
    assert_eq!(view_calls(&chat).len(), 1);
}

#[test]
fn without_charts_in_the_grid_a_description_prepares_nothing() {
    let book = space_colony();
    let allowed = Permissions::all().with(Permission::NewViews, false);
    let (chat, answer) = ask(&book, allowed, DESCRIBED_CHARTS[1].description);
    assert!(view_calls(&chat).is_empty());
    assert!(answer.changes.is_empty());
}
