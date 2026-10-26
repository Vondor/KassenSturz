// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A model that prepares changes in the chat (D-171, D-199 to D-201,
//! threat T-27): the change set comes back with the answer and nothing in
//! the book changes until the user's confirm applies it with the store's
//! real operations, as one change that undo takes back.

#![cfg(feature = "ai")]

mod common;

use common::{Script, Step, TODAY, calls, fails, says, settings, space_colony};
use kassensturz_ai::{AiError, KeyUse, Message, Role};
use kassensturz_ai_chat::{
    BuildInfo, ChatSession, DEMO_CATEGORY, DEMO_LABEL, DemoModel, LogEntry, NoHooks, Outcome,
    demo_provider,
};
use kassensturz_ai_tools::{Confirmed, PROPOSE_CHANGES, ProposedChange};
use kassensturz_store::Action;
use serde_json::{Value, json};

fn session() -> ChatSession {
    ChatSession::new(BuildInfo::new("0.261025.0", "1775d22"))
        .with_permissions(kassensturz_core::ai_switch::Permissions::all())
}

fn on() -> kassensturz_ai::ConfigFile {
    settings(true, None, KeyUse::None)
}

/// A model that reads the search result it got and prepares a new category
/// for the first entry that has one.
fn propose_for_first_found() -> Step {
    Box::new(|messages: &[Message], offered: &[String]| {
        assert!(offered.iter().any(|name| name == PROPOSE_CHANGES.name));
        let result = messages
            .iter()
            .rev()
            .find(|message| message.role == Role::Tool)
            .expect("the search result");
        let found: Value = serde_json::from_str(&result.content).expect("JSON");
        let row = found["rows"]
            .as_array()
            .expect("rows")
            .iter()
            .find(|row| row.get("category").is_some())
            .expect("an entry with a category");
        let arguments = json!({
            "new_categories": [{"key": "new", "name": "Prepared in the chat", "currency": row["currency"]}],
            "categorize": [{"transaction_id": row["transaction_id"], "new_category": "new"}],
        });
        let body = json!({"choices": [{"finish_reason": "tool_calls",
        "message": {"role": "assistant", "content": null, "tool_calls": [{
            "id": "c2", "type": "function",
            "function": {"name": PROPOSE_CHANGES.name, "arguments": arguments.to_string()},
        }]}}]});
        Ok(body.to_string())
    })
}

