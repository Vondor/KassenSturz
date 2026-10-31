// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `propose_view`: the one function a model prepares a chart with (M6c.8,
//! D-352 to D-354, threat T-46).
//!
//! It replaces the list `new_views` of `propose_changes` (M6a.9), so the
//! model reads one schema built for charts. Every value but the title and
//! the days of a range is from a fixed list, and the lists are made from
//! the catalog and the component schema of `kassensturz-report` each time
//! the function is offered, so they can never drift from the code.
//!
//! A call is untrusted input. [`propose_view_text`] checks it in the fixed
//! order of D-353 before anything is built, and the first failure answers:
//! size and nesting, a key twice, an unknown key, each value's type and
//! list, the ids by the permission grid and the book, then the schema
//! itself (the same `Component::set` and `Document::read` a chart made by
//! hand goes through). No answer repeats the model's own text: an argument
//! is named by its fixed name only.
//!
//! What it prepares is the same change-set entry as before
//! ([`super::NewView`] in the list `new_views` of the set), so the review
//! page, the user's edits (D-297), the confirm, the AI-made label and Undo
//! stay exactly as they are.

use std::collections::{HashMap, HashSet};

use kassensturz_core::AccountId;
use kassensturz_core::ai_switch::{Permission, Permissions, Row};
use kassensturz_report::{
    ACCOUNT_BALANCES, BUDGET_UTILIZATION, CASH_FLOW_BY_MONTH, CATALOG, ComponentType, DatasetInfo,
    FORMULA_MEASURE, GROUPS_GROUPING, Kind, NET_WORTH_OVER_TIME, PERIODS, range_word,
};
use kassensturz_store::Book;
use serde_json::{Map, Value, json};

use super::{Proposal, ProposeSpec, prepare, views};
use crate::error::ToolError;

/// The name a model calls it by.
pub const PROPOSE_VIEW_NAME: &str = "propose_view";

/// The longest call, in bytes of its JSON text (D-353). A call with every
/// argument and the most ids is far smaller.
pub const MAX_VIEW_CALL_BYTES: usize = 4 * 1024;

/// The deepest nesting of a call: the call, a period range or a list of
/// ids, and nothing inside those.
const MAX_DEPTH: usize = 3;

/// The most categories a chart the AI prepares may be limited to.
pub const MAX_VIEW_CATEGORIES: usize = 12;

/// The most accounts a chart the AI prepares may be limited to.
pub const MAX_VIEW_ACCOUNTS: usize = 20;

/// The most charts one answer of the model may prepare (D-282, D-352): the
/// chat refuses a fourth call in the same answer.
pub const MAX_VIEWS_PER_ANSWER: usize = 3;

/// Every argument of `propose_view`: the keywords of
/// `ai-tools.bad-argument` for it.
pub(crate) const ARGUMENTS: &[&str] = &[
    "title",
    "data",
    "show_as",
    "measure",
    "group_by",
    "period",
    "compare",
    "categories",
    "accounts",
];

/// The words of `show_as` and the kind of component each one is; `auto`
/// lets KassenSturz pick (D-352).
const SHOW_AS: [(&str, Option<ComponentType>); 11] = [
    ("auto", None),
    ("line", Some(ComponentType::ChartLine)),
    ("area", Some(ComponentType::ChartArea)),
    ("bar", Some(ComponentType::ChartBar)),
    ("stacked_bar", Some(ComponentType::ChartStackedBar)),
    ("pie", Some(ComponentType::ChartPie)),
    ("donut", Some(ComponentType::ChartDonut)),
    ("figure", Some(ComponentType::KpiCard)),
    ("progress", Some(ComponentType::ProgressBar)),
    ("table", Some(ComponentType::TableRows)),
    ("budget_bars", Some(ComponentType::BudgetProgress)),
];

/// The period word that means "the page's own": for a chart saved under
/// Statistics, the period its dataset starts with.
const PAGE: &str = "page";

/// The `compare` word for no comparison.
const NO_COMPARISON: &str = "none";

/// The datasets the AI may name (D-354): the catalog's entries with
/// `for_ai`, in its order.
fn datasets() -> impl Iterator<Item = &'static DatasetInfo> {
    CATALOG.iter().filter(|dataset| dataset.for_ai)
}

/// The kinds of component that can show some dataset the AI may name.
fn shown_kinds() -> impl Iterator<Item = (&'static str, Option<ComponentType>)> {
    SHOW_AS.into_iter().filter(|(_, kind)| {
        kind.is_none_or(|kind| datasets().any(|dataset| dataset.suits.contains(&kind.suit())))
    })
}

