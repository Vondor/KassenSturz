// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A request ready to go, its preview and its redaction (D-035, D-182;
//! threat T-22).
//!
//! The preview is the exact body that is sent, byte for byte: the adapter
//! sends [`PreparedRequest::preview`] and nothing else. Redacting replaces
//! a phrase in every message with [`REDACTED`] and builds the body again,
//! so the user sees what goes out after the redaction too. The provider's
//! key is not part of the body; the adapter adds it as a header only when
//! it sends, after the preview was built (D-169, threat T-25).
//!
//! For a provider that isn't OpenAI-compatible, a [`Translator`] turns the
//! OpenAI-compatible body into the provider's (D-928); the preview is then
//! the translated body, the one that is sent.

use std::sync::Arc;

use crate::address::{Place, ProviderAddress};
use crate::chat::{FunctionSpec, Message, Role};
use crate::error::AiError;
use crate::openai;
use crate::translate::Translator;

/// What a redacted phrase is replaced with.
pub const REDACTED: &str = "[redacted]";

/// The largest body KassenSturz sends at once.
pub const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;

/// A chat request for one provider and model, with its body built.
#[derive(Clone, Debug)]
pub struct PreparedRequest {
    address: ProviderAddress,
    model: String,
    messages: Vec<Message>,
    functions: Vec<FunctionSpec>,
    translator: Option<Arc<dyn Translator>>,
    body: String,
    redacted: Vec<String>,
}

impl PartialEq for PreparedRequest {
    fn eq(&self, other: &Self) -> bool {
        self.address == other.address
            && self.model == other.model
            && self.messages == other.messages
            && self.functions == other.functions
            && self.translator_id() == other.translator_id()
            && self.body == other.body
    }
}

impl PreparedRequest {
    pub(crate) fn new(
        address: ProviderAddress,
        model: String,
        messages: Vec<Message>,
        functions: Vec<FunctionSpec>,
        translator: Option<Arc<dyn Translator>>,
    ) -> Result<Self, AiError> {
        let mut request = Self {
            address,
            model,
            messages,
            functions,
            translator,
            body: String::new(),
            redacted: Vec::new(),
        };
        request.build()?;
        Ok(request)
    }

    fn build(&mut self) -> Result<(), AiError> {
        // Only a server on this computer is asked to keep its answer short
        // (D-595).
        let max_tokens =
            (self.address.place() == Place::ThisComputer).then_some(openai::MAX_ANSWER_TOKENS);
        let body =
            openai::chat_body_limited(&self.model, &self.messages, &self.functions, max_tokens);
        let body = match &self.translator {
            None => body,
            Some(translator) => {
                // Read back, so what the translator built is JSON and the
                // preview shows it indented like any request.
                if body.to_string().len() > MAX_REQUEST_BYTES {
                    return Err(AiError::RequestTooLarge);
                }
                let translated = translator.request(&body.to_string())?;
                if translated.len() > MAX_REQUEST_BYTES {
                    return Err(AiError::RequestTooLarge);
                }
                match serde_json::from_str::<serde_json::Value>(&translated) {
                    Ok(value @ serde_json::Value::Object(_)) => value,
                    _ => return Err(AiError::PluginFailed),
                }
            }
        };
        // Indented, so the preview is readable; it is still the body sent.
        self.body = serde_json::to_string_pretty(&body).map_err(|_| AiError::BadReply)?;
        if self.body.len() > MAX_REQUEST_BYTES {
            return Err(AiError::RequestTooLarge);
        }
        Ok(())
    }

    /// The provider it is for.
    pub fn address(&self) -> &ProviderAddress {
        &self.address
    }

    /// The model it is for.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// The messages, as they will be sent.
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// The ID of the translator it was built with, `None` for an
    /// OpenAI-compatible provider.
    pub fn translator_id(&self) -> Option<&str> {
        self.translator.as_deref().map(Translator::id)
    }

    /// The names of the functions offered to the model.
    pub fn function_names(&self) -> Vec<String> {
        self.functions.iter().map(|f| f.name.clone()).collect()
    }

    /// The exact body that is sent.
    pub fn preview(&self) -> &str {
        &self.body
    }

    /// Phrases redacted from this request, in the order they were applied.
    pub fn redacted_phrases(&self) -> &[String] {
        &self.redacted
    }

    /// Replaces every `phrase` in the messages' text and tool-call arguments
    /// with [`REDACTED`] and builds the body again. Answers how many places
    /// were replaced; a phrase of only spaces replaces nothing.
    pub fn redact(&mut self, phrase: &str) -> Result<usize, AiError> {
        if phrase.trim().is_empty() {
            return Ok(0);
        }
        if !self.redacted.iter().any(|existing| existing == phrase) {
            self.redacted.push(phrase.to_owned());
        }
        let mut count = 0;
        for message in &mut self.messages {
            if message.role == Role::Tool {
                count += redact_json_text(&mut message.content, phrase);
            } else {
                let found = message.content.matches(phrase).count();
                if found > 0 {
                    message.content = message.content.replace(phrase, REDACTED);
                    count += found;
                }
            }
            for call in &mut message.calls {
                count += redact_json_text(&mut call.arguments, phrase);
            }
        }
        if count > 0 {
            self.build()?;
        }
        Ok(count)
    }
}

