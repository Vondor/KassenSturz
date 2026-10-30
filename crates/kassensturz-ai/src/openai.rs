// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The OpenAI-compatible API (D-035): the request body KassenSturz sends
//! to `chat/completions`, and the reading of the replies of
//! `chat/completions` and `models`. Pure functions, without the network.
//!
//! A reply is read with limits (threat T-31): its JSON may nest at most
//! [`MAX_DEPTH`] levels, a reply holds at most [`MAX_CALLS`] function
//! calls, and a text field that isn't text makes the whole reply
//! unusable.
//!
//! Providers that speak this API differ in small ways (M5.11, D-207): a
//! call may come without an ID or with the same ID as another, its
//! arguments may be empty for a function that takes none, the answer may
//! be a refusal in a field of its own, and a call may come with the finish
//! reason `stop`. The reading below takes all of that; the tests in
//! `tests/dialects.rs` pin it for each provider.

use serde_json::{Map, Value, json};

use crate::chat::{Finish, FunctionSpec, Message, ModelInfo, Reply, Role, ToolCall};
use crate::error::AiError;

/// How deeply a reply's JSON may nest. A real reply nests about six
/// levels; the arguments of a call are a JSON text inside a string, so
/// they don't count here.
pub const MAX_DEPTH: usize = 32;

/// The most function calls one reply may hold.
pub const MAX_CALLS: usize = 32;

/// The most models a `models` reply is read for.
pub const MAX_MODELS: usize = 500;

/// The request body for `chat/completions`.
pub fn chat_body(model: &str, messages: &[Message], functions: &[FunctionSpec]) -> Value {
    let messages: Vec<Value> = messages.iter().map(message_json).collect();
    let mut body = Map::new();
    body.insert("model".into(), json!(model));
    body.insert("messages".into(), Value::Array(messages));
    if !functions.is_empty() {
        let tools: Vec<Value> = functions
            .iter()
            .map(|function| {
                json!({
                    "type": "function",
                    "function": {
                        "name": function.name,
                        "description": function.description,
                        "parameters": function.parameters,
                    }
                })
            })
            .collect();
        body.insert("tools".into(), Value::Array(tools));
    }
    body.insert("stream".into(), json!(false));
    Value::Object(body)
}

fn message_json(message: &Message) -> Value {
    let mut out = Map::new();
    out.insert("role".into(), json!(message.role.as_str()));
    // A message that only calls functions has no content, which is how the
    // API describes it; an empty text could be refused as an empty block
    // (D-207).
    let calls_only =
        message.role == Role::Assistant && !message.calls.is_empty() && message.content.is_empty();
    if calls_only {
        out.insert("content".into(), Value::Null);
    } else {
        out.insert("content".into(), json!(message.content));
    }
    if message.role == Role::Assistant && !message.calls.is_empty() {
        let calls: Vec<Value> = message
            .calls
            .iter()
            .map(|call| {
                json!({
                    "id": call.id,
                    "type": "function",
                    "function": { "name": call.name, "arguments": call.arguments },
                })
            })
            .collect();
        out.insert("tool_calls".into(), Value::Array(calls));
    }
    if let Some(id) = &message.call_id {
        out.insert("tool_call_id".into(), json!(id));
    }
    Value::Object(out)
}

