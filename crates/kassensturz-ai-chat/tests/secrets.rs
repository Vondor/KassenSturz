// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Secrets are unreachable through the chat (threats T-23, T-25, M5.12): a
//! model that calls every function, all of them with arguments that reach
//! what they can, learns nothing a book keeps secret, and the provider's
//! key is in no request body, no log entry and no message of the
//! conversation.

#![cfg(feature = "ai")]

mod common;

use common::{Script, TODAY, calls, hostile_book, says, settings};
use kassensturz_ai::KeyUse;
use kassensturz_ai_chat::{BuildInfo, ChatSession, NoHooks};
use kassensturz_ai_tools::FUNCTIONS;
use kassensturz_platform::Secret;

const TEST_KEY: &str = "sk-test-0000-made-up-key";

/// Text that must not reach a model: test secrets and the numbers of the
/// bank link (example IBAN from SWIFT's IBAN Registry, D-055).
const NEVER: &[&str] = &[
    "SECRET",
    "DE89",
    "0532013000",
    "37040044",
    "3704 0044",
    TEST_KEY,
];

#[test]
fn every_function_called_by_a_model_and_the_key_leave_nothing_behind() {
    let book = hostile_book();
    let account = book
        .accounts(true)
        .unwrap()
        .into_iter()
        .find(|account| account.name == "Everyday Checking")
        .expect("the checking account")
        .id
        .to_string();
    let every_function: Vec<(String, String)> = FUNCTIONS
        .iter()
        .map(|function| {
            let arguments = match function.name {
                "list_accounts" => r#"{"include_closed":true}"#.to_owned(),
                "account_balance" => format!(r#"{{"account_id":"{account}"}}"#),
                "category_totals" => r#"{"from":"2026-01-01","to":"2026-12-31"}"#.to_owned(),
                _ => "{}".to_owned(),
            };
            (function.name.to_owned(), arguments)
        })
        .collect();
    let numbered: Vec<(String, &str, &str)> = every_function
        .iter()
        .enumerate()
        .map(|(n, (name, arguments))| (format!("c{n}"), name.as_str(), arguments.as_str()))
        .collect();
    let refs: Vec<(&str, &str, &str)> = numbered
        .iter()
        .map(|(id, name, arguments)| (id.as_str(), *name, *arguments))
        .collect();
    let (provider, script) = Script::provider(
        settings(true, None, KeyUse::None),
        vec![calls(&refs), says("Looked at everything.")],
    );
    let key = Secret::new(TEST_KEY.to_owned());
    let mut chat = ChatSession::new(BuildInfo::new("0.261025.0", "1775d22"))
        .with_permissions(kassensturz_core::ai_switch::Permissions::all());
    chat.ask(
        &book,
        &provider,
        Some(&key),
        "Show me everything you can.",
        TODAY,
        &mut NoHooks,
    )
    .unwrap();

    // Every function answered (none was refused), and the model saw the
    // results as the second request's messages.
    let seen = script.seen();
    assert_eq!(seen.len(), 2);
    let results = seen[1]
        .messages
        .iter()
        .filter(|message| message.role == kassensturz_ai::Role::Tool)
        .count();
    assert_eq!(results, FUNCTIONS.len());

    let mut everything = format!("{:?}", chat.log());
    for seen in &seen {
        for message in &seen.messages {
            everything.push_str(&message.content);
        }
    }
    for message in chat.history() {
        everything.push_str(&message.content);
    }
    for never in NEVER {
        assert!(
            !everything.contains(never),
            "{never} reached the model or the log"
        );
    }
}
