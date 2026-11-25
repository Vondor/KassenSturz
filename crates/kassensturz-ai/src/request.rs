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

/// The largest text-only body KassenSturz sends at once.
pub const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;

/// The largest body KassenSturz sends at once when image attachments are
/// included (D-990, D-996). Text content stays within [`MAX_REQUEST_BYTES`].
pub const MAX_MULTIMODAL_REQUEST_BYTES: usize = 6 * 1024 * 1024;

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
        let max_bytes = self.max_request_bytes();
        // Reject obviously oversized text or image payloads before allocating
        // base64-encoded JSON strings (D-990, D-996).
        let text_bytes: usize = self
            .messages
            .iter()
            .map(|message| {
                message.content.len().saturating_add(
                    message
                        .calls
                        .iter()
                        .map(|call| call.arguments.len())
                        .fold(0usize, usize::saturating_add),
                )
            })
            .fold(0usize, usize::saturating_add);
        if text_bytes > MAX_REQUEST_BYTES {
            return Err(AiError::RequestTooLarge);
        }
        let encoded_image_bytes: usize = self
            .messages
            .iter()
            .filter(|message| message.has_images())
            .flat_map(|message| message.images.iter())
            .map(|image| image.data.len().div_ceil(3).saturating_mul(4))
            .fold(0usize, usize::saturating_add);
        if text_bytes.saturating_add(encoded_image_bytes) > max_bytes {
            return Err(AiError::RequestTooLarge);
        }
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
                let compact = body.to_string();
                if compact.len() > max_bytes {
                    return Err(AiError::RequestTooLarge);
                }
                let translated = translator.request(&compact)?;
                if translated.len() > max_bytes {
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
        if self.body.len() > max_bytes {
            return Err(AiError::RequestTooLarge);
        }
        Ok(())
    }

    fn max_request_bytes(&self) -> usize {
        if self.has_images() {
            MAX_MULTIMODAL_REQUEST_BYTES
        } else {
            MAX_REQUEST_BYTES
        }
    }

    /// Whether any message in this request carries image attachments that will
    /// be sent to the provider.
    pub fn has_images(&self) -> bool {
        self.messages.iter().any(Message::has_images)
    }

    /// How many image attachments will be sent to the provider in this request.
    pub fn image_count(&self) -> usize {
        self.messages
            .iter()
            .filter(|message| message.has_images())
            .map(|message| message.images.len())
            .sum()
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

    /// A readable view of the body for UI display or logs, replacing base64
    /// `image_url` payloads with `[image: <mime>, <size>]` markers while
    /// keeping all text and redactions intact.
    ///
    /// For text-only requests this matches [`Self::preview`] verbatim.
    /// [`crate::AiClient::send`] always sends [`Self::preview`], never this
    /// summary (threat T-22).
    pub fn display_preview(&self) -> String {
        if !self.has_images() {
            return self.body.clone();
        }
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&self.body) else {
            return self.body.clone();
        };
        summarize_image_urls(&mut value);
        serde_json::to_string_pretty(&value).unwrap_or_else(|_| self.body.clone())
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

/// Replaces `data:image/...;base64,...` strings inside OpenAI `image_url` parts
/// with compact `[image: <mime>, <size>]` markers for readable display.
fn summarize_image_urls(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                summarize_image_urls(item);
            }
        }
        serde_json::Value::Object(entries) => {
            if entries.get("type").and_then(serde_json::Value::as_str) == Some("image_url")
                && let Some(serde_json::Value::Object(image_url)) = entries.get_mut("image_url")
                && let Some(serde_json::Value::String(url)) = image_url.get_mut("url")
                && let Some(summary) = summarize_data_uri(url)
            {
                *url = summary;
                return;
            }
            for entry in entries.values_mut() {
                summarize_image_urls(entry);
            }
        }
        _ => {}
    }
}

