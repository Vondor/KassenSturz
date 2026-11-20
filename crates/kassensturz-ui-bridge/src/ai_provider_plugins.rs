// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The kind of AI service (#791 step 8, D-754, D-928): OpenAI-compatible,
//! or an AI provider plugin that translates for another interface. The
//! choice is kept as the plugin's ID under `AI/ProviderPlugin` in the
//! settings of this computer, empty for OpenAI-compatible.
//!
//! The plugin is loaded each time a client is made, and only while AI is
//! on (D-013). When the chosen plugin is gone, every request fails with
//! `ai.plugin-missing` before anything is built or sent; KassenSturz never
//! falls back to sending to the provider without the user's choice.

use std::path::Path;
use std::sync::Arc;

use kassensturz_ai::{AiError, Route, Translator};
use kassensturz_core::ai_switch;
use kassensturz_plugin_api::ExtensionPoint;
use kassensturz_plugin_formats::PluginAiProvider;

use crate::settings::Settings;

/// The key of the chosen plugin in the AI settings group.
const KEY: &str = "ProviderPlugin";

/// The ID of the chosen AI provider plugin, `None` for OpenAI-compatible.
pub fn chosen(settings: &Settings) -> Option<String> {
    Some(settings.text(ai_switch::GROUP, KEY)).filter(|id| !id.is_empty())
}

/// Chooses the AI provider plugin `id`, or OpenAI-compatible for `None`.
/// Returns whether it was saved.
pub fn choose(settings: &Settings, id: Option<&str>) -> bool {
    settings.set_text(ai_switch::GROUP, KEY, id.unwrap_or_default())
}

/// The usable AI provider plugins in `folder` as `(id, name)`, sorted by
/// name. Only asked while AI is on.
pub fn available(folder: &Path) -> Vec<(String, String)> {
    crate::plugins::loaded(folder, ExtensionPoint::AiProvider, true)
        .into_iter()
        .filter_map(|(_, plugin)| {
            let name = plugin.manifest().name.clone();
            let provider = PluginAiProvider::new(plugin).ok()?;
            Some((provider.id().to_owned(), name))
        })
        .collect()
}

/// The translator of the plugin `id` in `folder`, or one that refuses
/// every request with `ai.plugin-missing` when it isn't there or doesn't
/// load.
pub fn translator(folder: Option<&Path>, id: &str) -> Arc<dyn Translator> {
    let found = folder.and_then(|folder| {
        crate::plugins::loaded(folder, ExtensionPoint::AiProvider, true)
            .into_iter()
            .filter(|(_, plugin)| plugin.manifest().id.as_str() == id)
            .find_map(|(_, plugin)| PluginAiProvider::new(plugin).ok())
    });
    match found {
        Some(provider) => Arc::new(provider),
        None => Arc::new(Missing {
            id: id.to_owned(),
            route: Route::openai(),
        }),
    }
}

/// The chosen plugin that is gone: it translates nothing.
#[derive(Debug)]
struct Missing {
    id: String,
    route: Route,
}

impl Translator for Missing {
    fn id(&self) -> &str {
        &self.id
    }

    fn ready(&self) -> Result<(), AiError> {
        Err(AiError::PluginMissing)
    }

    fn route(&self) -> &Route {
        &self.route
    }

    fn request(&self, _openai_body: &str) -> Result<String, AiError> {
        Err(AiError::PluginMissing)
    }

    fn reply(&self, _provider_answer: &[u8]) -> Result<Vec<u8>, AiError> {
        Err(AiError::PluginMissing)
    }

    fn models(&self, _provider_answer: &[u8]) -> Result<Vec<u8>, AiError> {
        Err(AiError::PluginMissing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_folder::TestFolder;
    use crate::test_plugins;

    const INFO: &str =
        r#"{"chat_path": "messages", "models_path": "models", "key_header": "x-api-key"}"#;

    /// An AI provider plugin whose calls hand back their input.
    fn echo_file(id: &str, name: &str) -> Vec<u8> {
        let mut data = String::new();
        for byte in INFO.bytes() {
            data.push_str(&format!("\\{byte:02x}"));
        }
        let echo = |name: &str| {
            format!(
                r#"(func (export "{name}") (param $ptr i32) (param $len i32) (result i64)
                    (i64.or (i64.shl (i64.extend_i32_u (local.get $ptr)) (i64.const 32))
                            (i64.extend_i32_u (local.get $len))))"#
            )
        };
        let wat = format!(
            r#"(module
                (memory (export "memory") 32)
                (global $next (mut i32) (i32.const 1048576))
                (func (export "ks_alloc") (param $len i32) (result i32)
                    (local $at i32)
                    (local.set $at (global.get $next))
                    (global.set $next (i32.add (global.get $next) (local.get $len)))
                    (local.get $at))
                (data (i32.const 65536) "{data}")
                (func (export "ks_ai_info") (param i32 i32) (result i64)
                    (i64.const {info}))
                {request} {reply} {models})"#,
            info = (65_536_u64 << 32) | INFO.len() as u64,
            request = echo("ks_ai_request"),
            reply = echo("ks_ai_reply"),
            models = echo("ks_ai_models"),
        );
        test_plugins::file_asking(id, name, &["ai_provider"], &["network"], &wat)
    }

    #[test]
    fn finds_the_ai_provider_plugins_and_their_translators() {
        let folder = TestFolder::new();
        std::fs::write(
            folder.join("b.wasm"),
            echo_file("org.example.messages", "Messages"),
        )
        .unwrap();
        std::fs::write(
            folder.join("a.wasm"),
            echo_file("org.example.another", "Another"),
        )
        .unwrap();
        std::fs::write(
            folder.join("c.wasm"),
            test_plugins::file("org.example.reader", "Reader", &["importer"], "(module)"),
        )
        .unwrap();
        assert_eq!(
            available(folder.path()),
            [
                ("org.example.another".to_owned(), "Another".to_owned()),
                ("org.example.messages".to_owned(), "Messages".to_owned()),
            ]
        );
        let translator = translator(Some(folder.path()), "org.example.messages");
        assert_eq!(translator.id(), "org.example.messages");
        assert_eq!(translator.ready(), Ok(()));
        assert_eq!(translator.route().key_header(), "x-api-key");
        assert_eq!(translator.request("{}").unwrap(), "{}");
    }

    #[test]
    fn a_plugin_that_is_gone_sends_nothing() {
        let folder = TestFolder::new();
        let gone = translator(Some(folder.path()), "org.example.messages");
        assert_eq!(gone.id(), "org.example.messages");
        assert_eq!(gone.ready(), Err(AiError::PluginMissing));
        assert_eq!(gone.request("{}"), Err(AiError::PluginMissing));
        assert_eq!(
            translator(None, "org.example.messages").ready(),
            Err(AiError::PluginMissing)
        );
    }

    #[test]
    fn the_choice_is_kept_under_ai_provider_plugin() {
        let folder = TestFolder::new();
        let file = folder.join("kassensturzrc");
        let settings = Settings::at(&file);
        assert_eq!(chosen(&settings), None);
        assert!(choose(&settings, Some("org.example.messages")));
        assert_eq!(
            chosen(&Settings::at(&file)),
            Some("org.example.messages".into())
        );
        let written = std::fs::read_to_string(&file).unwrap();
        assert!(
            written.contains("ProviderPlugin=org.example.messages"),
            "{written}"
        );
        assert!(choose(&settings, None));
        assert_eq!(chosen(&Settings::at(&file)), None);
    }
}
