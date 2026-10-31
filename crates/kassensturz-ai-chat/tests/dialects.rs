// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The chat against the four providers KassenSturz is tried with (M5.11,
//! D-206 to D-209), where the steps meet (AGENTS.md §9.4): M5.2's real
//! `AiClient` over HTTP to the dialect fakes of its tests, M5.3's read
//! functions on the Space Colony book, and the chat's own tool loop. Every
//! request the chat sends, with its real function list, must pass the
//! rules of the provider's documentation (`dialect::check_request`).

#![cfg(feature = "ai")]

mod common;
#[path = "../../kassensturz-ai/tests/dialect/mod.rs"]
mod dialect;
#[path = "../../kassensturz-ai/tests/fake/mod.rs"]
mod fake;

use common::{TODAY, settings, space_colony};
use dialect::{ALL, Dialect, TEXT, check_request};
use fake::{Answer, FakeProvider};
use kassensturz_ai::{AiClient, AiError, Finish, KeyUse};
use kassensturz_ai_chat::{BuildInfo, ChatError, ChatSession, LogEntry, Mode, NoHooks};
use kassensturz_platform::Secret;
use serde_json::Value;

const TEST_KEY: &str = "sk-test-0000-made-up-key";

fn session() -> ChatSession {
    ChatSession::new(BuildInfo::new("0.261025.0", "1775d22"))
        .with_permissions(kassensturz_core::ai_switch::Permissions::all())
}

/// A fake that gives `answers` and a client for the provider `dialect`.
fn provider(dialect: Dialect, answers: Vec<Answer>) -> (FakeProvider, AiClient) {
    let fake = FakeProvider::start(answers);
    let key_use = if dialect.needs_key() {
        KeyUse::AskEachTime
    } else {
        KeyUse::None
    };
    let config = settings(true, Some(&fake.address_at(dialect.base_path())), key_use);
    (fake, AiClient::new(config))
}

fn ask(
    chat: &mut ChatSession,
    client: &AiClient,
    question: &str,
) -> Result<kassensturz_ai_chat::Answer, ChatError> {
    let key = Secret::new(TEST_KEY.to_owned());
    chat.ask(
        &space_colony(),
        client,
        Some(&key),
        question,
        TODAY,
        &mut NoHooks,
    )
}

fn bodies(fake: &FakeProvider) -> Vec<Value> {
    fake.received()
        .iter()
        .map(|request| serde_json::from_slice(&request.body).unwrap())
        .collect()
}

/// Every request the fake got passes the dialect's rules.
fn assert_accepted(dialect: Dialect, fake: &FakeProvider) {
    for (index, request) in fake.received().iter().enumerate() {
        if let Err(problem) = check_request(dialect, &request.body) {
            panic!("{} request {}: {problem}", dialect.name(), index + 1);
        }
        assert_eq!(
            request.line,
            format!("POST {} HTTP/1.1", dialect.chat_path())
        );
    }
}

#[test]
fn a_question_with_a_function_call_works_in_every_dialect() {
    let first_ids = [
        (Dialect::OpenAi, "call_Tst1AaBbCc"),
        (Dialect::Claude, "toolu_01TestAaBbCcDd"),
        (Dialect::Gemini, "function-call-1700000000000000001"),
        (Dialect::LmStudio, "365174485"),
    ];
    for (dialect, id) in first_ids {
        let (fake, client) = provider(
            dialect,
            vec![dialect.answer("tool_call"), dialect.answer("text")],
        );
        let mut chat = session();
        let answer = ask(&mut chat, &client, "How much is on Everyday Checking?").unwrap();
        assert_eq!(answer.text, TEXT, "{}", dialect.name());
        assert_eq!(answer.mode, Mode::Functions);
        assert!(
            answer.unverified.is_empty(),
            "{}: 5531.90 comes from the function: {:?}",
            dialect.name(),
            answer.unverified
        );

        assert_accepted(dialect, &fake);
        let sent = bodies(&fake);
        assert_eq!(sent.len(), 2, "{}", dialect.name());
        // The function offered are the real ones, propose_view among them.
        assert_eq!(sent[0]["tools"].as_array().unwrap().len(), 10);
        // The second request answers the call, by the provider's ID, with
        // the real result.
        let messages = sent[1]["messages"].as_array().unwrap();
        assert_eq!(messages[2]["role"], "assistant");
        assert_eq!(messages[2]["content"], Value::Null);
        assert_eq!(messages[2]["tool_calls"][0]["id"], id);
        assert_eq!(messages[3]["role"], "tool");
        assert_eq!(messages[3]["tool_call_id"], id);
        assert!(messages[3]["content"].as_str().unwrap().contains("5531.90"));

        // What the log shows is what went out.
        let logged: Vec<&str> = chat
            .log()
            .iter()
            .filter_map(|entry| match entry {
                LogEntry::Sent { body, .. } => Some(body.as_str()),
                _ => None,
            })
            .collect();
        for (body, request) in logged.iter().zip(fake.received()) {
            assert_eq!(body.as_bytes(), request.body.as_slice());
        }
    }
}

