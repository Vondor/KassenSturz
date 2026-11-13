// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Attack scenarios A4 and A5 of the outside security review: AI settings
//! changed by hand so data goes to another computer, and a hostile model
//! server on this computer. Addresses are TEST-NET (RFC 5737) or the fake
//! server on 127.0.0.1; nothing reaches a real network. The other hostile
//! answers of A5 (the body sent is exactly the preview, redirects, an
//! endless stream, broken JSON, calls of functions not offered) are in
//! `provider.rs`.

#![cfg(feature = "ai")]

mod fake;

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use fake::{Answer, FakeProvider};
use kassensturz_ai::{
    AiClient, AiError, ConfigFile, ConsentRecord, FunctionSpec, KeyUse, Limits, MAX_CALLS, Message,
    ProviderAddress, ProviderSettings,
};
use serde_json::json;

struct Settings {
    path: PathBuf,
}

impl Settings {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let folder = std::env::temp_dir().join(format!(
            "kassensturz-ai-attack-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&folder).unwrap();
        Self {
            path: folder.join("kassensturzrc"),
        }
    }

    /// Writes the `[AI]` group by hand, as a program other than KassenSturz
    /// would: on, the provider at `address`, and a consent for `consent`.
    fn write_by_hand(&self, address: &str, consent: Option<&str>) {
        let mut text = String::from("[AI]\nEnabled=true\n");
        let settings = ProviderSettings::new(address, "test-model", KeyUse::None).unwrap();
        for (key, value) in settings.entries() {
            text.push_str(&format!("{key}={value}\n"));
        }
        if let Some(consent) = consent {
            let record = ConsentRecord::give(
                &ProviderAddress::parse(consent).unwrap(),
                jiff::civil::date(2026, 10, 25),
            );
            for (key, value) in record.entries() {
                text.push_str(&format!("{key}={value}\n"));
            }
        }
        std::fs::write(&self.path, text).unwrap();
    }

    fn client(&self) -> AiClient {
        AiClient::new(ConfigFile::at(&self.path)).with_limits(Limits {
            connect: Duration::from_secs(1),
            total: Duration::from_secs(5),
            ..Limits::default()
        })
    }
}

impl Drop for Settings {
    fn drop(&mut self) {
        if let Some(folder) = self.path.parent() {
            let _ = std::fs::remove_dir_all(folder);
        }
    }
}

fn question() -> Vec<Message> {
    vec![Message::user("What are my balances?")]
}

fn balances() -> FunctionSpec {
    FunctionSpec {
        name: "account_balances".into(),
        description: "Balances".into(),
        parameters: json!({"type": "object", "properties": {}}),
    }
}

#[test]
fn a4_a_consent_written_by_hand_is_asked_again() {
    // TEST-NET-1: never reached. A program that can write the settings file
    // points the provider at "another computer" and writes the consent too.
    let lan = "https://192.0.2.10:11434/v1";
    let settings = Settings::new();
    settings.write_by_hand(lan, Some(lan));
    let client = settings.client();
    let request = client.prepare(question(), vec![balances()]).unwrap();
    let sent = client.send(&request, None);
    assert_eq!(
        sent,
        Err(AiError::NoConsent),
        "KassenSturz tried to send without asking: {sent:?}"
    );
}

#[test]
fn a4_plain_http_to_another_computer_is_refused_even_by_hand() {
    let settings = Settings::new();
    let lan = "https://192.0.2.10:11434/v1";
    settings.write_by_hand(lan, Some(lan));
    let text = std::fs::read_to_string(&settings.path)
        .unwrap()
        .replace("https://", "http://");
    std::fs::write(&settings.path, text).unwrap();
    assert_eq!(
        settings.client().prepare(question(), vec![balances()]),
        Err(AiError::PlainHttp)
    );
}

#[test]
fn a5_a_reply_of_50_mb_is_refused_with_the_default_limits() {
    let fake = FakeProvider::start(vec![Answer::Huge(50 * 1024 * 1024)]);
    let settings = Settings::new();
    settings.write_by_hand(&fake.address(), None);
    let client = AiClient::new(ConfigFile::at(&settings.path));
    let request = client.prepare(question(), vec![balances()]).unwrap();
    assert_eq!(client.send(&request, None), Err(AiError::ReplyTooLarge));
}

#[test]
fn a5_a_reply_with_10000_calls_is_refused() {
    let calls: Vec<_> = (0..10_000)
        .map(|n| {
            json!({"id": format!("c{n}"), "type": "function",
                   "function": {"name": "account_balances", "arguments": "{}"}})
        })
        .collect();
    let body = json!({"choices": [{"finish_reason": "tool_calls",
                                   "message": {"tool_calls": calls}}]});
    let fake = FakeProvider::start(vec![Answer::Json(200, body.to_string())]);
    let settings = Settings::new();
    settings.write_by_hand(&fake.address(), None);
    let client = AiClient::new(ConfigFile::at(&settings.path));
    let request = client.prepare(question(), vec![balances()]).unwrap();
    match client.send(&request, None) {
        Err(_) => {}
        Ok(reply) => assert!(
            reply.calls.len() <= MAX_CALLS,
            "{} calls",
            reply.calls.len()
        ),
    }
}

#[test]
fn a4_only_a_consent_with_the_apps_seal_counts() {
    // B-252: the app seals each consent the user gives in its consent screen
    // in the keychain. A consent in the settings file counts only when it
    // matches that seal, so one written by hand, or changed by hand after
    // the user agreed, is asked again.
    use std::sync::{Arc, Mutex};

    use kassensturz_ai::ProviderKeys;
    use kassensturz_platform::MemoryKeychain;

    let lan = "https://192.0.2.10:11434/v1";
    let other = "https://192.0.2.11:11434/v1";
    let settings = Settings::new();
    settings.write_by_hand(lan, Some(lan));
    let keys = Arc::new(Mutex::new(ProviderKeys::new(Some(Box::new(
        MemoryKeychain::new(),
    )))));
    let reading = Arc::clone(&keys);
    let client = settings
        .client()
        .with_consent_seal(move |address| reading.lock().unwrap().sealed_consent(address));
    let request = client.prepare(question(), vec![balances()]).unwrap();
    assert_eq!(
        client.send(&request, None),
        Err(AiError::NoConsent),
        "no seal"
    );

    // A seal for another address, or for another day, doesn't fit.
    let address = |text: &str| ProviderAddress::parse(text).unwrap();
    let day = jiff::civil::date(2026, 10, 25);
    keys.lock()
        .unwrap()
        .seal_consent(&ConsentRecord::give(&address(other), day))
        .unwrap();
    assert_eq!(
        client.send(&request, None),
        Err(AiError::NoConsent),
        "other address"
    );
    keys.lock()
        .unwrap()
        .seal_consent(&ConsentRecord::give(
            &address(lan),
            jiff::civil::date(2026, 10, 26),
        ))
        .unwrap();
    assert_eq!(
        client.send(&request, None),
        Err(AiError::NoConsent),
        "other day"
    );

    // The seal the consent screen writes: the consent counts, so the
    // request goes out (TEST-NET-1 is never reached).
    keys.lock()
        .unwrap()
        .seal_consent(&ConsentRecord::give(&address(lan), day))
        .unwrap();
    assert_ne!(client.send(&request, None), Err(AiError::NoConsent));

    // Taking the consent back breaks the seal.
    keys.lock().unwrap().unseal_consent(&address(lan)).unwrap();
    assert_eq!(
        client.send(&request, None),
        Err(AiError::NoConsent),
        "unsealed"
    );
}