/// The words in `lists`, each once, in the order first seen.
fn union(lists: impl Iterator<Item = &'static [&'static str]>) -> Vec<&'static str> {
    let mut words = Vec::new();
    for word in lists.flatten() {
        if !words.contains(word) {
            words.push(*word);
        }
    }
    words
}

/// The words of a choice property of the schema, from the first kind of
/// component that has it.
fn schema_words(key: &str) -> &'static [&'static str] {
    ComponentType::ALL
        .iter()
        .flat_map(|kind| kind.properties())
        .find_map(|property| match property.kind {
            Kind::Choice(words) if property.key == key => Some(words),
            _ => None,
        })
        .unwrap_or(&[])
}

/// Whether some kind of component the AI may choose has a list of ids
/// called `key`: `categories` since D-331, `accounts` once the report
/// layer filters by account (D-351, M6c.6).
fn schema_has_ids(key: &str) -> bool {
    shown_kinds()
        .filter_map(|(_, kind)| kind)
        .flat_map(|kind| kind.properties())
        .any(|property| property.key == key && matches!(property.kind, Kind::Ids))
}

/// The words an argument can take, from the catalog and the schema, for the
/// enums of the schema and the answer to a refused call; `None` for an
/// argument that isn't a choice.
pub fn view_choices(argument: &str) -> Option<Vec<&'static str>> {
    let words = match argument {
        "data" => datasets().map(|dataset| dataset.id).collect(),
        "show_as" => shown_kinds().map(|(word, _)| word).collect(),
        // A formula is written, not chosen: the AI writes none (D-282).
        "measure" => union(datasets().map(|dataset| dataset.measures))
            .into_iter()
            .filter(|word| *word != FORMULA_MEASURE)
            .collect(),
        // The AI knows no groups (D-306, Q-268 = A).
        "group_by" => union(datasets().map(|dataset| dataset.groupings))
            .into_iter()
            .filter(|word| *word != GROUPS_GROUPING)
            .collect(),
        "period" => std::iter::once(PAGE).chain(PERIODS).collect(),
        "compare" => std::iter::once(NO_COMPARISON)
            .chain(schema_words("compare").iter().copied())
            .collect(),
        _ => return None,
    };
    Some(words)
}

/// The arguments the user lets a model send: the ids of categories and
/// accounts only with the reading of them in the grid (D-252), and only
/// where the schema has the list.
fn offered(argument: &str, allowed: Permissions) -> bool {
    match argument {
        "categories" => allowed.reads(Row::Categories) && schema_has_ids("categories"),
        "accounts" => allowed.reads(Row::Accounts) && schema_has_ids("accounts"),
        _ => true,
    }
}

