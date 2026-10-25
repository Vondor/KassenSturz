// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The adapter against the four providers KassenSturz is tried with:
//! OpenAI, Claude's and Gemini's OpenAI-compatible endpoints, and LM Studio
//! (M5.11, D-206 to D-209). What each sends is written down in
//! `dialect/fixtures/` and served by the fake of `fake/mod.rs` on this
//! computer's loopback; the keys are made up. Where a fixture is a shape
//! users reported rather than one a provider documents, the README of the
//! fixtures says so, and M5.B checks it against the real provider.

#![cfg(feature = "ai")]

mod dialect;
mod fake;

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use dialect::{ALL, Dialect, TEXT, check_request};
use fake::{Answer, FakeProvider};
use kassensturz_ai::{
    AiClient, AiError, ConfigFile, Finish, FunctionSpec, KeyUse, Message, ProviderSettings, Reply,
    ToolCall,
};
use kassensturz_platform::Secret;
use serde_json::{Value, json};

const TEST_KEY: &str = "sk-test-0000-made-up-key";

/// A settings file of its own for each test, with the provider of
/// `dialect` at the fake.
struct Setup {
    folder: PathBuf,
    client: AiClient,
}

impl Setup {
    fn new(dialect: Dialect, fake: &FakeProvider) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let folder = std::env::temp_dir().join(format!(
            "kassensturz-ai-dialect-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&folder).unwrap();
        let key_use = if dialect.needs_key() {
            KeyUse::AskEachTime
        } else {
            KeyUse::None
        };
        let provider =
            ProviderSettings::new(&fake.address_at(dialect.base_path()), "test-model", key_use)
                .unwrap();
        let mut text = "[AI]\nEnabled=true\n".to_owned();
        for (key, value) in provider.entries() {
            text.push_str(&format!("{key}={value}\n"));
        }
        let path = folder.join("kassensturz.rc");
        std::fs::write(&path, text).unwrap();
        Self {
            folder,
            client: AiClient::new(ConfigFile::at(path)),
        }
    }

    /// Asks the provider once and answers what it sent back.
    fn ask(&self, messages: Vec<Message>) -> Result<Reply, AiError> {
        let request = self.client.prepare(messages, functions())?;
        self.client.send(&request, Some(&key()))
    }
}

impl Drop for Setup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}

fn key() -> Secret {
    Secret::new(TEST_KEY.to_owned())
}

fn question() -> Vec<Message> {
    vec![
        Message::system("You answer questions about a made-up book."),
        Message::user("How much is on Everyday Checking?"),
    ]
}

/// Functions with the names the fixtures call.
fn functions() -> Vec<FunctionSpec> {
    ["list_accounts", "list_categories"]
        .into_iter()
        .map(|name| FunctionSpec {
            name: name.into(),
            description: "Made up for the test.".into(),
            parameters: json!({"type": "object", "properties": {}}),
        })
        .collect()
}

/// A fake that gives `answers` for `dialect`, and a client for it.
fn provider(dialect: Dialect, answers: Vec<Answer>) -> (FakeProvider, Setup) {
    let fake = FakeProvider::start(answers);
    let setup = Setup::new(dialect, &fake);
    (fake, setup)
}

#[test]
fn every_dialect_gets_a_request_it_accepts() {
    for dialect in ALL {
        let (fake, setup) = provider(dialect, vec![dialect.answer("text")]);
        let reply = setup.ask(question()).unwrap();
        assert_eq!(reply.text, TEXT, "{}", dialect.name());
        assert_eq!(reply.finish, Finish::Done, "{}", dialect.name());
        assert!(reply.calls.is_empty());

        let sent = &fake.received()[0];
        assert_eq!(
            sent.line,
            format!("POST {} HTTP/1.1", dialect.chat_path()),
            "{}: one slash between the base address and the endpoint",
            dialect.name()
        );
        assert_eq!(sent.header("content-type"), Some("application/json"));
        let bearer = format!("Bearer {TEST_KEY}");
        let expected = dialect.needs_key().then_some(bearer.as_str());
        assert_eq!(sent.header("authorization"), expected, "{}", dialect.name());
        if let Err(problem) = check_request(dialect, &sent.body) {
            panic!("{}: {problem}", dialect.name());
        }
    }
}

#[test]
fn a_function_call_is_read_in_every_dialect() {
    let ids = [
        (Dialect::OpenAi, "call_Tst1AaBbCc"),
        (Dialect::Claude, "toolu_01TestAaBbCcDd"),
        (Dialect::Gemini, "function-call-1700000000000000001"),
        (Dialect::LmStudio, "365174485"),
    ];
    for (dialect, id) in ids {
        let (_fake, setup) = provider(dialect, vec![dialect.answer("tool_call")]);
        let reply = setup.ask(question()).unwrap();
        assert_eq!(
            reply.calls,
            [ToolCall {
                id: id.into(),
                name: "list_accounts".into(),
                arguments: "{}".into()
            }],
            "{}",
            dialect.name()
        );
        assert!(reply.ignored_calls.is_empty());
        // Gemini says "stop" although it calls a function (reported); the
        // chat goes by the calls, not by this (D-207).
        let finish = if dialect == Dialect::Gemini {
            Finish::Done
        } else {
            Finish::Calls
        };
        assert_eq!(reply.finish, finish, "{}", dialect.name());
    }
}

