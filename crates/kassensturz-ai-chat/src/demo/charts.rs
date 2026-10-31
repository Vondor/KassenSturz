// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The charts the fake model prepares from plain descriptions (M6c.10b,
//! D-356, D-411).
//!
//! A fixed list of descriptions, each answered with a fixed call of
//! `propose_view`, so *Describe a chart* can be tried in Phase 2 without
//! any AI and the tests go through the real function, schema, datasets and
//! filters. Together they name every dataset the AI may name, `auto`, a
//! range of days, a comparison, category and account ids (looked up first
//! with the read function, as a model must), and one call that is refused
//! and then corrected. The same descriptions are what the hand-run script
//! for a small local model asks (`tools/describe-charts-local.sh`), so the
//! fake's calls are the answers a real model is measured against.

use kassensturz_ai::{Message, Role};
use kassensturz_ai_tools::PROPOSE_VIEW_NAME;
use serde_json::{Map, Value};

/// Where the ids of a description come from: a read function's list, and
/// which entry of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DescribedIds {
    /// The read function that lists them (`list_categories`,
    /// `list_accounts`).
    pub function: &'static str,
    /// The list in its result, which is also the argument of
    /// `propose_view` (`categories`, `accounts`).
    pub list: &'static str,
    /// A word of the name to pick, compared in lowercase; the first entry
    /// of the list when none has it.
    pub named: &'static str,
}

/// One plain description and the call the fake model answers it with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DescribedChart {
    /// The description, as a user would type it.
    pub description: &'static str,
    /// Other wordings it also answers, e.g. the German example of the
    /// *Describe a chart* button.
    pub also: &'static [&'static str],
    /// The call of `propose_view` it answers with, as JSON text; the ids of
    /// [`DescribedChart::ids`] are added when the grid lets it read them.
    pub call: &'static str,
    /// Ids it looks up first, if any.
    pub ids: Option<DescribedIds>,
    /// A call it sends first that `propose_view` refuses, so the correction
    /// can be tried; then it sends [`DescribedChart::call`].
    pub refused_first: Option<&'static str>,
}

const CATEGORY_FOOD: DescribedIds = DescribedIds {
    function: "list_categories",
    list: "categories",
    named: "food",
};

const ACCOUNT_CHECKING: DescribedIds = DescribedIds {
    function: "list_accounts",
    list: "accounts",
    named: "checking",
};

/// The descriptions the fake model knows, in the order the testing guide
/// and the local-model script list them. The first is the example the
/// *Describe a chart* button puts into the question field.
pub const DESCRIBED_CHARTS: [DescribedChart; 10] = [
    DescribedChart {
        description: "My spending on food each month this year, as bars",
        also: &["Meine Ausgaben für Essen in jedem Monat dieses Jahres, als Balken"],
        call: r#"{"title":"Food each month this year","data":"income_expense_by_month","show_as":"stacked_bar","period":"this-year"}"#,
        ids: Some(CATEGORY_FOOD),
        refused_first: None,
    },
    DescribedChart {
        description: "Where my money went in the last twelve months, as a pie",
        also: &[],
        call: r#"{"title":"Where my money went","data":"spending_by_category","show_as":"pie","group_by":"category","period":"last-12-months"}"#,
        ids: None,
        refused_first: None,
    },
    DescribedChart {
        description: "Pick the best chart for my spending by main category last year",
        also: &[],
        call: r#"{"title":"Spending by main category last year","data":"spending_by_category","show_as":"auto","group_by":"top_level_category","period":"last-year"}"#,
        ids: None,
        refused_first: None,
    },
    DescribedChart {
        description: "Money in and money out each month this year",
        also: &[],
        call: r#"{"title":"Money in and out this year","data":"income_expense_by_month","show_as":"auto","period":"this-year"}"#,
        ids: None,
        refused_first: None,
    },
    DescribedChart {
        description: "How much of my income I saved this month, compared with last month",
        also: &[],
        call: r#"{"title":"Saved this month","data":"income_expense_by_month","show_as":"figure","measure":"savings-rate","period":"this-month","compare":"previous_period"}"#,
        ids: None,
        refused_first: None,
    },
    DescribedChart {
        description: "What each of my accounts holds today, as a table",
        also: &[],
        call: r#"{"title":"What my accounts hold","data":"account_balances","show_as":"table","group_by":"account","period":"this-month"}"#,
        ids: None,
        refused_first: None,
    },
    DescribedChart {
        description: "My net worth over the last twelve months",
        also: &[],
        call: r#"{"title":"Net worth","data":"net_worth_over_time","show_as":"auto","period":"last-12-months"}"#,
        ids: None,
        refused_first: None,
    },
    DescribedChart {
        description: "Money in and out of my checking account each month from January to June 2026",
        also: &[],
        call: r#"{"title":"Checking account, January to June","data":"cash_flow_by_month","show_as":"bar","period":{"from":"2026-01-01","to":"2026-06-30"}}"#,
        ids: Some(ACCOUNT_CHECKING),
        refused_first: None,
    },
    DescribedChart {
        description: "What I owe today, as one figure",
        also: &[],
        call: r#"{"title":"What I owe","data":"account_balances","show_as":"figure","measure":"liabilities","period":"this-month"}"#,
        ids: None,
        refused_first: None,
    },
    DescribedChart {
        description: "My income and spending each month of the last year, as a pie",
        also: &[],
        call: r#"{"title":"Income and spending each month","data":"income_expense_by_month","show_as":"line","period":"last-12-months"}"#,
        ids: None,
        // A pie can't show a dataset by month: the schema refuses it, and
        // the fake model corrects it to a line.
        refused_first: Some(
            r#"{"title":"Income and spending each month","data":"income_expense_by_month","show_as":"pie","period":"last-12-months"}"#,
        ),
    },
];