/// `propose_view` as a model is offered it under `allowed`, or `None` when
/// the user doesn't allow Charts › Add (D-282): then it isn't offered at
/// all, and a call is refused.
pub fn propose_view_spec(allowed: Permissions) -> Option<ProposeSpec> {
    if !allowed.allows(Permission::NewViews) {
        return None;
    }
    let choice = |argument: &str, description: &str| {
        json!({
            "type": "string",
            "enum": view_choices(argument).unwrap_or_default(),
            "description": description,
        })
    };
    let ids = |max: usize, description: &str| {
        json!({
            "type": "array",
            "maxItems": max,
            "uniqueItems": true,
            "items": {"type": "string"},
            "description": description,
        })
    };
    let mut properties = Map::new();
    properties.insert(
        "title".into(),
        json!({
            "type": "string",
            "minLength": 1,
            "maxLength": kassensturz_report::MAX_TEXT_CHARS,
            "description": "A short plain-text title for the chart, one line. It is also the \
                name it is saved under. Never put numbers in it as facts.",
        }),
    );
    properties.insert(
        "data".into(),
        choice(
            "data",
            "What the chart shows. The description of this function says what each one counts.",
        ),
    );
    properties.insert(
        "show_as".into(),
        choice(
            "show_as",
            "How it is drawn, or auto to let KassenSturz pick what suits the data. figure is one \
             key figure, progress a bar towards a figure. Not every way suits every data.",
        ),
    );
    properties.insert(
        "measure".into(),
        choice(
            "measure",
            "Only for figure: which key figure of the data. Default: the data's first.",
        ),
    );
    properties.insert(
        "group_by".into(),
        choice(
            "group_by",
            "How the figures are grouped, if the data offers a choice. Default: the data's first.",
        ),
    );
    properties.insert(
        "period".into(),
        json!({
            "anyOf": [
                {"type": "string", "enum": view_choices("period").unwrap_or_default()},
                {
                    "type": "object",
                    "properties": {
                        "from": {"type": "string", "format": "date", "description": "First day, YYYY-MM-DD. Leave it out for everything up to the last day."},
                        "to": {"type": "string", "format": "date", "description": "Last day, YYYY-MM-DD. Leave it out for everything up to today."},
                    },
                    "additionalProperties": false,
                },
            ],
            "description": "The period shown: page (the data's usual period), a fixed period, \
                or a range of days. Default: last-12-months, this-month for account_balances \
                and budget_utilization.",
        }),
    );
    properties.insert(
        "compare".into(),
        choice(
            "compare",
            "Only for figure and progress: what to compare with. Default none.",
        ),
    );
    if offered("categories", allowed) {
        properties.insert(
            "categories".into(),
            ids(
                MAX_VIEW_CATEGORIES,
                "Only these categories, IDs from list_categories. Leave it out for all.",
            ),
        );
    }
    if offered("accounts", allowed) {
        properties.insert(
            "accounts".into(),
            ids(
                MAX_VIEW_ACCOUNTS,
                "Only these accounts, IDs from list_accounts. Leave it out for all.",
            ),
        );
    }
    let parameters = json!({
        "type": "object",
        "properties": properties,
        "required": ["title", "data", "show_as"],
        "additionalProperties": false,
    });
    let mut description = String::from(
        "Prepares one chart for the user to review and save under Statistics, from choices in \
         the lists only: what it shows (data), how it is drawn (show_as), and optionally a key \
         figure, a grouping, a period, a comparison",
    );
    if offered("categories", allowed) || offered("accounts", allowed) {
        description.push_str(" and the categories or accounts it is limited to");
    }
    description.push_str(&format!(
        ", plus a short title. Call it once per chart, at most {MAX_VIEWS_PER_ANSWER} charts in \
         one answer. KassenSturz computes every figure itself and shows the user a preview; \
         nothing is saved until the user confirms it in KassenSturz's window, and the user may \
         refuse it. A refused call says which argument is wrong and, for a list, which values \
         it takes. The data are:"
    ));
    for dataset in datasets() {
        description.push_str(&format!(" {}: {}", dataset.id, dataset.description));
    }
    Some(ProposeSpec {
        name: PROPOSE_VIEW_NAME,
        description,
        parameters: parameters.to_string(),
    })
}

/// Checks a call of `propose_view` as the model wrote it, `text`, against
/// `book` and turns it into a change set with one new chart for the user to
/// review (D-352, D-353). The book doesn't change.
pub fn propose_view_text(
    book: &Book,
    text: &str,
    allowed: Permissions,
) -> Result<Proposal, ToolError> {
    if text.len() > MAX_VIEW_CALL_BYTES {
        return Err(ToolError::CallTooBig {
            max_bytes: MAX_VIEW_CALL_BYTES,
        });
    }
    let value: Value = serde_json::from_str(text).map_err(|_| ToolError::BadArgument {
        argument: "arguments",
    })?;
    guard(text)?;
    propose_view(book, &value, allowed)
}

/// [`propose_view_text`] for a call whose text was read already, as the
/// MCP server and the request files have it: a key twice was decided by
/// whoever read the text.
pub fn propose_view(
    book: &Book,
    arguments: &Value,
    allowed: Permissions,
) -> Result<Proposal, ToolError> {
    if !allowed.allows(Permission::NewViews) {
        return Err(ToolError::NotAllowed {
            argument: PROPOSE_VIEW_NAME,
        });
    }
    let item = check(book, arguments, allowed)?;
    prepare(book, &json!({ "new_views": [item] }), allowed).map_err(|error| match error {
        // The schema's refusals name the argument of this function, not the
        // item's key it was put under.
        ToolError::BadArgument { argument } => ToolError::BadArgument {
            argument: match argument {
                "chart" => "show_as",
                "dataset" => "data",
                "grouping" => "group_by",
                "new_views" => "arguments",
                other => other,
            },
        },
        other => other,
    })
}

