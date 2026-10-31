// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The fake model, for trying the chat without any AI (D-193).
//!
//! It is KassenSturz's own code playing a model: no server, no network, no
//! key, and nothing leaves the app ([`ScriptedProvider`]). It picks a read
//! function by a few words of the question, then answers with the figures
//! the function gave, so the whole chat can be tried with the real book,
//! functions, number check and tool-call log. A question with "invent" in
//! it gets a made-up number added, to show the number check. A question
//! with "sort" in it makes it prepare a change set: a new category for the
//! newest few transactions it can move, for the review and undo (D-199). A
//! question with "open" in it about a Rule or an account lists them and then
//! opens the first one's page (D-265). A question with "chart" in it makes
//! it propose a chart, by choices alone (M6a.9, D-282), and each of a fixed
//! list of plain descriptions gets its own chart ([`DESCRIBED_CHARTS`],
//! M6c.10b). The app
//! offers it only when started with [`DEMO_ENV`] set, as the fake bank is
//! (D-159).

use kassensturz_ai::{
    AiError, ConfigFile, Message, PROBE_FUNCTION, PROBE_WORD, Role, ScriptedProvider,
};
use kassensturz_ai_tools::{OPEN_PAGE, PROPOSE_CHANGES, PROPOSE_VIEW_NAME};
use serde_json::{Value, json};

use crate::prompt::{DATA_END, DATA_START};

mod charts;

use crate::session::{EMPTY_ANSWER, NOT_OFFERED};
pub use charts::{DESCRIBED_CHARTS, DescribedChart, DescribedIds, described};

/// The environment variable that offers the fake model: `1` for a model
/// that calls functions, `overview` for one that can't.
pub const DEMO_ENV: &str = "KASSENSTURZ_FAKE_AI";

/// What every answer of the fake model starts with, so it is never taken
/// for a real AI.
pub const DEMO_LABEL: &str = "Fake model for testing, not a real AI.";

/// The number the fake model makes up when asked to "invent" one.
pub const INVENTED: &str = "4711.08";

/// The category the fake model prepares when asked to sort.
pub const DEMO_CATEGORY: &str = "Sorted by the fake model";

/// The title of the chart the fake model proposes when asked for one.
pub const DEMO_CHART: &str = "Chart from the fake model";

/// What it says once a described chart is prepared.
pub const CHART_PREPARED: &str =
    "I prepared the chart. It waits for you: keep it, change it or leave it out.";

/// The most transactions it puts into that category at once.
const DEMO_MOVES: usize = 3;

/// Which fake model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DemoModel {
    /// One that calls the read functions.
    Functions,
    /// One that can't call functions, as some local models: its provider
    /// refuses requests that offer them (HTTP 400), so the chat falls back
    /// to the overview.
    NoFunctions,
}

impl DemoModel {
    /// The model [`DEMO_ENV`]'s `value` asks for, if any.
    pub fn from_env_value(value: &str) -> Option<Self> {
        match value.trim() {
            "1" => Some(Self::Functions),
            "overview" => Some(Self::NoFunctions),
            _ => None,
        }
    }
}

/// The fake model as a provider, while `config` switches AI on.
pub fn demo_provider(config: ConfigFile, model: DemoModel) -> ScriptedProvider {
    ScriptedProvider::new(config, move |messages, offered| {
        respond(model, messages, offered)
    })
}