#[test]
fn parallel_calls_each_get_their_own_answer() {
    for dialect in ALL {
        let (fake, setup) = provider(
            dialect,
            vec![dialect.answer("parallel_calls"), dialect.answer("text")],
        );
        let reply = setup.ask(question()).unwrap();
        let names: Vec<&str> = reply.calls.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["list_accounts", "list_categories"]);
        let mut ids: Vec<&str> = reply.calls.iter().map(|c| c.id.as_str()).collect();
        if dialect == Dialect::Gemini {
            // The fixture has no usable IDs; KassenSturz's own are used.
            assert_eq!(ids, ["ks-call-1", "ks-call-2"]);
        }
        ids.dedup();
        assert_eq!(ids.len(), 2, "{}: two IDs", dialect.name());
        if dialect == Dialect::Claude {
            assert_eq!(reply.text, "I'll look at both.");
        }

        // The next request: the model's message and one answer per call.
        let mut messages = question();
        messages.push(Message::assistant(reply.text.clone(), reply.calls.clone()));
        for call in &reply.calls {
            messages.push(Message::tool_result(call.id.clone(), "[]"));
        }
        assert_eq!(setup.ask(messages).unwrap().text, TEXT);
        let second = &fake.received()[1];
        if let Err(problem) = check_request(dialect, &second.body) {
            panic!("{}: {problem}", dialect.name());
        }
    }
}

#[test]
fn models_are_listed_in_every_dialect() {
    let expected = [
        (Dialect::OpenAi, vec!["gpt-test", "gpt-test-mini"]),
        (
            Dialect::Claude,
            vec!["claude-test-large", "claude-test-small"],
        ),
        // Gemini's list names the models with the prefix `models/`; they
        // are listed as they come (M5.B checks that the chat takes them).
        (
            Dialect::Gemini,
            vec!["models/gemini-test-flash", "models/gemini-test-pro"],
        ),
        (
            Dialect::LmStudio,
            vec!["qwen-test-7b", "text-embedding-test"],
        ),
    ];
    for (dialect, ids) in expected {
        let (fake, setup) = provider(dialect, vec![dialect.answer("models")]);
        assert_eq!(
            setup.client.list_models(Some(&key())).unwrap(),
            ids,
            "{}",
            dialect.name()
        );
        let sent = &fake.received()[0];
        assert_eq!(sent.line, format!("GET {} HTTP/1.1", dialect.models_path()));
        let bearer = format!("Bearer {TEST_KEY}");
        let header = dialect.needs_key().then_some(bearer.as_str());
        assert_eq!(sent.header("authorization"), header);
    }
}

#[test]
fn errors_are_codes_and_never_the_providers_words() {
    let cases = [
        (Dialect::OpenAi, 401, "error_auth", AiError::KeyRejected),
        (
            Dialect::OpenAi,
            429,
            "error_rate_limit",
            AiError::RateLimited,
        ),
        (
            Dialect::OpenAi,
            500,
            "error_server",
            AiError::ProviderFailed { status: 500 },
        ),
        (Dialect::Claude, 401, "error_auth", AiError::KeyRejected),
        (
            Dialect::Claude,
            429,
            "error_rate_limit",
            AiError::RateLimited,
        ),
        // Claude's "overloaded" status.
        (
            Dialect::Claude,
            529,
            "error_overloaded",
            AiError::ProviderFailed { status: 529 },
        ),
        // Gemini answers a bad key with 400, not 401 (reported): it can't
        // be told from a bad request by the status, which is all that is
        // read (D-208).
        (
            Dialect::Gemini,
            400,
            "error_auth",
            AiError::ProviderFailed { status: 400 },
        ),
        (Dialect::Gemini, 403, "error_auth", AiError::KeyRejected),
        (
            Dialect::Gemini,
            429,
            "error_rate_limit",
            AiError::RateLimited,
        ),
        (
            Dialect::Gemini,
            503,
            "error_server",
            AiError::ProviderFailed { status: 503 },
        ),
        (
            Dialect::LmStudio,
            400,
            "error_no_model",
            AiError::ProviderFailed { status: 400 },
        ),
        (
            Dialect::LmStudio,
            500,
            "error_server",
            AiError::ProviderFailed { status: 500 },
        ),
    ];
    for (dialect, status, fixture, expected) in cases {
        let (_fake, setup) = provider(dialect, vec![dialect.failure(status, fixture)]);
        let error = setup.ask(question()).unwrap_err();
        assert_eq!(error, expected, "{} {status} {fixture}", dialect.name());
        // The fixtures put the key and the provider's own sentences in the
        // body; none of it comes back out.
        let shown = format!("{error:?} {error}");
        for word in [
            TEST_KEY,
            "API key",
            "Rate limit",
            "Overloaded",
            "loaded",
            "crashed",
        ] {
            assert!(!shown.contains(word), "{}: {shown}", dialect.name());
        }
    }
}

