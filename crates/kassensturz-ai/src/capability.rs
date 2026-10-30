// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What "Test connection" finds out about a model besides that it answers
//! (M6b.9, D-294, D-320; threat T-45): whether it can call functions.
//!
//! The test request is fixed. It is one short user message and one made-up
//! function, `kassensturz_test`, which takes a word and does nothing; it
//! holds no instructions of KassenSturz's, no function of the book and
//! nothing from a book, so there is nothing to redact and nothing to
//! consent to beyond the usual rules of [`AiClient::send`](crate::AiClient).
//! The reply is read like every reply (T-31): a call counts only if it
//! names the one function offered and its arguments are the expected word.

use kassensturz_platform::Secret;
use serde_json::{Value, json};

use crate::Provider;
use crate::chat::{FunctionSpec, Message};
use crate::error::AiError;

/// The made-up function the test offers.
pub const PROBE_FUNCTION: &str = "kassensturz_test";

/// The word the model is asked to pass to it.
pub const PROBE_WORD: &str = "ping";

/// The context size, in tokens, below which KassenSturz warns: a question
/// with the instructions and the functions needs about this much (D-240).
pub const MIN_CONTEXT_TOKENS: u64 = 8192;

/// Whether a model called the test function.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FunctionCalling {
    /// It called the function with the word it was asked to pass.
    Yes,
    /// It answered in words, called something else, passed another word,
    /// or the provider refused a request that offers functions (HTTP 400,
    /// as D-190 reads it).
    No,
}

fn probe_messages() -> Vec<Message> {
    vec![Message::user(format!(
        "This is a connection test from KassenSturz. Call the function {PROBE_FUNCTION} with the word \"{PROBE_WORD}\"."
    ))]
}

fn probe_spec() -> FunctionSpec {
    FunctionSpec {
        name: PROBE_FUNCTION.to_owned(),
        description: "Does nothing. Only used to test that functions can be called.".to_owned(),
        parameters: json!({
            "type": "object",
            "properties": { "word": { "type": "string", "description": "The word to pass." } },
            "required": ["word"],
        }),
    }
}

/// The exact body the test sends to `model`, for the screen to show before
/// it runs (D-204's promise, kept for the test as for the chat).
pub fn probe_request_body(model: &str) -> String {
    let body = crate::openai::chat_body(model, &probe_messages(), &[probe_spec()]);
    serde_json::to_string_pretty(&body).unwrap_or_default()
}

/// Sends the test request through `provider` and says whether the model
/// called the function. Errors other than a refusal of the request itself
/// (no consent, a key, a time-out, a context that is too small…) are the
/// provider's own and are handed on.
pub fn probe_functions(
    provider: &dyn Provider,
    key: Option<&Secret>,
) -> Result<FunctionCalling, AiError> {
    let request = provider.prepare(probe_messages(), vec![probe_spec()])?;
    match provider.send(&request, key) {
        Ok(reply) => {
            let called = reply
                .calls
                .iter()
                .any(|call| call.name == PROBE_FUNCTION && passes_the_word(&call.arguments));
            Ok(if called {
                FunctionCalling::Yes
            } else {
                FunctionCalling::No
            })
        }
        Err(AiError::ProviderFailed { status: 400 }) => Ok(FunctionCalling::No),
        Err(error) => Err(error),
    }
}

