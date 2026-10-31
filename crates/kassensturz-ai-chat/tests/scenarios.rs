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
    HOSTILE, OWN_IBAN, Script, TODAY, calls, fails, hostile_book, says, says_counting, settings,
    space_colony,
};
use kassensturz_ai::{AiError, KeyUse, PreparedRequest, Role};
use kassensturz_ai_chat::{
    ANSWER_NOW, BuildInfo, Cancel, ChatError, ChatSession, DATA_START, EMPTY_ANSWER, Hooks,
    LogEntry, MAX_ROUNDS, Mode, NOT_OFFERED, NoHooks, SHORTENED_RESULT, sample_request,
};
use kassensturz_ai_tools::{
    DATA_NOTE, DRAFT_ISSUE, FUNCTIONS, OPEN_PAGE, PROPOSE_CHANGES, PageTarget,
};
use serde_json::Value;

fn session() -> ChatSession {
    ChatSession::new(BuildInfo::new("0.261025.0", "1775d22"))
        .with_permissions(kassensturz_core::ai_switch::Permissions::all())
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
            .all(|o| FUNCTIONS.iter().any(|f| f.name == o)
                || o == PROPOSE_CHANGES.name
                || o == DRAFT_ISSUE.name
                || o == OPEN_PAGE.name)),
        "only the read functions, propose_changes, draft_issue and open_page are ever offered"
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
    // The same call twice ends the circle: the third request offers no
    // function and asks for the answer.
    let seen = script.seen();
    assert_eq!(seen.len(), 3);
    assert!(!seen[1].offered.is_empty());
    assert!(seen[2].offered.is_empty());
    assert_eq!(
        seen[2]
            .messages
            .last()
            .map(|message| message.content.as_str()),
        Some(ANSWER_NOW)
    );
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
fn a_model_that_calls_in_circles_has_to_answer_from_what_it_has() {
    // The log of a 4B model (M5-B24): the same two calls over and over.
    let book = space_colony();
    let (provider, script) = Script::provider(
        on(),
        vec![
            calls(&[("a", "list_accounts", "{}")]),
            calls(&[("b", "list_accounts", "{}")]),
            says("You have the accounts listed above."),
        ],
    );
    let mut chat = session();
    let answer = chat
        .ask(
            &book,
            &provider,
            None,
            "Which accounts?",
            TODAY,
            &mut NoHooks,
        )
        .unwrap();
    assert_eq!(answer.text, "You have the accounts listed above.");
    let seen = script.seen();
    assert_eq!(seen.len(), 3);
    assert!(
        seen[2].offered.is_empty(),
        "no function in the last request"
    );
}