fn respond(model: DemoModel, messages: &[Message], offered: &[String]) -> Result<String, AiError> {
    if model == DemoModel::NoFunctions && !offered.is_empty() {
        return Err(AiError::ProviderFailed { status: 400 });
    }
    // The function test of "Test connection" (M6b.9): the model that can
    // call functions passes the word it is asked to pass.
    if offered == [PROBE_FUNCTION] {
        return Ok(calls_reply(PROBE_FUNCTION, &json!({ "word": PROBE_WORD })));
    }
    let question = messages
        .iter()
        .rev()
        .find(|message| {
            message.role == Role::User
                && message.content != NOT_OFFERED
                && message.content != EMPTY_ANSWER
        })
        .map(|message| message.content.to_lowercase())
        .unwrap_or_default();
    let last = messages.last().map(|message| message.role);
    if let Some(chart) = described(&question) {
        if let Some((name, arguments)) = charts::next_call(chart, messages, offered) {
            return Ok(calls_reply(name, &arguments));
        }
        let prepared = messages
            .last()
            .filter(|message| message.role == Role::Tool)
            .and_then(|result| serde_json::from_str::<Value>(&result.content).ok())
            .is_some_and(|result| result.get("prepared").is_some());
        if prepared {
            return Ok(text_reply(&format!("{DEMO_LABEL} {CHART_PREPARED}")));
        }
    }
    let sorting = wants_sorting(&question);
    if sorting
        && last == Some(Role::Tool)
        && offered.iter().any(|offer| offer == PROPOSE_CHANGES.name)
        && let Some(arguments) = messages
            .last()
            .and_then(|result| serde_json::from_str::<Value>(&result.content).ok())
            .and_then(|result| sorting_proposal(&result))
    {
        return Ok(calls_reply(PROPOSE_CHANGES.name, &arguments));
    }
    if wants_chart(&question)
        && last != Some(Role::Tool)
        && offered.iter().any(|offer| offer == PROPOSE_VIEW_NAME)
    {
        return Ok(calls_reply(PROPOSE_VIEW_NAME, &chart_proposal(&question)));
    }
    if wants_opening(&question)
        && last == Some(Role::Tool)
        && offered.iter().any(|offer| offer == OPEN_PAGE.name)
        && let Some(arguments) = messages
            .last()
            .and_then(|result| serde_json::from_str::<Value>(&result.content).ok())
            .and_then(|result| opening_call(&result))
    {
        return Ok(calls_reply(OPEN_PAGE.name, &arguments));
    }
    let text = if last == Some(Role::Tool) {
        let results: Vec<&str> = messages
            .iter()
            .rev()
            .take_while(|message| message.role == Role::Tool)
            .map(|message| message.content.as_str())
            .collect();
        describe_all(results.into_iter().rev())
    } else if offered.is_empty() {
        let overview = messages
            .first()
            .and_then(|system| system.content.split_once(DATA_START))
            .and_then(|(_, rest)| rest.split_once(DATA_END))
            .and_then(|(block, _)| block.lines().last());
        match overview {
            Some(json) => format!("From the overview: {}", describe_all([json])),
            None => "I got no data from KassenSturz.".to_owned(),
        }
    } else {
        let today = messages
            .first()
            .and_then(|system| system.content.split_once("Today is "))
            .and_then(|(_, rest)| rest.get(..10))
            .unwrap_or("2026-01-01");
        let (name, arguments) = choose(&question, today);
        if offered.iter().any(|offer| offer == name) {
            return Ok(calls_reply(name, &arguments));
        }
        "The function I wanted is not offered.".to_owned()
    };
    let mut text = format!("{DEMO_LABEL} {text}");
    if question.contains("invent") || question.contains("erfind") {
        text.push_str(&format!(
            " I also made up {INVENTED} to show the number check."
        ));
    }
    Ok(text_reply(&text))
}

/// Whether `question` (lowercase) asks to sort transactions.
fn wants_sorting(question: &str) -> bool {
    ["sort", "einsortier"]
        .iter()
        .any(|word| question.contains(word))
}

/// Whether `question` (lowercase) asks for a chart.
fn wants_chart(question: &str) -> bool {
    ["chart", "diagramm"]
        .iter()
        .any(|word| question.contains(word))
}

/// A chart by choices alone: the months of a year when the question says
/// month, else what the money went to.
fn chart_proposal(question: &str) -> Value {
    let monthly = ["month", "monat"]
        .iter()
        .any(|word| question.contains(word));
    json!({
        "title": DEMO_CHART,
        "data": if monthly { "income_expense_by_month" } else { "spending_by_category" },
        "show_as": "auto",
        "period": "last-12-months",
    })
}

/// Whether `question` (lowercase) asks to open a page.
fn wants_opening(question: &str) -> bool {
    ["open", "öffne"].iter().any(|word| question.contains(word))
}

/// The call of `open_page` for the first Rule, or else the first account,
/// of the list `found`. `None` when it holds neither.
fn opening_call(found: &Value) -> Option<Value> {
    ["rules", "accounts"].iter().find_map(|list| {
        let id = found[list][0]["id"].as_str()?;
        Some(json!({"page": if *list == "rules" { "rule" } else { "account" }, "id": id}))
    })
}