#[test]
fn parallel_calls_are_all_answered_in_every_dialect() {
    for dialect in ALL {
        let (fake, client) = provider(
            dialect,
            vec![dialect.answer("parallel_calls"), dialect.answer("text")],
        );
        let mut chat = session();
        ask(&mut chat, &client, "What do I have?").unwrap();
        assert_accepted(dialect, &fake);

        let sent = bodies(&fake);
        let messages = sent[1]["messages"].as_array().unwrap();
        let called: Vec<&Value> = messages[2]["tool_calls"]
            .as_array()
            .unwrap()
            .iter()
            .collect();
        let answered: Vec<&Value> = messages[3..5].iter().collect();
        assert_eq!(called.len(), 2, "{}", dialect.name());
        assert_ne!(called[0]["id"], called[1]["id"], "{}", dialect.name());
        for (call, answer) in called.iter().zip(&answered) {
            assert_eq!(answer["role"], "tool");
            assert_eq!(answer["tool_call_id"], call["id"], "{}", dialect.name());
        }
        // list_accounts and list_categories gave different results.
        assert_ne!(answered[0]["content"], answered[1]["content"]);
        assert_eq!(messages.len(), 5);
    }
}

#[test]
fn a_local_model_without_function_calling_gets_the_overview() {
    // A server that refuses the request that offers functions, in the
    // words of a local server (Ollama's), and answers the one without.
    let refused = Answer::Json(
        400,
        r#"{"error":{"message":"registry.test/library/small:2b does not support tools","type":"api_error","param":null,"code":null}}"#.into(),
    );
    let dialect = Dialect::LmStudio;
    let (fake, client) = provider(dialect, vec![refused, dialect.answer("text")]);
    let mut chat = session();
    let answer = ask(&mut chat, &client, "What is on my accounts?").unwrap();
    assert_eq!(answer.mode, Mode::Overview);
    assert_eq!(answer.text, TEXT);
    assert_accepted(dialect, &fake);
    let sent = bodies(&fake);
    assert!(sent[0].get("tools").is_some());
    assert!(sent[1].get("tools").is_none());
    let system = sent[1]["messages"][0]["content"].as_str().unwrap();
    assert!(
        system.contains("Everyday Checking"),
        "the overview is in the instructions"
    );
}

#[test]
fn a_model_that_ignores_the_functions_just_answers() {
    let dialect = Dialect::LmStudio;
    let (fake, client) = provider(dialect, vec![dialect.answer("tools_unused")]);
    let mut chat = session();
    let answer = ask(&mut chat, &client, "How much is on Everyday Checking?").unwrap();
    assert_eq!(answer.text, "I don't have access to your accounts.");
    assert_eq!(answer.mode, Mode::Functions);
    assert_eq!(fake.received().len(), 1);
}

#[test]
fn a_bad_gemini_key_is_a_400_and_costs_one_more_request() {
    // Gemini answers a bad key with 400 (reported), which the chat takes
    // for "this model can't call functions" and tries once more without
    // them (D-190); then the 400 stands (D-208).
    let dialect = Dialect::Gemini;
    let (fake, client) = provider(
        dialect,
        vec![
            dialect.failure(400, "error_auth"),
            dialect.failure(400, "error_auth"),
        ],
    );
    let mut chat = session();
    let error = ask(&mut chat, &client, "What is on my accounts?").unwrap_err();
    assert!(
        matches!(
            error,
            ChatError::Ai(AiError::ProviderFailed { status: 400 })
        ),
        "{error:?}"
    );
    assert_eq!(fake.received().len(), 2);
    assert!(!format!("{error:?} {error} {:?}", chat.log()).contains(TEST_KEY));
}

#[test]
fn answers_without_text_reach_the_caller_with_their_reason() {
    // A refusal and a safety block: the chat doesn't invent a text. The
    // chat panel (M5.8) shows its own words for an empty answer.
    let (_fake, client) = provider(Dialect::OpenAi, vec![Dialect::OpenAi.answer("refusal")]);
    let answer = ask(&mut session(), &client, "Tell me a secret").unwrap();
    assert_eq!(answer.text, "I can't help with that request.");

    let (_fake, client) = provider(
        Dialect::Gemini,
        vec![Dialect::Gemini.answer("safety_blocked")],
    );
    let answer = ask(&mut session(), &client, "Tell me a secret").unwrap();
    assert_eq!(answer.text, "");
    assert_eq!(answer.finish, Finish::Other);

    let (_fake, client) = provider(Dialect::Claude, vec![Dialect::Claude.answer("cut_off")]);
    let answer = ask(&mut session(), &client, "Tell me everything").unwrap();
    assert_eq!(answer.finish, Finish::Length);
}