/// Whether `arguments` (a JSON text the model wrote) pass [`PROBE_WORD`],
/// in any case and with spaces around it.
fn passes_the_word(arguments: &str) -> bool {
    serde_json::from_str::<Value>(arguments)
        .ok()
        .and_then(|value| value.get("word").and_then(Value::as_str).map(str::to_owned))
        .is_some_and(|word| word.trim().eq_ignore_ascii_case(PROBE_WORD))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigFile;
    use crate::scripted::ScriptedProvider;

    /// A settings file with AI on, of its own for every call, since the
    /// tests run side by side.
    fn on() -> ConfigFile {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let number = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let folder = std::env::temp_dir().join(format!(
            "kassensturz-ai-probe-{}-{number}",
            std::process::id()
        ));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("kassensturzrc");
        std::fs::write(&path, "[AI]\nEnabled=true\n").unwrap();
        ConfigFile::at(path)
    }

    fn calls(arguments: &'static str) -> ScriptedProvider {
        ScriptedProvider::new(on(), move |_, _| {
            Ok(
                json!({"choices": [{"finish_reason": "tool_calls", "message": {"tool_calls": [
                {"id": "a", "function": {"name": PROBE_FUNCTION, "arguments": arguments}}]}}]})
                .to_string(),
            )
        })
    }

    #[test]
    fn a_call_with_the_word_is_a_yes() {
        for arguments in [r#"{"word":"ping"}"#, r#"{"word":" Ping "}"#] {
            assert_eq!(
                probe_functions(&calls(arguments), None),
                Ok(FunctionCalling::Yes),
                "{arguments}"
            );
        }
    }

    #[test]
    fn anything_else_is_a_no() {
        for arguments in [r#"{"word":"pong"}"#, "{}", "ping", r#"{"word":5}"#, ""] {
            assert_eq!(
                probe_functions(&calls(arguments), None),
                Ok(FunctionCalling::No),
                "{arguments:?}"
            );
        }
        let words = ScriptedProvider::new(on(), |_, _| {
            Ok(
                r#"{"choices":[{"finish_reason":"stop","message":{"content":"Sure, ping!"}}]}"#
                    .into(),
            )
        });
        assert_eq!(probe_functions(&words, None), Ok(FunctionCalling::No));
        let other = ScriptedProvider::new(on(), |_, _| {
            Ok(
                r#"{"choices":[{"finish_reason":"tool_calls","message":{"tool_calls":[
                {"id":"a","function":{"name":"send_money","arguments":"{\"word\":\"ping\"}"}}]}}]}"#
                    .into(),
            )
        });
        assert_eq!(probe_functions(&other, None), Ok(FunctionCalling::No));
        let refused =
            ScriptedProvider::new(on(), |_, _| Err(AiError::ProviderFailed { status: 400 }));
        assert_eq!(probe_functions(&refused, None), Ok(FunctionCalling::No));
    }

    #[test]
    fn other_errors_are_handed_on() {
        for error in [
            AiError::ProviderFailed { status: 503 },
            AiError::ContextTooSmall,
            AiError::Timeout,
            AiError::NoConsent,
            AiError::BadReply,
        ] {
            let given = error.clone();
            let failing = ScriptedProvider::new(on(), move |_, _| Err(given.clone()));
            assert_eq!(probe_functions(&failing, None), Err(error));
        }
    }

    #[test]
    fn the_request_is_fixed_and_holds_nothing_of_a_book() {
        let body: Value = serde_json::from_str(&probe_request_body("m")).unwrap();
        assert_eq!(body["model"], "m");
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["tools"].as_array().unwrap().len(), 1);
        assert_eq!(body["tools"][0]["function"]["name"], PROBE_FUNCTION);
        // The same body goes out through the real provider path.
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = seen.clone();
        let provider = ScriptedProvider::new(on(), move |messages, offered| {
            log.lock()
                .unwrap()
                .push((messages.to_vec(), offered.to_vec()));
            Ok(r#"{"choices":[{"finish_reason":"stop","message":{"content":"x"}}]}"#.into())
        });
        probe_functions(&provider, None).unwrap();
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0, probe_messages());
        assert_eq!(seen[0].1, [PROBE_FUNCTION]);
        assert_eq!(
            probe_request_body("m"),
            provider
                .prepare(probe_messages(), vec![probe_spec()])
                .map(|r| r.preview().replace(crate::scripted::SCRIPTED_MODEL, "m"))
                .unwrap(),
            "the preview shown is the body that goes"
        );
    }
}
