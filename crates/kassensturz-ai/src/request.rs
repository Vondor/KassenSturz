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

use crate::address::ProviderAddress;
use crate::chat::{FunctionSpec, Message};
use crate::error::AiError;
use crate::openai;

/// What a redacted phrase is replaced with.
pub const REDACTED: &str = "[redacted]";

/// The largest body KassenSturz sends at once.
pub const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;

/// A chat request for one provider and model, with its body built.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedRequest {
    address: ProviderAddress,
    model: String,
    messages: Vec<Message>,
    functions: Vec<FunctionSpec>,
    body: String,
}

impl PreparedRequest {
    pub(crate) fn new(
        address: ProviderAddress,
        model: String,
        messages: Vec<Message>,
        functions: Vec<FunctionSpec>,
    ) -> Result<Self, AiError> {
        let mut request = Self {
            address,
            model,
            messages,
            functions,
            body: String::new(),
        };
        request.build()?;
        Ok(request)
    }

    fn build(&mut self) -> Result<(), AiError> {
        let body = openai::chat_body(&self.model, &self.messages, &self.functions);
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

    /// The names of the functions offered to the model.
    pub fn function_names(&self) -> Vec<String> {
        self.functions.iter().map(|f| f.name.clone()).collect()
    }

    /// The exact body that is sent.
    pub fn preview(&self) -> &str {
        &self.body
    }

    /// Replaces every `phrase` in the messages' text with [`REDACTED`] and
    /// builds the body again. Answers how many places were replaced; a
    /// phrase of only spaces replaces nothing.
    pub fn redact(&mut self, phrase: &str) -> Result<usize, AiError> {
        if phrase.trim().is_empty() {
            return Ok(0);
        }
        let mut count = 0;
        for message in &mut self.messages {
            let found = message.content.matches(phrase).count();
            if found > 0 {
                message.content = message.content.replace(phrase, REDACTED);
                count += found;
            }
        }
        if count > 0 {
            self.build()?;
        }
        Ok(count)
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
        )
        .unwrap()
    }

    #[test]
    fn the_preview_is_the_body() {
        let request = request(vec![Message::user("Hi")]);
        let parsed: serde_json::Value = serde_json::from_str(request.preview()).unwrap();
        assert_eq!(
            parsed,
            openai::chat_body("m", &[Message::user("Hi")], &[]),
            "the preview is the body KassenSturz builds"
        );
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
    fn a_huge_request_is_refused() {
        let result = PreparedRequest::new(
            ProviderAddress::parse("http://localhost:11434/v1").unwrap(),
            "m".into(),
            vec![Message::user("x".repeat(MAX_REQUEST_BYTES))],
            Vec::new(),
        );
        assert_eq!(result, Err(AiError::RequestTooLarge));
    }
}