/// A change set for the search result `found`: a new category for the
/// newest few entries that have one category or none, in the currency of
/// the first. `None` when there is none, or when `found` isn't a search.
fn sorting_proposal(found: &Value) -> Option<Value> {
    let rows = found["rows"].as_array()?;
    let movable: Vec<&Value> = rows
        .iter()
        .filter(|row| row.get("category").is_some() || row.get("unsorted").is_some())
        .take(DEMO_MOVES)
        .collect();
    let currency = movable.first()?["currency"].clone();
    let categorize: Vec<Value> = movable
        .iter()
        .filter(|row| row["currency"] == currency)
        .map(|row| json!({"transaction_id": row["transaction_id"], "new_category": "sorted"}))
        .collect();
    Some(json!({
        "new_categories": [{"key": "sorted", "name": DEMO_CATEGORY, "currency": currency}],
        "categorize": categorize,
    }))
}

/// The function for `question` (lowercase) and its arguments.
fn choose(question: &str, today: &str) -> (&'static str, Value) {
    let has = |words: &[&str]| words.iter().any(|word| question.contains(word));
    if question.contains("list_rules") {
        // The Rules button's question.
        ("list_rules", json!({}))
    } else if wants_sorting(question) {
        // The button's question asks for entries without a category.
        let open = question.contains("unsorted_only");
        (
            "search_transactions",
            json!({"limit": 10, "unsorted_only": open}),
        )
    } else if has(&["spen", "categor", "month", "ausgab", "kategor", "monat"]) {
        let first = format!("{}01", today.get(..8).unwrap_or("2026-01-"));
        ("category_totals", json!({"from": first, "to": today}))
    } else if has(&[
        "find",
        "search",
        "last",
        "recent",
        "transaction",
        "such",
        "letzte",
        "buchung",
    ]) {
        ("search_transactions", json!({"limit": 5}))
    } else if has(&["rule", "regel"]) {
        ("list_rules", json!({}))
    } else {
        ("list_accounts", json!({}))
    }
}

