// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A question about one category (B-434): in BS2's VM round 3 a model asked
//! about one spending category called `monthly_totals` without the category
//! and gave the month's money out of everything as that category's figure.
//! The figure is a real one, so the number check passes it; a fixed note
//! under the answer says that it is the total of all categories. The real
//! session, the real functions, the Space Colony book and a scripted model.

#![cfg(feature = "ai")]

mod common;

use common::{Script, Step, TODAY, calls, settings, space_colony};
use kassensturz_ai::{KeyUse, Role};
use kassensturz_ai_chat::{AnswerNote, BuildInfo, ChatSession, NoHooks};
use kassensturz_ai_tools::call;
use kassensturz_core::ai_switch::Permissions;
use kassensturz_store::Book;
use serde_json::{Value, json};

const OCTOBER: &str = r#"{"from":"2026-10-01","to":"2026-10-31"}"#;

/// The largest spending category of October 2026 in `book`: the last
/// part of its name, its ID and its total. Each import of the book gives
/// new IDs, so the same book is asked.
fn largest_category(book: &Book) -> (String, String, String) {
    let result = call(
        book,
        "category_totals",
        &serde_json::from_str::<Value>(OCTOBER).unwrap(),
        TODAY,
    )
    .unwrap();
    let row = &result.value["currencies"][0]["spending"][0];
    let name = row["category"].as_str().expect("a category").to_owned();
    let last = name
        .rsplit(['›', ':', '>'])
        .next()
        .unwrap()
        .trim()
        .to_owned();
    (
        last,
        row["category_id"].as_str().expect("an ID").to_owned(),
        row["total"].as_str().expect("a total").to_owned(),
    )
}

/// A model that answers with the figures `pick` takes from the last
/// function result.
fn answers(pick: fn(&Value) -> String) -> Step {
    Box::new(move |messages, _| {
        let result = messages
            .iter()
            .rev()
            .find(|message| message.role == Role::Tool)
            .expect("a function result");
        let result: Value = serde_json::from_str(&result.content).expect("JSON");
        Ok(json!({"choices": [{"finish_reason": "stop",
            "message": {"role": "assistant", "content": pick(&result)}}]})
        .to_string())
    })
}

fn ask(book: &Book, question: &str, steps: Vec<Step>) -> kassensturz_ai_chat::Answer {
    let (provider, _script) = Script::provider(settings(true, None, KeyUse::None), steps);
    ChatSession::new(BuildInfo::new("0.261025.0", "1775d22"))
        .with_permissions(Permissions::all())
        .ask(book, &provider, None, question, TODAY, &mut NoHooks)
        .unwrap()
}

fn noted(answer: &kassensturz_ai_chat::Answer) -> bool {
    answer.notes.contains(&AnswerNote::AllCategoriesTotal)
}

fn month_spending(result: &Value) -> String {
    let spent = result["months"][0]["currencies"][0]["money_out"]
        .as_str()
        .expect("the month's money out");
    format!("You spent {spent} on it in October.")
}

#[test]
fn all_spending_given_as_one_categorys_is_noted() {
    let book = space_colony();
    let (category, _, _) = largest_category(&book);
    let answer = ask(
        &book,
        &format!("How much did I spend on {category} in October 2026?"),
        vec![
            calls(&[("c1", "monthly_totals", OCTOBER)]),
            answers(month_spending),
        ],
    );
    // A real figure: the number check passes it.
    assert!(answer.unverified.is_empty(), "{:?}", answer.unverified);
    assert!(noted(&answer), "{:?}", answer.notes);
    assert_eq!(AnswerNote::AllCategoriesTotal.key(), "all-categories-total");
}

#[test]
fn the_result_says_its_totals_are_of_all_categories() {
    let book = space_colony();
    let all = call(
        &book,
        "monthly_totals",
        &serde_json::from_str::<Value>(OCTOBER).unwrap(),
        TODAY,
    )
    .unwrap();
    assert_eq!(all.value["all_categories"], json!(true));
    let (_, id, _) = largest_category(&book);
    let one = call(
        &book,
        "monthly_totals",
        &json!({"from": "2026-10-01", "to": "2026-10-31", "category_id": id}),
        TODAY,
    )
    .unwrap_or_else(|error| panic!("{error:?} for {id}"));
    assert!(one.value.get("all_categories").is_none(), "{}", one.value);
}

#[test]
fn the_categorys_own_figure_is_not_noted() {
    let book = space_colony();
    let (category, id, total) = largest_category(&book);
    let arguments =
        json!({"from": "2026-10-01", "to": "2026-10-31", "category_id": id}).to_string();
    let answer = ask(
        &book,
        &format!("How much did I spend on {category} in October 2026?"),
        vec![
            calls(&[("c1", "monthly_totals", &arguments)]),
            answers(|result| {
                let total = result["months"][0]["currencies"][0]["spending"][0]["total"]
                    .as_str()
                    .expect("the category's row");
                format!("You spent {total} on it in October.")
            }),
        ],
    );
    assert!(answer.unverified.is_empty(), "{:?}", answer.unverified);
    assert!(!noted(&answer), "{:?} for {total}", answer.notes);
}

#[test]
fn all_spending_next_to_the_categorys_figure_is_not_noted() {
    let book = space_colony();
    let (category, _, _) = largest_category(&book);
    let answer = ask(
        &book,
        &format!("How much did I spend on {category} in October 2026?"),
        vec![
            calls(&[("c1", "category_totals", OCTOBER)]),
            answers(|result| {
                let currency = &result["currencies"][0];
                format!(
                    "Of {} in all, {} went on it.",
                    currency["money_out"].as_str().unwrap(),
                    currency["spending"][0]["total"].as_str().unwrap()
                )
            }),
        ],
    );
    assert!(answer.unverified.is_empty(), "{:?}", answer.unverified);
    assert!(!noted(&answer), "{:?}", answer.notes);
}

#[test]
fn all_spending_for_a_question_about_all_spending_is_not_noted() {
    let answer = ask(
        &space_colony(),
        "How much did I spend in October 2026?",
        vec![
            calls(&[("c1", "monthly_totals", OCTOBER)]),
            answers(month_spending),
        ],
    );
    assert!(!noted(&answer), "{:?}", answer.notes);
}