#[test]
fn a_provider_that_streams_anyway_is_refused() {
    for dialect in ALL {
        let (fake, setup) = provider(dialect, vec![dialect::stream_ignored()]);
        assert_eq!(
            setup.ask(question()),
            Err(AiError::BadReply),
            "{}",
            dialect.name()
        );
        let body: Value = serde_json::from_slice(&fake.received()[0].body).unwrap();
        assert_eq!(body["stream"], false, "the request said so");
    }
}

#[test]
fn refused_cut_off_and_filtered_answers_are_read() {
    let cases = [
        (
            Dialect::OpenAi,
            "refusal",
            "I can't help with that request.",
            Finish::Done,
        ),
        (
            Dialect::OpenAi,
            "cut_off",
            "Everyday Checking has",
            Finish::Length,
        ),
        (
            Dialect::Claude,
            "cut_off",
            "Everyday Checking has",
            Finish::Length,
        ),
        // Nothing to say, and the reason is in the finish: the chat panel
        // has to tell the user (M5.8).
        (Dialect::Gemini, "safety_blocked", "", Finish::Other),
        // A model without tool training answers in words and no calls.
        (
            Dialect::LmStudio,
            "tools_unused",
            "I don't have access to your accounts.",
            Finish::Done,
        ),
    ];
    for (dialect, fixture, text, finish) in cases {
        let (_fake, setup) = provider(dialect, vec![dialect.answer(fixture)]);
        let reply = setup.ask(question()).unwrap();
        assert_eq!(reply.text, text, "{} {fixture}", dialect.name());
        assert_eq!(reply.finish, finish, "{} {fixture}", dialect.name());
        assert!(reply.calls.is_empty());
    }
}

#[test]
fn the_request_check_is_not_vacuous() {
    let good = json!({
        "model": "m", "stream": false,
        "messages": [
            {"role": "system", "content": "rules"},
            {"role": "user", "content": "hi"},
            {"role": "assistant", "content": null, "tool_calls": [
                {"id": "a", "type": "function",
                 "function": {"name": "list_accounts", "arguments": "{}"}}]},
            {"role": "tool", "content": "[]", "tool_call_id": "a"},
        ],
        "tools": [{"type": "function", "function": {
            "name": "list_accounts", "description": "d",
            "parameters": {"type": "object", "properties": {"x": {"type": "string"}},
                           "required": ["x"]}}}],
    });
    let check = |body: &Value| check_request(Dialect::Claude, body.to_string().as_bytes());
    assert_eq!(check(&good), Ok(()));

    type Change = Box<dyn Fn(&mut Value)>;
    let broken: Vec<(&str, Change)> = vec![
        ("extra field", Box::new(|b| b["temperature"] = json!(0.2))),
        (
            "tool_choice",
            Box::new(|b| b["tool_choice"] = json!("required")),
        ),
        ("stream", Box::new(|b| b["stream"] = json!(true))),
        ("no model", Box::new(|b| b["model"] = json!(""))),
        (
            "late system",
            Box::new(|b| b["messages"][1]["role"] = json!("system")),
        ),
        (
            "empty text with calls",
            Box::new(|b| b["messages"][2]["content"] = json!("")),
        ),
        (
            "unknown answer",
            Box::new(|b| b["messages"][3]["tool_call_id"] = json!("b")),
        ),
        (
            "no answer",
            Box::new(|b| {
                b["messages"].as_array_mut().unwrap().pop();
            }),
        ),
        (
            "ends on the model",
            Box::new(|b| {
                b["messages"].as_array_mut().unwrap().pop();
                b["messages"][2]["tool_calls"] = json!([]);
                b["messages"][2]["content"] = json!("text");
            }),
        ),
        (
            "empty id",
            Box::new(|b| b["messages"][2]["tool_calls"][0]["id"] = json!("")),
        ),
        (
            "arguments not text",
            Box::new(|b| b["messages"][2]["tool_calls"][0]["function"]["arguments"] = json!({})),
        ),
        (
            "bad name",
            Box::new(|b| b["tools"][0]["function"]["name"] = json!("list accounts")),
        ),
        (
            "required missing",
            Box::new(|b| b["tools"][0]["function"]["parameters"]["required"] = json!(["y"])),
        ),
        (
            "no description",
            Box::new(|b| b["tools"][0]["function"]["description"] = json!("")),
        ),
    ];
    for (what, change) in broken {
        let mut body = good.clone();
        change(&mut body);
        assert!(check(&body).is_err(), "{what} should be refused");
    }
}