/// The description `question` is, if it is one of [`DESCRIBED_CHARTS`]:
/// compared in lowercase, without blanks around it and without a full stop,
/// question or exclamation mark at its end.
pub fn described(question: &str) -> Option<&'static DescribedChart> {
    let question = normalized(question);
    DESCRIBED_CHARTS.iter().find(|chart| {
        std::iter::once(chart.description)
            .chain(chart.also.iter().copied())
            .any(|wording| normalized(wording) == question)
    })
}

fn normalized(text: &str) -> String {
    text.trim()
        .trim_end_matches(['.', '?', '!'])
        .trim_end()
        .to_lowercase()
}

/// The next call for `chart` after `messages`, given the functions that
/// are `offered`: the read function for its ids, then `propose_view` (the
/// refused call first, if it has one, then the right one). `None` once the
/// chart is prepared or refused for good: the model then answers in words.
pub(super) fn next_call(
    chart: &DescribedChart,
    messages: &[Message],
    offered: &[String],
) -> Option<(&'static str, Value)> {
    let is_offered = |name: &str| offered.iter().any(|offer| offer == name);
    if !is_offered(PROPOSE_VIEW_NAME) {
        return None;
    }
    // The calls and results of this question, newest last.
    let this_question: Vec<&Message> = messages
        .iter()
        .rev()
        .take_while(|message| message.role != Role::User)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let views_called = this_question
        .iter()
        .flat_map(|message| &message.calls)
        .filter(|call| call.name == PROPOSE_VIEW_NAME)
        .count();
    let last_call = this_question
        .iter()
        .rev()
        .find_map(|message| message.calls.last());
    let result = messages
        .last()
        .filter(|message| message.role == Role::Tool)
        .and_then(|message| serde_json::from_str::<Value>(&message.content).ok());
    match last_call {
        // A new question: look the ids up first, when the grid lets it.
        None => match chart.ids {
            Some(ids) if is_offered(ids.function) => {
                Some((ids.function, Value::Object(Map::new())))
            }
            _ => Some((PROPOSE_VIEW_NAME, first_call(chart, &[]))),
        },
        Some(call) if chart.ids.is_some_and(|ids| call.name == ids.function) => {
            let picked = chart
                .ids
                .zip(result.as_ref())
                .and_then(|(ids, result)| pick(ids, result))
                .into_iter()
                .collect::<Vec<_>>();
            Some((PROPOSE_VIEW_NAME, first_call(chart, &picked)))
        }
        Some(call) if call.name == PROPOSE_VIEW_NAME => {
            let refused = result
                .as_ref()
                .is_some_and(|result| result.get("error").is_some());
            if refused && views_called == 1 && chart.refused_first.is_some() {
                let ids = ids_of(chart, &call.arguments);
                Some((PROPOSE_VIEW_NAME, with_ids(chart.call, chart, &ids)))
            } else {
                None
            }
        }
        Some(_) => None,
    }
}