/// Reads a `chat/completions` reply. Calls for functions not in `offered`
/// are left out and their IDs listed in [`Reply::ignored_calls`].
pub fn parse_chat_reply(bytes: &[u8], offered: &[String]) -> Result<Reply, AiError> {
    let reply = parse(bytes)?;
    let message = reply
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(Value::as_object)
        .ok_or(AiError::BadReply)?;
    let text = match message.get("content") {
        // A refusal has no content but a text of its own, which the user
        // should see (D-207).
        None | Some(Value::Null) => match message.get("refusal") {
            Some(Value::String(refusal)) => refusal.clone(),
            None | Some(Value::Null) => String::new(),
            Some(_) => return Err(AiError::BadReply),
        },
        Some(Value::String(text)) => text.clone(),
        Some(_) => return Err(AiError::BadReply),
    };
    let finish = match reply["choices"][0]
        .get("finish_reason")
        .and_then(Value::as_str)
    {
        Some("stop") => Finish::Done,
        Some("tool_calls" | "function_call") => Finish::Calls,
        Some("length") => Finish::Length,
        _ => Finish::Other,
    };
    let raw_calls = match message.get("tool_calls") {
        None | Some(Value::Null) => &[][..],
        Some(Value::Array(calls)) if calls.len() <= MAX_CALLS => calls.as_slice(),
        Some(_) => return Err(AiError::BadReply),
    };
    let mut calls = Vec::new();
    let mut ignored_calls = Vec::new();
    let mut ids: Vec<String> = Vec::with_capacity(raw_calls.len());
    for call in raw_calls {
        let id = call_id(call, &ids)?;
        ids.push(id.clone());
        let name = text_at(call, &["function", "name"])?;
        let arguments = match call.get("function").and_then(|f| f.get("arguments")) {
            None | Some(Value::Null) => "{}".to_owned(),
            // A function without parameters is sometimes called with no
            // text at all.
            Some(Value::String(arguments)) if arguments.trim().is_empty() => "{}".to_owned(),
            Some(Value::String(arguments)) => arguments.clone(),
            Some(_) => return Err(AiError::BadReply),
        };
        if offered.contains(&name) {
            calls.push(ToolCall {
                id,
                name,
                arguments,
            });
        } else {
            ignored_calls.push(id);
        }
    }
    Ok(Reply {
        text,
        calls,
        ignored_calls,
        finish,
        prompt_tokens: reply
            .get("usage")
            .and_then(|usage| usage.get("prompt_tokens"))
            .and_then(Value::as_u64),
    })
}

/// The fields of a model's entry in which servers that say it name the
/// number of tokens the model can read at once, in the order they are
/// trusted: what is loaded now, what the server accepts, then the model's
/// own window. A training length (`n_ctx_train` of llama.cpp) is not here:
/// it is not what the server was started with (D-320).
const CONTEXT_FIELDS: [&str; 4] = [
    "loaded_context_length",
    "max_model_len",
    "context_length",
    "context_window",
];

/// Reads a `models` reply: each model's ID and, where the server says it,
/// its context size in tokens. Nothing is guessed: a missing, zero, negative
/// or non-integer number is "not known" (D-320).
pub fn parse_model_infos(bytes: &[u8]) -> Result<Vec<ModelInfo>, AiError> {
    let reply = parse(bytes)?;
    let models = reply
        .get("data")
        .and_then(Value::as_array)
        .ok_or(AiError::BadReply)?;
    models
        .iter()
        .take(MAX_MODELS)
        .map(|model| {
            Ok(ModelInfo {
                id: text_at(model, &["id"])?,
                context_tokens: CONTEXT_FIELDS
                    .iter()
                    .find_map(|field| model.get(field).and_then(Value::as_u64))
                    .filter(|tokens| *tokens > 0),
            })
        })
        .collect()
}

/// The ID of a call. A provider that sends none, an empty one or one it
/// already used in this reply gets one of KassenSturz's own, so that every
/// call has its own answer (D-207). Anything but text is a broken reply.
fn call_id(call: &Value, used: &[String]) -> Result<String, AiError> {
    match call.get("id") {
        Some(Value::String(id)) if !id.is_empty() && !used.contains(id) => Ok(id.clone()),
        None | Some(Value::Null | Value::String(_)) => {
            let mut number = used.len() + 1;
            loop {
                let id = format!("ks-call-{number}");
                if !used.contains(&id) {
                    return Ok(id);
                }
                number += 1;
            }
        }
        Some(_) => Err(AiError::BadReply),
    }
}

fn text_at(value: &Value, path: &[&str]) -> Result<String, AiError> {
    path.iter()
        .try_fold(value, |value, key| value.get(key))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or(AiError::BadReply)
}

/// Parses `bytes` as JSON after checking how deeply it nests.
fn parse(bytes: &[u8]) -> Result<Value, AiError> {
    if depth(bytes) > MAX_DEPTH {
        return Err(AiError::BadReply);
    }
    serde_json::from_slice(bytes).map_err(|_| AiError::BadReply)
}

