// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The malformed-call suite through the chat (M6c.10a, D-356, D-397,
//! threat T-46): every refused row of the shared table, sent as a model's
//! calls of `propose_view` on the synthetic Space Colony book. What the
//! model is told is the row's code with the argument's fixed name and, for
//! a list, the values it takes from the catalog and schema; never its own
//! text. No call prepares anything.

#![cfg(feature = "ai")]

mod common;
#[path = "../../kassensturz-ai-tools/tests/malformed/mod.rs"]
mod malformed;

use common::{Script, TODAY, calls, says, settings, space_colony};
use kassensturz_ai::{KeyUse, Role};
use kassensturz_ai_chat::{BuildInfo, ChatSession, NoHooks};
use kassensturz_ai_tools::{PROPOSE_VIEW_NAME, propose_view_text, view_choices};
use kassensturz_core::ai_switch::Permissions;
use malformed::{Case, Ids, MARKER, PREPARED, cases, misspellings};
use serde_json::Value;

/// Fewer than the most calls one answer of a model may hold (`MAX_CALLS`
/// of `kassensturz-ai`).
const CALLS_PER_ANSWER: usize = 30;

/// The row's answer as the model gets it: the code and, for a bad
/// argument, its name.
fn expected(case: &Case) -> (String, Option<String>) {
    // The provider reads blank arguments as no arguments, `{}`, as models
    // send them for a function without any (`kassensturz-ai`, openai.rs),
    // so the title is the first thing missing.
    if case.text.trim().is_empty() {
        return ("ai-tools.bad-argument".into(), Some("title".into()));
    }
    if let Some(argument) = case.expected.strip_prefix("bad-formula ") {
        return ("ai-tools.bad-formula".into(), Some(argument.into()));
    }
    match case.expected.strip_prefix("bad-argument ") {
        Some(argument) => ("ai-tools.bad-argument".into(), Some(argument.into())),
        None => (case.expected.clone(), None),
    }
}

#[test]
fn every_refused_call_is_answered_with_fixed_words_only_and_prepares_nothing() {
    let book = space_colony();
    let mut rows = cases(&Ids::of(&book));
    rows.extend(misspellings());
    rows.retain(|case| case.expected != PREPARED);
    let mut failures = Vec::new();
    for chunk in rows.chunks(CALLS_PER_ANSWER) {
        let ids: Vec<String> = (0..chunk.len()).map(|n| format!("c{n}")).collect();
        let script_calls: Vec<(&str, &str, &str)> = chunk
            .iter()
            .zip(&ids)
            .map(|(case, id)| (id.as_str(), PROPOSE_VIEW_NAME, case.text.as_str()))
            .collect();
        let (provider, script) = Script::provider(
            settings(true, None, KeyUse::None),
            vec![calls(&script_calls), says("I could not prepare the chart.")],
        );
        let mut chat = ChatSession::new(BuildInfo::new("0.261031.0", "0000000"))
            .with_permissions(Permissions::all());
        let answer = chat
            .ask(&book, &provider, None, "Draw a chart", TODAY, &mut NoHooks)
            .expect("an answer");
        assert!(answer.changes.is_empty(), "nothing is prepared");
        let seen = script.seen();
        let results: Vec<_> = seen[1]
            .messages
            .iter()
            .filter(|message| message.role == Role::Tool)
            .collect();
        assert_eq!(results.len(), chunk.len());
        for (case, result) in chunk.iter().zip(results) {
            let name = &case.name;
            if result.content.contains(MARKER) {
                failures.push(format!("{name}: the model's text came back"));
            }
            let Ok(value) = serde_json::from_str::<Value>(&result.content) else {
                failures.push(format!("{name}: not JSON"));
                continue;
            };
            let (code, argument) = expected(case);
            let got = (
                value["error"].as_str().unwrap_or_default().to_owned(),
                value["argument"].as_str().map(str::to_owned),
            );
            if got != (code.clone(), argument.clone()) {
                failures.push(format!("{name}: {got:?}, not {:?}", (code, argument)));
                continue;
            }
            // A list argument is answered with its values, and nothing
            // else is (D-353, D-393): the values that suit the call's data
            // where the tool knows them, otherwise all of the argument's
            // (M6-B204). An argument the chosen way of drawing doesn't take
            // gets the ways that do instead of a list.
            let text = if case.text.trim().is_empty() {
                "{}"
            } else {
                case.text.as_str()
            };
            let error = propose_view_text(
                &book,
                text,
                Permissions::all(),
                kassensturz_core::Date::constant(2026, 10, 31),
            )
            .expect_err("refused");
            let general = argument.as_deref().and_then(view_choices);
            let allowed = match (error.only_for_show_as(), error.allowed_fields()) {
                (Some(_), _) => None,
                (None, Some(words)) => Some(words.to_vec()),
                (None, None) => general.clone(),
            };
            if value.get("allowed") != allowed.map(|words| serde_json::json!(words)).as_ref() {
                failures.push(format!("{name}: allowed {:?}", value.get("allowed")));
            }
            // Every word is one of the argument's own.
            if let (Some(words), Some(general)) = (error.allowed_fields(), &general)
                && let Some(word) = words.iter().find(|word| !general.contains(word))
            {
                failures.push(format!("{name}: {word} isn't a word of the argument"));
            }
            for (key, words) in [
                (
                    "only_for_show_as",
                    error.only_for_show_as().map(<[_]>::to_vec),
                ),
                ("use_instead", error.use_instead().map(|word| vec![word])),
            ] {
                let expected = words.map(|words| {
                    if key == "use_instead" {
                        serde_json::json!(words[0])
                    } else {
                        serde_json::json!(words)
                    }
                });
                if value.get(key) != expected.as_ref() {
                    failures.push(format!("{name}: {key} {:?}", value.get(key)));
                }
            }
            let keys: Vec<&str> = value
                .as_object()
                .expect("an object")
                .keys()
                .map(String::as_str)
                .collect();
            if let Some(key) = keys.iter().find(|key| {
                ![
                    "error",
                    "argument",
                    "allowed",
                    "only_for_show_as",
                    "use_instead",
                    "max_bytes",
                    "hint",
                    // A refused formula's place and fixed reason (D-650).
                    "position",
                    "reason",
                ]
                .contains(key)
            }) {
                failures.push(format!("{name}: unexpected key {key}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