fn summarize_data_uri(text: &str) -> Option<String> {
    let rest = text.strip_prefix("data:")?;
    let (mime, payload) = rest.split_once(";base64,")?;
    if !mime.starts_with("image/") {
        return None;
    }
    let padding = payload
        .as_bytes()
        .iter()
        .rev()
        .take(2)
        .filter(|&&b| b == b'=')
        .count();
    let decoded_bytes = (payload.len() / 4)
        .saturating_mul(3)
        .saturating_sub(padding);
    let size = if decoded_bytes < 1024 {
        format!("{decoded_bytes} B")
    } else {
        format!("{} KB", decoded_bytes.div_ceil(1024))
    };
    Some(format!("[image: {mime}, {size}]"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::ImageAttachment;

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
        assert_eq!(request.display_preview(), request.preview());
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
    fn redaction_updates_multimodal_text_parts_without_corrupting_images() {
        // Choose image bytes whose base64 encoding contains "AAAA" so we verify
        // redacting a substring that also appears in the base64 data URI leaves
        // the image untouched.
        let png = ImageAttachment::png(vec![0, 0, 0, 1, 2, 3]);
        let expected_uri = png.data_uri();
        assert!(expected_uri.contains("AAAA"));
        let jpeg = ImageAttachment::jpeg(vec![0xFF, 0xD8, 0xFF, 0xD9]);
        let expected_jpeg_uri = jpeg.data_uri();

        let mut req = request(vec![
            Message::user_with_images("Check invoice AAAA from Space Colony Shop", vec![png]),
            Message::tool_result_with_images(
                "c1",
                r#"{"file":"Space Colony Shop AAAA.pdf","pages":1}"#,
                vec![jpeg],
            ),
        ]);
        assert!(req.has_images());
        assert_eq!(req.image_count(), 2);

        assert_eq!(req.redact("Space Colony Shop").unwrap(), 2);
        assert_eq!(req.redact("AAAA").unwrap(), 2);

        let parsed: serde_json::Value = serde_json::from_str(req.preview()).unwrap();
        // Multimodal user message: text part redacted, image_url intact.
        assert_eq!(
            parsed["messages"][0]["content"][0]["text"],
            "Check invoice [redacted] from [redacted]"
        );
        assert_eq!(
            parsed["messages"][0]["content"][1]["image_url"]["url"],
            expected_uri
        );
        // Tool result: JSON text redacted, follow-up user message image_url intact.
        assert_eq!(
            parsed["messages"][1]["content"],
            r#"{"file":"[redacted] [redacted].pdf","pages":1}"#
        );
        assert_eq!(parsed["messages"][2]["role"], "user");
        assert_eq!(
            parsed["messages"][2]["content"][1]["image_url"]["url"],
            expected_jpeg_uri
        );

        // Readable display preview keeps redactions and summarizes image payloads.
        let display: serde_json::Value = serde_json::from_str(&req.display_preview()).unwrap();
        assert_eq!(
            display["messages"][0]["content"][0]["text"],
            "Check invoice [redacted] from [redacted]"
        );
        assert_eq!(
            display["messages"][0]["content"][1]["image_url"]["url"],
            "[image: image/png, 6 B]"
        );
        assert_eq!(
            display["messages"][2]["content"][1]["image_url"]["url"],
            "[image: image/jpeg, 4 B]"
        );
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

    #[test]
    fn multimodal_request_size_limit_allows_bounded_images_and_rejects_oversized() {
        // 2 MiB raw PNG expands to ~2.67 MiB base64: exceeds text-only
        // MAX_REQUEST_BYTES (2 MiB) but fits within MAX_MULTIMODAL_REQUEST_BYTES (6 MiB).
        let bounded = ImageAttachment::png(vec![0xAB; 2 * 1024 * 1024]);
        let ok = PreparedRequest::new(
            ProviderAddress::parse("http://localhost:11434/v1").unwrap(),
            "m".into(),
            vec![Message::user_with_images(
                "Read this receipt",
                vec![bounded],
            )],
            Vec::new(),
            None,
        )
        .expect("bounded multimodal request fits");
        assert!(ok.preview().len() > MAX_REQUEST_BYTES);
        assert!(ok.preview().len() <= MAX_MULTIMODAL_REQUEST_BYTES);
        assert!(ok.display_preview().contains("[image: image/png, 2048 KB]"));
        assert!(ok.display_preview().len() < 1024);

        // 5 MiB raw PNG expands to ~6.67 MiB base64: refused.
        let huge_image = ImageAttachment::png(vec![0xAB; 5 * 1024 * 1024]);
        let too_large = PreparedRequest::new(
            ProviderAddress::parse("http://localhost:11434/v1").unwrap(),
            "m".into(),
            vec![Message::user_with_images("Read this", vec![huge_image])],
            Vec::new(),
            None,
        );
        assert_eq!(too_large, Err(AiError::RequestTooLarge));

        // Attaching a tiny image does not raise the text budget past MAX_REQUEST_BYTES.
        let huge_text_with_image = PreparedRequest::new(
            ProviderAddress::parse("http://localhost:11434/v1").unwrap(),
            "m".into(),
            vec![Message::user_with_images(
                "x".repeat(MAX_REQUEST_BYTES + 1),
                vec![ImageAttachment::png(vec![1, 2, 3])],
            )],
            Vec::new(),
            None,
        );
        assert_eq!(huge_text_with_image, Err(AiError::RequestTooLarge));
    }
}
