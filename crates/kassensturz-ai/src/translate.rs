// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Providers that aren't OpenAI-compatible, through a translator (D-754,
//! D-928): an AI provider plugin turns the OpenAI-compatible request that
//! KassenSturz builds into its provider's request, and its provider's
//! answers back into OpenAI-compatible ones. It only translates.
//!
//! KassenSturz keeps everything else, so a translator changes no rule of
//! [`crate::AiClient`]:
//!
//! - KassenSturz sends, with its own HTTPS client, only to the address the
//!   user typed; a translator names only a [`Route`]: fixed paths under
//!   that address and fixed headers, checked here.
//! - The key goes into the header the route names, added by KassenSturz
//!   when it sends; a translator never sees it.
//! - The body a translator builds is the body that is sent and shown in the
//!   preview, byte for byte, and redacting builds it again.
//! - What it hands back is read like any OpenAI-compatible answer, with
//!   the same limits and checks.

use std::fmt;

use crate::error::AiError;

/// The most headers a route adds besides the key's.
pub const MAX_ROUTE_HEADERS: usize = 8;
/// The most characters of a path, a header's name or a header's value.
pub const MAX_ROUTE_TEXT: usize = 200;

/// Headers a route may not set, because KassenSturz sets them or they
/// would change where or how the request goes. Compared in lowercase.
const RESERVED_HEADERS: [&str; 12] = [
    "host",
    "content-type",
    "content-length",
    "transfer-encoding",
    "connection",
    "upgrade",
    "te",
    "trailer",
    "expect",
    "cookie",
    "user-agent",
    "forwarded",
];

/// Where a translated request goes under the user's address, and how the
/// key is sent. Fixed for a translator: it can't depend on what is sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route {
    chat_path: String,
    models_path: String,
    key_header: String,
    key_prefix: String,
    headers: Vec<(String, String)>,
}

impl Route {
    /// A route, checked: paths of letters, digits and `. _ ~ / -` without
    /// `..`, a leading `/` or `//`; header names of letters, digits and
    /// `-`, none reserved, none twice; header values and the key's prefix
    /// of printable ASCII. `ai.plugin-failed` otherwise.
    pub fn new(
        chat_path: &str,
        models_path: &str,
        key_header: &str,
        key_prefix: &str,
        headers: Vec<(String, String)>,
    ) -> Result<Self, AiError> {
        let fine = path(chat_path)
            && path(models_path)
            && header_name(key_header)
            && value(key_prefix)
            && headers.len() <= MAX_ROUTE_HEADERS
            && headers
                .iter()
                .all(|(name, text)| header_name(name) && value(text) && !text.is_empty());
        let mut names: Vec<String> = headers
            .iter()
            .map(|(name, _)| name.to_lowercase())
            .collect();
        names.push(key_header.to_lowercase());
        names.sort();
        let unique = names.windows(2).all(|pair| pair[0] != pair[1]);
        if !fine || !unique {
            return Err(AiError::PluginFailed);
        }
        Ok(Self {
            chat_path: chat_path.to_owned(),
            models_path: models_path.to_owned(),
            key_header: key_header.to_owned(),
            key_prefix: key_prefix.to_owned(),
            headers,
        })
    }

    /// The OpenAI-compatible route: `chat/completions`, `models`, and the
    /// key as a bearer token.
    pub fn openai() -> Self {
        Self {
            chat_path: "chat/completions".into(),
            models_path: "models".into(),
            key_header: "Authorization".into(),
            key_prefix: "Bearer ".into(),
            headers: Vec::new(),
        }
    }

    /// The path of a chat request, under the provider's address.
    pub fn chat_path(&self) -> &str {
        &self.chat_path
    }

    /// The path of the list of models, under the provider's address.
    pub fn models_path(&self) -> &str {
        &self.models_path
    }

    /// The header the key goes into.
    pub fn key_header(&self) -> &str {
        &self.key_header
    }

    /// What goes before the key in that header, such as `Bearer `.
    pub fn key_prefix(&self) -> &str {
        &self.key_prefix
    }

