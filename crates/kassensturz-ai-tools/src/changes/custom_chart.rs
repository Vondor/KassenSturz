// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A custom chart's description written by the AI (#700 part B, D-709,
//! D-729, threat T-59).
//!
//! The description is untrusted text from the model. It is checked here
//! before anything is built, and again when the change set is built from
//! it, so a set can't hold one that wasn't: it is at most
//! [`MAX_AI_DESCRIPTION_BYTES`], no text in it hides or controls anything,
//! it is read by the same parser a pasted description goes through and
//! must be **fully** supported (the AI can't choose "Draw it without
//! these", D-508), and it names one of [`DESCRIPTION_DATA`]. A refusal
//! names what is wrong by KassenSturz's own words only, never by a key or
//! a value of the model's text (X-8).
//!
//! The model never sees what the chart draws: KassenSturz computes every
//! figure from the book (AGENTS.md §8).

use kassensturz_report::ReportError;
use kassensturz_report::spec::{SPEC_DATASETS, SPEC_EXAMPLES};
use serde_json::Value;

use crate::error::ToolError;
use crate::text::hides_text;

/// The word of `show_as` for a chart drawn from a description.
pub const SHOW_AS_CUSTOM: &str = "custom";

/// The longest description the AI may write, in bytes. A person's may be
/// four times as long (D-281); every example of the help is under 1 KiB.
pub const MAX_AI_DESCRIPTION_BYTES: usize = 8 * 1024;

/// The data a description by the AI may name: those it may name in
/// `propose_view` (D-354), without `budget_utilization`, whose budgets are
/// chosen in the Properties panel (Q-353), and `category_over_time`, which
/// stays out (D-306). A test checks the list against the catalog.
pub const DESCRIPTION_DATA: [&str; 5] = [
    "spending_by_category",
    "income_expense_by_month",
    "account_balances",
    "net_worth_over_time",
    "cash_flow_by_month",
];

/// The parts of a description a refusal may name: the top-level keys and
/// the channels of the subset (`docs/REPORT_DESIGNER_M6E.md`). A path is
/// named by the deepest of its keys that is one of them, so the answer
/// never repeats a key the model made up.
const PARTS: [&str; 23] = [
    "title",
    "data",
    "mark",
    "encoding",
    "layer",
    "transform",
    "width",
    "height",
    "config",
    "x",
    "y",
    "color",
    "size",
    "theta",
    "text",
    "tooltip",
    "field",
    "type",
    "aggregate",
    "timeUnit",
    "sort",
    "filter",
    "name",
];

/// The data name the example of the tool text uses.
const EXAMPLE_KEY: &str = "balance-and-flows";

/// What a description the AI wrote names and says, once it was checked.
#[derive(Debug)]
pub(crate) struct Checked {
    /// The data it shows, one of [`DESCRIPTION_DATA`].
    pub(crate) dataset: &'static str,
}

/// Checks a description the AI wrote, as the module comment says.
pub(crate) fn check(text: &str) -> Result<Checked, ToolError> {
    if text.len() > MAX_AI_DESCRIPTION_BYTES {
        return Err(bad("too-big").because("bytes"));
    }
    let read = kassensturz_report::spec::read(text).map_err(|error| match error {
        ReportError::SpecTooBig { limit } => bad("too-big").because(limit),
        ReportError::SpecInvalid { reason, path } => bad(reason).placed(&path),
        _ => bad("not-readable"),
    })?;
    // The parser read it, so it is JSON.
    let value: Value = serde_json::from_str(text).map_err(|_| bad("not-readable"))?;
    if strings(&value).any(|text| text.chars().any(|c| c.is_control() || hides_text(c))) {
        return Err(bad("hidden-character"));
    }
    if let Some(first) = read.unsupported().first() {
        return Err(bad("not-supported")
            .because(first.reason())
            .placed(first.path())
            .counted(read.unsupported().len()));
    }
    let dataset = read
        .chart()
        .map(|chart| chart.dataset().id())
        .ok_or_else(|| bad("not-readable"))?;
    let dataset = DESCRIPTION_DATA
        .iter()
        .copied()
        .find(|id| *id == dataset)
        .ok_or(ToolError::BadDescription {
            reason: "data-not-offered",
            why: None,
            part: Some("data"),
            layer: None,
            count: None,
        })?;
    Ok(Checked { dataset })
}

