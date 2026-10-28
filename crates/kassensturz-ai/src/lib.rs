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
    KEY_USE_KEY, KeyUse, MODEL_KEY, ProviderSettings, SLOW_ANSWER_KEY, SlowAnswer,
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

    /// Like [`Provider::send`], but gives up as soon as `stopped` answers
    /// true, with an error the caller throws away: the user pressed Stop
    /// or Cancel (D-246). The default waits for the reply.
    fn send_until(
        &self,
        request: &PreparedRequest,
        key: Option<&Secret>,
        stopped: &dyn Fn() -> bool,
    ) -> Result<Reply, AiError> {
        let _ = stopped;
        self.send(request, key)
    }

    /// The provider's last reply as it came, before it was read, so the
    /// developer mode can show why a reply wasn't understood (D-235). Kept
    /// in memory only; a provider without a wire gives none.
    fn last_reply_text(&self) -> Option<String> {
        None
    }
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

    fn send_until(
        &self,
        request: &PreparedRequest,
        key: Option<&Secret>,
        stopped: &dyn Fn() -> bool,
    ) -> Result<Reply, AiError> {
        AiClient::send_until(self, request, key, stopped)
    }

    fn last_reply_text(&self) -> Option<String> {
        self.last_reply.lock().ok().and_then(|reply| reply.clone())
    }
}

/// How often a call that can be stopped looks at the stop button.
const STOP_POLL: std::time::Duration = std::time::Duration::from_millis(200);

/// The way to the provider set up on this computer.
#[derive(Clone, Debug)]
pub struct AiClient {
    config: ConfigFile,
    limits: Limits,
    /// The last reply's bytes as text, shared by the clones (D-235).
    last_reply: std::sync::Arc<std::sync::Mutex<Option<String>>>,
}

impl AiClient {
    /// A client that reads the AI settings from `config` at every call:
    /// [`ConfigFile::user`] in the app.
    pub fn new(config: ConfigFile) -> Self {
        Self {
            config,
            limits: Limits::default(),
            last_reply: Default::default(),
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
        let limits = match config.slow_answer() {
            SlowAnswer::Timeout => self.limits,
            SlowAnswer::Warn => self.limits.keep_waiting(),
        };
        // An earlier reply must not be shown as this call's (D-235).
        if let Ok(mut last) = self.last_reply.lock() {
            *last = None;
        }
        let mut error_body = Vec::new();
        let reply = http::post_json(
            &provider.address.endpoint("chat/completions"),
            request.preview(),
            key,
            &limits,
            &mut error_body,
        )
        .inspect_err(|_| {
            if !error_body.is_empty()
                && let Ok(mut last) = self.last_reply.lock()
            {
                *last = Some(String::from_utf8_lossy(&error_body).into_owned());
            }
        })
        .and_then(|reply| {
            if let Ok(mut last) = self.last_reply.lock() {
                *last = Some(String::from_utf8_lossy(&reply).into_owned());
            }
            openai::parse_chat_reply(&reply, &request.function_names())
        });
        reply.map_err(|error| {
            let error = match error {
                AiError::ProviderFailed { status: 400 | 413 } if context_exceeded(&error_body) => {
                    AiError::ContextTooSmall
                }
                other => other,
            };
            self.missing_v1(&provider.address, key, error)
        })
    }

    /// Like [`AiClient::send`] on a thread of its own, so the caller can
    /// stop waiting: when `stopped` answers true, this returns at once with
    /// [`AiError::Timeout`], which the caller throws away. The call itself
    /// can't be called back; it ends by its own time limit and its reply is
    /// dropped (D-246).
    pub fn send_until(
        &self,
        request: &PreparedRequest,
        key: Option<&Secret>,
        stopped: &dyn Fn() -> bool,
    ) -> Result<Reply, AiError> {
        let (sender, receiver) = std::sync::mpsc::channel();
        let client = self.clone();
        let request = request.clone();
        let key = key.cloned();
        std::thread::spawn(move || {
            let _ = sender.send(client.send(&request, key.as_ref()));
        });
        loop {
            match receiver.recv_timeout(STOP_POLL) {
                Ok(reply) => return reply,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) if stopped() => {
                    return Err(AiError::Timeout);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(AiError::Timeout);
                }
            }
        }
    }

    /// The models the provider offers, for choosing one and to test the
    /// connection. Sends nothing from the book, only the key, so it needs
    /// no consent.
    pub fn list_models(&self, key: Option<&Secret>) -> Result<Vec<String>, AiError> {
        let config = self.config.read();
        if !config.enabled() {
            return Err(AiError::Off);
        }
        // The model needn't be chosen yet: this is how the user finds it.
        let provider = config.connection()?;
        let key = key_to_send(provider, key)?;
        self.models_at(&provider.address, key)
            .map_err(|error| self.missing_v1(&provider.address, key, error))
    }

    fn models_at(
        &self,
        address: &ProviderAddress,
        key: Option<&Secret>,
    ) -> Result<Vec<String>, AiError> {
        let reply = http::get(&address.endpoint("models"), key, &self.limits)?;
        openai::parse_models(&reply)
    }

    /// An address without a path whose answer is an error, or isn't an
    /// answer of the interface, is often the provider's address missing
    /// `/v1`, and some providers (e.g. LM Studio) say so with HTTP 200 and
    /// an error text. When the list of models is there with `/v1`, the error
    /// becomes [`AiError::MissingV1`], so the app can offer the right
    /// address (D-233). Only the key goes to the extra address, the same
    /// server the user chose.
    fn missing_v1(
        &self,
        address: &ProviderAddress,
        key: Option<&Secret>,
        error: AiError,
    ) -> AiError {
        if !matches!(error, AiError::BadReply | AiError::NotFound) {
            return error;
        }
        match address.with_v1() {
            Some(better) if self.models_at(&better, key).is_ok() => AiError::MissingV1,
            _ => error,
        }
    }
}

/// Whether the body of a provider's error says the request is longer than
/// the model's context. Providers word it differently ("exceeds the
/// available context size", "maximum context length", "n_ctx"), so a few
/// phrases are looked for; the text is only read, never shown.
fn context_exceeded(body: &[u8]) -> bool {
    let text = String::from_utf8_lossy(body).to_lowercase();
    ["context size", "context length", "context_length", "n_ctx"]
        .iter()
        .any(|phrase| text.contains(phrase))
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