/// Replaces `phrase` inside string values of a JSON text while keeping the
/// JSON valid, falling back to plain string replacement if `text` is not JSON.
fn redact_json_text(text: &mut String, phrase: &str) -> usize {
    if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(text) {
        let count = redact_json_value(&mut value, phrase);
        if count > 0
            && let Ok(updated) = serde_json::to_string(&value)
        {
            *text = updated;
        }
        return count;
    }
    let found = text.matches(phrase).count();
    if found > 0 {
        *text = text.replace(phrase, REDACTED);
    }
    found
}

fn redact_json_value(value: &mut serde_json::Value, phrase: &str) -> usize {
    match value {
        serde_json::Value::String(text) => {
            let found = text.matches(phrase).count();
            if found > 0 {
                *text = text.replace(phrase, REDACTED);
            }
            found
        }
        serde_json::Value::Array(items) => items
            .iter_mut()
            .map(|item| redact_json_value(item, phrase))
            .sum(),
        serde_json::Value::Object(entries) => entries
            .values_mut()
            .map(|entry| redact_json_value(entry, phrase))
            .sum(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(messages: Vec<Message>) -> PreparedRequest {
        PreparedRequest::new(
            ProviderAddress::parse("http://localhost:11434/v1").unwrap(),
            "m".into(),
            messages,
            Vec::new(),
            None,
        )
        .unwrap()
    }

    #[test]
    fn the_preview_is_the_body() {
        let request = request(vec![Message::user("Hi")]);
        let parsed: serde_json::Value = serde_json::from_str(request.preview()).unwrap();
        assert_eq!(
            parsed,
            openai::chat_body_limited(
                "m",
                &[Message::user("Hi")],
                &[],
                Some(openai::MAX_ANSWER_TOKENS)
            ),
            "the preview is the body KassenSturz builds"
        );
        assert_eq!(parsed["max_tokens"], 8192, "a server on this computer");
    }

    #[test]
    fn only_a_server_on_this_computer_gets_an_answer_limit() {
        // Some cloud models refuse max_tokens (D-595).
        let request = PreparedRequest::new(
            ProviderAddress::parse("https://api.example.com/v1").unwrap(),
            "m".into(),
            vec![Message::user("Hi")],
            Vec::new(),
            None,
        )
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(request.preview()).unwrap();
        assert!(parsed.get("max_tokens").is_none(), "{parsed}");
    }

    #[test]
    fn redaction_reaches_every_message_and_the_preview() {
        let mut request = request(vec![
            Message::system("You help with Space Colony Bank."),
            Message::user("What did Space Colony Bank charge?"),
            Message::tool_result("c", "[{\"payee\":\"Space Colony Bank\"}]"),
        ]);
        assert_eq!(request.redact("Space Colony Bank").unwrap(), 3);
        assert!(!request.preview().contains("Space Colony"));
        assert_eq!(request.preview().matches(REDACTED).count(), 3);
        assert_eq!(request.messages()[1].content, "What did [redacted] charge?");
        assert_eq!(request.redact("Space Colony Bank").unwrap(), 0);
        assert_eq!(request.redact("  ").unwrap(), 0);
    }

    #[test]
    fn redaction_also_reaches_tool_call_arguments() {
        use crate::chat::ToolCall;

        let mut request = request(vec![
            Message::user("Find Clinic \"Nord\""),
            Message::assistant(
                "",
                vec![ToolCall {
                    id: "c1".into(),
                    name: "search_transactions".into(),
                    arguments: r#"{"payee":"Clinic \"Nord\"","note":"Visit at Clinic \"Nord\""}"#
                        .into(),
                }],
            ),
            Message::tool_result("c1", r#"[{"payee":"Clinic \"Nord\""}]"#),
        ]);
        let count = request.redact(r#"Clinic "Nord""#).unwrap();
        assert_eq!(count, 4);
        assert!(!request.preview().contains("Nord"), "{}", request.preview());
        assert_eq!(request.redacted_phrases(), [r#"Clinic "Nord""#]);
        // The redacted arguments remain valid JSON.
        let args: serde_json::Value =
            serde_json::from_str(&request.messages()[1].calls[0].arguments).unwrap();
        assert_eq!(args["payee"], REDACTED);
        assert_eq!(args["note"], format!("Visit at {REDACTED}"));
    }

    #[test]
    fn a_huge_request_is_refused() {
        let result = PreparedRequest::new(
            ProviderAddress::parse("http://localhost:11434/v1").unwrap(),
            "m".into(),
            vec![Message::user("x".repeat(MAX_REQUEST_BYTES))],
            Vec::new(),
            None,
        );
        assert_eq!(result, Err(AiError::RequestTooLarge));
    }
}