/// The texts of a description that are shown as words: every `title` at
/// any depth (the chart's, an axis', a legend's). They are the model's
/// text, so the chat checks their numbers like a chart's title (D-188).
pub fn description_titles(text: &str) -> Vec<String> {
    fn walk(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                for (key, value) in map {
                    match value {
                        Value::String(text) if key == "title" => out.push(text.clone()),
                        // A title may be an object with its own `text`.
                        Value::Object(inner) if key == "title" => {
                            if let Some(Value::String(text)) = inner.get("text") {
                                out.push(text.clone());
                            }
                        }
                        other => walk(other, out),
                    }
                }
            }
            Value::Array(items) => items.iter().for_each(|item| walk(item, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    if let Ok(value) = serde_json::from_str::<Value>(text) {
        walk(&value, &mut out);
    }
    out
}

/// Every string of `value`, keys and values.
fn strings(value: &Value) -> Box<dyn Iterator<Item = &str> + '_> {
    match value {
        Value::String(text) => Box::new(std::iter::once(text.as_str())),
        Value::Array(items) => Box::new(items.iter().flat_map(strings)),
        Value::Object(map) => Box::new(
            map.iter()
                .flat_map(|(key, value)| std::iter::once(key.as_str()).chain(strings(value))),
        ),
        _ => Box::new(std::iter::empty()),
    }
}

/// The help of the argument `description`: the data names with their
/// fields, from [`SPEC_DATASETS`], and one ready example from the help page
/// (part A, D-720), so the text can never drift from what is read.
pub(crate) fn description_help() -> String {
    let mut help = String::from(
        "Only with show_as custom: the chart's description, a JSON object in a small subset \
         of Vega-Lite, as text. It names its data in data.name (no url, values or datasets); \
         marks bar, line, area, point, rule, arc and text; channels x, y, color, size, theta, \
         text and tooltip; at most 4 layers and 8 encodings; transform holds only filters \
         {\"filter\": {\"field\": ..., \"equal\", \"oneOf\" or \"range\": ...}}. No expressions, other \
         transforms, facet, repeat or concat. Every part \
         must be supported, or the call is refused with the reason and the part. Write titles \
         in the user's language and no numbers as facts in them. The data and their fields:",
    );
    for id in DESCRIPTION_DATA {
        if let Some(dataset) = SPEC_DATASETS.iter().find(|dataset| dataset.id() == id) {
            let fields: Vec<&str> = dataset.fields().iter().map(|field| field.name()).collect();
            help.push_str(&format!("\n- {id}: {}", fields.join(", ")));
        }
    }
    if let Some(example) = SPEC_EXAMPLES
        .iter()
        .find(|example| example.key == EXAMPLE_KEY)
    {
        let compact: String = serde_json::from_str::<Value>(example.text)
            .map_or_else(|_| example.text.to_owned(), |value| value.to_string());
        help.push_str("\nExample, balance as bars with money in and out as lines: ");
        help.push_str(&compact);
    }
    help
}

/// A refusal with `reason` and nothing else yet.
fn bad(reason: &'static str) -> ToolError {
    ToolError::BadDescription {
        reason,
        why: None,
        part: None,
        layer: None,
        count: None,
    }
}

/// Builder steps for [`ToolError::BadDescription`].
trait Refusal {
    fn because(self, why: &'static str) -> Self;
    fn placed(self, path: &str) -> Self;
    fn counted(self, count: usize) -> Self;
}

impl Refusal for ToolError {
    fn because(mut self, why_word: &'static str) -> Self {
        if let ToolError::BadDescription { why, .. } = &mut self {
            *why = Some(why_word);
        }
        self
    }

    fn placed(mut self, path: &str) -> Self {
        if let ToolError::BadDescription { part, layer, .. } = &mut self {
            let (named, in_layer) = place(path);
            *part = named;
            *layer = in_layer;
        }
        self
    }

    fn counted(mut self, many: usize) -> Self {
        if let ToolError::BadDescription { count, .. } = &mut self {
            *count = Some(many);
        }
        self
    }
}

/// Where `path` (JSON-path text such as `layer[1].encoding.y.aggregate`)
/// is, in KassenSturz's own words: the deepest key that is one of
/// [`PARTS`], and the layer (counted from 1) when it is inside one.
fn place(path: &str) -> (Option<&'static str>, Option<usize>) {
    let mut part = None;
    let mut layer = None;
    for segment in path.split('.') {
        let (key, index) = match segment.split_once('[') {
            Some((key, rest)) => (key, rest.trim_end_matches(']').parse::<usize>().ok()),
            None => (segment, None),
        };
        if let Some(known) = PARTS.iter().find(|known| **known == key) {
            part = Some(*known);
            if key == "layer" && layer.is_none() {
                layer = index.map(|index| index + 1);
            }
        }
    }
    (part, layer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kassensturz_report::BUDGET_UTILIZATION;

    #[test]
    fn the_data_list_is_the_catalog_s_for_the_ai_without_the_two_left_out() {
        let expected: Vec<&str> = SPEC_DATASETS
            .iter()
            .map(|dataset| dataset.id())
            .filter(|id| kassensturz_report::dataset(id).is_some_and(|info| info.for_ai))
            .filter(|id| *id != BUDGET_UTILIZATION)
            .collect();
        assert_eq!(expected, DESCRIPTION_DATA);
    }

    #[test]
    fn every_example_of_the_help_that_names_offered_data_passes() {
        let mut checked = 0;
        for example in SPEC_EXAMPLES {
            if DESCRIPTION_DATA.contains(&example.dataset) {
                let got = check(example.text).expect(example.key);
                assert_eq!(got.dataset, example.dataset);
                checked += 1;
            }
        }
        assert_eq!(
            checked,
            SPEC_EXAMPLES.len(),
            "every example names offered data"
        );
    }

    #[test]
    fn a_place_is_named_only_by_the_subset_s_own_words() {
        assert_eq!(
            place("layer[1].encoding.y.aggregate"),
            (Some("aggregate"), Some(2))
        );
        assert_eq!(place("transform[0].calculate"), (Some("transform"), None));
        assert_eq!(place("encoding.made_up_key"), (Some("encoding"), None));
        assert_eq!(place("made_up"), (None, None));
    }

    #[test]
    fn the_help_holds_every_offered_data_and_the_example() {
        let help = description_help();
        for id in DESCRIPTION_DATA {
            assert!(help.contains(&format!("- {id}: ")), "{id}");
        }
        assert!(help.contains("\"cash_flow_by_month\""));
        assert!(!help.contains("category_over_time"));
    }

    #[test]
    fn titles_are_found_at_every_depth() {
        let text = r#"{"title":"A 12","data":{"name":"net_worth_over_time"},"mark":"line",
            "encoding":{"x":{"field":"month","type":"temporal","title":"B"},
            "y":{"field":"net_worth","type":"quantitative","title":{"text":"C"}}}}"#;
        let mut found = description_titles(text);
        found.sort();
        assert_eq!(found, ["A 12", "B", "C"]);
    }
}