#[test]
fn prepared_changes_come_with_the_answer_and_wait_for_the_user() {
    let mut book = space_colony();
    let before = book.transactions().unwrap();
    let accounts = book.all_accounts().unwrap();
    let (provider, script) = Script::provider(
        on(),
        vec![
            calls(&[("c1", "search_transactions", r#"{"limit":20}"#)]),
            propose_for_first_found(),
            says("I prepared it. It waits for your review."),
        ],
    );
    let mut chat = session();
    let answer = chat
        .ask(
            &book,
            &provider,
            None,
            "Put my newest purchase into a category of its own",
            TODAY,
            &mut NoHooks,
        )
        .unwrap();
    assert_eq!(answer.changes.len(), 1);
    assert!(answer.unverified.is_empty(), "{:?}", answer.unverified);
    assert!(chat.log().iter().any(|entry| matches!(
        entry,
        LogEntry::Called { function, error: None, .. } if function == PROPOSE_CHANGES.name
    )));
    // The model was told that nothing changed yet.
    let told = &script.seen()[2].messages;
    let result: Value = serde_json::from_str(&told.last().unwrap().content).unwrap();
    assert_eq!(result["prepared"], 2);
    assert!(
        result["status"]
            .as_str()
            .unwrap()
            .contains("nothing is changed")
    );
    assert_eq!(book.transactions().unwrap(), before);
    assert_eq!(book.all_accounts().unwrap(), accounts);

    // The user confirms: one change, which undo takes back.
    let changes = answer.changes.into_iter().next().unwrap();
    let ProposedChange::Categorize(moved) = changes.changes()[1].clone() else {
        panic!("the transaction comes second");
    };
    changes.apply(&mut book, Confirmed::by_the_user()).unwrap();
    let after = book.transaction(moved.transaction).unwrap();
    assert!(
        after
            .splits()
            .iter()
            .any(|part| part.account == Some(moved.category))
    );
    assert_eq!(
        book.change_to_undo().unwrap().unwrap().action,
        Action::ApplyAiChanges
    );
    book.undo().unwrap();
    assert_eq!(book.transactions().unwrap(), before);
    assert_eq!(book.all_accounts().unwrap(), accounts);
}

#[test]
fn a_hostile_or_broken_proposal_prepares_nothing() {
    let book = space_colony();
    let (provider, _) = Script::provider(
        on(),
        vec![
            calls(&[(
                "c1",
                PROPOSE_CHANGES.name,
                r#"{"send_money":{"to":"evil.example"},"categorize":[]}"#,
            )]),
            says("Done."),
        ],
    );
    let mut chat = session();
    let answer = chat
        .ask(&book, &provider, None, "Sort things", TODAY, &mut NoHooks)
        .unwrap();
    assert!(answer.changes.is_empty());
    assert!(chat.log().iter().any(|entry| matches!(
        entry,
        LogEntry::Called {
            error: Some("ai-tools.bad-argument"),
            ..
        }
    )));
}

#[test]
fn changes_of_a_question_that_failed_are_dropped() {
    let book = space_colony();
    let category = book
        .categories()
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
        .category;
    let arguments = json!({"new_rules": [{
        "name": "Prepared before the failure",
        "conditions": [{"money_out": true}],
        "category_id": category.id.to_string(),
    }]})
    .to_string();
    let (provider, _) = Script::provider(
        on(),
        vec![
            calls(&[("c1", PROPOSE_CHANGES.name, &arguments)]),
            fails(AiError::Timeout),
            says("Nothing to prepare."),
        ],
    );
    let mut chat = session();
    assert!(
        chat.ask(&book, &provider, None, "Add a Rule", TODAY, &mut NoHooks)
            .is_err()
    );
    let answer = chat
        .ask(&book, &provider, None, "Hello", TODAY, &mut NoHooks)
        .unwrap();
    assert!(answer.changes.is_empty());
}

#[test]
fn the_fake_model_prepares_a_change_set_to_try_in_phase_2() {
    let mut book = space_colony();
    let before = book.transactions().unwrap();
    let provider = demo_provider(on(), DemoModel::Functions);
    let mut chat = session();
    let answer = chat
        .ask(
            &book,
            &provider,
            None,
            "Sort my newest purchases",
            TODAY,
            &mut NoHooks,
        )
        .unwrap();
    assert!(answer.text.starts_with(DEMO_LABEL), "{}", answer.text);
    assert!(
        answer.text.contains("wait for your review"),
        "{}",
        answer.text
    );
    assert!(answer.unverified.is_empty(), "{:?}", answer.unverified);
    let [changes] = <[_; 1]>::try_from(answer.changes).unwrap();
    let ProposedChange::NewCategory(category) = &changes.changes()[0] else {
        panic!("a new category first");
    };
    assert_eq!(category.path, [DEMO_CATEGORY]);
    assert!(changes.changes().len() > 1, "and the transactions it gets");

    changes.apply(&mut book, Confirmed::by_the_user()).unwrap();
    assert_ne!(book.transactions().unwrap(), before);
    book.undo().unwrap();
    assert_eq!(book.transactions().unwrap(), before);
}

#[test]
fn what_the_user_did_with_prepared_changes_reaches_the_model_with_the_next_question() {
    let book = space_colony();
    let (provider, script) = Script::provider(on(), vec![says("Done."), says("Ok.")]);
    let mut chat = session();
    chat.ask(&book, &provider, None, "Hello", TODAY, &mut NoHooks)
        .unwrap();
    chat.note_outcome(Outcome::Confirmed);
    chat.note_outcome(Outcome::Undone);
    chat.ask(&book, &provider, None, "Thanks", TODAY, &mut NoHooks)
        .unwrap();
    let seen = script.seen();
    let texts: Vec<(&Role, &str)> = seen[1]
        .messages
        .iter()
        .map(|message| (&message.role, message.content.as_str()))
        .collect();
    let told: Vec<&str> = texts
        .iter()
        .filter(|(role, _)| **role == Role::User)
        .map(|(_, text)| *text)
        .collect();
    assert_eq!(
        told,
        [
            "Hello",
            Outcome::Confirmed.text(),
            Outcome::Undone.text(),
            "Thanks"
        ]
    );
    // Told once: the question after that doesn't hear it again.
    chat.ask(&book, &provider, None, "Again", TODAY, &mut NoHooks)
        .unwrap();
    let seen = script.seen();
    let count = seen[2]
        .messages
        .iter()
        .filter(|message| message.content == Outcome::Confirmed.text())
        .count();
    assert_eq!(count, 1);
}

#[test]
fn the_outcomes_say_plainly_what_happened_and_invent_no_screen() {
    for outcome in [
        Outcome::Confirmed,
        Outcome::Declined,
        Outcome::Failed,
        Outcome::Undone,
    ] {
        assert!(outcome.text().starts_with("KassenSturz: "));
    }
    assert!(Outcome::Confirmed.text().contains("were made"));
    assert!(Outcome::Declined.text().contains("refused"));
    assert!(Outcome::Failed.text().contains("nothing was changed"));
}

#[test]
fn the_instructions_say_where_the_user_reviews_changes() {
    let book = space_colony();
    let (provider, script) = Script::provider(on(), vec![says("Hi.")]);
    session()
        .ask(&book, &provider, None, "Hello", TODAY, &mut NoHooks)
        .unwrap();
    let seen = script.seen();
    let instructions = &seen[0].messages[0].content;
    assert!(instructions.contains("chat sidebar"), "{instructions}");
    assert!(instructions.contains("Confirm and Undo"), "{instructions}");
    assert!(instructions.contains("don't invent one"), "{instructions}");
    // IDs come from the functions, Rules from what the entries show, and a
    // period that isn't over is not called much lower (D-232).
    assert!(
        instructions.contains("never invent or guess an ID"),
        "{instructions}"
    );
    assert!(instructions.contains("never invent them"), "{instructions}");
    assert!(instructions.contains("isn't over"), "{instructions}");
}
