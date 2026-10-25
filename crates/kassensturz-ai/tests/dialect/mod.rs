// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The four providers KassenSturz is tried with (M5.11, D-206), as test
//! doubles: what each answers, written down in `fixtures/` from the
//! providers' public documentation and users' reports (see
//! `fixtures/README.md`), and what each accepts, as rules a request is
//! checked against. No key, no network beyond this computer's loopback.
//!
//! The rules are only what the providers' documentation states for
//! requests like KassenSturz's; the real providers decide in M5.B (D-172).

#![allow(dead_code)]

use serde_json::Value;

use crate::fake::Answer;

/// A provider that speaks the OpenAI-compatible API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    /// OpenAI (ChatGPT's API).
    OpenAi,
    /// Claude through its OpenAI-compatible endpoint.
    Claude,
    /// Gemini through its OpenAI-compatible endpoint.
    Gemini,
    /// LM Studio's local server.
    LmStudio,
}

pub const ALL: [Dialect; 4] = [
    Dialect::OpenAi,
    Dialect::Claude,
    Dialect::Gemini,
    Dialect::LmStudio,
];

/// The text every `text.json` fixture answers with.
pub const TEXT: &str = "Everyday Checking has 5531.90 EUR.";

impl Dialect {
    pub fn name(self) -> &'static str {
        match self {
            Self::OpenAi => "OpenAI",
            Self::Claude => "Claude",
            Self::Gemini => "Gemini",
            Self::LmStudio => "LM Studio",
        }
    }

    fn folder(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Claude => "claude",
            Self::Gemini => "gemini",
            Self::LmStudio => "lmstudio",
        }
    }

    /// The path of the provider's base address as its documentation writes
    /// it, e.g. with the trailing `/` that Claude's and Gemini's pages
    /// have. The request must still go to `<base>/chat/completions` with
    /// one `/`.
    pub fn base_path(self) -> &'static str {
        match self {
            Self::OpenAi | Self::LmStudio => "/v1",
            Self::Claude => "/v1/",
            Self::Gemini => "/v1beta/openai/",
        }
    }

    /// The path the request line must have.
    pub fn chat_path(self) -> String {
        format!(
            "{}/chat/completions",
            self.base_path().trim_end_matches('/')
        )
    }

    pub fn models_path(self) -> String {
        format!("{}/models", self.base_path().trim_end_matches('/'))
    }

    /// Whether the provider wants a key. A local server doesn't.
    pub fn needs_key(self) -> bool {
        self != Self::LmStudio
    }

    /// The text of a fixture, e.g. `"tool_call"`.
    pub fn fixture(self, name: &str) -> String {
        let path = format!("{}/{}/{name}.json", fixtures(), self.folder());
        std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
    }

    /// The fixture as an answer with HTTP status 200.
    pub fn answer(self, name: &str) -> Answer {
        Answer::Json(200, self.fixture(name))
    }

    /// The fixture as an answer with another HTTP status.
    pub fn failure(self, status: u16, name: &str) -> Answer {
        Answer::Json(status, self.fixture(name))
    }
}

/// What a provider that ignores `"stream": false` sends.
pub fn stream_ignored() -> Answer {
    let path = format!("{}/stream_ignored.txt", fixtures());
    Answer::Json(200, std::fs::read_to_string(path).unwrap())
}

/// The fixtures' folder, seen from `kassensturz-ai` and from the crates
/// that include this module.
fn fixtures() -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let own = format!("{manifest}/tests/dialect/fixtures");
    if std::path::Path::new(&own).is_dir() {
        own
    } else {
        format!("{manifest}/../kassensturz-ai/tests/dialect/fixtures")
    }
}

/// Checks a request body against what `dialect`'s documentation says it
/// accepts. The rules are the same for all four on the points KassenSturz
/// uses; a rule one provider is stricter about is noted at the rule.
pub fn check_request(dialect: Dialect, body: &[u8]) -> Result<(), String> {
    let body: Value = serde_json::from_slice(body).map_err(|e| format!("not JSON: {e}"))?;
    let object = body.as_object().ok_or("the body is not an object")?;

    // Fields KassenSturz doesn't send and some providers refuse: Claude
    // returns a 400 for a temperature below 1 or any top_p on its newest
    // models, and for forced tool use (tool_choice) on some of them.
    for key in object.keys() {
        if !["model", "messages", "tools", "stream"].contains(&key.as_str()) {
            return Err(format!("{}: unexpected field {key}", dialect.name()));
        }
    }
    if object
        .get("model")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        return Err("no model".into());
    }
    // No streaming in v1 (D-193).
    if object.get("stream") != Some(&Value::Bool(false)) {
        return Err("stream is not false".into());
    }

    let messages = object
        .get("messages")
        .and_then(Value::as_array)
        .filter(|messages| !messages.is_empty())
        .ok_or("no messages")?;
    check_messages(messages)?;

    if let Some(tools) = object.get("tools") {
        let tools = tools
            .as_array()
            .filter(|t| !t.is_empty())
            .ok_or("empty tools")?;
        for tool in tools {
            check_tool(tool)?;
        }
    }
    Ok(())
}