    /// The fixed headers every request of the route carries.
    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }
}

fn path(text: &str) -> bool {
    (1..=MAX_ROUTE_TEXT).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._~/-".contains(&b))
        && !text.starts_with('/')
        && !text.contains("//")
        && !text.split('/').any(|part| part == ".." || part == ".")
}

fn header_name(text: &str) -> bool {
    (1..=MAX_ROUTE_TEXT).contains(&text.len())
        && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && !RESERVED_HEADERS.contains(&text.to_lowercase().as_str())
        && !text.to_lowercase().starts_with("proxy-")
}

fn value(text: &str) -> bool {
    text.len() <= MAX_ROUTE_TEXT && text.bytes().all(|b| (b' '..=b'~').contains(&b))
}

/// Translates between KassenSturz's OpenAI-compatible requests and answers
/// and another provider's (D-928). Every answer of a translator is
/// untrusted, and a failure is `ai.plugin-failed`.
pub trait Translator: Send + Sync + fmt::Debug {
    /// Names the translator, such as the plugin's ID: a request prepared
    /// with one is sent only with the same one.
    fn id(&self) -> &str;

    /// Whether it can translate now; `ai.plugin-missing` when the plugin
    /// behind it is gone. Asked before anything is built or sent.
    fn ready(&self) -> Result<(), AiError> {
        Ok(())
    }

    /// Where requests go and how the key is sent.
    fn route(&self) -> &Route;

    /// The provider's request body for `openai_body`, an OpenAI-compatible
    /// chat request.
    fn request(&self, openai_body: &str) -> Result<String, AiError>;

    /// The provider's answer to a chat request, as an OpenAI-compatible
    /// answer.
    fn reply(&self, provider_answer: &[u8]) -> Result<Vec<u8>, AiError>;

    /// The provider's list of models, as an OpenAI-compatible one.
    fn models(&self, provider_answer: &[u8]) -> Result<Vec<u8>, AiError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(chat: &str, header: &str, headers: &[(&str, &str)]) -> Result<Route, AiError> {
        Route::new(
            chat,
            "models",
            header,
            "",
            headers
                .iter()
                .map(|(name, text)| ((*name).to_owned(), (*text).to_owned()))
                .collect(),
        )
    }

    #[test]
    fn a_route_stays_under_the_users_address() {
        assert!(
            route(
                "v1/messages",
                "x-api-key",
                &[("example-version", "2026-01-01")]
            )
            .is_ok()
        );
        for chat in [
            "",
            "/messages",
            "../messages",
            "v1/../../x",
            "a//b",
            "messages?key=1",
            "messages#x",
            "https://elsewhere.example/x",
            "a/./b",
            "with space",
        ] {
            assert_eq!(
                route(chat, "x-api-key", &[]),
                Err(AiError::PluginFailed),
                "{chat}"
            );
        }
    }

    #[test]
    fn a_route_sets_no_header_that_changes_where_or_how_it_goes() {
        for name in [
            "Host",
            "content-type",
            "Cookie",
            "Proxy-Authorization",
            "x key",
            "",
        ] {
            assert_eq!(
                route("messages", "x-api-key", &[(name, "v")]),
                Err(AiError::PluginFailed),
                "{name}"
            );
            assert_eq!(
                route("messages", name, &[]),
                Err(AiError::PluginFailed),
                "{name}"
            );
        }
        // The key's header only once, and a value without line breaks.
        assert_eq!(
            route("messages", "x-api-key", &[("X-Api-Key", "v")]),
            Err(AiError::PluginFailed)
        );
        assert_eq!(
            route(
                "messages",
                "x-api-key",
                &[("example-version", "a\r\nHost: x")]
            ),
            Err(AiError::PluginFailed)
        );
        let many: Vec<(String, String)> = (0..=MAX_ROUTE_HEADERS)
            .map(|n| (format!("x-h{n}"), "v".to_owned()))
            .collect();
        assert_eq!(
            Route::new("messages", "models", "x-api-key", "", many),
            Err(AiError::PluginFailed)
        );
    }
}