/// How deeply the arrays and objects of a JSON text nest, counting outside
/// strings only. Text that isn't JSON gets some number; the parser refuses
/// it afterwards.
fn depth(bytes: &[u8]) -> usize {
    let mut depth = 0usize;
    let mut deepest = 0;
    let mut in_string = false;
    let mut escaped = false;
    for &byte in bytes {
        if in_string {
            match byte {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'[' | b'{' => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    deepest
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offered() -> Vec<String> {
        vec!["account_balances".to_owned()]
    }

    #[test]
    fn the_prompt_tokens_the_provider_counted_are_read() {
        let counted = br#"{"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"Hi"}}],"usage":{"prompt_tokens":3858,"completion_tokens":5}}"#;
        assert_eq!(
            parse_chat_reply(counted, &offered()).unwrap().prompt_tokens,
            Some(3858)
        );
        let silent = br#"{"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"Hi"}}]}"#;
        assert_eq!(
            parse_chat_reply(silent, &offered()).unwrap().prompt_tokens,
            None
        );
    }

    #[test]
    fn the_body_has_model_messages_tools_and_no_stream() {
        let messages = [
            Message::system("rules"),
            Message::user("How much is on Giro?"),
            Message::assistant(
                "",
                vec![ToolCall {
                    id: "call_1".into(),
                    name: "account_balances".into(),
                    arguments: "{}".into(),
                }],
            ),
            Message::tool_result("call_1", "[]"),
        ];
        let functions = [FunctionSpec {
            name: "account_balances".into(),
            description: "Balances".into(),
            parameters: json!({"type": "object", "properties": {}}),
        }];
        let body = chat_body("m", &messages, &functions);
        assert_eq!(
            body,
            json!({
                "model": "m",
                "messages": [
                    {"role": "system", "content": "rules"},
                    {"role": "user", "content": "How much is on Giro?"},
                    {"role": "assistant", "content": null, "tool_calls": [
                        {"id": "call_1", "type": "function",
                         "function": {"name": "account_balances", "arguments": "{}"}}
                    ]},
                    {"role": "tool", "content": "[]", "tool_call_id": "call_1"},
                ],
                "tools": [{"type": "function", "function": {
                    "name": "account_balances", "description": "Balances",
                    "parameters": {"type": "object", "properties": {}}
                }}],
                "stream": false,
            })
        );
        assert!(chat_body("m", &messages, &[]).get("tools").is_none());
    }

    #[test]
    fn a_reply_with_text_and_calls_is_read() {
        let bytes = br#"{"choices":[{"index":0,"finish_reason":"tool_calls","message":{
            "role":"assistant","content":null,"tool_calls":[
              {"id":"a","type":"function","function":{"name":"account_balances","arguments":"{\"x\":1}"}},
              {"id":"b","type":"function","function":{"name":"send_money","arguments":"{}"}}
            ]}}]}"#;
        let reply = parse_chat_reply(bytes, &offered()).unwrap();
        assert_eq!(reply.text, "");
        assert_eq!(reply.finish, Finish::Calls);
        assert_eq!(
            reply.calls,
            [ToolCall {
                id: "a".into(),
                name: "account_balances".into(),
                arguments: "{\"x\":1}".into()
            }]
        );
        assert_eq!(reply.ignored_calls, ["b"]);

        let text = br#"{"choices":[{"finish_reason":"stop","message":{"content":"Hello"}}]}"#;
        let reply = parse_chat_reply(text, &offered()).unwrap();
        assert_eq!(reply.text, "Hello");
        assert_eq!(reply.finish, Finish::Done);
        assert!(reply.calls.is_empty());
    }

    #[test]
    fn calls_without_a_usable_id_get_their_own() {
        let bytes = br#"{"choices":[{"finish_reason":"stop","message":{"tool_calls":[
              {"function":{"name":"account_balances","arguments":""}},
              {"id":"","function":{"name":"account_balances","arguments":"  "}},
              {"id":"a","function":{"name":"account_balances"}},
              {"id":"a","function":{"name":"send_money"}},
              {"id":null,"function":{"name":"account_balances","arguments":"{\"x\":1}"}}
            ]}}]}"#;
        let reply = parse_chat_reply(bytes, &offered()).unwrap();
        let ids: Vec<&str> = reply.calls.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["ks-call-1", "ks-call-2", "a", "ks-call-5"]);
        assert_eq!(reply.ignored_calls, ["ks-call-4"]);
        let arguments: Vec<&str> = reply.calls.iter().map(|c| c.arguments.as_str()).collect();
        assert_eq!(arguments, ["{}", "{}", "{}", "{\"x\":1}"]);
        // The finish reason isn't needed to know there are calls.
        assert_eq!(reply.finish, Finish::Done);
    }

    #[test]
    fn a_refusal_is_its_text() {
        let bytes = br#"{"choices":[{"finish_reason":"stop","message":{
            "content":null,"refusal":"I can't help with that."}}]}"#;
        let reply = parse_chat_reply(bytes, &offered()).unwrap();
        assert_eq!(reply.text, "I can't help with that.");
        let bytes = br#"{"choices":[{"finish_reason":"stop","message":{
            "content":"Hello","refusal":"ignored"}}]}"#;
        assert_eq!(parse_chat_reply(bytes, &offered()).unwrap().text, "Hello");
    }

    #[test]
    fn broken_replies_are_refused() {
        let deep = format!("{}{}", "[".repeat(MAX_DEPTH + 1), "]".repeat(MAX_DEPTH + 1));
        let too_many_calls = format!(
            r#"{{"choices":[{{"message":{{"tool_calls":[{}]}}}}]}}"#,
            vec![r#"{"id":"x","function":{"name":"account_balances"}}"#; MAX_CALLS + 1].join(",")
        );
        for bytes in [
            &b""[..],
            b"not json",
            b"{}",
            br#"{"choices":[]}"#,
            br#"{"choices":[{"message":{"content":5}}]}"#,
            br#"{"choices":[{"message":{"tool_calls":[{"id":7,"function":{"name":"x"}}]}}]}"#,
            br#"{"choices":[{"message":{"content":null,"refusal":5}}]}"#,
            br#"{"choices":[{"message":{"tool_calls":[{"id":"a","function":{"name":"account_balances","arguments":{}}}]}}]}"#,
            deep.as_bytes(),
            too_many_calls.as_bytes(),
        ] {
            assert_eq!(
                parse_chat_reply(bytes, &offered()),
                Err(AiError::BadReply),
                "{}",
                String::from_utf8_lossy(bytes)
            );
        }
    }

    #[test]
    fn depth_skips_brackets_in_strings() {
        assert_eq!(depth(br#"{"a":"[[[[{{{{\"]]"}"#), 1);
        assert_eq!(depth(br#"[[1],[2,[3]]]"#), 3);
        let nested = format!("{}{}", "[".repeat(MAX_DEPTH), "]".repeat(MAX_DEPTH));
        assert!(parse(nested.as_bytes()).is_ok());
    }

    #[test]
    fn models_are_read() {
        let bytes = br#"{"object":"list","data":[{"id":"qwen3","object":"model"},{"id":"llama"}]}"#;
        let ids: Vec<String> = parse_model_infos(bytes)
            .unwrap()
            .into_iter()
            .map(|model| model.id)
            .collect();
        assert_eq!(ids, ["qwen3", "llama"]);
        assert_eq!(
            parse_model_infos(br#"{"data":[{}]}"#),
            Err(AiError::BadReply)
        );
        assert_eq!(parse_model_infos(br#"[]"#), Err(AiError::BadReply));
    }

    #[test]
    fn a_context_size_is_read_only_where_the_server_says_it() {
        let bytes = br#"{"data":[
            {"id":"a"},
            {"id":"b","max_model_len":32768},
            {"id":"c","context_length":131072,"loaded_context_length":8192},
            {"id":"d","context_window":0},
            {"id":"e","context_length":-5},
            {"id":"f","context_length":"8192"},
            {"id":"g","context_length":4096.5},
            {"id":"h","meta":{"n_ctx_train":32768}}]}"#;
        let tokens: Vec<Option<u64>> = parse_model_infos(bytes)
            .unwrap()
            .iter()
            .map(|model| model.context_tokens)
            .collect();
        assert_eq!(
            tokens,
            [None, Some(32768), Some(8192), None, None, None, None, None],
            "the loaded size beats the model's own, and a training length is not read"
        );
    }
}
