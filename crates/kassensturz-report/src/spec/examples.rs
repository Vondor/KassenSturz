// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Ready chart descriptions for the help page and the "Insert an example"
//! menu of a custom chart (US4.11, #700 part A, D-720). They are the one
//! place the examples are written: the help page shows them, the menu puts
//! one into the description field, and a test checks that each is read
//! without a part that is not supported and draws from the book. They name
//! no person, shop or bank, only the book's own fields. The AI's tool text
//! may later quote them (#700 part B, D-709).

/// One example: a fixed key, which the screen translates into a name, and
/// the description itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpecExample {
    /// The fixed key, e.g. `balance-and-flows`.
    pub key: &'static str,
    /// The data name it uses, as `data.name` writes it.
    pub dataset: &'static str,
    /// The description, as a person would paste it.
    pub text: &'static str,
}

/// The examples, in the order the menu and the help page list them. The
/// first is the bars-plus-lines chart the help was asked for.
pub const SPEC_EXAMPLES: &[SpecExample] = &[
    SpecExample {
        key: "balance-and-flows",
        dataset: "cash_flow_by_month",
        text: r#"{
  "title": "Balance, money in and money out",
  "data": { "name": "cash_flow_by_month" },
  "layer": [
    { "mark": "bar",
      "encoding": {
        "x": { "field": "month", "type": "temporal", "timeUnit": "yearmonth" },
        "y": { "field": "end", "type": "quantitative", "title": "Balance at the end" } } },
    { "mark": { "type": "line", "color": "bluish-green" },
      "encoding": {
        "x": { "field": "month", "type": "temporal", "timeUnit": "yearmonth" },
        "y": { "field": "money_in", "type": "quantitative", "title": "Money in" } } },
    { "mark": { "type": "line", "color": "vermillion" },
      "encoding": {
        "x": { "field": "month", "type": "temporal", "timeUnit": "yearmonth" },
        "y": { "field": "money_out", "type": "quantitative", "title": "Money out" } } }
  ]
}
"#,
    },
    SpecExample {
        key: "left-each-month",
        dataset: "income_expense_by_month",
        text: r#"{
  "title": "What is left each month",
  "data": { "name": "income_expense_by_month" },
  "mark": "bar",
  "encoding": {
    "x": { "field": "month", "type": "temporal", "timeUnit": "yearmonth" },
    "y": { "field": "net", "type": "quantitative", "title": "Left over" }
  }
}
"#,
    },
    SpecExample {
        key: "spending-ring",
        dataset: "spending_by_category",
        text: r#"{
  "title": "Where the money went",
  "data": { "name": "spending_by_category" },
  "mark": { "type": "arc", "innerRadius": 60 },
  "encoding": {
    "theta": { "field": "total", "type": "quantitative" },
    "color": { "field": "top_level", "type": "nominal" }
  }
}
"#,
    },
    SpecExample {
        key: "worth-and-average",
        dataset: "net_worth_over_time",
        text: r#"{
  "title": "What you are worth, with its average",
  "data": { "name": "net_worth_over_time" },
  "layer": [
    { "mark": "line",
      "encoding": {
        "x": { "field": "month", "type": "temporal", "timeUnit": "yearmonth" },
        "y": { "field": "net_worth", "type": "quantitative", "title": "What you are worth" } } },
    { "mark": { "type": "rule", "color": "grey" },
      "encoding": {
        "y": { "field": "net_worth", "type": "quantitative", "aggregate": "mean", "title": "Average" } } }
  ]
}
"#,
    },
];
