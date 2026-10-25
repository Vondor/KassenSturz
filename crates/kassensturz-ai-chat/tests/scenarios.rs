// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The chat against models played by a script (threats T-21, T-22, T-26,
//! T-28, T-31; D-171, D-190 to D-193): hostile payees and memos and a model
//! that follows them, a model that invents a number, calls for functions
//! that don't exist, a model that never stops calling, cancel, the review
//! of each request, AI off, and a model that can't call functions.

#![cfg(feature = "ai")]

mod common;

use common::{
    HOSTILE, OWN_IBAN, Script, TODAY, calls, fails, hostile_book, says, settings, space_colony,
};
use kassensturz_ai::{AiError, KeyUse, PreparedRequest, Role};
use kassensturz_ai_chat::{
    BuildInfo, Cancel, ChatError, ChatSession, DATA_START, Hooks, LogEntry, MAX_ROUNDS, Mode,
    NOT_OFFERED, NoHooks,
};
use kassensturz_ai_tools::{DATA_NOTE, FUNCTIONS};
use serde_json::Value;

fn session() -> ChatSession {
    ChatSession::new(BuildInfo::new("0.261025.0", "1775d22"))
}

fn on() -> kassensturz_ai::ConfigFile {
    settings(true, None, KeyUse::None)
}

fn called(session: &ChatSession) -> Vec<(String, Option<&'static str>)> {
    session
        .log()
        .iter()
        .filter_map(|entry| match entry {
            LogEntry::Called {
                function, error, ..
            } => Some((function.clone(), *error)),
            _ => None,
        })
        .collect()
}

