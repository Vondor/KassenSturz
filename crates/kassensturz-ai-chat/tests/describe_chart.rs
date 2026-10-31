// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Describing a chart in the chat (M6c.9, D-355, D-402): the model may
//! correct a refused chart twice in one question, then the chat gives up and
//! says so; the numbers in a chart's title are checked like those of an
//! answer. Everything on the synthetic Space Colony book, through the real
//! `propose_view`.

#![cfg(feature = "ai")]

mod common;

use common::{Script, TODAY, calls, says, settings, space_colony};
use kassensturz_ai::{KeyUse, Role};
use kassensturz_ai_chat::{
    BuildInfo, CHART_NOT_PREPARED, ChatSession, MAX_VIEW_CORRECTIONS, NoHooks,
};
use kassensturz_ai_tools::PROPOSE_VIEW_NAME;

const GOOD: &str = r#"{"title":"Where my money went","data":"spending_by_category","show_as":"auto","period":"last-12-months"}"#;
const BAD: &str =
    r#"{"title":"Where my money went","data":"spending_by_category","show_as":"hologram"}"#;
const BAD_AGAIN: &str =
    r#"{"title":"Where my money went","data":"spending_by_category","show_as":"mosaic"}"#;
const BAD_THIRD: &str =
    r#"{"title":"Where my money went","data":"spending_by_category","show_as":"fresco"}"#;
const BAD_DATASET: &str = r#"{"title":"Anything","data":"my_secret_table","show_as":"auto"}"#;
const BAD_DATASET_AGAIN: &str = r#"{"title":"Anything","data":"my_other_table","show_as":"auto"}"#;
const BAD_DATASET_THIRD: &str = r#"{"title":"Anything","data":"my_third_table","show_as":"auto"}"#;

fn chat() -> ChatSession {
    ChatSession::new(BuildInfo::new("0.261031.0", "0000000"))
        .with_permissions(kassensturz_core::ai_switch::Permissions::all())
}

#[test]
fn two_corrections_are_allowed_and_the_third_refusal_ends_the_attempt() {
    assert_eq!(MAX_VIEW_CORRECTIONS, 2);
    let book = space_colony();
    let (provider, script) = Script::provider(
        settings(true, None, KeyUse::None),
        vec![
            calls(&[("a", PROPOSE_VIEW_NAME, BAD)]),
            calls(&[("b", PROPOSE_VIEW_NAME, BAD_AGAIN)]),
            calls(&[("c", PROPOSE_VIEW_NAME, BAD_THIRD)]),
            says("Sorry, I could not do it."),
        ],
    );
    let mut chat = chat();
    let answer = chat
        .ask(&book, &provider, None, "Draw a chart", TODAY, &mut NoHooks)
        .expect("an answer");
    assert!(answer.changes.is_empty());
    let failed = answer.chart_failed.expect("the chat gave up");
    assert_eq!(failed.code, "ai-tools.bad-argument");
    // What it understood: the dataset it named, from the catalog's own ids.
    assert_eq!(failed.dataset, Some("spending_by_category"));
    // The last request had no function and told the model to stop.
    let seen = script.seen();
    assert_eq!(seen.len(), 4);
    assert!(seen[3].offered.is_empty(), "{:?}", seen[3].offered);
    let last = seen[3].messages.last().expect("a message");
    assert_eq!(last.role, Role::User);
    assert_eq!(last.content, CHART_NOT_PREPARED);
}

#[test]
fn a_chart_that_passes_after_two_corrections_is_prepared() {
    let book = space_colony();
    let (provider, _) = Script::provider(
        settings(true, None, KeyUse::None),
        vec![
            calls(&[("a", PROPOSE_VIEW_NAME, BAD)]),
            calls(&[("b", PROPOSE_VIEW_NAME, BAD_AGAIN)]),
            calls(&[("c", PROPOSE_VIEW_NAME, GOOD)]),
            says("Here is the chart."),
        ],
    );
    let mut chat = chat();
    let answer = chat
        .ask(&book, &provider, None, "Draw a chart", TODAY, &mut NoHooks)
        .expect("an answer");
    assert!(answer.chart_failed.is_none());
    assert_eq!(answer.changes.len(), 1);
    assert_eq!(answer.title_numbers, vec![vec![]]);
}