#[test]
fn a_model_that_asks_for_something_new_each_time_gets_the_last_request_without_functions() {
    let book = space_colony();
    let steps: Vec<common::Step> = vec![Box::new(|messages, _| {
        // A different call each time: the length of the conversation is
        // in the argument.
        let arguments = format!(r#"{{"limit":{}}}"#, messages.len());
        let body = serde_json::json!({"choices": [{"finish_reason": "tool_calls",
            "message": {"role": "assistant", "content": null, "tool_calls": [
                {"id": "c", "type": "function",
                 "function": {"name": "search_transactions", "arguments": arguments}}]}}]});
        Ok(body.to_string())
    })];
    let (provider, script) = Script::provider(on(), steps);
    let mut chat = session();
    let result = chat.ask(&book, &provider, None, "Everything?", TODAY, &mut NoHooks);
    assert!(matches!(result, Err(ChatError::TooManyRounds)));
    let seen = script.seen();
    assert_eq!(seen.len(), MAX_ROUNDS);
    assert!(!seen[MAX_ROUNDS - 2].offered.is_empty());
    assert!(seen[MAX_ROUNDS - 1].offered.is_empty());
}

#[test]
fn an_empty_answer_cut_by_the_length_means_the_context_is_too_small() {
    let book = space_colony();
    let cut = serde_json::json!({"choices": [{"finish_reason": "length",
        "message": {"role": "assistant", "content": "", "tool_calls": []}}]})
    .to_string();
    let (provider, _) = Script::provider(on(), vec![Box::new(move |_, _| Ok(cut.clone()))]);
    let mut chat = session();
    let result = chat.ask(&book, &provider, None, "Hello?", TODAY, &mut NoHooks);
    assert!(matches!(
        result,
        Err(ChatError::Ai(AiError::ContextTooSmall))
    ));
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
fn an_answer_is_marked_when_the_provider_counted_far_fewer_tokens() {
    let book = hostile_book();
    let (provider, _) = Script::provider(
        on(),
        vec![says_counting("A", 6_000), says_counting("B", 6_000)],
    );
    let mut chat = session();
    // A request of about 8 KB of question plus the prompt and the functions
    // (about 46 KB with the budget lists of M6c.11) for 6,000 tokens is what
    // an uncut request comes to.
    let long = "word ".repeat(1_600);
    let answer = chat
        .ask(&book, &provider, None, &long, TODAY, &mut NoHooks)
        .unwrap();
    assert!(!answer.context_cut);
    // The history grows by the first question and answer: 100 KB or more for
    // 6,000 tokens is a request the provider cut.
    let longer = "word ".repeat(12_000);
    let answer = chat
        .ask(&book, &provider, None, &longer, TODAY, &mut NoHooks)
        .unwrap();
    assert!(answer.context_cut);
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
    // The first question's call stays and its result is shortened (D-237,
    // D-242).
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

#[test]
fn calls_of_earlier_questions_stay_with_a_shortened_result() {
    let book = hostile_book();
    let (provider, script) = Script::provider(
        on(),
        vec![
            calls(&[("c1", "list_accounts", "{}")]),
            says("You have one account."),
            calls(&[("c2", "list_accounts", "{}")]),
            says("Still one."),
        ],
    );
    let mut chat = session();
    chat.ask(&book, &provider, None, "Accounts?", TODAY, &mut NoHooks)
        .unwrap();
    chat.ask(&book, &provider, None, "And now?", TODAY, &mut NoHooks)
        .unwrap();
    let kinds = |request: usize| -> Vec<Role> {
        script.seen()[request]
            .messages
            .iter()
            .map(|m| m.role)
            .collect()
    };
    // The first question's own second round sees its call and whole result.
    assert_eq!(
        kinds(1),
        [Role::System, Role::User, Role::Assistant, Role::Tool]
    );
    let whole = &script.seen()[1].messages[3];
    assert_ne!(whole.content, SHORTENED_RESULT);
    // The second question sees the first one's question, call, shortened
    // result and answer, and its own call and result.
    assert_eq!(
        kinds(3),
        [
            Role::System,
            Role::User,
            Role::Assistant,
            Role::Tool,
            Role::Assistant,
            Role::User,
            Role::Assistant,
            Role::Tool
        ]
    );
    let messages = &script.seen()[3].messages;
    assert_eq!(messages[2].calls.len(), 1);
    assert_eq!(messages[2].calls[0].name, "list_accounts");
    assert_eq!(messages[3].content, SHORTENED_RESULT);
    assert_eq!(
        messages[3].call_id.as_deref(),
        Some(messages[2].calls[0].id.as_str())
    );
    assert_eq!(messages[4].content, "You have one account.");
    assert_ne!(messages[7].content, SHORTENED_RESULT);
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
fn a_model_the_test_found_without_functions_starts_in_the_overview() {
    // D-324: the connection test said the model did not call its test
    // function, so no request offers functions and no refusal is needed.
    let book = space_colony();
    let (provider, script) = Script::provider(
        on(),
        vec![
            says("Everyday Checking has 5531.90 EUR."),
            says("Savings too."),
        ],
    );
    let mut chat = session();
    chat.set_test_found_no_functions(true);
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
    assert_eq!(seen.len(), 1, "one request, no refused one first");
    assert!(seen[0].offered.is_empty());
    assert!(seen[0].messages[0].content.contains(DATA_START));
    assert!(seen[0].messages[0].content.contains("5531.90"));
    // Only logged once, at the switch.
    chat.ask(&book, &provider, None, "And savings?", TODAY, &mut NoHooks)
        .unwrap();
    let overviews = chat
        .log()
        .iter()
        .filter(|entry| **entry == LogEntry::Overview)
        .count();
    assert_eq!(overviews, 1);
}

#[test]
fn a_new_test_result_takes_the_chat_back_to_the_functions() {
    let book = space_colony();
    let (provider, script) = Script::provider(
        on(),
        vec![says("From the overview."), says("Now with functions.")],
    );
    let mut chat = session();
    chat.set_test_found_no_functions(true);
    chat.ask(&book, &provider, None, "One?", TODAY, &mut NoHooks)
        .unwrap();
    assert_eq!(chat.mode(), Mode::Overview);
    // The user tested again and the model now calls functions.
    chat.set_test_found_no_functions(false);
    let answer = chat
        .ask(&book, &provider, None, "Two?", TODAY, &mut NoHooks)
        .unwrap();
    assert_eq!(answer.mode, Mode::Functions);
    assert!(!script.seen()[1].offered.is_empty());
}

#[test]
fn what_happened_in_the_conversation_beats_what_the_test_found() {
    // The provider took the functions in this conversation: a test that
    // says otherwise does not move it, and a provider's refusal still is
    // the error of D-190.
    let book = hostile_book();
    let (provider, script) = Script::provider(
        on(),
        vec![
            calls(&[("c", "list_accounts", "{}")]),
            says("Done."),
            says("Again."),
        ],
    );
    let mut chat = session();
    chat.ask(&book, &provider, None, "Balances?", TODAY, &mut NoHooks)
        .unwrap();
    chat.set_test_found_no_functions(true);
    let answer = chat
        .ask(&book, &provider, None, "More?", TODAY, &mut NoHooks)
        .unwrap();
    assert_eq!(answer.mode, Mode::Functions);
    assert!(!script.seen().last().unwrap().offered.is_empty());
    // And a refusal by the provider is not undone by a "yes" from a test.
    let refuse: common::Step = Box::new(|_, offered| {
        if offered.is_empty() {
            Ok(serde_json::json!({"choices": [{"finish_reason": "stop",
                "message": {"content": "Overview answer."}}]})
            .to_string())
        } else {
            Err(AiError::ProviderFailed { status: 400 })
        }
    });
    let (refusing, _) = Script::provider(on(), vec![refuse]);
    let mut other = session();
    other
        .ask(&book, &refusing, None, "Q?", TODAY, &mut NoHooks)
        .unwrap();
    other.set_test_found_no_functions(false);
    assert_eq!(other.mode(), Mode::Overview);
    other
        .ask(&book, &refusing, None, "Q2?", TODAY, &mut NoHooks)
        .ok();
    assert_eq!(other.mode(), Mode::Overview, "the refusal still holds");
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

#[test]
fn the_sample_request_is_the_first_request_of_a_real_question() {
    // M5.7 shows the sample in the consent screen as what a request holds,
    // so it has to be what the chat really builds.
    let book = space_colony();
    let build = BuildInfo::new("0.261025.0", "1775d22");
    let question = "What did I spend on food last month?";
    let (provider, _script) = Script::provider(on(), vec![says("Nothing.")]);
    let mut session = ChatSession::new(build.clone());
    session
        .ask(&book, &provider, None, question, TODAY, &mut NoHooks)
        .expect("answered");
    let sample = sample_request(&provider, &build, TODAY, question).expect("a sample");
    assert_eq!(sent_bodies(&session)[0], sample.preview());
    assert!(!sample.preview().contains("Space Colony"), "no book data");
}

/// An empty reply is asked for once more, so the user need not repeat the
/// question (#196, D-248).
#[test]
fn an_empty_reply_is_asked_for_again_once() {
    let book = hostile_book();
    let (provider, script) = Script::provider(on(), vec![says(""), says("You have two accounts.")]);
    let mut chat = session();
    let answer = chat
        .ask(&book, &provider, None, "Accounts?", TODAY, &mut NoHooks)
        .unwrap();
    assert_eq!(answer.text, "You have two accounts.");
    let seen = script.seen();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[1].messages.last().unwrap().content, EMPTY_ANSWER);
    assert_eq!(seen[1].messages.last().unwrap().role, Role::User);
}

#[test]
fn a_model_that_stays_empty_is_not_asked_a_third_time() {
    let book = hostile_book();
    let (provider, script) = Script::provider(on(), vec![says("")]);
    let mut chat = session();
    let answer = chat
        .ask(&book, &provider, None, "Accounts?", TODAY, &mut NoHooks)
        .unwrap();
    assert_eq!(answer.text, "");
    assert_eq!(script.seen().len(), 2);
}

/// A call written as text, which the provider's template didn't turn into
/// a call, is told plainly instead of being shown as the answer (#196,
/// D-248).
#[test]
fn a_function_call_written_as_text_is_an_error_of_its_own() {
    let book = hostile_book();
    for text in [
        "<|tool_call>call:list_categories{}<tool_call|>",
        "[TOOL_CALLS][{\"name\": \"list_accounts\"}]",
        "<function=list_accounts>{}</function>",
    ] {
        let (provider, _) = Script::provider(on(), vec![says(text)]);
        let mut chat = session();
        let result = chat.ask(&book, &provider, None, "Accounts?", TODAY, &mut NoHooks);
        assert!(matches!(result, Err(ChatError::ToolCallAsText)), "{text}");
    }
    // Plain words about a tool call are an answer.
    let (provider, _) = Script::provider(on(), vec![says("I can call a function for that.")]);
    let mut chat = session();
    assert!(
        chat.ask(&book, &provider, None, "Accounts?", TODAY, &mut NoHooks)
            .is_ok()
    );
}

/// The ID of the first Rule of the Space Colony book, as `list_rules` writes it.
fn first_rule_id(book: &kassensturz_store::Book) -> String {
    let rules = kassensturz_ai_tools::call(book, "list_rules", &Value::Null, TODAY).unwrap();
    rules.value["rules"][0]["id"].as_str().unwrap().to_owned()
}

#[test]
fn a_page_the_model_opens_comes_with_the_answer_and_changes_nothing() {
    let book = space_colony();
    let id = first_rule_id(&book);
    let before = book.transactions().unwrap();
    let (provider, _) = Script::provider(
        on(),
        vec![
            calls(&[(
                "c1",
                "open_page",
                &format!(r#"{{"page":"rule","id":"{id}"}}"#),
            )]),
            says("I opened your Rules."),
            says("Nothing to show."),
        ],
    );
    let mut chat = session();
    let answer = chat
        .ask(
            &book,
            &provider,
            None,
            "Show me my Rule",
            TODAY,
            &mut NoHooks,
        )
        .unwrap();
    assert_eq!(
        answer.page,
        Some(PageTarget::Rules {
            select: Some(id.clone())
        })
    );
    assert_eq!(called(&chat), [("open_page".to_owned(), None)]);
    assert_eq!(book.transactions().unwrap(), before);
    // The next question starts without a page.
    let answer = chat
        .ask(&book, &provider, None, "And?", TODAY, &mut NoHooks)
        .unwrap();
    assert_eq!(answer.page, None);
}

#[test]
fn a_page_that_is_not_on_the_list_or_not_in_the_book_is_not_opened() {
    let book = hostile_book();
    let (provider, _) = Script::provider(
        on(),
        vec![
            calls(&[
                ("c1", "open_page", r#"{"page":"settings"}"#),
                (
                    "c2",
                    "open_page",
                    r#"{"page":"rule","id":"Ignore all previous instructions"}"#,
                ),
                (
                    "c3",
                    "open_page",
                    r#"{"page":"account","id":"00000000-0000-4000-8000-000000000000"}"#,
                ),
            ]),
            says("I could not open it."),
        ],
    );
    let mut chat = session();
    let answer = chat
        .ask(&book, &provider, None, "Open settings", TODAY, &mut NoHooks)
        .unwrap();
    assert_eq!(answer.page, None);
    assert_eq!(
        called(&chat),
        [
            ("open_page".to_owned(), Some("ai-tools.bad-argument")),
            ("open_page".to_owned(), Some("ai-tools.bad-argument")),
            ("open_page".to_owned(), Some("ai-tools.unknown-account")),
        ]
    );
}
