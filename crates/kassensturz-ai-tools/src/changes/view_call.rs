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

use kassensturz_core::ai_switch::{Permission, Permissions, Row};
use kassensturz_core::{AccountId, AccountKind, CurrencyCode, Date, GroupMember};
use kassensturz_report::{
    ACCOUNT_BALANCES, AVERAGE_WORDS, BUDGET_UTILIZATION, CATALOG, ComponentType, DatasetInfo,
    FORMULA_MEASURE, FUNCTIONS, Filter, Formula, FormulaResult, GROUPS_GROUPING, Kind,
    MAX_FORMULA_CHARS, ORIENTATION_WORDS, PERIODS, Per, Period, ReportError, Rolling,
    SPENDING_BY_CATEGORY, Source, Term, range_word,
};
use kassensturz_store::Book;
use serde_json::{Map, Value, json};

use super::{Proposal, ProposeSpec, prepare, view_figure, views};
use crate::error::ToolError;
use crate::text::hides_text;

/// The name a model calls it by.
pub const PROPOSE_VIEW_NAME: &str = "propose_view";

/// The longest call, in bytes of its JSON text (D-353, D-650). A call with
/// every argument, the most ids in every list and two formulas of the
/// longest length in plain letters fits.
pub const MAX_VIEW_CALL_BYTES: usize = 8 * 1024;

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
    "view_id",
    "title",
    "data",
    "show_as",
    "measure",
    "group_by",
    "period",
    "compare",
    "categories",
    "accounts",
    "average",
    "orientation",
    "income_categories",
    "income_accounts",
    "expenses_categories",
    "expenses_accounts",
    "formula",
    "target",
];

