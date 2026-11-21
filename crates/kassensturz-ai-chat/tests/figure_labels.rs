// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Labels for the figures in an answer (D-952, #1358): a figure that came
//! from a function says what it is in the book. The real session, the real
//! functions, the Space Colony book and a scripted model.

#![cfg(feature = "ai")]

mod common;

use common::{Script, Step, TODAY, calls, settings, space_colony};
use kassensturz_ai::{KeyUse, Role};
use kassensturz_ai_chat::{Answer, BuildInfo, ChatSession, NoHooks};
use kassensturz_ai_tools::LabelKind;
use kassensturz_core::ai_switch::Permissions;
use kassensturz_store::Book;
use serde_json::{Value, json};

/// A model that answers with `text(result)`, `result` being the last
/// function result.
fn answers(text: fn(&Value) -> String) -> Step {
    Box::new(move |messages, _| {
        let result = messages
            .iter()
            .rev()
            .find(|message| message.role == Role::Tool)
            .expect("a function result");
        let result: Value = serde_json::from_str(&result.content).expect("JSON");
        Ok(json!({"choices": [{"finish_reason": "stop",
            "message": {"role": "assistant", "content": text(&result)}}]})
        .to_string())
    })
}

fn ask(book: &Book, steps: Vec<Step>) -> Answer {
    let (provider, _script) = Script::provider(settings(true, None, KeyUse::None), steps);
    ChatSession::new(BuildInfo::new("0.261025.0", "1775d22"))
        .with_permissions(Permissions::all())
        .ask(book, &provider, None, "Question", TODAY, &mut NoHooks)
        .unwrap()
}

#[test]
fn an_account_balance_says_whose_it_is() {
    let book = space_colony();
    let answer = ask(
        &book,
        vec![
            calls(&[("c1", "list_accounts", "{}")]),
            answers(|result| {
                let first = result["accounts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|row| row["balance"] != "0.00")
                    .expect("an account with money");
                format!(
                    "{} holds {}.",
                    first["name"].as_str().unwrap(),
                    first["balance"].as_str().unwrap()
                )
            }),
        ],
    );
    assert!(answer.unverified.is_empty(), "{:?}", answer.unverified);
    let first = answer
        .labelled
        .first()
        .unwrap_or_else(|| panic!("no label for {:?}", answer.text));
    let account = answer.text.split(" holds ").next().unwrap();
    assert!(
        first
            .labels
            .iter()
            .any(|label| label.kind == LabelKind::AccountBalance && label.name == account),
        "{first:?}"
    );
}

#[test]
fn a_category_total_says_its_category_and_period() {
    let book = space_colony();
    let arguments = r#"{"from":"2026-10-01","to":"2026-10-31"}"#;
    let answer = ask(
        &book,
        vec![
            calls(&[("c1", "category_totals", arguments)]),
            answers(|result| {
                let row = &result["currencies"][0]["spending"][0];
                format!(
                    "{}: {}",
                    row["category"].as_str().unwrap(),
                    row["total"].as_str().unwrap()
                )
            }),
        ],
    );
    let total = answer
        .labelled
        .iter()
        .find(|number| {
            number
                .labels
                .iter()
                .any(|label| label.kind == LabelKind::CategoryTotal)
        })
        .unwrap_or_else(|| panic!("{:?} for {:?}", answer.labelled, answer.text));
    let label = total
        .labels
        .iter()
        .find(|label| label.kind == LabelKind::CategoryTotal)
        .unwrap();
    assert!(!label.name.is_empty());
    assert_eq!(
        (label.from.as_str(), label.to.as_str()),
        ("2026-10-01", "2026-10-31")
    );
}

#[test]
fn a_made_up_number_gets_no_label() {
    let book = space_colony();
    let answer = ask(
        &book,
        vec![
            calls(&[("c1", "list_accounts", "{}")]),
            answers(|_| "You have 4711.08 in total.".to_owned()),
        ],
    );
    assert_eq!(answer.unverified.len(), 1);
    assert!(answer.labelled.is_empty(), "{:?}", answer.labelled);
}
