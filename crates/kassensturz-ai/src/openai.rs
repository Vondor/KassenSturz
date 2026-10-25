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

use serde_json::{Map, Value, json};

use crate::chat::{Finish, FunctionSpec, Message, Reply, Role, ToolCall};
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
    out.insert("content".into(), json!(message.content));
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
        None | Some(Value::Null) => String::new(),
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
    for call in raw_calls {
        let id = text_at(call, &["id"])?;
        let name = text_at(call, &["function", "name"])?;
        let arguments = match call.get("function").and_then(|f| f.get("arguments")) {
            None | Some(Value::Null) => "{}".to_owned(),
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
    })
}

/// Reads a `models` reply: the IDs of the models the provider offers, in
/// its order.
pub fn parse_models(bytes: &[u8]) -> Result<Vec<String>, AiError> {
    let reply = parse(bytes)?;
    let models = reply
        .get("data")
        .and_then(Value::as_array)
        .ok_or(AiError::BadReply)?;
    models
        .iter()
        .take(MAX_MODELS)
        .map(|model| text_at(model, &["id"]))
        .collect()
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
                    {"role": "assistant", "content": "", "tool_calls": [
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
            br#"{"choices":[{"message":{"tool_calls":[{"function":{"name":"x"}}]}}]}"#,
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
        assert_eq!(parse_models(bytes).unwrap(), ["qwen3", "llama"]);
        assert_eq!(parse_models(br#"{"data":[{}]}"#), Err(AiError::BadReply));
        assert_eq!(parse_models(br#"[]"#), Err(AiError::BadReply));
    }
}
