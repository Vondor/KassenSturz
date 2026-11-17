// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A month-by-month question through the chat (US7, #852, T-65): one call of
//! `monthly_totals` answers it, its result is far smaller than twelve calls
//! of `category_totals`, and the figures in it pass the number check while
//! a made-up one does not. The real session, the real functions, the Space
//! Colony book and a scripted model.

#![cfg(feature = "ai")]

mod common;

use common::{Script, Step, TODAY, calls, says, settings, space_colony};
use kassensturz_ai::{KeyUse, Role};
use kassensturz_ai_chat::{BuildInfo, ChatSession, NoHooks};
use kassensturz_ai_tools::call;
use kassensturz_core::ai_switch::Permissions;
use serde_json::{Value, json};

/// A model that, after the function's result, says what it found in
/// `month` (0 is January), plus `extra`.
fn quotes(month: usize, extra: &'static str) -> Step {
    Box::new(move |messages, _| {
        let result = messages
            .iter()
            .rev()
            .find(|message| message.role == Role::Tool)
            .expect("a function result");
        let result: Value = serde_json::from_str(&result.content).expect("JSON");
        let spent = result["months"][month]["currencies"][0]["money_out"]
            .as_str()
            .expect("a figure")
            .to_owned();
        let text = format!("In that month you spent {spent}.{extra}");
        Ok(json!({"choices": [{"finish_reason": "stop",
            "message": {"role": "assistant", "content": text}}]})
        .to_string())
    })
}

fn ask(steps: Vec<Step>) -> (kassensturz_ai_chat::Answer, ChatSession) {
    let (provider, _script) = Script::provider(settings(true, None, KeyUse::None), steps);
    let mut chat = ChatSession::new(BuildInfo::new("0.261025.0", "1775d22"))
        .with_permissions(Permissions::all());
    let answer = chat
        .ask(
            &space_colony(),
            &provider,
            None,
            "How much did I spend each month this year?",
            TODAY,
            &mut NoHooks,
        )
        .unwrap();
    (answer, chat)
}

const YEAR: &str = r#"{"from":"2026-01-01","to":"2026-12-31"}"#;

#[test]
fn one_call_answers_each_month_and_its_figures_pass_the_number_check() {
    let (answer, chat) = ask(vec![
        calls(&[("c1", "monthly_totals", YEAR)]),
        quotes(9, ""),
    ]);
    assert!(answer.unverified.is_empty(), "{:?}", answer.unverified);
    assert!(
        chat.log()
            .iter()
            .any(|entry| format!("{entry:?}").contains("monthly_totals")),
        "the call is in the tool-call log"
    );
}

#[test]
fn a_figure_the_model_made_up_is_flagged() {
    let (answer, _) = ask(vec![
        calls(&[("c1", "monthly_totals", YEAR)]),
        quotes(9, " In total that is 123456.78."),
    ]);
    let flagged: Vec<&str> = answer.unverified.iter().map(|u| u.text.as_str()).collect();
    assert_eq!(flagged, ["123456.78"]);
}

#[test]
fn the_year_in_one_call_is_far_smaller_than_twelve_calls() {
    let book = space_colony();
    let one = call(
        &book,
        "monthly_totals",
        &serde_json::from_str::<Value>(YEAR).unwrap(),
        TODAY,
    )
    .unwrap();
    let one = one.value.to_string().len();
    let twelve: usize = (1..=12)
        .map(|month| {
            let last = ["31", "28", "31", "30", "31", "30", "31", "31", "30", "31", "30", "31"]
                [month - 1];
            call(
                &book,
                "category_totals",
                &json!({"from": format!("2026-{month:02}-01"), "to": format!("2026-{month:02}-{last}")}),
                TODAY,
            )
            .unwrap()
            .value
            .to_string()
            .len()
        })
        .sum();
    assert!(
        one * 3 < twelve,
        "one call {one} bytes, twelve calls {twelve} bytes"
    );
}

#[test]
fn the_model_is_offered_it() {
    let (provider, script) = Script::provider(
        settings(true, None, KeyUse::None),
        vec![says("Nothing to look up.")],
    );
    ChatSession::new(BuildInfo::new("0.261025.0", "1775d22"))
        .with_permissions(Permissions::all())
        .ask(
            &space_colony(),
            &provider,
            None,
            "Hello",
            TODAY,
            &mut NoHooks,
        )
        .unwrap();
    assert!(
        script.seen()[0]
            .offered
            .iter()
            .any(|name| name == "monthly_totals")
    );
}
