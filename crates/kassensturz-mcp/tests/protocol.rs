// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The protocol messages of the running program, as an AI tool sees them
//! (D-194, D-197): golden files of a whole session, the tools' answers
//! against the read functions' own output, and reading beside the app.
//!
//! `KASSENSTURZ_BLESS=1 cargo test -p kassensturz-mcp --test protocol`
//! writes the golden files instead of comparing; review the change before
//! committing it.

// Only Linux knows where the settings file is yet (D-177); elsewhere the
// switch reads as off and the program never starts.
#![cfg(all(feature = "ai", target_os = "linux"))]

mod common;

use std::path::{Path, PathBuf};

use common::{CHECKING, Folder, GROCERIES, HOSTILE_PAYEE, Session};
use kassensturz_core::{AccountKind, Date};
use kassensturz_store::Book;
use serde_json::{Value, json};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const COMMIT: &str = env!("KASSENSTURZ_BUILD_COMMIT");

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

/// `session` as text, with the parts that change with every build (the
/// version and the commit) put in braces.
fn normalized(session: &Value) -> String {
    let build = if COMMIT.is_empty() {
        format!("KassenSturz {VERSION} ")
    } else {
        format!("KassenSturz {VERSION} (commit {COMMIT}) ")
    };
    let text = serde_json::to_string_pretty(session).expect("JSON") + "\n";
    text.replace(&build, "KassenSturz {version} (commit {commit}) ")
        .replace(
            &format!("\"version\": \"{VERSION}\""),
            "\"version\": \"{version}\"",
        )
}

fn compare_with_golden(name: &str, session: &Value) {
    let text = normalized(session);
    let file = golden(name);
    if std::env::var_os("KASSENSTURZ_BLESS").is_some_and(|bless| bless == "1") {
        std::fs::create_dir_all(file.parent().expect("a folder")).expect("folder");
        std::fs::write(&file, text).expect("golden file written");
        return;
    }
    let expected = std::fs::read_to_string(&file).expect("golden file there");
    assert_eq!(
        text, expected,
        "{name} differs; KASSENSTURZ_BLESS=1 writes it anew"
    );
}

/// Each request and its answer, in order.
struct Transcript(Vec<Value>);

impl Transcript {
    fn request(&mut self, session: &mut Session, method: &str, params: Value) -> Value {
        let answer = session.request(method, params.clone());
        self.0
            .push(json!({"method": method, "params": params, "answer": answer}));
        answer
    }
}

#[test]
fn a_session_of_protocol_2025_06_18_matches_its_golden_file() {
    let folder = Folder::new("golden-2025-06-18");
    folder.switch_ai(true);
    let book = folder.book();
    let mut session = Session::start(folder.path(), &book);
    let mut transcript = Transcript(Vec::new());
    transcript.request(
        &mut session,
        "initialize",
        json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "test-client", "version": "1.0"}
        }),
    );
    session.notify("notifications/initialized");
    transcript.request(&mut session, "tools/list", json!({}));
    let id = CHECKING.to_string();
    for (name, arguments) in [
        ("list_accounts", json!({})),
        (
            "account_balance",
            json!({"account_id": id, "date": "2026-10-02"}),
        ),
        (
            "category_totals",
            json!({"from": "2026-10-01", "to": "2026-10-31"}),
        ),
        ("search_transactions", json!({"text": "corner"})),
        ("list_categories", json!({})),
        ("list_rules", json!({})),
        // Refused calls: an unknown function, an argument it doesn't take,
        // an unknown account.
        ("delete_everything", json!({})),
        ("list_accounts", json!({"sql": "DROP TABLE accounts"})),
        (
            "account_balance",
            json!({"account_id": GROCERIES.to_string()}),
        ),
    ] {
        transcript.request(
            &mut session,
            "tools/call",
            json!({"name": name, "arguments": arguments}),
        );
    }
    transcript.request(&mut session, "ping", json!({}));
    compare_with_golden("session-2025-06-18.json", &Value::Array(transcript.0));
    assert!(session.finish().status.success());
}