/// The first call of `propose_view` for `chart`, with `ids`.
fn first_call(chart: &DescribedChart, ids: &[String]) -> Value {
    with_ids(chart.refused_first.unwrap_or(chart.call), chart, ids)
}

/// `call` with `ids` under the chart's list argument, when there are any.
fn with_ids(call: &str, chart: &DescribedChart, ids: &[String]) -> Value {
    let mut call: Value = serde_json::from_str(call).expect("a fixed call is JSON");
    if let (Some(described), false) = (chart.ids, ids.is_empty()) {
        call[described.list] = Value::from(ids.to_vec());
    }
    call
}

/// The ids an earlier call of the chart sent.
fn ids_of(chart: &DescribedChart, arguments: &str) -> Vec<String> {
    let Some(described) = chart.ids else {
        return Vec::new();
    };
    serde_json::from_str::<Value>(arguments)
        .ok()
        .and_then(|call| call[described.list].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|id| id.as_str().map(str::to_owned))
        .collect()
}

/// The id of the entry of `result`'s list whose name has the word, or the
/// first entry's.
fn pick(ids: DescribedIds, result: &Value) -> Option<String> {
    let list = result[ids.list].as_array()?;
    let named = |entry: &&Value| {
        entry["name"]
            .as_str()
            .is_some_and(|name| name.to_lowercase().contains(ids.named))
    };
    list.iter()
        .find(named)
        .or_else(|| list.first())
        .and_then(|entry| entry["id"].as_str())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_description_is_found_in_any_case_and_with_a_full_stop() {
        let first = &DESCRIBED_CHARTS[0];
        assert_eq!(
            described("  my spending on FOOD each month this year, as bars. "),
            Some(first)
        );
        assert_eq!(
            described("Meine Ausgaben für Essen in jedem Monat dieses Jahres, als Balken"),
            Some(first)
        );
        assert_eq!(described("my spending on food"), None);
        assert_eq!(described(""), None);
    }

    #[test]
    fn every_fixed_call_is_json_and_every_title_is_its_own() {
        let mut titles = Vec::new();
        for chart in &DESCRIBED_CHARTS {
            for call in std::iter::once(chart.call).chain(chart.refused_first) {
                let call: Value = serde_json::from_str(call).expect(chart.description);
                assert!(call["title"].is_string(), "{}", chart.description);
            }
            let call: Value = serde_json::from_str(chart.call).unwrap();
            titles.push(call["title"].as_str().unwrap().to_owned());
        }
        let count = titles.len();
        titles.sort();
        titles.dedup();
        assert_eq!(titles.len(), count, "a title twice would be name-taken");
    }

    #[test]
    fn no_description_is_another_one_or_its_wording() {
        let wordings: Vec<String> = DESCRIBED_CHARTS
            .iter()
            .flat_map(|chart| std::iter::once(chart.description).chain(chart.also.iter().copied()))
            .map(normalized)
            .collect();
        let mut unique = wordings.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), wordings.len());
    }

    #[test]
    fn the_named_entry_is_picked_or_else_the_first() {
        let result = json!({"categories": [
            {"id": "c1", "name": "Housing"},
            {"id": "c2", "name": "Living › Food"},
        ]});
        assert_eq!(pick(CATEGORY_FOOD, &result), Some("c2".to_owned()));
        let result = json!({"categories": [{"id": "c1", "name": "Housing"}]});
        assert_eq!(pick(CATEGORY_FOOD, &result), Some("c1".to_owned()));
        assert_eq!(pick(CATEGORY_FOOD, &json!({"categories": []})), None);
        assert_eq!(pick(CATEGORY_FOOD, &json!({"error": "x"})), None);
    }
}
