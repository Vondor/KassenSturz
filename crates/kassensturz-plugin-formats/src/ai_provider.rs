// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! AI provider plugins (#791 step 8, D-754, D-928): a plugin that only
//! translates between KassenSturz's OpenAI-compatible requests and another
//! provider's interface, behind `kassensturz_ai::Translator`.
//!
//! The plugin never reaches the network: it gets no host function for it,
//! only bytes in and bytes out. KassenSturz sends the request it built, to
//! the address the user typed, along the plugin's fixed [`Route`] (checked
//! by `kassensturz-ai`), with the key in the header the route names. The
//! plugin never sees the key.

use std::sync::{Mutex, PoisonError};

use kassensturz_ai::{AiError, Route, Translator};
use kassensturz_plugin_api::{ExtensionPoint, Permission};
use kassensturz_plugin_host::{HostError, Plugin};
use serde_json::Value;

use crate::read::{self, Fields};

/// The call that tells where requests go and how the key is sent.
pub const AI_INFO: &str = "ks_ai_info";
/// The call that turns an OpenAI-compatible chat request into the
/// provider's.
pub const AI_REQUEST: &str = "ks_ai_request";
/// The call that turns the provider's chat answer into an
/// OpenAI-compatible one.
pub const AI_REPLY: &str = "ks_ai_reply";
/// The call that turns the provider's list of models into an
/// OpenAI-compatible one.
pub const AI_MODELS: &str = "ks_ai_models";

/// An AI provider plugin as a translator. The plugin must ask for
/// `network` (D-848), so the Plugins tab shows that what it builds goes to
/// the address the user enters.
pub struct PluginAiProvider {
    id: String,
    plugin: Mutex<Plugin>,
    route: Route,
}

impl std::fmt::Debug for PluginAiProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PluginAiProvider")
            .field("id", &self.id)
            .field("route", &self.route)
            .finish_non_exhaustive()
    }
}

impl PluginAiProvider {
    /// Makes a translator of a loaded plugin and asks it for its route.
    pub fn new(mut plugin: Plugin) -> Result<Self, HostError> {
        let manifest = plugin.manifest();
        if !manifest
            .extension_points
            .contains(&ExtensionPoint::AiProvider)
        {
            return Err(HostError::ExportMissing {
                name: AI_INFO.to_owned(),
            });
        }
        if !manifest.permissions.contains(&Permission::Network) {
            return Err(HostError::PermissionMissing {
                permission: Permission::Network,
            });
        }
        let id = manifest.id.as_str().to_owned();
        let answer = plugin.call(AI_INFO, &[])?;
        let route = route(&answer).ok_or(HostError::OutputInvalid)?;
        Ok(Self {
            id,
            plugin: Mutex::new(plugin),
            route,
        })
    }

    /// Calls `name` with `input`; any failure is `ai.plugin-failed`.
    fn call(&self, name: &str, input: &[u8]) -> Result<Vec<u8>, AiError> {
        self.plugin
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .call(name, input)
            .map_err(|_| AiError::PluginFailed)
    }
}

impl Translator for PluginAiProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn route(&self) -> &Route {
        &self.route
    }

    fn request(&self, openai_body: &str) -> Result<String, AiError> {
        let answer = self.call(AI_REQUEST, openai_body.as_bytes())?;
        String::from_utf8(answer).map_err(|_| AiError::PluginFailed)
    }

    fn reply(&self, provider_answer: &[u8]) -> Result<Vec<u8>, AiError> {
        self.call(AI_REPLY, provider_answer)
    }

    fn models(&self, provider_answer: &[u8]) -> Result<Vec<u8>, AiError> {
        self.call(AI_MODELS, provider_answer)
    }
}

/// The route from `ks_ai_info`'s answer: `{"chat_path", "models_path",
/// "key_header", "key_prefix"?, "headers"?: {name: value}}`.
fn route(answer: &[u8]) -> Option<Route> {
    let object = read::object(answer).ok()?;
    let fields = Fields::of_map(
        &object,
        &[
            "chat_path",
            "models_path",
            "key_header",
            "key_prefix",
            "headers",
        ],
    )
    .ok()?;
    let prefix = fields.optional_text("key_prefix").ok()?.unwrap_or_default();
    let headers = match fields.optional("headers") {
        None => Vec::new(),
        Some(Value::Object(headers)) => headers
            .iter()
            .map(|(name, value)| Some((name.clone(), value.as_str()?.to_owned())))
            .collect::<Option<Vec<_>>>()?,
        Some(_) => return None,
    };
    Route::new(
        fields.text("chat_path").ok()?,
        fields.text("models_path").ok()?,
        fields.text("key_header").ok()?,
        &prefix,
        headers,
    )
    .ok()
}