/// The own lists of money in and money out (D-646): each list's argument,
/// whether it holds categories (else accounts), and the kind of category
/// it takes.
const SERIES_LISTS: [(&str, bool, Option<AccountKind>); 4] = [
    ("income_categories", true, Some(AccountKind::Income)),
    ("income_accounts", false, None),
    ("expenses_categories", true, Some(AccountKind::Expense)),
    ("expenses_accounts", false, None),
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

/// What a rolling period word looks like (D-644, D-645), for the schema;
/// [`Rolling::read`] decides which numbers it takes.
const ROLLING_PATTERN: &str =
    "^(last-[1-9][0-9]{0,2}(-complete)?-months|(this|last)-salary-month-[1-9][0-9]?)$";

/// How a formula is written, for the schema (D-292, D-295, D-649, D-653),
/// with the functions of [`FUNCTIONS`].
pub(crate) fn formula_help() -> String {
    let functions: Vec<&str> = FUNCTIONS.iter().map(|function| function.name).collect();
    format!(
        "Only for figure and progress, with data income_expense_by_month: a spreadsheet \
         formula as in LibreOffice Calc, e.g. =Groceries - Restaurants or =SUM(Rent; Energy), \
         the figure shown instead of a key figure. Names are categories from list_categories, \
         written as they are named there, in single quotes when they have blanks ('Eating \
         out'), a category inside another as 'Home > Rent'; Spending.Pets or Income.Pets picks \
         the kind when both have the name; Spending.ALL and Income.ALL are all money out and \
         all money in. A name stands for its money in the chart's period as a positive amount, \
         counted from the chart's accounts and categories (income_accounts, expenses_accounts, \
         income_categories and expenses_categories don't apply to it). @ after a name takes \
         another period, e.g. =Groceries@\"last 3 complete months\" / 3 for the average month. \
         After @ write, in double quotes, \"this month\", \"last month\", \"this quarter\", \
         \"last quarter\", \"this year\", \"last year\", \"last N months\" (the chart's month \
         and the N-1 before it) or \"last N complete months\" (the N whole months before it), \
         N from 1 to 120, each counted from the chart's period; or, without quotes, a year \
         2026, a month 2026-09, a day 2026-09-15, or a range of them such as 2026-01..2026-03. \
         There is no salary month after @. Write ; between arguments and . as the decimal \
         mark. The functions are {}. KassenSturz works the figure out itself; a refused \
         formula says at which character it is wrong.",
        functions.join(", ")
    )
}

/// The period word that means "the page's own": for a chart saved under
/// Statistics, the period its dataset starts with.
const PAGE: &str = "page";

/// The `compare` word for no comparison.
const NO_COMPARISON: &str = "none";

/// The `average` word for the total, which is what a chart shows when it
/// has no average.
const TOTAL: &str = "total";
/// Bars that go the way KassenSturz picks (D-679): the default, so it is
/// not kept in the item.
const AUTO: &str = "auto";

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

/// The choices that suit one dataset, as the description of `propose_view`
/// lists them (A5 of the US1 tool review); `auto` suits every dataset.
#[derive(Debug)]
pub(crate) struct Fits {
    pub(crate) show_as: Vec<&'static str>,
    pub(crate) group_by: Vec<&'static str>,
    /// The ways of `show_as` that take a `group_by`: a key figure, for one,
    /// has none.
    pub(crate) grouped_as: Vec<&'static str>,
    pub(crate) measure: Vec<&'static str>,
}

pub(crate) fn fits(dataset: &DatasetInfo) -> Fits {
    Fits {
        show_as: SHOW_AS
            .into_iter()
            .filter_map(|(word, kind)| {
                kind.filter(|kind| dataset.suits.contains(&kind.suit()))
                    .map(|_| word)
            })
            .collect(),
        group_by: dataset
            .groupings
            .iter()
            .copied()
            .filter(|word| *word != GROUPS_GROUPING)
            .collect(),
        grouped_as: SHOW_AS
            .into_iter()
            .filter_map(|(word, kind)| {
                kind.filter(|kind| {
                    dataset.suits.contains(&kind.suit())
                        && kind
                            .properties()
                            .iter()
                            .any(|property| property.key == "grouping")
                })
                .map(|_| word)
            })
            .collect(),
        measure: dataset
            .measures
            .iter()
            .copied()
            .filter(|word| *word != FORMULA_MEASURE)
            .collect(),
    }
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
/// layer filters by account (D-351, M6c.6, `Kind::Accounts`).
fn schema_has_ids(key: &str) -> bool {
    shown_kinds()
        .filter_map(|(_, kind)| kind)
        .flat_map(|kind| kind.properties())
        .any(|property| property.key == key && matches!(property.kind, Kind::Ids | Kind::Accounts))
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
        "average" => AVERAGE_WORDS.to_vec(),
        "orientation" => ORIENTATION_WORDS.to_vec(),
        _ => return None,
    };
    Some(words)
}

/// The arguments the user lets a model send: the ids of categories and
/// accounts only with the reading of them in the grid (D-252), and only
/// where the schema has the list.
///
/// A formula names categories, so it is offered only with the reading of
/// them too (D-650): a model that may not read them can't learn from a
/// refused formula which names the book has.
fn offered(argument: &str, allowed: Permissions) -> bool {
    match argument {
        "categories" | "income_categories" | "expenses_categories" => {
            allowed.reads(Row::Categories) && schema_has_ids(argument)
        }
        "accounts" | "income_accounts" | "expenses_accounts" => {
            allowed.reads(Row::Accounts) && schema_has_ids(argument)
        }
        "formula" | "target" => allowed.reads(Row::Categories),
        // Changing a saved chart is its own cell (D-681).
        "view_id" => allowed.allows(Permission::EditViews),
        _ => true,
    }
}

/// `propose_view` as a model is offered it under `allowed`, or `None` when
/// the user allows neither Charts › Add (D-282) nor Charts › Change
/// (D-681): then it isn't offered at all, and a call is refused. With
/// Charts › Change it takes `view_id`, and a call with one changes that
/// saved chart.
pub fn propose_view_spec(allowed: Permissions) -> Option<ProposeSpec> {
    let adds = allowed.allows(Permission::NewViews);
    let changes = allowed.allows(Permission::EditViews);
    if !adds && !changes {
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
    if changes {
        properties.insert(
            "view_id".into(),
            json!({
                "type": "string",
                "description": "Change this saved chart instead of preparing a new one, ID from \
                    list_views. Only the arguments you give change; everything else stays as it \
                    is. Words such as none, total, auto and page put a setting back to its \
                    default, and an empty list takes a limit away.",
            }),
        );
    }
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
    properties.insert("data".into(), choice("data", &data_help()));
    properties.insert(
        "show_as".into(),
        choice(
            "show_as",
            "How it is drawn, or auto to let KassenSturz pick what suits the data. figure is one \
             key figure, progress a bar towards a figure. The description of data lists the ways \
             each data can be drawn.",
        ),
    );
    properties.insert(
        "measure".into(),
        choice(
            "measure",
            "Only for figure: which key figure of the data, as the description of data lists \
             them. Default: the data's first.",
        ),
    );
    properties.insert(
        "group_by".into(),
        choice(
            "group_by",
            "How the figures are grouped, as the description of data lists them. Default: the \
             data's first.",
        ),
    );
    properties.insert(
        "period".into(),
        json!({
            "anyOf": [
                {"type": "string", "enum": view_choices("period").unwrap_or_default()},
                {
                    "type": "string",
                    "pattern": ROLLING_PATTERN,
                    "description": "A period that moves with today (D-644, D-645): \
                        last-N-months is this month and the N-1 before it, \
                        last-N-complete-months the N whole months before this one (N from 1 \
                        to 120); this-salary-month-D is the month from day D of a month to the \
                        day before day D of the next (D from 1 to 31, the last day in a shorter \
                        month), the one today is in, last-salary-month-D the one before it.",
                },
                {
                    "type": "object",
                    "properties": {
                        "from": {"type": "string", "format": "date", "description": "First day, YYYY-MM-DD. Leave it out for everything up to the last day."},
                        "to": {"type": "string", "format": "date", "description": "Last day, YYYY-MM-DD. Leave it out for everything up to today."},
                    },
                    "additionalProperties": false,
                },
            ],
            "description": "The period shown: page (the data's usual period, the same as \
                leaving period out: this-month for account_balances and budget_utilization, \
                last-12-months for every other data), a fixed period, a period that moves with \
                today, or a range of days. Data counted per month always splits the period into \
                calendar months, also a salary month.",
        }),
    );
    properties.insert(
        "compare".into(),
        choice(
            "compare",
            "Only for figure and progress: what to compare with. Default none.",
        ),
    );
    properties.insert(
        "average".into(),
        choice(
            "average",
            "Only for spending_by_category: show each category's total (the default) or its \
             average per month, quarter or year of the period. KassenSturz divides the exact \
             total once by the months (quarters, years) of the period up to today. Months \
             before the first transaction of its accounts and categories count as zero; the \
             answer then has an average_note saying how many.",
        ),
    );
    properties.insert(
        "orientation".into(),
        choice(
            "orientation",
            "Only for bar and stacked_bar: which way the bars go. auto (the default) lets \
             KassenSturz pick: rows when names are too long to stand under their bars. \
             horizontal draws one row per name, vertical upright bars. The figures stay \
             the same.",
        ),
    );
    if offered("categories", allowed) {
        properties.insert(
            "categories".into(),
            ids(
                MAX_VIEW_CATEGORIES,
                "Only these categories, IDs from list_categories. Leave it out for all. Not for \
                 figure or progress, which have no category filter: there, write a formula that \
                 names the categories instead.",
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
    for (argument, categories, _) in SERIES_LISTS {
        if !offered(argument, allowed) {
            continue;
        }
        let (side, what, max, list) = match (argument.starts_with("income"), categories) {
            (true, true) => (
                "money in",
                "income categories",
                MAX_VIEW_CATEGORIES,
                "list_categories",
            ),
            (true, false) => ("money in", "accounts", MAX_VIEW_ACCOUNTS, "list_accounts"),
            (false, true) => (
                "money out",
                "spending categories",
                MAX_VIEW_CATEGORIES,
                "list_categories",
            ),
            (false, false) => ("money out", "accounts", MAX_VIEW_ACCOUNTS, "list_accounts"),
        };
        properties.insert(
            argument.into(),
            ids(
                max,
                &format!(
                    "Only for income_expense_by_month: {side} counts only these {what}, IDs \
                     from {list}. Leave it out for all."
                ),
            ),
        );
    }
    if offered("formula", allowed) {
        properties.insert(
            "formula".into(),
            json!({
                "type": "string",
                "minLength": 1,
                "maxLength": MAX_FORMULA_CHARS,
                "description": formula_help(),
            }),
        );
        properties.insert(
            "target".into(),
            json!({
                "type": "string",
                "minLength": 1,
                "maxLength": MAX_FORMULA_CHARS,
                "description": "Only for progress with a formula: what a full bar stands for, \
                    a number or a formula written the same way, of the same kind as the \
                    formula (an amount for an amount).",
            }),
        );
    }
    let parameters = json!({
        "type": "object",
        "properties": properties,
        "required": match (adds, changes) {
            (true, false) => vec!["title", "data", "show_as"],
            (false, true) => vec!["view_id"],
            _ => Vec::new(),
        },
        "additionalProperties": false,
    });
    let mut description = String::from(match (adds, changes) {
        (true, false) => "Prepares one chart for the user to review and save under Statistics.",
        (false, true) => {
            "Prepares a change to one chart the user saved under Statistics, named by view_id \
             from list_views, for the user to review. Only the arguments you give change."
        }
        _ => {
            "Prepares one chart for the user to review and save under Statistics. A new chart \
             needs title, data and show_as. With view_id from list_views it changes that saved \
             chart instead: only the arguments you give change."
        }
    });
    description.push_str(
        "\n- Choose only from the lists: what it shows (data), how it is drawn (show_as), and \
         optionally a key figure, a grouping, a period, a comparison",
    );
    if offered("categories", allowed) || offered("accounts", allowed) {
        description.push_str(" and the categories or accounts it is limited to");
    }
    if offered("formula", allowed) {
        description.push_str(
            "; a key figure or a progress bar of income_expense_by_month can show a formula \
             instead, which the user sees in plain words before confirming",
        );
    }
    description.push_str(&format!(
        ", plus a short title.\n- Call it once per chart, at most {MAX_VIEWS_PER_ANSWER} charts \
         in one answer.\n- KassenSturz computes every figure itself and shows the user a \
         preview; nothing is saved until the user confirms it in KassenSturz's window, and the \
         user may refuse it. For figure and progress the answer's computed holds the figure \
         the chart shows; tell the user that one.\n- A refused call says which argument is wrong and, for a list, \
         which values suit the chosen data (allowed), which show_as take the argument \
         (only_for_show_as) or which argument to use instead (use_instead).\n- The description of data says what each data counts and how \
         it can be drawn, grouped and measured."
    ));
    Some(ProposeSpec {
        name: PROPOSE_VIEW_NAME,
        description,
        parameters: parameters.to_string(),
    })
}

/// The description of `data`: each dataset the AI may choose, with what it
/// counts, the ways it can be drawn, grouped and measured, and its usual
/// period. It is the longest part of `propose_view`'s help, so it lives
/// with the argument it explains rather than in the function's description,
/// which some AI tools cut short (M6-B201).
fn data_help() -> String {
    let mut description = String::from(
        "What the chart shows. Each data, with what it counts, the ways it can be drawn \
         (show_as), grouped (group_by) and, for figure, measured (measure), and its usual \
         period:",
    );
    for dataset in datasets() {
        description.push_str(&format!("\n- {}: {}", dataset.id, dataset.description));
        let fits = fits(dataset);
        description.push_str(&format!(" show_as: auto, {}.", fits.show_as.join(", ")));
        if !fits.group_by.is_empty() && !fits.grouped_as.is_empty() {
            description.push_str(&format!(
                " group_by: {} (with show_as {}).",
                fits.group_by.join(", "),
                fits.grouped_as.join(", ")
            ));
        }
        if !fits.measure.is_empty() && fits.show_as.contains(&"figure") {
            description.push_str(&format!(" measure: {}.", fits.measure.join(", ")));
        }
        description.push_str(&format!(" Usual period: {}.", default_period(dataset.id)));
    }
    description
}

/// Checks a call of `propose_view` as the model wrote it, `text`, against
/// `book` and turns it into a change set with one new chart for the user to
/// review (D-352, D-353). The book doesn't change.
pub fn propose_view_text(
    book: &Book,
    text: &str,
    allowed: Permissions,
    today: Date,
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
    propose_view(book, &value, allowed, today)
}

/// [`propose_view_text`] for a call whose text was read already, as the
/// MCP server and the request files have it: a key twice was decided by
/// whoever read the text.
///
/// `today` is the day the chart's period and its average are counted on:
/// the answer names the months an average counts before the first
/// transaction of its accounts and categories (M6-B207, #590, D-668).
pub fn propose_view(
    book: &Book,
    arguments: &Value,
    allowed: Permissions,
    today: Date,
) -> Result<Proposal, ToolError> {
    let editing = arguments.get("view_id").is_some_and(|id| !id.is_null());
    let needed = if editing {
        Permission::EditViews
    } else {
        Permission::NewViews
    };
    if !allowed.allows(needed) {
        return Err(ToolError::NotAllowed {
            argument: if editing {
                "view_id"
            } else {
                PROPOSE_VIEW_NAME
            },
        });
    }
    let item = check(book, arguments, allowed, editing)?;
    let mut proposal = prepare_view(book, arguments, &item, allowed)?;
    if let Some(early) = counted_early(book, &item, today)? {
        let per = match early.per {
            Per::Month => "months",
            Per::Quarter => "quarters",
            Per::Year => "years",
        };
        if let Value::Object(value) = &mut proposal.output.value {
            value.insert(
                "average_note".into(),
                json!(format!(
                    "The average divides by {counted} {per}; {before} of them are before the \
                     first transaction of the accounts and categories it counts, so they count \
                     as zero and the average is lower than what is spent in the {per} that have \
                     transactions. Tell the user so; the chart shows a note about it too.",
                    counted = early.counted,
                    before = early.before,
                )),
            );
        }
        proposal.output.figures.add_count(u64::from(early.counted));
        proposal.output.figures.add_count(u64::from(early.before));
    }
    // What a key figure or a progress bar shows (D-685).
    view_figure::add_figure(book, &mut proposal, today)?;
    Ok(proposal)
}

/// The months (quarters, years) the average of a checked `item` counts
/// before the first transaction it could count, as the chart counts them on
/// `today` (D-668). `None` for a chart without an average, and for a period
/// that can't be read here; the chart then says what it says.
fn counted_early(
    book: &Book,
    item: &Value,
    today: Date,
) -> Result<Option<kassensturz_report::CountedEarly>, ToolError> {
    let word = |key: &str| item.get(key).and_then(Value::as_str);
    let Some(per) = word("average").and_then(Per::read) else {
        return Ok(None);
    };
    if word("dataset") != Some(SPENDING_BY_CATEGORY) {
        return Ok(None);
    }
    let Some(period) = word("period") else {
        return Ok(None);
    };
    let earliest = || {
        book.booking_date_range()
            .ok()
            .flatten()
            .map(|(first, _)| first)
    };
    let earliest = if period.contains("..") {
        earliest()
    } else {
        None
    };
    let Some(Ok(period)) = Period::named(period, today, earliest) else {
        return Ok(None);
    };
    let ids = |key: &str| -> Option<Vec<AccountId>> {
        let list: Vec<AccountId> = item
            .get(key)?
            .as_array()?
            .iter()
            .filter_map(|id| uuid::Uuid::try_parse(id.as_str()?).ok())
            .map(|id| AccountId::from_bytes(id.into_bytes()))
            .collect();
        (!list.is_empty()).then_some(list)
    };
    let filter = Filter {
        accounts: ids("accounts"),
        categories: ids("categories"),
    };
    let source = Source::read(book, period).map_err(from_report)?;
    kassensturz_report::counted_early(book, &source, period, &filter, per, today)
        .map_err(from_report)
}

/// A report error while counting a proposed chart's average: the book's
/// own errors as they are, anything else as a period the book can't count.
fn from_report(error: ReportError) -> ToolError {
    match error {
        ReportError::Store(error) => ToolError::Store(error),
        _ => ToolError::BadArgument { argument: "period" },
    }
}

/// The change set of a checked chart `item` made of the call `arguments`,
/// with the schema's refusals named by the arguments of `propose_view`.
fn prepare_view(
    book: &Book,
    arguments: &Value,
    item: &Value,
    allowed: Permissions,
) -> Result<Proposal, ToolError> {
    let list = if item.get("view_id").is_some() {
        "edit_views"
    } else {
        "new_views"
    };
    prepare(book, &json!({ list: [item] }), allowed).map_err(|error| {
        match error.without_item() {
            // The schema's refusals name the argument of this function, not the
            // item's key it was put under.
            ToolError::BadArgument { argument } => what_suits(
                arguments,
                allowed,
                match argument {
                    "chart" => "show_as",
                    "dataset" => "data",
                    "grouping" => "group_by",
                    "new_views" | "edit_views" => "arguments",
                    other => other,
                },
            ),
            other => other,
        }
    })
}

/// The data whose key figures and progress bars can show a formula (D-650).
const FORMULA_DATA: &str = "income_expense_by_month";

/// The refusal of `argument` in a call of `propose_view` whose data is
/// known, with what the model can use instead (M6-B204): for `show_as`,
/// `group_by` and `measure` the words that suit the data (and the way of
/// drawing it), as the description of `data` lists them; for `group_by`,
/// `measure` and `compare` on a way of drawing that takes none, the ways
/// that do, so no list of words misleads; for a category list on a key
/// figure or progress bar, which have no category filter, `formula`; for
/// `formula` or `target` on a way of drawing that shows none, the ways that
/// do.
/// Everything comes from the catalog, never from the call.
fn what_suits(arguments: &Value, allowed: Permissions, argument: &'static str) -> ToolError {
    let bad = ToolError::BadArgument { argument };
    let Some(dataset) = arguments
        .get("data")
        .and_then(Value::as_str)
        .and_then(kassensturz_report::dataset)
        .filter(|dataset| dataset.for_ai)
    else {
        return bad;
    };
    let fits = fits(dataset);
    let show_as = arguments.get("show_as").and_then(Value::as_str);
    let drawn_as = |words: &[&str]| show_as.is_some_and(|way| words.contains(&way));
    // The ways of drawing this data among `ways`.
    let suiting = |ways: &[&'static str]| -> Vec<&'static str> {
        fits.show_as
            .iter()
            .copied()
            .filter(|way| ways.contains(way))
            .collect()
    };
    const SINGLE: [&str; 2] = ["figure", "progress"];
    let (words, use_instead, only_for) = match argument {
        "show_as" => {
            let mut words = vec!["auto"];
            words.extend(&fits.show_as);
            (words, None, Vec::new())
        }
        "group_by" if drawn_as(&fits.grouped_as) => (fits.group_by.clone(), None, Vec::new()),
        "group_by" => (Vec::new(), None, fits.grouped_as.clone()),
        "measure" if drawn_as(&["figure"]) => (fits.measure.clone(), None, Vec::new()),
        "measure" => (Vec::new(), None, suiting(&["figure"])),
        "compare" if !drawn_as(&SINGLE) => (Vec::new(), None, suiting(&SINGLE)),
        "categories" if drawn_as(&SINGLE) && offered("formula", allowed) => {
            (Vec::new(), Some("formula"), Vec::new())
        }
        // A formula is the figure of a key figure or a progress bar of money
        // in and out (D-650); a chart draws none, so the refusal names the
        // ways that take it.
        "formula" if dataset.id == FORMULA_DATA && !drawn_as(&SINGLE) => {
            (Vec::new(), None, suiting(&SINGLE))
        }
        "target" if dataset.id == FORMULA_DATA && !drawn_as(&["progress"]) => {
            (Vec::new(), None, suiting(&["progress"]))
        }
        _ => return bad,
    };
    if words.is_empty() && use_instead.is_none() && only_for.is_empty() {
        return bad;
    }
    ToolError::NotForChart {
        argument,
        allowed: words,
        use_instead,
        only_for_show_as: only_for,
    }
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
/// With `editing` the call changes the saved chart `view_id` (D-681): every
/// other argument may be left out, and the item holds only those given,
/// with the words that mean a default (`none`, `total`, `auto`, `page`)
/// kept, so the change puts that setting back.
fn check(
    book: &Book,
    arguments: &Value,
    allowed: Permissions,
    editing: bool,
) -> Result<Value, ToolError> {
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
    let view_id = if editing {
        let id = given("view_id")
            .and_then(Value::as_str)
            .and_then(|text| uuid::Uuid::try_parse(text).ok())
            .ok_or(ToolError::BadArgument {
                argument: "view_id",
            })?;
        Some(id.to_string())
    } else {
        None
    };
    let title = match given("title") {
        None if editing => None,
        title => Some(views::title(
            title
                .and_then(Value::as_str)
                .ok_or(ToolError::BadArgument { argument: "title" })?,
        )?),
    };
    let data = word("data")?;
    let show_as = word("show_as")?;
    if !editing {
        data.ok_or(ToolError::BadArgument { argument: "data" })?;
        show_as.ok_or(ToolError::BadArgument {
            argument: "show_as",
        })?;
    }
    // A new chart leaves a default out; a change keeps the word, so the
    // setting goes back to its default.
    let unless_new = |word, default| unless_new(word, default, editing);
    let measure = word("measure")?;
    let group_by = word("group_by")?;
    let compare = unless_new(word("compare")?, NO_COMPARISON);
    let average = unless_new(word("average")?, TOTAL);
    let orientation = unless_new(word("orientation")?, AUTO);
    let period = match given("period") {
        None => None,
        // A period that moves with today (D-644, D-645) is not in the list.
        Some(Value::String(text)) if Rolling::read(text).is_some() => Some(text.clone()),
        Some(Value::String(_)) => unless_new(word("period")?, PAGE).map(str::to_owned),
        Some(Value::Object(range)) => Some(range_period(range)?),
        Some(_) => return Err(ToolError::BadArgument { argument: "period" }),
    };
    let categories = ids(given("categories"), "categories", MAX_VIEW_CATEGORIES)?;
    let accounts = ids(given("accounts"), "accounts", MAX_VIEW_ACCOUNTS)?;
    let mut series = Vec::new();
    for (argument, of_categories, _) in SERIES_LISTS {
        let max = if of_categories {
            MAX_VIEW_CATEGORIES
        } else {
            MAX_VIEW_ACCOUNTS
        };
        series.push(ids(given(argument), argument, max)?);
    }
    let formula = text(given("formula"), "formula")?;
    let target = text(given("target"), "target")?;
    // 5. Ids by the grid and the book.
    if !categories.is_empty() || !accounts.is_empty() || series.iter().any(|list| !list.is_empty())
    {
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
        let account = |id: &AccountId| kinds.get(id).is_some_and(|(kind, _)| kind.is_account());
        if !accounts.iter().all(account) {
            return Err(ToolError::UnknownAccount);
        }
        // A series' own list takes categories of its own kind only: money in
        // from a spending category would count nothing (D-646).
        for ((_, _, of_kind), list) in SERIES_LISTS.iter().zip(&series) {
            let fits = |id: &AccountId| match of_kind {
                Some(of_kind) => kinds
                    .get(id)
                    .is_some_and(|(kind, closed)| kind == of_kind && !closed),
                None => account(id),
            };
            if !list.iter().all(fits) {
                return Err(if of_kind.is_some() {
                    ToolError::UnknownCategory
                } else {
                    ToolError::UnknownAccount
                });
            }
        }
    }
    // A formula's names by the book (D-650), as the formula field checks
    // them: no figure of the period decides, only the names and the kinds.
    if let Some(formula) = formula {
        check_formula(book, formula, None, "formula")?;
        if let Some(target) = target {
            check_formula(book, target, Some(formula), "target")?;
        }
    } else if target.is_some() && !editing {
        // A change may give a new target to the saved chart's formula,
        // which the schema checks when the set is built.
        return Err(ToolError::BadArgument { argument: "target" });
    }
    // 6. A dimension the dataset doesn't apply is refused, never kept: a
    // chart of net worth with categories would look limited while every
    // category counts (M6-B29, D-350). A change that names no data is
    // checked against the saved chart's when the set is built.
    if let Some(view_id) = view_id {
        return Ok(edit_item(EditItem {
            view_id,
            title,
            data,
            show_as,
            words: [
                ("grouping", group_by),
                ("measure", measure),
                ("compare", compare),
                ("average", average),
                ("orientation", orientation),
                ("formula", formula),
                ("target", target),
            ],
            period,
            lists: [("categories", &categories), ("accounts", &accounts)]
                .into_iter()
                .chain(SERIES_LISTS.iter().map(|(key, _, _)| *key).zip(&series))
                .filter(|(key, _)| given(key).is_some())
                .collect(),
        }));
    }
    let (Some(title), Some(data), Some(show_as)) = (title, data, show_as) else {
        return Err(ToolError::BadArgument {
            argument: "arguments",
        });
    };
    let takes = kassensturz_report::dataset(data).map(|info| info.takes);
    if !categories.is_empty() && !takes.is_some_and(|takes| takes.categories) {
        return Err(ToolError::BadArgument {
            argument: "categories",
        });
    }
    if !accounts.is_empty() && !takes.is_some_and(|takes| takes.accounts) {
        return Err(ToolError::BadArgument {
            argument: "accounts",
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
        ("average", average),
        ("orientation", orientation),
        ("formula", formula),
        ("target", target),
    ] {
        if let Some(value) = value {
            item.insert(key.into(), json!(value));
        }
    }
    item.insert(
        "period".into(),
        json!(period.unwrap_or_else(|| default_period(data).to_owned())),
    );
    let lists = [("categories", &categories), ("accounts", &accounts)]
        .into_iter()
        .chain(SERIES_LISTS.iter().map(|(key, _, _)| *key).zip(&series));
    for (key, list) in lists {
        if !list.is_empty() {
            let list: Vec<String> = list.iter().map(ToString::to_string).collect();
            item.insert(key.into(), json!(list));
        }
    }
    Ok(Value::Object(item))
}

/// `word`, or `None` for a new chart's `default`, which is left out; a
/// change keeps it, so the setting goes back to its default (D-681).
fn unless_new<'a>(word: Option<&'a str>, default: &str, editing: bool) -> Option<&'a str> {
    word.filter(|word| editing || *word != default)
}

/// What a checked call that changes a saved chart named (D-681).
struct EditItem<'a> {
    view_id: String,
    title: Option<String>,
    data: Option<&'a str>,
    show_as: Option<&'a str>,
    words: [(&'static str, Option<&'a str>); 7],
    period: Option<String>,
    lists: Vec<(&'static str, &'a Vec<AccountId>)>,
}

/// The item of `edit_views` made of a checked call (D-681): the keys of
/// `new_views` for what the call named, and nothing for what it left out.
fn edit_item(edit: EditItem<'_>) -> Value {
    let mut item = Map::new();
    item.insert("view_id".into(), json!(edit.view_id));
    if let Some(title) = edit.title {
        item.insert("title".into(), json!(title));
    }
    if let Some(data) = edit.data {
        item.insert("dataset".into(), json!(data));
    }
    if let Some(show_as) = edit.show_as {
        let chart = SHOW_AS
            .iter()
            .find(|(word, _)| *word == show_as)
            .and_then(|(_, kind)| *kind)
            .map_or("auto", ComponentType::id);
        item.insert("chart".into(), json!(chart));
    }
    for (key, value) in edit.words {
        if let Some(value) = value {
            item.insert(key.into(), json!(value));
        }
    }
    if let Some(period) = edit.period {
        item.insert("period".into(), json!(period));
    }
    for (key, list) in edit.lists {
        let list: Vec<String> = list.iter().map(ToString::to_string).collect();
        item.insert(key.into(), json!(list));
    }
    Value::Object(item)
}

/// The period a dataset starts with: today's balances and budgets this
/// month, everything else the last twelve months (D-352).
pub(crate) fn default_period(dataset: &str) -> &'static str {
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

/// A formula's text: a string of one line, without hidden characters,
/// at most [`MAX_FORMULA_CHARS`] characters; `None` when left out.
fn text<'a>(
    value: Option<&'a Value>,
    argument: &'static str,
) -> Result<Option<&'a str>, ToolError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let bad = ToolError::BadFormula {
        argument,
        position: None,
        reason: "text",
    };
    let text = value.as_str().ok_or(ToolError::BadArgument { argument })?;
    if text.trim().is_empty()
        || text.chars().count() > MAX_FORMULA_CHARS
        || text.chars().any(|c| c.is_control() || hides_text(c))
    {
        return Err(bad);
    }
    Ok(Some(text))
}

/// Checks a formula the model wrote against `book` (D-650): it is read in
/// the form the book stores and its names are looked up, the way the
/// formula field and the card do it. A target is also checked against the
/// formula it measures. A mistake is answered with a fixed reason and the
/// character it is at, never with the model's text or the book's names
/// (D-353); a name of a group is unknown, since the AI knows no groups
/// (D-306). What depends on the figures (a division by zero, two
/// currencies, a target of zero) is no mistake of the formula: the preview
/// shows it to the user.
fn check_formula(
    book: &Book,
    text: &str,
    measured: Option<&str>,
    argument: &'static str,
) -> Result<(), ToolError> {
    let refused = |error: ReportError| -> Result<(), ToolError> {
        let (position, reason) = match error {
            ReportError::FormulaSyntax { position, .. } => (Some(position), "syntax"),
            ReportError::FormulaUnknownName { position, .. } => (Some(position), "unknown-name"),
            ReportError::FormulaAmbiguousName { position, by, .. } => (
                Some(position),
                if by == "kind" {
                    "ambiguous-kind"
                } else {
                    "ambiguous-path"
                },
            ),
            ReportError::FormulaUnknownFunction { position, .. } => {
                (Some(position), "unknown-function")
            }
            ReportError::FormulaArguments { position, .. } => (Some(position), "arguments"),
            ReportError::FormulaKinds { position, .. } => (Some(position), "kinds"),
            ReportError::FormulaTooBig { .. } => (None, "too-big"),
            ReportError::FormulaTarget { reason: "kinds" } => (None, "target-kinds"),
            ReportError::FormulaNoValue { .. }
            | ReportError::FormulaTooLarge { .. }
            | ReportError::FormulaMixedCurrencies { .. }
            | ReportError::FormulaTarget { .. } => return Ok(()),
            ReportError::Store(error) => return Err(ToolError::Store(error)),
            _ => (None, "invalid"),
        };
        Err(ToolError::BadFormula {
            argument,
            position,
            reason,
        })
    };
    // One day with no money is enough: the names are looked up before any
    // figure is read, and the kinds don't depend on the amounts.
    let day = Date::constant(2000, 1, 1);
    let period = Period::new(day, day).map_err(|_| ToolError::BadPeriod)?;
    let currency = CurrencyCode::new("EUR").map_err(|_| ToolError::BadPeriod)?;
    let work = |text: &str| -> Result<FormulaResult, ReportError> {
        let formula = Formula::parse(text)?;
        let source = Source::read(book, formula.span(period)?)?;
        formula.evaluate(&source, period, &Filter::default(), currency)
    };
    let result = match work(text) {
        Ok(result) => result,
        Err(error) => return refused(error),
    };
    let group = result
        .counted
        .iter()
        .any(|counted| matches!(counted.of, Term::Member(GroupMember::Group(_))));
    if group {
        return Err(ToolError::BadFormula {
            argument,
            position: None,
            reason: "unknown-name",
        });
    }
    if let Some(measured) = measured {
        let figure = match work(measured) {
            Ok(figure) => figure,
            Err(error) => return refused(error),
        };
        if let Err(error) = figure.value.fill(result.value) {
            return refused(error);
        }
    }
    Ok(())
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

/// The most saved charts `list_views` lists.
pub(crate) const MAX_LISTED_VIEWS: usize = 100;

/// `list_views`: the charts the user saved under Statistics, in the order
/// of their list, each with its ID and its settings in the words of
/// `propose_view` (D-681), so a model can change one with `view_id` or
/// delete it. The categories, accounts and formulas a chart is limited to
/// are listed only as the grid lets the model read them (D-252, D-650);
/// otherwise `not_shown` names them. A chart this version can't read has
/// `"unreadable": true`.
pub(crate) fn list_views(
    book: &Book,
    w: &mut crate::writer::Writer<'_>,
    allowed: Permissions,
) -> Result<Value, ToolError> {
    let views = book.views(Some(kassensturz_core::ViewKind::View))?;
    let mut rows = Vec::new();
    for view in views.iter().take(MAX_LISTED_VIEWS) {
        let mut row = Map::new();
        row.insert("view_id".into(), json!(view.id.to_string()));
        row.insert("title".into(), w.text(&view.name));
        let read = kassensturz_report::Document::read(&view.document)
            .ok()
            .and_then(|document| match document.entries.first().map(|e| &e.item) {
                Some(kassensturz_report::Item::Component(component)) => {
                    Some((document.period.clone(), component.clone()))
                }
                _ => None,
            });
        let Some((period, component)) = read else {
            row.insert("unreadable".into(), Value::Bool(true));
            rows.push(Value::Object(row));
            continue;
        };
        row.insert("data".into(), json!(component.dataset().id));
        let show_as = SHOW_AS
            .iter()
            .find(|(_, kind)| *kind == Some(component.kind()))
            .map_or("other", |(word, _)| *word);
        row.insert("show_as".into(), json!(show_as));
        row.insert(
            "period".into(),
            json!(period.unwrap_or_else(|| default_period(component.dataset().id).to_owned())),
        );
        let mut not_shown = Vec::new();
        for (key, argument) in [
            ("grouping", "group_by"),
            ("measure", "measure"),
            ("compare", "compare"),
            ("average", "average"),
            ("orientation", "orientation"),
        ] {
            if let Some(kassensturz_report::Setting::Word(word)) = component.get(key)
                && !word.is_empty()
                && !(key == "measure" && word == FORMULA_MEASURE)
            {
                row.insert(argument.into(), json!(word));
            }
        }
        let lists = ["categories", "accounts"]
            .into_iter()
            .chain(SERIES_LISTS.iter().map(|(key, _, _)| *key));
        for key in lists {
            if let Some(kassensturz_report::Setting::Ids(ids)) = component.get(key)
                && !ids.is_empty()
            {
                if offered(key, allowed) {
                    row.insert(key.into(), json!(ids));
                } else {
                    not_shown.push(key);
                }
            }
        }
        for key in ["formula", "target"] {
            if let Some(kassensturz_report::Setting::Text(text)) = component.get(key)
                && !text.is_empty()
            {
                if offered(key, allowed) {
                    row.insert(key.into(), w.text(text));
                } else {
                    not_shown.push(key);
                }
            }
        }
        if !not_shown.is_empty() {
            row.insert("not_shown".into(), json!(not_shown));
        }
        rows.push(Value::Object(row));
    }
    let mut result = Map::new();
    result.insert("charts".into(), Value::Array(rows));
    crate::read::left_out(&mut result, w, views.len(), MAX_LISTED_VIEWS);
    Ok(Value::Object(result))
}
