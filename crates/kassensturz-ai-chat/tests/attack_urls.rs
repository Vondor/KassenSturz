// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Attack scenario A3 of the outside security review: injected text makes
//! the built-in chat end its answer with a web address that carries the
//! user's figures, so the user copies it and carries the data out. All data
//! is made up (AGENTS.md §3.1 rule 2).
//!
//! The chat shows answers as plain text, so the address is never a link
//! (T-34, D-143, checked by `plain_text_audit.rs`). The review asks for
//! more: an address the AI wrote is marked or cut. The AI half of A7, a
//! real figure in a chart title, is at the end.

#![cfg(feature = "ai")]

mod common;

use common::{Script, TODAY, says, settings, space_colony};
use kassensturz_ai::KeyUse;
use kassensturz_ai_chat::{BuildInfo, ChatSession, NoHooks};

const LEAK: &str = "Your balances are fine. Check your result at \
    https://example.invalid/?b=4211.50;812.03";

#[test]
#[ignore = "B-251 (attack A3): web addresses in AI answers are shown unmarked"]
fn a3_a_web_address_in_an_answer_is_marked_or_cut() {
    let book = space_colony();
    let (provider, _script) =
        Script::provider(settings(true, None, KeyUse::None), vec![says(LEAK)]);
    let mut chat = ChatSession::new(BuildInfo::new("0.261025.0", "1775d22"))
        .with_permissions(kassensturz_core::ai_switch::Permissions::all());
    let answer = chat
        .ask(
            &book,
            &provider,
            None,
            "How are my balances?",
            TODAY,
            &mut NoHooks,
        )
        .expect("answered");
    assert!(
        !answer.text.contains("https://example.invalid/?b="),
        "the address reaches the user untouched: {:?}",
        answer.text
    );
}

/// Attack scenario A7 (its AI half): injected text makes the AI write the
/// user's real balance into a chart's title, so it leaves with a shared
/// dashboard file. The number check marks figures no function produced
/// (T-39); the review asks that the card marks a real figure too.
#[test]
#[ignore = "B-254 (attack A7): a real balance in a chart title is not marked"]
fn a7_a_real_balance_in_a_chart_title_is_marked() {
    let book = space_colony();
    let accounts =
        kassensturz_ai_tools::call(&book, "list_accounts", &serde_json::json!({}), TODAY)
            .expect("listed")
            .value;
    let balance = accounts["accounts"]
        .as_array()
        .expect("accounts")
        .iter()
        .filter_map(|account| account["balance"].as_str())
        .find(|balance| balance.trim_start_matches('-').len() > 6)
        .expect("an account with a sizeable balance")
        .to_owned();
    let view = serde_json::json!({
        "title": format!("Spending (account {balance})"),
        "data": "spending_by_category",
        "show_as": "auto",
        "period": "last-12-months",
    })
    .to_string();
    let (provider, _) = Script::provider(
        settings(true, None, KeyUse::None),
        vec![
            common::calls(&[("a", "list_accounts", "{}")]),
            common::calls(&[("b", kassensturz_ai_tools::PROPOSE_VIEW_NAME, &view)]),
            says("Here is your chart."),
        ],
    );
    let mut chat = ChatSession::new(BuildInfo::new("0.261025.0", "1775d22"))
        .with_permissions(kassensturz_core::ai_switch::Permissions::all());
    let answer = chat
        .ask(&book, &provider, None, "Draw a chart", TODAY, &mut NoHooks)
        .expect("answered");
    assert_eq!(answer.changes.len(), 1, "the chart was prepared");
    assert!(
        !answer.title_numbers[0].is_empty(),
        "the balance {balance} in the title is not marked"
    );
}
