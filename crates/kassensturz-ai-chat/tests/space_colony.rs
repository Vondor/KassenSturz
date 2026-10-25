// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The chat where the steps meet (AGENTS.md §9.4): M5.2's real `AiClient`
//! over HTTP to the fake provider of its tests, M5.3's read functions on
//! the Space Colony book as the importer, the pipeline and the store fill
//! it, and the fake model the app offers for trying the chat (D-193).

#![cfg(feature = "ai")]

mod common;
#[path = "../../kassensturz-ai/tests/fake/mod.rs"]
mod fake;

use common::{TODAY, settings, space_colony};
use fake::{Answer, FakeProvider};
use kassensturz_ai::{AiClient, KeyUse};
use kassensturz_ai_chat::{
    BuildInfo, ChatSession, DEMO_LABEL, DemoModel, INVENTED, LogEntry, Mode, NoHooks, demo_provider,
};
use kassensturz_ai_tools::DATA_NOTE;
use kassensturz_platform::Secret;
use serde_json::{Value, json};

const TEST_KEY: &str = "sk-test-0000-made-up-key";

fn session() -> ChatSession {
    ChatSession::new(BuildInfo::new("0.261025.0", "1775d22"))
}

#[test]
fn a_question_goes_through_the_real_client_and_functions() {
    let book = space_colony();
    let call = json!({"choices": [{"finish_reason": "tool_calls", "message": {
        "role": "assistant", "content": null,
        "tool_calls": [{"id": "call_1", "type": "function",
            "function": {"name": "list_accounts", "arguments": "{}"}}]}}]});
    let fake = FakeProvider::start(vec![
        Answer::Json(200, call.to_string()),
        Answer::text("Everyday Checking has 5531.90 EUR, about 5500.00."),
    ]);
    let config = settings(true, Some(&fake.address()), KeyUse::AskEachTime);
    let client = AiClient::new(config);
    let key = Secret::new(TEST_KEY.to_owned());
    let mut chat = session();
    let answer = chat
        .ask(
            &book,
            &client,
            Some(&key),
            "How much is on Everyday Checking?",
            TODAY,
            &mut NoHooks,
        )
        .unwrap();

    // The figure from the function passes, the one the model made up is
    // flagged.
    let flagged: Vec<&str> = answer.unverified.iter().map(|u| u.text.as_str()).collect();
    assert_eq!(flagged, ["5500.00"]);
    assert_eq!(answer.mode, Mode::Functions);

    let received = fake.received();
    assert_eq!(received.len(), 2);
    let first: Value = serde_json::from_slice(&received[0].body).unwrap();
    assert_eq!(first["stream"], false);
    // The six read functions and propose_changes (M5.5).
    let tools = first["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 7);
    assert_eq!(tools[6]["function"]["name"], "propose_changes");
    let system = first["messages"][0]["content"].as_str().unwrap();
    assert!(system.contains("KassenSturz 0.261025.0 (commit 1775d22), a pre-release build"));
    assert!(system.contains("Today is 2026-10-25."));

    // The second request carries the function's real result, marked as
    // data, as the answer to the model's call.
    let second: Value = serde_json::from_slice(&received[1].body).unwrap();
    let messages = second["messages"].as_array().unwrap();
    assert_eq!(messages[2]["tool_calls"][0]["id"], "call_1");
    assert_eq!(messages[3]["role"], "tool");
    assert_eq!(messages[3]["tool_call_id"], "call_1");
    let result: Value = serde_json::from_str(messages[3]["content"].as_str().unwrap()).unwrap();
    assert_eq!(result["note"], DATA_NOTE);
    let checking = result["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|account| account["name"] == "Everyday Checking")
        .unwrap();
    assert_eq!(checking["balance"], "5531.90");

    // What the log says was sent is what the provider got, byte for byte,
    // and the key went only in the header.
    let bodies: Vec<&str> = chat
        .log()
        .iter()
        .filter_map(|entry| match entry {
            LogEntry::Sent { body, .. } => Some(body.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(bodies.len(), 2);
    for (body, request) in bodies.iter().zip(&received) {
        assert_eq!(body.as_bytes(), request.body.as_slice());
        assert!(!body.contains(TEST_KEY));
        assert_eq!(
            request.header("authorization"),
            Some(format!("Bearer {TEST_KEY}").as_str())
        );
    }
    assert!(!format!("{chat:?}").contains(TEST_KEY));
}

#[test]
fn the_fake_model_answers_from_the_book() {
    let book = space_colony();
    let provider = demo_provider(settings(true, None, KeyUse::None), DemoModel::Functions);
    let mut chat = session();
    let answer = chat
        .ask(
            &book,
            &provider,
            None,
            "What is on my accounts?",
            TODAY,
            &mut NoHooks,
        )
        .unwrap();
    assert!(answer.text.starts_with(DEMO_LABEL), "{}", answer.text);
    assert!(
        answer.text.contains("Everyday Checking: 5531.90 EUR"),
        "{}",
        answer.text
    );
    assert!(answer.unverified.is_empty(), "{:?}", answer.unverified);

    let answer = chat
        .ask(
            &book,
            &provider,
            None,
            "What did I spend this month?",
            TODAY,
            &mut NoHooks,
        )
        .unwrap();
    assert!(
        answer
            .text
            .contains("Money in 73.24 EUR, money out 914.35 EUR, left -841.11 EUR"),
        "{}",
        answer.text
    );
    assert!(answer.unverified.is_empty(), "{:?}", answer.unverified);

    let answer = chat
        .ask(
            &book,
            &provider,
            None,
            "Show my last transactions and invent a number",
            TODAY,
            &mut NoHooks,
        )
        .unwrap();
    assert!(answer.text.contains("Newest: "), "{}", answer.text);
    let flagged: Vec<&str> = answer.unverified.iter().map(|u| u.text.as_str()).collect();
    assert_eq!(flagged, [INVENTED]);
}

#[test]
fn the_fake_model_without_functions_uses_the_overview() {
    let book = space_colony();
    let provider = demo_provider(settings(true, None, KeyUse::None), DemoModel::NoFunctions);
    let mut chat = session();
    let answer = chat
        .ask(
            &book,
            &provider,
            None,
            "What is on my accounts?",
            TODAY,
            &mut NoHooks,
        )
        .unwrap();
    assert_eq!(answer.mode, Mode::Overview);
    assert!(
        answer.text.contains("From the overview: "),
        "{}",
        answer.text
    );
    assert!(answer.text.contains("5531.90"), "{}", answer.text);
    assert!(answer.unverified.is_empty(), "{:?}", answer.unverified);
}