#[test]
fn the_answers_to_newer_and_older_clients_match_their_golden_file() {
    let folder = Folder::new("golden-versions");
    folder.switch_ai(true);
    let book = folder.book();
    let mut answers = Vec::new();
    for version in ["2024-11-05", "2025-11-25", "2099-01-01"] {
        let mut session = Session::start(folder.path(), &book);
        answers.push(json!({"protocolVersion": version, "answer": session.initialize(version)}));
    }
    // The newest revision has no `initialize`: every request says which
    // revision it speaks, and a client asks the server what it is.
    let mut session = Session::start(folder.path(), &book);
    let meta = json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {}
    });
    for (method, params) in [
        ("server/discover", json!({"_meta": meta})),
        (
            "tools/call",
            json!({"_meta": meta, "name": "list_rules", "arguments": {}}),
        ),
    ] {
        answers.push(json!({
            "method": method,
            "params": params,
            "answer": session.request(method, params.clone())
        }));
    }
    compare_with_golden("versions.json", &Value::Array(answers));
}

/// A test where the steps meet: what the AI tool gets is exactly what the
/// read function returns, cleaned text and data note included (D-171).
#[test]
fn a_tool_answers_exactly_what_the_read_function_returns() {
    let folder = Folder::new("same-as-read-functions");
    folder.switch_ai(true);
    let path = folder.book();
    let mut session = Session::start(folder.path(), &path);
    session.initialize("2025-06-18");
    let book = Book::open_to_read(&path).expect("read");
    let today = Date::constant(2026, 10, 25);
    for (name, arguments) in [
        ("list_accounts", json!({})),
        ("search_transactions", json!({"text": "instructions"})),
        (
            "category_totals",
            json!({"from": "2026-10-01", "to": "2026-10-31"}),
        ),
    ] {
        let expected = kassensturz_ai_tools::call(&book, name, &arguments, today)
            .expect("answered")
            .value;
        let (answer, is_error) = session.call(name, arguments);
        assert!(!is_error);
        assert_eq!(answer, expected, "{name}");
    }
    let (answer, _) = session.call("search_transactions", json!({"text": "corner"}));
    let payee = answer["rows"][0]["payee"].as_str().expect("a payee");
    assert!(HOSTILE_PAYEE.starts_with(payee) || payee.starts_with("Corner Shop"));
    assert!(answer["note"].as_str().is_some_and(|note| !note.is_empty()));
}

/// The server reads the book the app has open, sees the app's changes at
/// its next answer, and never makes the app's write fail (Q-036, D-195).
#[test]
fn the_server_reads_beside_the_open_app_and_sees_its_changes() {
    let folder = Folder::new("beside-the-app");
    folder.switch_ai(true);
    let path = folder.book();
    let mut app = Book::open(&path, &folder.computer()).expect("the app opens the book");
    let mut session = Session::start(folder.path(), &path);
    session.initialize("2025-06-18");

    let (before, _) = session.call("list_categories", json!({}));
    let rent = kassensturz_core::AccountId::from_bytes([9; 16]);
    app.add_account(&common::account(rent, "Rent", AccountKind::Expense))
        .expect("the app writes while the server is running");
    let (after, _) = session.call("list_categories", json!({}));
    assert_ne!(before, after);
    assert!(after.to_string().contains("Rent"), "{after}");

    // The app still holds its lock: the server took none.
    drop(app);
    Book::open(&path, &folder.computer()).expect("opened again without a lock warning");
    assert!(session.finish().status.success());
}

/// The AI switch is a kill switch (D-013): switched off while the server
/// runs, the next request gets no answer and the program ends.
#[test]
fn the_server_stops_when_ai_is_switched_off_while_it_runs() {
    let folder = Folder::new("switched-off");
    folder.switch_ai(true);
    let book = folder.book();
    let mut session = Session::start(folder.path(), &book);
    session.initialize("2025-06-18");
    session.call("list_accounts", json!({}));

    folder.switch_ai(false);
    session.send(&json!({
        "jsonrpc": "2.0", "id": 99, "method": "tools/call",
        "params": {"name": "list_accounts", "arguments": {}}
    }));
    assert_eq!(session.receive(), None, "no answer once AI is off");
    let output = session.finish();
    assert_eq!(output.status.code(), Some(common::REFUSED));
    assert!(
        common::stderr(&output).contains("switched off"),
        "{}",
        common::stderr(&output)
    );
}