#[test]
fn a_dataset_that_is_not_in_the_catalog_is_never_carried_on() {
    let book = space_colony();
    let (provider, _) = Script::provider(
        settings(true, None, KeyUse::None),
        vec![
            calls(&[
                ("a", PROPOSE_VIEW_NAME, BAD_DATASET),
                ("b", PROPOSE_VIEW_NAME, BAD_DATASET_AGAIN),
                ("c", PROPOSE_VIEW_NAME, BAD_DATASET_THIRD),
            ]),
            says("I could not."),
        ],
    );
    let mut chat = chat();
    let answer = chat
        .ask(&book, &provider, None, "Draw a chart", TODAY, &mut NoHooks)
        .expect("an answer");
    let failed = answer.chart_failed.expect("the chat gave up");
    assert_eq!(failed.dataset, None);
}

#[test]
fn the_next_question_starts_with_fresh_corrections() {
    let book = space_colony();
    let (provider, _) = Script::provider(
        settings(true, None, KeyUse::None),
        vec![
            calls(&[
                ("a", PROPOSE_VIEW_NAME, BAD),
                ("b", PROPOSE_VIEW_NAME, BAD_AGAIN),
                ("c", PROPOSE_VIEW_NAME, BAD_THIRD),
            ]),
            says("I could not."),
            calls(&[("d", PROPOSE_VIEW_NAME, BAD)]),
            calls(&[("e", PROPOSE_VIEW_NAME, GOOD)]),
            says("Done."),
        ],
    );
    let mut chat = chat();
    let first = chat
        .ask(&book, &provider, None, "Draw a chart", TODAY, &mut NoHooks)
        .expect("an answer");
    assert!(first.chart_failed.is_some());
    let second = chat
        .ask(&book, &provider, None, "Try again", TODAY, &mut NoHooks)
        .expect("an answer");
    assert!(second.chart_failed.is_none());
    assert_eq!(second.changes.len(), 1);
}

#[test]
fn a_number_in_a_title_that_no_function_produced_is_marked_and_a_year_is_not() {
    let book = space_colony();
    let invented = r#"{"title":"Savings of 12345.67 in 2026","data":"spending_by_category","show_as":"auto","period":"last-12-months"}"#;
    let (provider, _) = Script::provider(
        settings(true, None, KeyUse::None),
        vec![
            calls(&[
                ("a", PROPOSE_VIEW_NAME, invented),
                ("b", PROPOSE_VIEW_NAME, GOOD),
            ]),
            says("Two charts."),
        ],
    );
    let mut chat = chat();
    let answer = chat
        .ask(&book, &provider, None, "Draw a chart", TODAY, &mut NoHooks)
        .expect("an answer");
    assert_eq!(answer.changes.len(), 2);
    assert_eq!(answer.title_numbers.len(), 2);
    let marked: Vec<&str> = answer.title_numbers[0]
        .iter()
        .map(|number| number.text.as_str())
        .collect();
    assert_eq!(marked, ["12345.67"]);
    assert!(answer.title_numbers[1].is_empty());
}

#[test]
fn a_number_the_user_wrote_is_not_marked() {
    let book = space_colony();
    let title = r#"{"title":"Spending, top 5","data":"spending_by_category","show_as":"auto","period":"last-12-months"}"#;
    let (provider, _) = Script::provider(
        settings(true, None, KeyUse::None),
        vec![calls(&[("a", PROPOSE_VIEW_NAME, title)]), says("Done.")],
    );
    let mut chat = chat();
    let answer = chat
        .ask(
            &book,
            &provider,
            None,
            "Draw a chart of my top 5 spending categories",
            TODAY,
            &mut NoHooks,
        )
        .expect("an answer");
    assert!(
        answer.title_numbers[0].is_empty(),
        "{:?}",
        answer.title_numbers
    );
}