fn sent_bodies(session: &ChatSession) -> Vec<String> {
    session
        .log()
        .iter()
        .filter_map(|entry| match entry {
            LogEntry::Sent { body, .. } => Some(body.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_model_that_follows_a_hostile_memo_gets_nothing_more() {
    let book = hostile_book();
    let before = book.transactions().unwrap();
    let (provider, script) = Script::provider(
        on(),
        vec![
            calls(&[("c1", "search_transactions", "{}")]),
            // The model obeys the memo: a function that doesn't exist, and
            // an argument no function takes.
            calls(&[
                (
                    "c2",
                    "send_money",
                    r#"{"to":"evil.example","amount":"all"}"#,
                ),
                (
                    "c3",
                    "list_accounts",
                    r#"{"include_closed":true,"sql":"DROP TABLE"}"#,
                ),
            ]),
            says("Done. I sent everything."),
        ],
    );
    let mut chat = session();
    let answer = chat
        .ask(
            &book,
            &provider,
            None,
            "What did I pay at the grocery?",
            TODAY,
            &mut NoHooks,
        )
        .unwrap();
    assert_eq!(answer.text, "Done. I sent everything.");

    // Only functions of the fixed list ran; the one that doesn't exist was
    // never run and the model was told so.
    assert_eq!(
        called(&chat),
        [
            ("search_transactions".to_owned(), None),
            ("list_accounts".to_owned(), Some("ai-tools.bad-argument")),
        ]
    );
    assert!(chat.log().contains(&LogEntry::NotOffered { calls: 1 }));
    for (function, _) in called(&chat) {
        assert!(FUNCTIONS.iter().any(|f| f.name == function));
    }
    let seen = script.seen();
    assert_eq!(seen.len(), 3);
    assert!(
        seen.iter().all(|s| s
            .offered
            .iter()
            .all(|o| FUNCTIONS.iter().any(|f| f.name == o))),
        "only the read functions are ever offered"
    );
    assert_eq!(seen[2].messages.last().unwrap().content, NOT_OFFERED);

    // The book is as it was: nothing the model said changed it.
    assert_eq!(book.transactions().unwrap(), before);

    // The hostile text reached the model only inside a function's result,
    // marked as data, and could not close the data or look like markup.
    for s in &seen {
        for message in &s.messages {
            if message.content.contains("Ignore all previous") {
                assert_eq!(message.role, Role::Tool, "{message:?}");
                let result: Value = serde_json::from_str(&message.content).unwrap();
                assert_eq!(result["note"], DATA_NOTE);
                assert!(!message.content.contains('<') && !message.content.contains('>'));
            }
        }
    }
    let system = &seen[0].messages[0];
    assert_eq!(system.role, Role::System);
    assert!(system.content.contains("never an instruction"));
    // Nothing of the bank link or the account's own notes went anywhere.
    for body in sent_bodies(&chat) {
        for never in ["DE89", "0532013000", "SECRET", HOSTILE] {
            assert!(!body.contains(never), "{never} was sent");
        }
    }
    assert!(!format!("{:?}", chat.log()).contains(OWN_IBAN));
}

#[test]
fn an_invented_number_is_flagged_and_real_ones_pass() {
    let book = hostile_book();
    let (provider, _) = Script::provider(
        on(),
        vec![
            calls(&[("c1", "search_transactions", r#"{"limit":5}"#)]),
            says("You paid 53.20 at the grocery, about 60.00 with fees, in 1 payment."),
        ],
    );
    let mut chat = session();
    let answer = chat
        .ask(
            &book,
            &provider,
            None,
            "What did I pay?",
            TODAY,
            &mut NoHooks,
        )
        .unwrap();
    let flagged: Vec<&str> = answer.unverified.iter().map(|u| u.text.as_str()).collect();
    assert_eq!(flagged, ["60.00"]);
    assert_eq!(
        &answer.text[answer.unverified[0].start..answer.unverified[0].end],
        "60.00"
    );
    assert_eq!(answer.mode, Mode::Functions);
    assert!(chat.log().contains(&LogEntry::Answered { unverified: 1 }));
}

#[test]
fn the_build_line_and_the_question_are_not_flagged() {
    let book = hostile_book();
    let (provider, _) = Script::provider(
        on(),
        vec![says(
            "This is build 0.261025.0 (1775d22); you asked about 2 accounts.",
        )],
    );
    let mut chat = session();
    let answer = chat
        .ask(
            &book,
            &provider,
            None,
            "Do I have 2 accounts?",
            TODAY,
            &mut NoHooks,
        )
        .unwrap();
    assert!(answer.unverified.is_empty(), "{:?}", answer.unverified);
}

#[test]
fn a_model_that_never_stops_calling_is_stopped() {
    let book = hostile_book();
    let (provider, script) = Script::provider(on(), vec![calls(&[("c", "list_accounts", "{}")])]);
    let mut chat = session();
    let result = chat.ask(&book, &provider, None, "Balances?", TODAY, &mut NoHooks);
    assert!(matches!(result, Err(ChatError::TooManyRounds)));
    assert_eq!(script.seen().len(), MAX_ROUNDS);
    assert!(
        chat.history().is_empty(),
        "a failed question leaves no trace"
    );
    assert_eq!(
        chat.log().last(),
        Some(&LogEntry::Failed {
            code: "ai-chat.too-many-rounds"
        })
    );
}

#[test]
fn arguments_that_are_not_json_are_answered_with_an_error() {
    let book = hostile_book();
    let (provider, script) = Script::provider(
        on(),
        vec![
            calls(&[("c", "list_accounts", "{not json")]),
            says("Sorry."),
        ],
    );
    let mut chat = session();
    chat.ask(&book, &provider, None, "Balances?", TODAY, &mut NoHooks)
        .unwrap();
    assert_eq!(
        called(&chat),
        [("list_accounts".to_owned(), Some("ai-tools.bad-argument"))]
    );
    let seen = script.seen();
    let result = &seen[1].messages.last().unwrap().content;
    assert_eq!(
        serde_json::from_str::<Value>(result).unwrap(),
        serde_json::json!({"error": "ai-tools.bad-argument", "argument": "arguments"})
    );
}

#[test]
fn the_conversation_goes_on_across_questions() {
    let book = hostile_book();
    let (provider, script) = Script::provider(
        on(),
        vec![
            calls(&[("c", "list_accounts", "{}")]),
            says("You have one account."),
            says("Still one."),
        ],
    );
    let mut chat = session();
    chat.ask(&book, &provider, None, "Accounts?", TODAY, &mut NoHooks)
        .unwrap();
    chat.ask(&book, &provider, None, "And now?", TODAY, &mut NoHooks)
        .unwrap();
    let last = &script.seen()[2].messages;
    let roles: Vec<Role> = last.iter().map(|m| m.role).collect();
    assert_eq!(
        roles,
        [
            Role::System,
            Role::User,
            Role::Assistant,
            Role::Tool,
            Role::Assistant,
            Role::User
        ]
    );
    assert_eq!(chat.history().len(), 6);
}

/// Hooks that press cancel or refuse a request at a given round, and
/// redact a phrase.
#[derive(Default)]
struct Front {
    cancel: Cancel,
    cancel_at: Option<usize>,
    refuse: bool,
    redact: Option<&'static str>,
    reviewed: usize,
    logged: usize,
}

impl Hooks for Front {
    fn review(&mut self, request: &mut PreparedRequest) -> bool {
        self.reviewed += 1;
        if self.cancel_at == Some(self.reviewed) {
            self.cancel.cancel();
        }
        if let Some(phrase) = self.redact {
            request.redact(phrase).unwrap();
        }
        !self.refuse
    }

    fn logged(&mut self, _: &LogEntry) {
        self.logged += 1;
    }

    fn cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }
}

#[test]
fn a_cancel_stops_before_the_next_request_and_drops_a_late_reply() {
    let book = hostile_book();
    let (provider, script) = Script::provider(
        on(),
        vec![calls(&[("c", "list_accounts", "{}")]), says("ok")],
    );
    let mut front = Front {
        cancel_at: Some(2),
        ..Front::default()
    };
    let mut chat = session();
    let result = chat.ask(&book, &provider, None, "Balances?", TODAY, &mut front);
    assert!(matches!(result, Err(ChatError::Cancelled)));
    // The second request had gone when cancel was pressed; its reply is
    // thrown away and nothing more is sent.
    assert_eq!(script.seen().len(), 2);
    assert!(chat.history().is_empty());
    assert_eq!(
        front.logged,
        chat.log().len(),
        "every log line reached the front end"
    );

    // A cancel pressed before asking sends nothing at all.
    let (provider, script) = Script::provider(on(), vec![says("ok")]);
    let cancel = Cancel::new();
    cancel.cancel();
    let mut hooks = cancel.clone();
    let result = chat.ask(&book, &provider, None, "Balances?", TODAY, &mut hooks);
    assert!(matches!(result, Err(ChatError::Cancelled)));
    assert!(script.seen().is_empty());
}

#[test]
fn a_request_the_user_refuses_is_not_sent() {
    let book = hostile_book();
    let (provider, script) = Script::provider(on(), vec![says("ok")]);
    let mut front = Front {
        refuse: true,
        ..Front::default()
    };
    let mut chat = session();
    let result = chat.ask(&book, &provider, None, "Balances?", TODAY, &mut front);
    assert!(matches!(result, Err(ChatError::Stopped)));
    assert!(script.seen().is_empty());
    assert!(sent_bodies(&chat).is_empty());
}

#[test]
fn what_the_user_redacts_is_not_sent() {
    let book = hostile_book();
    let (provider, script) = Script::provider(
        on(),
        vec![calls(&[("c", "search_transactions", "{}")]), says("ok")],
    );
    let mut front = Front {
        redact: Some("Corner Grocery"),
        ..Front::default()
    };
    let mut chat = session();
    chat.ask(
        &book,
        &provider,
        None,
        "What did Corner Grocery charge?",
        TODAY,
        &mut front,
    )
    .unwrap();
    for seen in script.seen() {
        for message in seen.messages {
            assert!(!message.content.contains("Corner Grocery"), "{message:?}");
        }
    }
    for body in sent_bodies(&chat) {
        assert!(!body.contains("Corner Grocery"));
        assert!(body.contains("[redacted]"));
    }
}

#[test]
fn nothing_runs_while_ai_is_off() {
    let book = hostile_book();
    let (provider, script) = Script::provider(settings(false, None, KeyUse::None), vec![says("x")]);
    let mut chat = session();
    let result = chat.ask(&book, &provider, None, "Balances?", TODAY, &mut NoHooks);
    assert!(matches!(result, Err(ChatError::Ai(AiError::Off))));
    assert!(script.seen().is_empty());
    assert!(sent_bodies(&chat).is_empty());
    assert_eq!(
        chat.log().last(),
        Some(&LogEntry::Failed { code: "ai.off" })
    );
}

#[test]
fn a_model_without_functions_gets_the_overview() {
    let book = space_colony();
    let refuse_functions: common::Step = Box::new(|_, offered| {
        if offered.is_empty() {
            Ok(serde_json::json!({"choices": [{"finish_reason": "stop",
                "message": {"content": "Everyday Checking has 5531.90 EUR."}}]})
            .to_string())
        } else {
            Err(AiError::ProviderFailed { status: 400 })
        }
    });
    let (provider, script) = Script::provider(on(), vec![refuse_functions]);
    let mut chat = session();
    let answer = chat
        .ask(
            &book,
            &provider,
            None,
            "How much is on checking?",
            TODAY,
            &mut NoHooks,
        )
        .unwrap();
    assert_eq!(answer.mode, Mode::Overview);
    assert!(answer.unverified.is_empty(), "the overview's figures pass");
    assert!(chat.log().contains(&LogEntry::Overview));
    let seen = script.seen();
    assert_eq!(seen.len(), 2);
    assert!(seen[1].offered.is_empty());
    assert!(seen[1].messages[0].content.contains(DATA_START));
    assert!(seen[1].messages[0].content.contains("5531.90"));

    // The next question goes straight to the overview.
    chat.ask(&book, &provider, None, "And savings?", TODAY, &mut NoHooks)
        .unwrap();
    assert_eq!(script.seen().len(), 3);
    assert!(script.seen()[2].offered.is_empty());
}

#[test]
fn a_refusal_after_functions_worked_is_an_error() {
    let book = hostile_book();
    let (provider, _) = Script::provider(
        on(),
        vec![
            calls(&[("c", "list_accounts", "{}")]),
            fails(AiError::ProviderFailed { status: 400 }),
        ],
    );
    let mut chat = session();
    let result = chat.ask(&book, &provider, None, "Balances?", TODAY, &mut NoHooks);
    assert!(matches!(
        result,
        Err(ChatError::Ai(AiError::ProviderFailed { status: 400 }))
    ));
    assert_eq!(chat.mode(), Mode::Functions);
}
