// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! KassenSturz's connection to an AI provider (AGENTS.md §8, M5.2).
//!
//! v1 speaks one API, the OpenAI-compatible one (D-035), which local
//! servers (Ollama, LM Studio, llama.cpp, vLLM) and many cloud services
//! offer. [`AiClient`] is the only way a request reaches a provider, and
//! every call checks, in this order, against the settings file of this
//! computer, never the book (D-168, threats T-24, T-28):
//!
//! 1. AI is switched on (`ai.off` otherwise);
//! 2. a provider is set up with a usable address: plain `http://` only for
//!    this computer (D-175, threat T-30);
//! 3. for [`AiClient::send`]: the request was prepared for that same
//!    provider and model, and a provider on another computer has the
//!    user's consent to the current statement of what is sent (D-035,
//!    D-182, threat T-22);
//! 4. a provider that needs a key gets the one the caller passes, added
//!    after the preview was built (D-169, threat T-25).
//!
//! Only then is the HTTP client built ([`Limits`], D-183). A request is
//! first prepared ([`AiClient::prepare`]), so the user can see its exact
//! body ([`PreparedRequest::preview`]) and remove phrases from it
//! ([`PreparedRequest::redact`]) before it goes.
//!
//! Everything the provider answers is untrusted ([`Reply`], threats T-21,
//! T-31). Errors are codes ([`AiError`], D-034).
//!
//! [`ScriptedProvider`] is a provider played by a function inside the app,
//! for tests and for trying the chat without any AI (D-193). The chat
//! itself, which joins a provider with the book's read functions, is
//! `kassensturz-ai-chat` (D-190).
//!
//! Without the `ai` feature this crate is empty and has no dependencies
//! (D-039, D-176).

#![cfg(feature = "ai")]
#![forbid(unsafe_code)]

mod address;
mod chat;
mod config;
mod consent;
mod error;
mod http;
mod keys;
mod openai;
mod request;
mod scripted;
#[cfg(test)]
mod tls_tests;

pub use address::{Place, ProviderAddress};
pub use chat::{Finish, FunctionSpec, Message, Reply, Role, ToolCall};
pub use config::{
    ADDRESS_KEY, AiConfig, CONSENT_ADDRESS_KEY, CONSENT_DAY_KEY, CONSENT_STATEMENT_KEY, ConfigFile,
    KEY_USE_KEY, KeyUse, MODEL_KEY, ProviderSettings,
};
pub use consent::{ConsentRecord, STATEMENT, STATEMENT_VERSION, SentData, consent_missing};
pub use error::AiError;
pub use http::Limits;
pub use keys::ProviderKeys;
pub use openai::{MAX_CALLS, MAX_DEPTH};
pub use request::{MAX_REQUEST_BYTES, PreparedRequest, REDACTED};
pub use scripted::{Respond, SCRIPTED_ADDRESS, SCRIPTED_MODEL, ScriptedProvider};

use kassensturz_platform::Secret;

/// What the chat (M5.4) needs from a provider: a request prepared for the
/// user to see, and sending it. [`AiClient`] is the OpenAI-compatible one
/// (D-035); a dedicated adapter can follow later behind the same trait.
pub trait Provider {
    /// Builds the request for `messages`, offering `functions`. Nothing is
    /// sent.
    fn prepare(
        &self,
        messages: Vec<Message>,
        functions: Vec<FunctionSpec>,
    ) -> Result<PreparedRequest, AiError>;

    /// Sends `request`, exactly as its preview shows, with `key` when the
    /// provider needs one, and reads the reply.
    fn send(&self, request: &PreparedRequest, key: Option<&Secret>) -> Result<Reply, AiError>;
}

impl Provider for AiClient {
    fn prepare(
        &self,
        messages: Vec<Message>,
        functions: Vec<FunctionSpec>,
    ) -> Result<PreparedRequest, AiError> {
        AiClient::prepare(self, messages, functions)
    }

    fn send(&self, request: &PreparedRequest, key: Option<&Secret>) -> Result<Reply, AiError> {
        AiClient::send(self, request, key)
    }
}

/// The way to the provider set up on this computer.
#[derive(Clone, Debug)]
pub struct AiClient {
    config: ConfigFile,
    limits: Limits,
}

impl AiClient {
    /// A client that reads the AI settings from `config` at every call:
    /// [`ConfigFile::user`] in the app.
    pub fn new(config: ConfigFile) -> Self {
        Self {
            config,
            limits: Limits::default(),
        }
    }

    /// The same client with other size and time limits.
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// The settings, if AI is on.
    fn settings(&self) -> Result<AiConfig, AiError> {
        let config = self.config.read();
        if !config.enabled() {
            return Err(AiError::Off);
        }
        config.provider()?;
        Ok(config)
    }

    /// Builds the request for `messages`, offering `functions`, for the
    /// provider set up now. Nothing is sent.
    pub fn prepare(
        &self,
        messages: Vec<Message>,
        functions: Vec<FunctionSpec>,
    ) -> Result<PreparedRequest, AiError> {
        let config = self.settings()?;
        let provider = config.provider()?;
        PreparedRequest::new(
            provider.address.clone(),
            provider.model.clone(),
            messages,
            functions,
        )
    }

    /// Sends `request`, exactly as its preview shows, with `key` when the
    /// provider needs one, and reads the reply.
    pub fn send(&self, request: &PreparedRequest, key: Option<&Secret>) -> Result<Reply, AiError> {
        let config = self.settings()?;
        let provider = config.provider()?;
        if provider.address != *request.address() || provider.model != request.model() {
            return Err(AiError::SettingsChanged);
        }
        if consent_missing(&provider.address, config.consent()) {
            return Err(AiError::NoConsent);
        }
        let key = key_to_send(provider, key)?;
        let reply = http::post_json(
            &provider.address.endpoint("chat/completions"),
            request.preview(),
            key,
            &self.limits,
        )?;
        openai::parse_chat_reply(&reply, &request.function_names())
    }

    /// The models the provider offers, for choosing one and to test the
    /// connection. Sends nothing from the book, only the key, so it needs
    /// no consent.
    pub fn list_models(&self, key: Option<&Secret>) -> Result<Vec<String>, AiError> {
        let config = self.settings()?;
        let provider = config.provider()?;
        let key = key_to_send(provider, key)?;
        let reply = http::get(&provider.address.endpoint("models"), key, &self.limits)?;
        openai::parse_models(&reply)
    }
}

/// The key to send: none to a provider that needs none, even if one is
/// passed, and `ai.key-needed` when one is needed and missing.
fn key_to_send<'k>(
    provider: &ProviderSettings,
    key: Option<&'k Secret>,
) -> Result<Option<&'k Secret>, AiError> {
    match (provider.key_use, key) {
        (KeyUse::None, _) => Ok(None),
        (_, Some(key)) if !key.is_empty() => Ok(Some(key)),
        _ => Err(AiError::KeyNeeded),
    }
}
