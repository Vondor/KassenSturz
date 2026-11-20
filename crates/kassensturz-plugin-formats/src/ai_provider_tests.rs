// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use kassensturz_ai::{
    AiClient, AiError, ConfigFile, KeyUse, Message, ProviderSettings, Translator,
};
use kassensturz_plugin_api::Permission;
use kassensturz_plugin_host::{HostError, Plugin};

use crate::PluginAiProvider;
use crate::tests::{module, plugin_file_asking};

const INFO: &str = r#"{"chat_path": "messages", "models_path": "models",
    "key_header": "x-api-key", "headers": {"example-version": "2026-01-01"}}"#;

fn load(info: &str, permissions: &[&str], request: Option<&str>) -> Plugin {
    let calls = [
        ("ks_ai_info", Some(info)),
        ("ks_ai_request", request),
        ("ks_ai_reply", None),
        ("ks_ai_models", None),
    ];
    Plugin::load(
        &plugin_file_asking(
            "org.example.messages",
            &["ai_provider"],
            permissions,
            &module(&calls),
        ),
        true,
    )
    .unwrap()
}

#[test]
fn reads_the_route_and_translates_through_the_plugin() {
    let provider = PluginAiProvider::new(load(INFO, &["network"], None)).unwrap();
    assert_eq!(provider.id(), "org.example.messages");
    let route = provider.route();
    assert_eq!(route.chat_path(), "messages");
    assert_eq!(route.models_path(), "models");
    assert_eq!(route.key_header(), "x-api-key");
    assert_eq!(route.key_prefix(), "");
    assert_eq!(
        route.headers(),
        [("example-version".to_owned(), "2026-01-01".to_owned())]
    );
    // This plugin hands back what it gets.
    assert_eq!(provider.request(r#"{"a": 1}"#).unwrap(), r#"{"a": 1}"#);
    assert_eq!(provider.reply(b"{}").unwrap(), b"{}");
    assert_eq!(provider.models(b"[]").unwrap(), b"[]");
}

#[test]
fn needs_the_network_permission() {
    let plugin = Plugin::load(
        &plugin_file_asking(
            "org.example.messages",
            &["ai_provider"],
            &[],
            &module(&[("ks_ai_info", Some(INFO))]),
        ),
        true,
    )
    .unwrap();
    assert!(matches!(
        PluginAiProvider::new(plugin),
        Err(HostError::PermissionMissing {
            permission: Permission::Network
        })
    ));
}

#[test]
fn refuses_a_route_that_leaves_the_users_address_or_sets_a_reserved_header() {
    for info in [
        r#"{"chat_path": "/messages", "models_path": "models", "key_header": "x-api-key"}"#,
        r#"{"chat_path": "../x", "models_path": "models", "key_header": "x-api-key"}"#,
        r#"{"chat_path": "messages", "models_path": "models", "key_header": "Host"}"#,
        r#"{"chat_path": "messages", "models_path": "models", "key_header": "x-api-key",
            "headers": {"Cookie": "a"}}"#,
        r#"{"chat_path": "messages", "models_path": "models", "key_header": "x-api-key",
            "headers": {"x-n": 1}}"#,
        r#"{"chat_path": "messages", "models_path": "models", "key_header": "x-api-key",
            "address": "https://elsewhere.example"}"#,
        r#"{"chat_path": "messages", "key_header": "x-api-key"}"#,
    ] {
        assert!(
            matches!(
                PluginAiProvider::new(load(info, &["network"], None)),
                Err(HostError::OutputInvalid)
            ),
            "{info}"
        );
    }
}

#[test]
fn an_ai_provider_plugin_is_never_loaded_while_ai_is_off() {
    let file = plugin_file_asking(
        "org.example.messages",
        &["ai_provider"],
        &["network"],
        &module(&[("ks_ai_info", Some(INFO))]),
    );
    assert!(matches!(Plugin::load(&file, false), Err(HostError::AiOff)));
}

/// Where the plugin meets the AI client: what it builds is the preview,
/// and an answer that isn't JSON is the plugin's failure. Nothing is sent.
#[test]
fn the_plugins_request_is_the_preview_of_the_ai_client() {
    let folder = std::env::temp_dir().join(format!(
        "kassensturz-ai-provider-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("kassensturzrc");
    let mut text = String::from("[AI]\nEnabled=true\n");
    let settings =
        ProviderSettings::new("http://localhost:11434/v1", "example-small", KeyUse::None).unwrap();
    for (key, value) in settings.entries() {
        text.push_str(&format!("{key}={value}\n"));
    }
    std::fs::write(&path, text).unwrap();

    let fixed = r#"{"model": "example-small", "input": "made up"}"#;
    let provider = PluginAiProvider::new(load(INFO, &["network"], Some(fixed))).unwrap();
    let client =
        AiClient::new(ConfigFile::at(&path)).with_translator(std::sync::Arc::new(provider));
    let request = client
        .prepare(vec![Message::user("How much is left?")], Vec::new())
        .unwrap();
    assert_eq!(request.translator_id(), Some("org.example.messages"));
    let preview: serde_json::Value = serde_json::from_str(request.preview()).unwrap();
    assert_eq!(
        preview,
        serde_json::from_str::<serde_json::Value>(fixed).unwrap()
    );

    let not_json = PluginAiProvider::new(load(INFO, &["network"], Some("plain text"))).unwrap();
    let client =
        AiClient::new(ConfigFile::at(&path)).with_translator(std::sync::Arc::new(not_json));
    assert_eq!(
        client.prepare(vec![Message::user("How much is left?")], Vec::new()),
        Err(AiError::PluginFailed)
    );
    let _ = std::fs::remove_dir_all(&folder);
}
