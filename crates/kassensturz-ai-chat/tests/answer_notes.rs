// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What an answer claims against what happened in the turn (US6, #848,
//! #849, #858, #862), where steps meet: the real functions on the Space
//! Colony book, a scripted model, and the chat's own checks.

#![cfg(feature = "ai")]

mod common;

use common::{Script, TODAY, calls, says, settings, space_colony};
use kassensturz_ai::KeyUse;
use kassensturz_ai_chat::{Answer, AnswerNote, BuildInfo, ChatSession, NoHooks};

fn ask(steps: Vec<common::Step>, question: &str) -> Answer {
    let (provider, _script) = Script::provider(settings(true, None, KeyUse::None), steps);
    let mut chat = ChatSession::new(BuildInfo::new("0.261025.0", "1775d22"))
        .with_permissions(kassensturz_core::ai_switch::Permissions::all());
    chat.ask(
        &space_colony(),
        &provider,
        None,
        question,
        TODAY,
        &mut NoHooks,
    )
    .unwrap()
}

#[test]
fn a_claimed_change_that_was_never_prepared_gets_a_note() {
    let answer = ask(
        vec![
            calls(&[("c1", "list_accounts", "{}")]),
            says("I have prepared a new Rule for you to confirm."),
        ],
        "Make a Rule for coffee.",
    );
    assert_eq!(answer.notes, vec![AnswerNote::NothingPrepared]);
}

#[test]
fn an_answer_about_the_book_without_a_function_call_is_marked_not_checked() {
    let answer = ask(
        vec![says("You have 12 transactions in March.")],
        "How many transactions in March?",
    );
    assert_eq!(answer.notes, vec![AnswerNote::NotChecked]);
}

#[test]
fn an_answer_after_a_function_call_has_no_note_and_a_bill_claim_does() {
    let answer = ask(
        vec![
            calls(&[("c1", "list_accounts", "{}")]),
            says("Done. You can pay the bill directly in KassenSturz."),
        ],
        "Where is my money?",
    );
    assert_eq!(answer.notes, vec![AnswerNote::CannotPay]);
}

#[test]
fn an_internal_id_in_an_answer_is_replaced_by_the_name_the_function_gave() {
    let mut chat = ChatSession::new(BuildInfo::new("0.261025.0", "1775d22"))
        .with_permissions(kassensturz_core::ai_switch::Permissions::all());
    // Read the real ID and name from the function's own result.
    let book = space_colony();
    let entries = book.categories().unwrap();
    let entry = &entries[0];
    let id = entry.category.id.to_string();
    let name = entry.path.join(" › ");
    let (provider, _script) = Script::provider(
        settings(true, None, KeyUse::None),
        vec![
            calls(&[("c1", "list_categories", "{}")]),
            says(&format!("The category {id} is high.")),
        ],
    );
    let answer = chat
        .ask(&book, &provider, None, "Which?", TODAY, &mut NoHooks)
        .unwrap();
    assert!(!answer.text.contains(&id), "{}", answer.text);
    assert!(answer.text.contains(&name), "{}", answer.text);
}