fn describe_all<'a>(results: impl IntoIterator<Item = &'a str>) -> String {
    results
        .into_iter()
        .map(|result| match serde_json::from_str::<Value>(result) {
            Ok(value) => describe(&value),
            Err(_) => "I couldn't read a result.".to_owned(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A few of the figures and names in one result, written exactly as the
/// function gave them.
fn describe(result: &Value) -> String {
    let text = |value: &Value, key: &str| value[key].as_str().unwrap_or("?").to_owned();
    let list = |key: &str| result[key].as_array().cloned().unwrap_or_default();
    let mut parts = Vec::new();
    if let Some(code) = result["error"].as_str() {
        parts.push(format!("The function answered with the error {code}."));
    }
    if let Some(page) = result["opened"].as_str() {
        parts.push(format!("I opened the {page} page for you."));
    }
    if let Some(prepared) = result["prepared"].as_u64() {
        parts.push(format!(
            "I prepared {prepared} changes. They wait for your review; nothing is changed \
             until you confirm them."
        ));
    }
    let accounts: Vec<String> = list("accounts")
        .iter()
        .take(8)
        .map(|a| {
            format!(
                "{}: {} {}",
                text(a, "name"),
                text(a, "balance"),
                text(a, "currency")
            )
        })
        .collect();
    if !accounts.is_empty() {
        parts.push(format!("Accounts: {}.", accounts.join("; ")));
    }
    for totals in list("currencies") {
        let currency = text(&totals, "currency");
        parts.push(format!(
            "Money in {} {currency}, money out {} {currency}, left {} {currency}.",
            text(&totals, "money_in"),
            text(&totals, "money_out"),
            text(&totals, "left"),
        ));
    }
    let rows: Vec<String> = list("rows")
        .iter()
        .take(5)
        .map(|row| {
            format!(
                "{} {} {} {}",
                text(row, "date"),
                text(row, "payee"),
                text(row, "amount"),
                text(row, "currency")
            )
        })
        .collect();
    if !rows.is_empty() {
        parts.push(format!("Newest: {}.", rows.join("; ")));
    }
    let rules: Vec<String> = list("rules")
        .iter()
        .take(8)
        .map(|rule| text(rule, "name"))
        .collect();
    if !rules.is_empty() {
        parts.push(format!("Rules: {}.", rules.join("; ")));
    }
    if parts.is_empty() {
        "The result holds nothing I can describe.".to_owned()
    } else {
        parts.join(" ")
    }
}

fn text_reply(text: &str) -> String {
    json!({
        "choices": [{
            "index": 0,
            "finish_reason": "stop",
            "message": {"role": "assistant", "content": text},
        }],
    })
    .to_string()
}

fn calls_reply(name: &str, arguments: &Value) -> String {
    json!({
        "choices": [{
            "index": 0,
            "finish_reason": "tool_calls",
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "fake-call-1",
                    "type": "function",
                    "function": {"name": name, "arguments": arguments.to_string()},
                }],
            },
        }],
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_environment_value_picks_the_model() {
        assert_eq!(DemoModel::from_env_value("1"), Some(DemoModel::Functions));
        assert_eq!(
            DemoModel::from_env_value(" overview\n"),
            Some(DemoModel::NoFunctions)
        );
        assert_eq!(DemoModel::from_env_value(""), None);
        assert_eq!(DemoModel::from_env_value("yes"), None);
    }

    #[test]
    fn the_function_test_of_test_connection_gets_each_models_answer() {
        use kassensturz_ai::{FunctionCalling, probe_functions};
        let folder =
            std::env::temp_dir().join(format!("kassensturz-demo-probe-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("kassensturzrc");
        std::fs::write(&path, "[AI]\nEnabled=true\n").unwrap();
        let answer = |model| probe_functions(&demo_provider(ConfigFile::at(&path), model), None);
        assert_eq!(answer(DemoModel::Functions), Ok(FunctionCalling::Yes));
        assert_eq!(answer(DemoModel::NoFunctions), Ok(FunctionCalling::No));
    }

    #[test]
    fn words_pick_the_function() {
        assert_eq!(
            choose("how much did i spend?", "2026-10-25"),
            (
                "category_totals",
                json!({"from": "2026-10-01", "to": "2026-10-25"})
            )
        );
        assert_eq!(
            choose("zeig die letzten buchungen", "2026-10-25").0,
            "search_transactions"
        );
        assert_eq!(
            choose("which rules do i have", "2026-10-25").0,
            "list_rules"
        );
        assert_eq!(
            choose("what is on my accounts?", "2026-10-25").0,
            "list_accounts"
        );
        assert_eq!(
            choose("sort my newest purchases", "2026-10-25").0,
            "search_transactions"
        );
    }

    #[test]
    fn opening_takes_the_first_rule_or_account_of_a_list() {
        let rules = json!({"rules": [{"id": "r1"}, {"id": "r2"}]});
        assert_eq!(
            opening_call(&rules),
            Some(json!({"page": "rule", "id": "r1"}))
        );
        let accounts = json!({"accounts": [{"id": "a1"}]});
        assert_eq!(
            opening_call(&accounts),
            Some(json!({"page": "account", "id": "a1"}))
        );
        assert_eq!(opening_call(&json!({"rules": []})), None);
        assert_eq!(opening_call(&json!({"opened": "rules"})), None);
        assert!(wants_opening("please open my rules"));
        assert!(!wants_opening("which rules do i have"));
    }

    #[test]
    fn sorting_moves_only_entries_with_one_category_or_none() {
        let found = json!({"rows": [
            {"transaction_id": "a", "currency": "EUR", "category": "Food"},
            {"transaction_id": "b", "currency": "EUR", "transfer": "Savings"},
            {"transaction_id": "c", "currency": "EUR", "categories": ["Food", "Fun"]},
            {"transaction_id": "d", "currency": "EUR", "unsorted": true},
        ]});
        let proposal = sorting_proposal(&found).expect("a proposal");
        let moved: Vec<&str> = proposal["categorize"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["transaction_id"].as_str().unwrap())
            .collect();
        assert_eq!(moved, ["a", "d"]);
        assert_eq!(proposal["new_categories"][0]["name"], DEMO_CATEGORY);
        assert_eq!(sorting_proposal(&json!({"accounts": []})), None);
    }
}