/// Refuses text in which an object has the same key twice, compared after
/// decoding escapes, or that is nested deeper than [`MAX_DEPTH`]. The text
/// is valid JSON already, so only brackets, commas and strings need
/// following. (`kassensturz-report` checks documents the same way, D-278,
/// with its own depth.)
fn guard(text: &str) -> Result<(), ToolError> {
    let mut open: Vec<Option<HashSet<String>>> = Vec::new();
    let mut key_next = false;
    let mut chars = text.char_indices();
    while let Some((start, c)) = chars.next() {
        match c {
            '{' | '[' => {
                if open.len() >= MAX_DEPTH {
                    return Err(ToolError::BadArgument {
                        argument: "arguments",
                    });
                }
                key_next = c == '{';
                open.push((c == '{').then(HashSet::new));
            }
            '}' | ']' => {
                open.pop();
                key_next = false;
            }
            ',' => key_next = matches!(open.last(), Some(Some(_))),
            '"' => {
                let mut escaped = false;
                let mut end = start;
                for (at, c) in chars.by_ref() {
                    end = at;
                    if escaped {
                        escaped = false;
                    } else if c == '\\' {
                        escaped = true;
                    } else if c == '"' {
                        break;
                    }
                }
                if key_next {
                    key_next = false;
                    let key: String = serde_json::from_str(&text[start..=end]).map_err(|_| {
                        ToolError::BadArgument {
                            argument: "arguments",
                        }
                    })?;
                    if let Some(Some(keys)) = open.last_mut()
                        && !keys.insert(key)
                    {
                        return Err(ToolError::RepeatedKey);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// How deep `value` is nested: 1 for an object of plain values.
fn depth(value: &Value) -> usize {
    match value {
        Value::Array(items) => 1 + items.iter().map(depth).max().unwrap_or(0),
        Value::Object(values) => 1 + values.values().map(depth).max().unwrap_or(0),
        _ => 0,
    }
}

/// The checks of D-353 in their order; on success, the item of `new_views`
/// the change set is built from.
fn check(book: &Book, arguments: &Value, allowed: Permissions) -> Result<Value, ToolError> {
    let whole = ToolError::BadArgument {
        argument: "arguments",
    };
    // 1. Size, nesting and shape.
    if arguments.to_string().len() > MAX_VIEW_CALL_BYTES {
        return Err(ToolError::CallTooBig {
            max_bytes: MAX_VIEW_CALL_BYTES,
        });
    }
    if depth(arguments) > MAX_DEPTH {
        return Err(whole);
    }
    let call = arguments.as_object().ok_or(whole)?;
    // 3. No unknown key, nor one the user doesn't let the model send. A
    // key twice (2.) was refused with the text.
    if call
        .keys()
        .any(|key| !ARGUMENTS.contains(&key.as_str()) || !offered(key, allowed))
    {
        return Err(ToolError::UnknownArgument);
    }
    // Null means left out: small models send it often.
    let given = |name: &str| call.get(name).filter(|value| !value.is_null());
    // 4. Each value's type and list.
    let word = |name: &'static str| -> Result<Option<&str>, ToolError> {
        let Some(value) = given(name) else {
            return Ok(None);
        };
        let choices = view_choices(name).unwrap_or_default();
        value
            .as_str()
            .filter(|word| choices.contains(word))
            .map(Some)
            .ok_or(ToolError::BadArgument { argument: name })
    };
    let title = views::title(
        given("title")
            .and_then(Value::as_str)
            .ok_or(ToolError::BadArgument { argument: "title" })?,
    )?;
    let data = word("data")?.ok_or(ToolError::BadArgument { argument: "data" })?;
    let show_as = word("show_as")?.ok_or(ToolError::BadArgument {
        argument: "show_as",
    })?;
    let measure = word("measure")?;
    let group_by = word("group_by")?;
    let compare = word("compare")?.filter(|word| *word != NO_COMPARISON);
    let period = match given("period") {
        None => None,
        Some(Value::String(_)) => word("period")?
            .filter(|word| *word != PAGE)
            .map(str::to_owned),
        Some(Value::Object(range)) => Some(range_period(range)?),
        Some(_) => return Err(ToolError::BadArgument { argument: "period" }),
    };
    let categories = ids(given("categories"), "categories", MAX_VIEW_CATEGORIES)?;
    let accounts = ids(given("accounts"), "accounts", MAX_VIEW_ACCOUNTS)?;
    // 5. Ids by the grid and the book.
    if !categories.is_empty() || !accounts.is_empty() {
        let kinds: HashMap<AccountId, _> = book
            .all_accounts()?
            .into_iter()
            .map(|account| (account.id, (account.kind, account.closed)))
            .collect();
        if !categories.is_empty() && !allowed.reads(Row::Categories) {
            return Err(ToolError::NotAllowed {
                argument: "categories",
            });
        }
        if !accounts.is_empty() && !allowed.reads(Row::Accounts) {
            return Err(ToolError::NotAllowed {
                argument: "accounts",
            });
        }
        let category = |id: &AccountId| {
            kinds
                .get(id)
                .is_some_and(|(kind, closed)| kind.is_category() && !closed)
        };
        if !categories.iter().all(category) {
            return Err(ToolError::UnknownCategory);
        }
        if !accounts
            .iter()
            .all(|id| kinds.get(id).is_some_and(|(kind, _)| kind.is_account()))
        {
            return Err(ToolError::UnknownAccount);
        }
    }
    // 6. A dimension the dataset doesn't apply is refused, never kept: a
    // chart of net worth with categories would look limited while every
    // category counts (M6-B29, D-350).
    if !categories.is_empty() && !takes_categories(data) {
        return Err(ToolError::BadArgument {
            argument: "categories",
        });
    }
    // The schema's own checks run when the set is built from this item.
    let chart = SHOW_AS
        .iter()
        .find(|(word, _)| *word == show_as)
        .and_then(|(_, kind)| *kind)
        .map_or("auto", ComponentType::id);
    let mut item = Map::new();
    item.insert("title".into(), json!(title));
    item.insert("dataset".into(), json!(data));
    item.insert("chart".into(), json!(chart));
    for (key, value) in [
        ("grouping", group_by),
        ("measure", measure),
        ("compare", compare),
    ] {
        if let Some(value) = value {
            item.insert(key.into(), json!(value));
        }
    }
    item.insert(
        "period".into(),
        json!(period.unwrap_or_else(|| default_period(data).to_owned())),
    );
    for (key, list) in [("categories", &categories), ("accounts", &accounts)] {
        if !list.is_empty() {
            let list: Vec<String> = list.iter().map(ToString::to_string).collect();
            item.insert(key.into(), json!(list));
        }
    }
    Ok(Value::Object(item))
}

/// Whether the dataset applies a choice of categories. The balances count
/// accounts, not categories (D-350). When the report layer lists what each
/// dataset takes (M6c.6), this reads that list instead.
fn takes_categories(dataset: &str) -> bool {
    ![ACCOUNT_BALANCES, NET_WORTH_OVER_TIME, CASH_FLOW_BY_MONTH].contains(&dataset)
}

/// The period a dataset starts with: today's balances and budgets this
/// month, everything else the last twelve months (D-352).
fn default_period(dataset: &str) -> &'static str {
    if dataset == ACCOUNT_BALANCES || dataset == BUDGET_UTILIZATION {
        "this-month"
    } else {
        views::DEFAULT_PERIOD
    }
}

/// A typed range `{"from": …, "to": …}` with one day or both, as the word
/// a saved chart keeps (D-332).
fn range_period(range: &Map<String, Value>) -> Result<String, ToolError> {
    let bad = ToolError::BadArgument { argument: "period" };
    if range.keys().any(|key| key != "from" && key != "to") {
        return Err(bad);
    }
    let day = |key: &str| -> Result<Option<kassensturz_core::Date>, ToolError> {
        match range.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(text)) => {
                let wellformed = text.len() == 10
                    && text.char_indices().all(|(at, c)| match at {
                        4 | 7 => c == '-',
                        _ => c.is_ascii_digit(),
                    });
                wellformed
                    .then(|| text.parse().ok())
                    .flatten()
                    .map(Some)
                    .ok_or(ToolError::BadArgument { argument: "period" })
            }
            Some(_) => Err(ToolError::BadArgument { argument: "period" }),
        }
    };
    let (from, to) = (day("from")?, day("to")?);
    if let (Some(from), Some(to)) = (from, to)
        && to < from
    {
        return Err(ToolError::BadPeriod);
    }
    range_word(from, to).map_err(|_| bad)
}

/// A list of ids: strings that are ids, none twice, at most `max`.
fn ids(
    value: Option<&Value>,
    argument: &'static str,
    max: usize,
) -> Result<Vec<AccountId>, ToolError> {
    let bad = ToolError::BadArgument { argument };
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let items = value
        .as_array()
        .ok_or(ToolError::BadArgument { argument })?;
    if items.len() > max {
        return Err(bad);
    }
    let mut seen = HashSet::new();
    items
        .iter()
        .map(|item| {
            let id = item
                .as_str()
                .and_then(|text| uuid::Uuid::try_parse(text).ok())
                .map(|id| AccountId::from_bytes(id.into_bytes()))
                .ok_or(ToolError::BadArgument { argument })?;
            if seen.insert(id) {
                Ok(id)
            } else {
                Err(ToolError::BadArgument { argument })
            }
        })
        .collect()
}
