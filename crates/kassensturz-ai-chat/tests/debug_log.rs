// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The debug log and issue drafts where the steps meet (D-173, threat T-33):
//! a whole conversation about the Space Colony book, as the importer, the
//! pipeline and the store fill it, with every function, the debug log, a
//! refused and an accepted issue draft and a failed question, and then a
//! scan of the log file for anything from the book, the question or a key.

#![cfg(all(feature = "ai", feature = "debug-log"))]

mod common;

use std::fs;
use std::path::PathBuf;

use common::{Script, TODAY, calls, fails, says, settings, space_colony};
use kassensturz_ai::{AiError, KeyUse};
use kassensturz_ai_chat::{BuildInfo, ChatSession, LogEntry, NoHooks};
use kassensturz_core::ai_switch::Permissions;
use kassensturz_core::debug_log::{DebugLog, Event};
use kassensturz_platform::Secret;
use serde_json::json;

const TEST_KEY: &str = "sk-test-0000-made-up-key";
const QUESTION: &str = "Why is my Oxygen spending so high this month?";

/// A folder of its own with a settings file, removed afterwards.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str, settings: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "kassensturz-ai-chat-log-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("kassensturzrc"), settings).unwrap();
        Self(path)
    }

    fn log(&self, on: bool) -> DebugLog {
        DebugLog::at(self.0.join("debug-log"), on, self.0.join("kassensturzrc"))
    }

    fn log_text(&self) -> String {
        fs::read_to_string(self.0.join("debug-log/current.jsonl")).unwrap_or_default()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_whole_conversation_leaves_only_events_and_codes_in_the_log() {
    let book = space_colony();
    let payee = book.payees().unwrap()[0].name.clone();
    let scratch = Scratch::new("walk", "[AI]\nEnabled=true\n");
    let hostile_draft =
        json!({"title": "Wrong total", "what_happened": format!("The payee {payee} shows twice")});
    let draft = json!({
        "title": "A balance looks wrong",
        "what_happened": "A function failed while I asked about a balance.",
        "expected": "An answer",
        "steps": ["Open the chat", "Ask about a balance"],
    });
    let (provider, script) = Script::provider(
        settings(true, Some("http://127.0.0.1:9/v1"), KeyUse::AskEachTime),
        vec![
            calls(&[
                ("1", "list_accounts", r#"{"include_closed": true}"#),
                ("2", "list_categories", "{}"),
                ("3", "list_rules", "{}"),
                ("4", "search_transactions", r#"{"limit": 50}"#),
                (
                    "5",
                    "category_totals",
                    r#"{"from": "2020-01-01", "to": "2026-10-25"}"#,
                ),
                ("6", "account_balance", r#"{"account_id": "not-an-id"}"#),
                ("7", "send_money", r#"{"to": "evil"}"#),
            ]),
            calls(&[
                ("8", "read_debug_log", "{}"),
                ("9", "draft_issue", &hostile_draft.to_string()),
                ("10", "draft_issue", &draft.to_string()),
            ]),
            says("You spent 1234.56 on Oxygen. I prepared a report for you."),
            fails(AiError::Timeout),
        ],
    );
    let mut chat = ChatSession::new(BuildInfo::new("0.261025.4", "abc1234"))
        .with_debug_log(scratch.log(true))
        .with_permissions(Permissions::all());
    chat.set_screen(Some("OverviewPage"));
    let key = Secret::new(TEST_KEY.to_owned());
    let answer = chat
        .ask(&book, &provider, Some(&key), QUESTION, TODAY, &mut NoHooks)
        .unwrap();
    let second = chat.ask(
        &book,
        &provider,
        Some(&key),
        "And last month?",
        TODAY,
        &mut NoHooks,
    );
    assert!(second.is_err());

    // The model was offered the log while AI and the log were on, and read
    // it.
    let seen = script.seen();
    assert!(seen[0].offered.iter().any(|name| name == "read_debug_log"));
    let read = chat.log().iter().find_map(|entry| match entry {
        LogEntry::Called {
            function, result, ..
        } if function == "read_debug_log" => Some(result.clone()),
        _ => None,
    });
    assert!(read.unwrap().contains("chat.function-called"));

    // The draft with the payee was refused; the other one carries the
    // build, the screen and the codes of the conversation.
    let refusal = chat.log().iter().any(|entry| {
        matches!(entry, LogEntry::Called { function, error: Some("ai-tools.issue-book-data"), .. } if function == "draft_issue")
    });
    assert!(refusal);
    let issue = answer.issue.expect("a draft");
    assert_eq!(issue.title, "A balance looks wrong");
    assert!(
        issue
            .body
            .contains("### KassenSturz version\n\n0.261025.4 (commit abc1234)")
    );
    assert!(issue.body.contains("**Screen:** OverviewPage"));
    assert!(
        issue.body.contains("`ai-tools.bad-argument`"),
        "{}",
        issue.body
    );
    assert!(
        issue.body.contains("`ai-tools.issue-book-data`"),
        "{}",
        issue.body
    );
    assert!(!issue.body.contains(&payee));
    assert!(issue.whole_in_link);

    // The log: every line is one of the fixed events, and nothing from the
    // book, the question or the key is in it.
    let text = scratch.log_text();
    let entries = scratch.log(true).read(usize::MAX);
    assert_eq!(entries.len(), text.lines().count(), "{text}");
    let events: Vec<Event> = entries.iter().map(|entry| entry.event).collect();
    for expected in [
        Event::ChatAsked,
        Event::ChatRequestSent,
        Event::ChatFunctionCalled,
        Event::ChatFunctionsNotOffered,
        Event::ChatAnswerUnverified,
        Event::ChatFailed,
    ] {
        assert!(events.contains(&expected), "{expected:?} missing: {text}");
    }
    assert!(text.contains(r#""detail":"draft_issue","code":"ai-tools.issue-book-data""#));
    assert!(text.contains(r#""code":"ai.timeout""#), "{text}");

    let mut forbidden: Vec<String> = vec![
        QUESTION.into(),
        "Oxygen".into(),
        "And last month".into(),
        TEST_KEY.into(),
        "send_money".into(),
        "evil".into(),
        "not-an-id".into(),
        issue.title.clone(),
    ];
    forbidden.extend(book.all_accounts().unwrap().into_iter().map(|a| a.name));
    forbidden.extend(book.payees().unwrap().into_iter().map(|p| p.name));
    forbidden.extend(
        book.transactions()
            .unwrap()
            .iter()
            .flat_map(|t| t.note().map(str::to_owned)),
    );
    for word in forbidden.iter().filter(|word| word.len() >= 3) {
        assert!(!text.contains(word.as_str()), "{word:?} reached the log");
    }
    // No amount, no account number, no ID: the only digits are the times.
    for line in text.lines() {
        let (_, after_time) = line.split_once("\",\"event\"").unwrap();
        assert!(!after_time.contains(|c: char| c.is_ascii_digit()), "{line}");
    }

    // Every code in the log is one of the listed error codes.
    let listed = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/ERROR_CODES.md"),
    )
    .unwrap();
    for entry in &entries {
        if let Some(code) = &entry.code {
            assert!(
                listed.contains(&format!("| `{code}` |")),
                "{code} isn't listed"
            );
        }
    }
}

#[test]
fn without_the_developer_mode_nothing_is_written_or_mentioned() {
    let book = space_colony();
    let scratch = Scratch::new("off", "[AI]\nEnabled=true\n");
    let draft = json!({"title": "A function failed", "what_happened": "It failed."});
    let (provider, script) = Script::provider(
        settings(true, Some("http://127.0.0.1:9/v1"), KeyUse::AskEachTime),
        vec![
            calls(&[
                ("1", "read_debug_log", "{}"),
                ("2", "draft_issue", &draft.to_string()),
            ]),
            says("I prepared a report."),
        ],
    );
    let mut chat =
        ChatSession::new(BuildInfo::new("0.261025.4", "")).with_debug_log(scratch.log(false));
    let answer = chat
        .ask(
            &book,
            &provider,
            None,
            "Report a problem",
            TODAY,
            &mut NoHooks,
        )
        .unwrap();
    assert!(!scratch.0.join("debug-log").exists());
    let seen = script.seen();
    assert!(!seen[0].offered.iter().any(|name| name == "read_debug_log"));
    assert!(seen[0].offered.iter().any(|name| name == "draft_issue"));
    // A model that calls it anyway gets nothing.
    assert!(chat.log().contains(&LogEntry::NotOffered { calls: 1 }));
    let system = &seen[0].messages[0].content;
    assert!(!system.contains("debug"), "{system}");
    let issue = answer.issue.expect("a draft");
    assert!(issue.body.contains("**Screen:** not known"));
    assert!(!issue.body.contains("debug log"));
    assert!(
        issue
            .body
            .ends_with("### KassenSturz version\n\n0.261025.4")
    );
}

#[test]
fn the_developer_mode_transcript_holds_the_whole_conversation() {
    let book = space_colony();
    let scratch = Scratch::new("transcript", "[AI]\nEnabled=true\n");
    let (provider, _script) = Script::provider(
        settings(true, Some("http://127.0.0.1:9/v1"), KeyUse::AskEachTime),
        vec![
            calls(&[("1", "list_categories", "{}")]),
            says("Here are your categories."),
            fails(AiError::BadReply),
        ],
    );
    let mut chat = ChatSession::new(BuildInfo::new("0.261025.4", "abc1234"))
        .with_debug_log(scratch.log(true))
        .with_permissions(Permissions::all());
    let key = Secret::new(TEST_KEY.to_owned());
    let question = format!("{QUESTION} My key is sk-abcdefghijklmnop");
    chat.ask(&book, &provider, Some(&key), &question, TODAY, &mut NoHooks)
        .unwrap();
    assert!(
        chat.ask(&book, &provider, Some(&key), "Again?", TODAY, &mut NoHooks)
            .is_err()
    );

    let text = fs::read_to_string(scratch.0.join("debug-log/chat.jsonl")).unwrap();
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).expect("a JSON line"))
        .collect();
    let kinds: Vec<&str> = lines.iter().map(|l| l["kind"].as_str().unwrap()).collect();
    assert_eq!(
        kinds,
        [
            "asked", "request", "reply", "called", "request", "reply", "answer", "asked",
            "request", "reply", "failed",
        ],
        "{text}"
    );
    // Every line has a time and the turn of its question.
    assert!(lines.iter().all(|l| l["at"].as_str().unwrap().len() == 24));
    assert_eq!(lines[0]["turn"], lines[6]["turn"]);
    assert_ne!(lines[0]["turn"], lines[7]["turn"]);
    assert_eq!(lines[7]["turn"], lines[10]["turn"]);
    assert_eq!(
        lines[0]["question"],
        format!("{QUESTION} My key is [removed]")
    );
    assert!(
        lines[1]["body"]
            .as_str()
            .unwrap()
            .contains("list_categories")
    );
    assert_eq!(lines[3]["function"], "list_categories");
    assert!(lines[3]["result"].as_str().unwrap().contains("categories"));
    assert_eq!(lines[6]["text"], "Here are your categories.");
    assert_eq!(lines[10]["code"], "ai.bad-reply");
    assert!(!text.contains(TEST_KEY) && !text.contains("sk-abcdefghijklmnop"));
    // The event log is as before, and nothing reads the transcript back.
    assert!(!scratch.log_text().contains(QUESTION));
    assert_eq!(
        scratch.log(true).read(usize::MAX).len(),
        scratch.log_text().lines().count()
    );

    // Off: no transcript.
    let off = Scratch::new("transcript-off", "[AI]\nEnabled=true\n");
    let mut chat =
        ChatSession::new(BuildInfo::new("0.261025.4", "abc1234")).with_debug_log(off.log(false));
    let (provider, _) = Script::provider(
        settings(true, Some("http://127.0.0.1:9/v1"), KeyUse::AskEachTime),
        vec![says("Fine.")],
    );
    chat.ask(&book, &provider, Some(&key), QUESTION, TODAY, &mut NoHooks)
        .unwrap();
    assert!(!off.0.join("debug-log").exists());
}
