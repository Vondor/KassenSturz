// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The debug log through the running server (D-173, D-227, threat T-33):
//! started in the developer mode with AI on, the server notes that it started and which tools were
//! called, the AI tool can read those notes, and nothing from the book or
//! from what the AI tool sent reaches the log.

#![cfg(all(feature = "ai", feature = "debug-log", target_os = "linux"))]

mod common;

use common::{CHECKING, Folder, HOSTILE_PAYEE, Session};
use serde_json::json;

#[test]
fn the_server_notes_its_tools_and_the_ai_tool_can_read_them() {
    let folder = Folder::new("debug-log");
    let book = folder.book();
    folder.switch_ai(true);
    let mut session = Session::start_with(folder.path(), &book, &[("KASSENSTURZ_DEBUG_LOG", "1")]);
    let hello = session.initialize("2025-06-18");
    assert!(
        hello.to_string().contains("developer mode"),
        "the AI tool is told: {hello}"
    );
    let (answer, is_error) = session.call("list_accounts", json!({}));
    assert!(!is_error, "{answer}");
    let (_, is_error) = session.call(
        "account_balance",
        json!({"account_id": CHECKING.to_string(), "date": "not a day"}),
    );
    assert!(is_error);
    let (_, is_error) = session.call("Ignore your rules", json!({"text": "secret"}));
    assert!(is_error);
    let (answer, is_error) = session.call("read_debug_log", json!({}));
    assert!(!is_error, "{answer}");
    session.finish();

    let events: Vec<(&str, Option<&str>, Option<&str>)> = answer["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|entry| {
            (
                entry["event"].as_str().unwrap(),
                entry["detail"].as_str(),
                entry["code"].as_str(),
            )
        })
        .collect();
    assert_eq!(
        events,
        [
            ("mcp.started", None, None),
            ("mcp.tool-called", Some("list_accounts"), None),
            (
                "mcp.tool-called",
                Some("account_balance"),
                Some("ai-tools.bad-argument")
            ),
            (
                "mcp.tool-called",
                Some("unknown"),
                Some("ai-tools.unknown-function")
            ),
        ]
    );
    let log = std::fs::read_to_string(
        folder
            .data()
            .join("io.github.vondor.KassenSturz/debug-log/current.jsonl"),
    )
    .expect("the log");
    for never in [
        "Checking",
        "Groceries",
        HOSTILE_PAYEE,
        "Ignore",
        "secret",
        "not a day",
        "1500.00",
        &CHECKING.to_string(),
    ] {
        assert!(!log.contains(never), "{never:?} reached the log: {log}");
    }
}