fn check_messages(messages: &[Value]) -> Result<(), String> {
    // Claude's layer moves system messages to the front and joins them, so
    // KassenSturz sends exactly one, first: nothing is reordered.
    let systems: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m["role"] == "system")
        .map(|(i, _)| i)
        .collect();
    if systems != [0] {
        return Err(format!("system messages at {systems:?}, not only at 0"));
    }

    // The calls of the last assistant message that nobody answered yet.
    let mut open: Vec<String> = Vec::new();
    for (i, message) in messages.iter().enumerate() {
        let role = message["role"].as_str().unwrap_or_default();
        match role {
            "system" | "user" => {
                if !open.is_empty() {
                    return Err(format!("message {i}: calls {open:?} have no answer"));
                }
                if message["content"].as_str().is_none_or(str::is_empty) {
                    return Err(format!("message {i}: empty {role} content"));
                }
            }
            "assistant" => {
                if !open.is_empty() {
                    return Err(format!("message {i}: calls {open:?} have no answer"));
                }
                let calls = message.get("tool_calls").and_then(Value::as_array);
                match (&message["content"], calls) {
                    // Claude's layer turns an empty text into an empty text
                    // block, which its API refuses; the API describes a
                    // message that only calls functions as having no content.
                    (Value::String(text), _) if text.is_empty() => {
                        return Err(format!("message {i}: empty assistant text"));
                    }
                    (Value::String(_), _) => {}
                    (Value::Null, Some(calls)) if !calls.is_empty() => {}
                    other => return Err(format!("message {i}: bad assistant content {other:?}")),
                }
                for call in calls.into_iter().flatten() {
                    let id = call["id"].as_str().filter(|id| !id.is_empty());
                    let id = id.ok_or(format!("message {i}: call without id"))?;
                    if open.iter().any(|open| open == id) {
                        return Err(format!("message {i}: id {id} twice"));
                    }
                    if call["type"] != "function" {
                        return Err(format!("message {i}: call type {}", call["type"]));
                    }
                    check_name(call["function"]["name"].as_str().unwrap_or_default())?;
                    let arguments = call["function"]["arguments"]
                        .as_str()
                        .ok_or(format!("message {i}: arguments are not text"))?;
                    serde_json::from_str::<Value>(arguments)
                        .map_err(|e| format!("message {i}: arguments are not JSON: {e}"))?;
                    open.push(id.to_owned());
                }
            }
            "tool" => {
                let id = message["tool_call_id"].as_str().unwrap_or_default();
                let Some(position) = open.iter().position(|open| open == id) else {
                    return Err(format!("message {i}: answer to unknown call {id:?}"));
                };
                open.remove(position);
                if !message["content"].is_string() {
                    return Err(format!("message {i}: tool content is not text"));
                }
            }
            other => return Err(format!("message {i}: role {other:?}")),
        }
    }
    if !open.is_empty() {
        return Err(format!("calls {open:?} have no answer"));
    }
    // A request that ends on the model's own message would be a prefill,
    // which Claude's newest models refuse.
    match messages
        .last()
        .map(|m| m["role"].as_str().unwrap_or_default())
    {
        Some("user" | "tool") => Ok(()),
        last => Err(format!("the request ends with {last:?}")),
    }
}

fn check_name(name: &str) -> Result<(), String> {
    // OpenAI, Claude and Gemini all name this pattern and length.
    let fine = !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if fine {
        Ok(())
    } else {
        Err(format!("function name {name:?}"))
    }
}

fn check_tool(tool: &Value) -> Result<(), String> {
    if tool["type"] != "function" {
        return Err(format!("tool type {}", tool["type"]));
    }
    let function = &tool["function"];
    let name = function["name"].as_str().unwrap_or_default();
    check_name(name)?;
    if function["description"].as_str().is_none_or(str::is_empty) {
        return Err(format!("{name}: no description"));
    }
    let parameters = &function["parameters"];
    if parameters["type"] != "object" {
        return Err(format!("{name}: parameters are not an object schema"));
    }
    check_required(name, parameters)
}

/// Gemini refuses a schema that requires a property it doesn't list; the
/// others accept it. Checked for every schema inside, too.
fn check_required(name: &str, schema: &Value) -> Result<(), String> {
    if let Some(required) = schema.get("required") {
        let required = required.as_array().ok_or(format!("{name}: required"))?;
        for property in required {
            let property = property.as_str().unwrap_or_default();
            if schema["properties"].get(property).is_none() {
                return Err(format!("{name}: required {property} is not a property"));
            }
        }
    }
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for inner in properties.values() {
            check_required(name, inner)?;
        }
    }
    if let Some(items) = schema.get("items") {
        check_required(name, items)?;
    }
    Ok(())
}
