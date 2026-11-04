// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reply shapes that Qwen models behind local servers sent in the stress
//! test of 2026-11-03 (M6-B166 to M6-B171, D-593), through the whole chat:
//! a call written as text runs like a native one, the same call twice in
//! one reply runs once, an answer only in the thinking field is an error
//! and not an empty answer, and thinking is never shown.

#![cfg(feature = "ai")]

mod common;

use common::{Script, Step, TODAY, calls, says, settings, space_colony};
use kassensturz_ai::{KeyUse, Role};
use kassensturz_ai_chat::{BuildInfo, ChatError, ChatSession, NoHooks};
use kassensturz_ai_tools::PROPOSE_CHANGES;
use serde_json::json;

fn session() -> ChatSession {
    ChatSession::new(BuildInfo::new("0.261025.0", "1775d22"))
        .with_permissions(kassensturz_core::ai_switch::Permissions::all())
}

fn ask(
    steps: Vec<Step>,
) -> (
    Result<kassensturz_ai_chat::Answer, ChatError>,
    Script,
    ChatSession,
) {
    let (provider, script) = Script::provider(settings(true, None, KeyUse::None), steps);
    let mut chat = session();
    let answer = chat.ask(
        &space_colony(),
        &provider,
        None,
        "What is on my accounts?",
        TODAY,
        &mut NoHooks,
    );
    (answer, script, chat)
}

/// A reply whose message is `message`, ending with `finish`.
fn replies(message: serde_json::Value, finish: &str) -> Step {
    let body = json!({"choices": [{"finish_reason": finish, "message": message}]}).to_string();
    Box::new(move |_, _| Ok(body.clone()))
}

#[test]
fn a_call_written_as_text_runs_like_a_native_one() {
    for text in [
        "<tool_call>\n{\"name\": \"list_accounts\", \"arguments\": {}}\n</tool_call>",
        "<tool_call>\n<function=list_accounts>\n</function>\n</tool_call>",
    ] {
        let (answer, script, _) = ask(vec![
            replies(json!({"role": "assistant", "content": text}), "stop"),
            says("You have several accounts."),
        ]);
        assert_eq!(answer.unwrap().text, "You have several accounts.");
        let seen = script.seen();
        assert_eq!(seen.len(), 2, "{text}");
        let result = seen[1]
            .messages
            .iter()
            .find(|message| message.role == Role::Tool)
            .expect("the function's result");
        assert!(result.content.contains("Everyday Checking"), "{text}");
    }
}

#[test]
fn the_same_call_twice_in_one_reply_runs_once() {
    let arguments =
        json!({"new_categories": [{"key": "pets", "name": "Pets", "currency": "EUR"}]}).to_string();
    let (answer, script, _) = ask(vec![
        calls(&[
            ("a", PROPOSE_CHANGES.name, &arguments),
            ("a-dup", PROPOSE_CHANGES.name, &arguments),
        ]),
        calls(&[("b", "list_accounts", "{}")]),
        says("I prepared the category Pets for you to confirm."),
    ]);
    let answer = answer.unwrap();
    assert_eq!(answer.changes.len(), 1, "one change set, not two");
    let seen = script.seen();
    // The duplicate was not counted as a repeat, so the model could still
    // call a function afterwards.
    assert_eq!(seen.len(), 3);
    assert!(!seen[1].offered.is_empty());
    let results: Vec<_> = seen[1]
        .messages
        .iter()
        .filter(|message| message.role == Role::Tool)
        .collect();
    assert_eq!(results.len(), 2, "each call got its answer");
    assert_eq!(results[0].content, results[1].content);
}

#[test]
fn an_answer_only_in_the_thinking_field_is_an_error() {
    let only_thinking = || {
        replies(
            json!({"role": "assistant", "content": "", "reasoning_content": "The balance is 12.00."}),
            "stop",
        )
    };
    let (answer, script, chat) = ask(vec![only_thinking(), only_thinking()]);
    assert!(matches!(answer, Err(ChatError::EmptyAnswer)), "{answer:?}");
    assert_eq!(script.seen().len(), 2, "asked once more first");
    assert!(
        chat.history().is_empty(),
        "the failed question left nothing"
    );
}

#[test]
fn thinking_is_not_shown() {
    let (answer, _, _) = ask(vec![says(
        "<think>\nThe user wants 9,999.99, maybe.\n</think>\n\nPlease name the account.",
    )]);
    let answer = answer.unwrap();
    assert_eq!(answer.text, "Please name the account.");
    assert!(answer.unverified.is_empty());
}
