// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the parser (M6e.1). Every description here is written by hand
//! for these tests from the subset in `docs/REPORT_DESIGNER.md` section 5,
//! never copied from Vega-Lite's examples (D-280).

use kassensturz_core::Report;
use proptest::prelude::*;

use super::parse::takes;
use super::*;
use crate::ReportError;
use crate::tests::date;

/// A bar chart of the net amount per month, the smallest full description.
const BAR: &str = r#"{
  "$schema": "https://vega.github.io/schema/vega-lite/v6.json",
  "data": {"name": "income_expense_by_month"},
  "mark": "bar",
  "encoding": {
    "x": {"field": "month", "type": "temporal", "timeUnit": "yearmonth"},
    "y": {"field": "net", "type": "quantitative"}
  }
}"#;

fn ok(text: &str) -> Read {
    match read(text) {
        Ok(read) => read,
        Err(error) => panic!("refused: {error:?}\n{text}"),
    }
}

fn chart(text: &str) -> ChartSpec {
    let read = ok(text);
    assert!(read.unsupported().is_empty(), "{:?}", read.unsupported());
    read.chart().cloned().expect("a chart")
}

/// The code and its first parameter, as the UI reads them.
fn refused(text: &str) -> (String, String, String) {
    let error = match read(text) {
        Ok(read) => panic!("read: {read:?}\n{text}"),
        Err(error) => error,
    };
    let report = error.report();
    let words: Vec<String> = report
        .params
        .iter()
        .map(|(_, value)| format!("{value:?}"))
        .collect();
    let first = words.first().cloned().unwrap_or_default();
    let path = match error {
        ReportError::SpecInvalid { path, .. } => path,
        _ => String::new(),
    };
    (report.code.to_owned(), first, path)
}

fn invalid(text: &str) -> (&'static str, String) {
    match read(text) {
        Err(ReportError::SpecInvalid { reason, path }) => (reason, path),
        other => panic!("not spec-invalid: {other:?}\n{text}"),
    }
}

fn too_big(text: &str) -> &'static str {
    match read(text) {
        Err(ReportError::SpecTooBig { limit }) => limit,
        other => panic!("not spec-too-big: {other:?}\n{text}"),
    }
}

fn listed(read: &Read) -> Vec<(&str, &str)> {
    read.unsupported()
        .iter()
        .map(|entry| (entry.path(), entry.reason()))
        .collect()
}

/// A description of `dataset` with a bar of `x` over `y` and `rest` added as
/// more top-level keys (a leading comma included).
fn with(rest: &str) -> String {
    format!(
        r#"{{"data": {{"name": "income_expense_by_month"}}, "mark": "bar",
            "encoding": {{"x": {{"field": "month", "type": "temporal"}},
                          "y": {{"field": "net", "type": "quantitative"}}}}{rest}}}"#
    )
}

fn filter(test: &str) -> String {
    with(&format!(r#", "transform": [{{"filter": {test}}}]"#))
}

#[test]
fn a_plain_bar_chart_is_read_into_the_model() {
    let spec = chart(BAR);
    assert_eq!(spec.dataset().id(), "income_expense_by_month");
    let [layer] = spec.layers() else {
        panic!("one layer")
    };
    assert_eq!(layer.mark().kind(), MarkKind::Bar);
    let [x, y] = layer.encodings() else {
        panic!("two encodings")
    };
    assert_eq!(
        (x.channel(), x.field().name(), x.kind(), x.time_unit()),
        (
            Channel::X,
            "month",
            FieldType::Temporal,
            Some(TimeUnit::YearMonth)
        )
    );
    assert_eq!(
        (y.channel(), y.field().name(), y.kind()),
        (Channel::Y, "net", FieldType::Quantitative)
    );
    assert!(spec.filters().is_empty() && spec.palette().is_empty() && spec.size().is_none());
}

#[test]
fn the_codes_carry_their_parameters() {
    assert_eq!(
        refused(&with(r#", "width": 12.5"#)),
        (
            "report.spec-invalid".to_owned(),
            r#"Keyword("fraction-number")"#.to_owned(),
            "width".to_owned()
        )
    );
    assert_eq!(
        refused(&" ".repeat(MAX_SPEC_BYTES + 1)).0,
        "report.spec-too-big"
    );
}

#[test]
fn the_schema_is_checked_as_text_and_never_fetched() {
    assert_eq!(
        invalid(&with(
            r#", "$schema": "https://example.org/vega-lite/v6.json""#
        )),
        ("bad-schema", "$schema".to_owned())
    );
    assert_eq!(invalid(&with(r#", "$schema": 6"#)).0, "bad-schema");
}

#[test]
fn size_is_checked_before_anything_else() {
    // Not JSON at all, but too big first.
    let text = format!("{{{}", "x".repeat(MAX_SPEC_BYTES));
    assert_eq!(too_big(&text), "bytes");
    // Exactly the limit is read.
    let padded = format!("{BAR}{}", " ".repeat(MAX_SPEC_BYTES - BAR.len()));
    assert_eq!(padded.len(), MAX_SPEC_BYTES);
    assert!(read(&padded).is_ok());
}

#[test]
fn nesting_deeper_than_ten_is_refused_before_parsing() {
    let nested = |depth: usize| {
        let mut text = with(r#", "description": "#);
        text.pop();
        // The top-level object is the first bracket.
        format!("{text}{}{}}}", "[".repeat(depth - 1), "]".repeat(depth - 1))
    };
    assert_eq!(too_big(&nested(11)), "depth");
    // Ten brackets deep is fine for the scan; a list as a description is a
    // value outside the subset.
    let read = ok(&nested(10));
    assert_eq!(listed(&read), [("description", "value")]);
    // Even far too deep for `serde_json` the scan stops first.
    assert_eq!(too_big(&"[".repeat(5000)), "depth");
}

#[test]
fn a_key_written_twice_is_refused_with_its_path() {
    let text = r#"{"data": {"name": "income_expense_by_month"}, "mark": "bar",
        "encoding": {"x": {"field": "month", "type": "temporal"},
                     "y": {"field": "net", "field": "income", "type": "quantitative"}}}"#;
    assert_eq!(
        invalid(text),
        ("duplicate-key", "encoding.y.field".to_owned())
    );
    // Escapes are decoded first, so these are the same key.
    let text = r#"{"mark": "bar", "mark": "line"}"#;
    assert_eq!(invalid(text), ("duplicate-key", "mark".to_owned()));
    let text = r#"{"layer": [{"mark": "bar"}, {"mark": "line", "mark": "bar"}]}"#;
    assert_eq!(invalid(text), ("duplicate-key", "layer[1].mark".to_owned()));
}

#[test]
fn text_that_is_not_one_json_object_is_refused() {
    assert_eq!(invalid("").0, "not-json");
    assert_eq!(invalid("{").0, "not-json");
    assert_eq!(invalid(r#"{"a": 1} {"b": 2}"#).0, "not-json");
    assert_eq!(invalid("[]").0, "not-an-object");
    assert_eq!(invalid("\"bar\"").0, "not-an-object");
    // An unclosed key ending in a character of more than one byte.
    assert_eq!(invalid("{\"a\u{e9}").0, "not-json");
    assert_eq!(invalid("{\"a\u{e9}\\").0, "not-json");
}

#[test]
fn unsupported_parts_are_listed_in_the_order_of_the_text() {
    let text = r#"{
      "padding": 5,
      "data": {"url": "https://example.org/data.csv", "name": "income_expense_by_month"},
      "transform": [
        {"filter": {"field": "net", "range": [0, 500]}},
        {"calculate": "datum.net * 2", "as": "double"},
        {"as": ["a"], "fold": ["income", "expenses"]},
        {"filter": "datum.net > 0"}
      ],
      "mark": {"type": "bar", "cornerRadius": 4},
      "encoding": {
        "x": {"field": "month", "type": "temporal", "axis": {"format": "%b"}},
        "y": {"field": "net", "type": "quantitative"},
        "opacity": {"value": 0.5},
        "size": {"field": "income", "type": "quantitative"}
      },
      "selection": {}
    }"#;
    let read = ok(text);
    assert_eq!(
        listed(&read),
        [
            ("padding", "option"),
            ("data.url", "data-source"),
            ("transform[1].calculate", "expression"),
            ("transform[2].fold", "transform"),
            ("transform[3].filter", "expression"),
            ("mark.cornerRadius", "option"),
            ("encoding.x.axis", "option"),
            ("encoding.opacity", "channel"),
            ("encoding.size", "channel"),
            ("selection", "composition"),
        ]
    );
    // What is left can be drawn, without the listed parts.
    let spec = read.chart().expect("a chart");
    assert_eq!(spec.filters().len(), 1);
    assert_eq!(spec.layers()[0].encodings().len(), 2);
    assert!(read.why_not().is_none());
}

#[test]
fn data_from_anywhere_but_the_book_leaves_nothing_to_draw() {
    let text = r#"{"data": {"url": "data.json"}, "mark": "bar",
        "encoding": {"x": {"field": "month", "type": "temporal"},
                     "y": {"field": "net", "type": "quantitative"}}}"#;
    let read = ok(text);
    assert_eq!(listed(&read), [("data.url", "data-source")]);
    assert!(read.chart().is_none());
    match read.why_not() {
        Some(ReportError::SpecInvalid { reason, path }) => {
            assert_eq!((reason, path.as_str()), ("missing", "data.name"));
        }
        other => panic!("{other:?}"),
    }
    let values = r#"{"data": {"values": [{"a": 1}]}, "datasets": {"b": []}, "mark": "bar"}"#;
    assert_eq!(
        listed(&ok(values)),
        [("data.values", "data-source"), ("datasets", "data-source")]
    );
}

#[test]
fn a_fully_supported_description_that_cannot_be_drawn_is_refused() {
    assert_eq!(
        invalid(r#"{"mark": "bar"}"#),
        ("missing", "data.name".to_owned())
    );
    let no_mark = r#"{"data": {"name": "spending_by_category"}}"#;
    assert_eq!(invalid(no_mark), ("missing", "mark".to_owned()));
    let no_y = r#"{"data": {"name": "spending_by_category"}, "mark": "bar",
        "encoding": {"x": {"field": "category", "type": "nominal"}}}"#;
    assert_eq!(invalid(no_y), ("missing", "encoding.y".to_owned()));
    let no_type = r#"{"data": {"name": "spending_by_category"}, "mark": "bar",
        "encoding": {"x": {"field": "category"}}}"#;
    assert_eq!(invalid(no_type), ("missing", "encoding.x.type".to_owned()));
    assert_eq!(
        invalid(r#"{"mark": {"tooltip": true}}"#),
        ("missing", "mark.type".to_owned())
    );
}

#[test]
fn datasets_and_fields_must_exist() {
    let text = BAR.replace("income_expense_by_month", "transactions");
    assert_eq!(invalid(&text), ("unknown-dataset", "data.name".to_owned()));
    let text = BAR.replace(r#""net""#, r#""profit""#);
    assert_eq!(
        invalid(&text),
        ("unknown-field", "encoding.y.field".to_owned())
    );
    // Every dataset of the catalog can be named.
    for dataset in SPEC_DATASETS {
        let first = dataset.fields()[0].name();
        let text = format!(
            r#"{{"data": {{"name": "{}"}}, "mark": "arc",
                "encoding": {{"theta": {{"aggregate": "count", "type": "quantitative"}},
                              "color": {{"field": "{first}", "type": "nominal"}}}}}}"#,
            dataset.id()
        );
        assert_eq!(chart(&text).dataset().id(), dataset.id());
    }
}

#[test]
fn types_must_suit_the_field() {
    let text = BAR.replace(r#""quantitative""#, r#""temporal""#);
    assert_eq!(invalid(&text), ("type-mismatch", "encoding.y".to_owned()));
    let percent = r#"{"data": {"name": "budget_utilization"}, "mark": "bar",
        "encoding": {"x": {"field": "category", "type": "nominal"},
                     "y": {"field": "used", "type": "quantitative", "aggregate": "sum"}}}"#;
    assert_eq!(invalid(percent), ("bad-aggregate", "encoding.y".to_owned()));
    let most = percent.replace(r#""sum""#, r#""max""#);
    let spec = chart(&most);
    assert_eq!(
        spec.layers()[0].encodings()[1].aggregate(),
        Some(Aggregate::Max)
    );
}

#[test]
fn a_count_without_a_field_counts_rows() {
    let text = r#"{"data": {"name": "account_balances"}, "mark": "bar",
        "encoding": {"x": {"field": "owed", "type": "nominal"},
                     "y": {"aggregate": "count"}}}"#;
    let spec = chart(text);
    let y = &spec.layers()[0].encodings()[1];
    assert_eq!(
        (y.aggregate(), y.kind(), y.field().name()),
        (Some(Aggregate::Count), FieldType::Quantitative, "account")
    );
}

#[test]
fn decimals_in_filters_are_read_from_their_digits() {
    let spec = chart(&filter(r#"{"field": "net", "equal": "12.50"}"#));
    let FilterTest::Equal(SpecValue::Number(number)) = spec.filters()[0].test() else {
        panic!("a number")
    };
    assert_eq!((number.digits(), number.places()), (1250, 2));
    let spec = chart(&filter(r#"{"field": "net", "range": [-500, null]}"#));
    assert_eq!(
        spec.filters()[0].test(),
        &FilterTest::Range {
            from: Some(SpecValue::Number(SpecNumber::whole(-500))),
            to: None
        }
    );
    for number in ["12.5", "1e3", "-0.0", "1E-2", "18446744073709551616"] {
        let text = filter(&format!(r#"{{"field": "net", "equal": {number}}}"#));
        assert_eq!(
            invalid(&text),
            ("fraction-number", "transform[0].filter.equal".to_owned()),
            "{number}"
        );
    }
    let text = filter(r#"{"field": "net", "equal": 9223372036854775808}"#);
    assert_eq!(invalid(&text).0, "bad-number");
    let text = filter(r#"{"field": "net", "equal": "12,50"}"#);
    assert_eq!(invalid(&text).0, "bad-number");
}

#[test]
fn filters_read_days_text_and_yes_or_no() {
    let spec = chart(&filter(
        r#"{"field": "month", "range": ["2026-01-01", "2026-06-01"]}"#,
    ));
    assert_eq!(
        spec.filters()[0].test(),
        &FilterTest::Range {
            from: Some(SpecValue::Day(date(2026, 1, 1))),
            to: Some(SpecValue::Day(date(2026, 6, 1)))
        }
    );
    let text = filter(r#"{"field": "month", "equal": "2026-13-01"}"#);
    assert_eq!(
        invalid(&text),
        ("bad-date", "transform[0].filter.equal".to_owned())
    );
    let text = filter(r#"{"field": "month", "equal": "1 May 2026"}"#);
    assert_eq!(invalid(&text).0, "bad-date");
    let text = filter(r#"{"field": "month", "equal": {"year": 2026, "month": 5}}"#);
    let read = ok(&text);
    assert_eq!(listed(&read), [("transform[0].filter.equal", "value")]);
    let yes_no = |test: &str| {
        format!(
            r#"{{"data": {{"name": "account_balances"}}, "mark": "bar",
                "encoding": {{"x": {{"field": "account", "type": "nominal"}},
                              "y": {{"field": "balance", "type": "quantitative"}}}},
                "transform": [{{"filter": {{"field": "closed", {test}}}}}]}}"#
        )
    };
    let spec = chart(&yes_no(r#""equal": false"#));
    assert_eq!(
        spec.filters()[0].test(),
        &FilterTest::Equal(SpecValue::YesNo(false))
    );
    assert_eq!(invalid(&yes_no(r#""equal": "no""#)).0, "type-mismatch");
    assert_eq!(
        invalid(&yes_no(r#""range": [false, true]"#)).0,
        "type-mismatch"
    );
    let text = filter(r#"{"field": "currency", "oneOf": ["EUR", "USD"]}"#);
    let FilterTest::OneOf(values) = chart(&text).filters()[0].test().clone() else {
        panic!("one of")
    };
    assert_eq!(values.len(), 2);
}

#[test]
fn a_filter_with_an_unsupported_part_is_left_out_whole() {
    let read = ok(&filter(r#"{"field": "net", "gt": 0}"#));
    assert_eq!(listed(&read), [("transform[0].filter.gt", "option")]);
    assert!(read.chart().expect("a chart").filters().is_empty());
    let read = ok(&filter(r#"{"param": "brush"}"#));
    assert_eq!(
        listed(&read),
        [("transform[0].filter.param", "composition")]
    );
    assert_eq!(
        invalid(&filter(r#"{"equal": 5}"#)),
        ("missing", "transform[0].filter.field".to_owned())
    );
    assert_eq!(
        invalid(&filter(r#"{"field": "net"}"#)),
        ("missing", "transform[0].filter".to_owned())
    );
    assert_eq!(
        invalid(&filter(r#"{"field": "net", "equal": 1, "oneOf": [1]}"#)).0,
        "bad-choice"
    );
    assert_eq!(
        invalid(&filter(r#"{"field": "net", "range": [500, 0]}"#)).0,
        "bad-choice"
    );
}

#[test]
fn every_count_is_checked_as_it_is_reached() {
    let nine = (0..9)
        .map(|_| r#"{"filter": {"field": "net", "range": [0, null]}}"#)
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(
        too_big(&with(&format!(r#", "transform": [{nine}]"#))),
        "filters"
    );
    let values = (0..51).map(|n| n.to_string()).collect::<Vec<_>>().join(",");
    let text = filter(&format!(r#"{{"field": "net", "oneOf": [{values}]}}"#));
    assert_eq!(too_big(&text), "values");
    let layer = r#"{"mark": "line", "encoding": {}}"#;
    let five = [layer; 5].join(",");
    let text = format!(r#"{{"data": {{"name": "net_worth_over_time"}}, "layer": [{five}]}}"#);
    assert_eq!(too_big(&text), "layers");
    let tips = (0..7)
        .map(|_| r#"{"field": "income", "type": "quantitative"}"#)
        .collect::<Vec<_>>()
        .join(",");
    let text = with(r#", "description": "x""#).replace(
        r#""type": "quantitative"}}"#,
        &format!(r#""type": "quantitative"}}, "tooltip": [{tips}]}}"#),
    );
    assert_eq!(too_big(&text), "encodings");
    let colors = [r#""blue""#; 9].join(",");
    let text = with(&format!(
        r#", "config": {{"range": {{"category": [{colors}]}}}}"#
    ));
    assert_eq!(too_big(&text), "values");
    let long = "a".repeat(MAX_TEXT_CHARS + 1);
    assert_eq!(too_big(&with(&format!(r#", "title": "{long}""#))), "label");
    let fits = "\u{e9}".repeat(MAX_TEXT_CHARS);
    let spec = chart(&with(&format!(r#", "title": "{fits}""#)));
    assert_eq!(spec.title().map(SpecText::as_str), Some(fits.as_str()));
    assert_eq!(
        invalid(&with(r#", "title": "two\nlines""#)),
        ("bad-text", "title".to_owned())
    );
}

#[test]
fn a_horizontal_bar_chart_is_listed() {
    let text = r#"{"data": {"name": "spending_by_category"}, "mark": "bar",
        "encoding": {"y": {"field": "category", "type": "nominal"},
                     "x": {"field": "total", "type": "quantitative"}}}"#;
    let read = ok(text);
    assert_eq!(listed(&read), [("encoding", "horizontal")]);
    assert!(read.chart().is_none());
}

#[test]
fn an_unsupported_value_leaves_its_encoding_out_whole() {
    // Drawing it without the median would show other figures.
    let text = BAR.replace(
        r#""type": "quantitative""#,
        r#""type": "quantitative", "aggregate": "median""#,
    );
    let read = ok(&text);
    assert_eq!(listed(&read), [("encoding.y.aggregate", "value")]);
    assert!(read.chart().is_none());
    let text = BAR.replace(r#""yearmonth""#, r#""quarter""#);
    assert_eq!(listed(&ok(&text)), [("encoding.x.timeUnit", "value")]);
    let text = BAR.replace(r#""timeUnit": "yearmonth""#, r#""bin": true"#);
    assert_eq!(listed(&ok(&text)), [("encoding.x.bin", "option")]);
    // A color given as a value has no field: listed, and the rest is drawn.
    let text = with("").replace(
        r#""type": "quantitative"}}"#,
        r#""type": "quantitative"}, "color": {"value": "red"}}"#,
    );
    let read = ok(&text);
    assert_eq!(listed(&read), [("encoding.color.value", "option")]);
    assert_eq!(
        read.chart().expect("a chart").layers()[0].encodings().len(),
        2
    );
    // Options that only change the look keep the encoding.
    let text = BAR.replace(
        r#""type": "quantitative""#,
        r#""type": "quantitative", "scale": {"type": "log"}, "stack": null"#,
    );
    let read = ok(&text);
    assert_eq!(
        listed(&read),
        [
            ("encoding.y.scale", "option"),
            ("encoding.y.stack", "option")
        ]
    );
    assert!(read.chart().is_some());
}

#[test]
fn marks_and_their_options() {
    let read = ok(&BAR.replace(r#""bar""#, r#""boxplot""#));
    assert_eq!(listed(&read), [("mark", "mark")]);
    assert!(read.chart().is_none());
    let pie = |options: &str| {
        format!(
            r#"{{"data": {{"name": "spending_by_category"}},
                "mark": {{"type": "arc"{options}}},
                "encoding": {{"theta": {{"field": "total", "type": "quantitative"}},
                              "color": {{"field": "top_level", "type": "nominal"}}}}}}"#
        )
    };
    let donut = chart(&pie(r#", "innerRadius": 50, "tooltip": true"#));
    let mark = donut.layers()[0].mark();
    assert_eq!(
        (mark.kind(), mark.inner_radius(), mark.tooltip()),
        (MarkKind::Arc, Some(50), true)
    );
    assert_eq!(
        chart(&pie(r#", "innerRadius": 0"#)).layers()[0]
            .mark()
            .inner_radius(),
        None
    );
    assert_eq!(
        invalid(&pie(r#", "innerRadius": 70000"#)),
        ("bad-number", "mark.innerRadius".to_owned())
    );
    assert_eq!(invalid(&pie(r#", "innerRadius": -5"#)).0, "bad-number");
    assert_eq!(
        listed(&ok(&pie(r#", "innerRadius": {"expr": "width / 4"}"#))),
        [("mark.innerRadius", "expression")]
    );
    assert_eq!(
        invalid(&pie(r#", "point": true"#)),
        ("bad-channel-for-mark", "mark".to_owned())
    );
    assert_eq!(invalid(&pie(r#", "tooltip": 1"#)).0, "wrong-type");
    let line = chart(&BAR.replace(r#""bar""#, r#"{"type": "line", "point": true}"#));
    assert!(line.layers()[0].mark().point());
}

#[test]
fn a_channel_the_mark_does_not_take_is_listed() {
    let text = with("").replace(
        r#""type": "quantitative"}}"#,
        r#""type": "quantitative"}, "size": {"field": "income", "type": "quantitative"}}"#,
    );
    let read = ok(&text);
    assert_eq!(listed(&read), [("encoding.size", "channel")]);
    assert_eq!(
        read.chart().expect("a chart").layers()[0].encodings().len(),
        2
    );
}

#[test]
fn the_parser_knows_the_same_channels_per_mark_as_the_model() {
    let field = spec_dataset("income_expense_by_month")
        .and_then(|d| d.field("net"))
        .expect("a field");
    let month = spec_dataset("income_expense_by_month")
        .and_then(|d| d.field("month"))
        .expect("a field");
    let kinds = [
        MarkKind::Line,
        MarkKind::Bar,
        MarkKind::Area,
        MarkKind::Arc,
        MarkKind::Point,
        MarkKind::Text,
        MarkKind::Rule,
    ];
    let channels = [
        Channel::X,
        Channel::Y,
        Channel::Color,
        Channel::Size,
        Channel::Theta,
        Channel::Text,
        Channel::Tooltip,
    ];
    for kind in kinds {
        for channel in channels {
            let one = match channel {
                Channel::X | Channel::Color => Encoding::plain(channel, month, FieldType::Ordinal),
                _ => Encoding::plain(channel, field, FieldType::Quantitative),
            }
            .expect("an encoding");
            let refused = Layer::new(Mark::plain(kind), vec![one], Vec::new())
                == Err(ModelError::Invalid("bad-channel-for-mark"));
            // A rule with `x` only is fine, and so is one with `y` only.
            assert_eq!(!refused, takes(kind, channel), "{kind:?} {channel:?}");
        }
    }
}

#[test]
fn layers_share_the_top_level_and_one_axis() {
    let text = r#"{
      "data": {"name": "net_worth_over_time"},
      "encoding": {"x": {"field": "month", "type": "temporal", "timeUnit": "yearmonth"},
                   "y": {"field": "net_worth", "type": "quantitative"}},
      "transform": [{"filter": {"field": "currency", "equal": "EUR"}}],
      "layer": [{"mark": "line"}, {"mark": "point"}]
    }"#;
    let spec = chart(text);
    assert_eq!(spec.layers().len(), 2);
    assert_eq!(spec.filters().len(), 1);
    for layer in spec.layers() {
        assert_eq!(layer.encodings().len(), 2);
    }
    let text = r#"{
      "data": {"name": "income_expense_by_month"},
      "layer": [
        {"mark": "bar", "encoding": {"x": {"field": "month", "type": "temporal"},
                                     "y": {"field": "net", "type": "quantitative"}}},
        {"mark": "rule", "encoding": {"y": {"field": "net", "type": "quantitative",
                                            "aggregate": "mean"}}}
      ]
    }"#;
    let spec = chart(text);
    assert_eq!(spec.layers()[1].mark().kind(), MarkKind::Rule);
    let differ = r#"{
      "data": {"name": "income_expense_by_month"},
      "layer": [
        {"mark": "bar", "encoding": {"x": {"field": "month", "type": "temporal"},
                                     "y": {"field": "net", "type": "quantitative"}}},
        {"mark": "line", "encoding": {"x": {"field": "month", "type": "ordinal"},
                                      "y": {"field": "income", "type": "quantitative"}}}
      ]
    }"#;
    assert_eq!(invalid(differ), ("layers-differ", "layer".to_owned()));
    // A rule with both `x` and `y` is no level line.
    let both = text.replace(
        r#""encoding": {"y": {"field": "net", "type": "quantitative","#,
        r#""encoding": {"x": {"field": "month", "type": "temporal"}, "y": {"field": "net", "type": "quantitative","#,
    );
    assert_eq!(
        invalid(&both),
        ("bad-channel-for-mark", "layer[1].encoding".to_owned())
    );
    let pie_and_bar = r#"{
      "data": {"name": "spending_by_category"},
      "layer": [
        {"mark": "arc", "encoding": {"theta": {"field": "total", "type": "quantitative"},
                                     "color": {"field": "category", "type": "nominal"}}},
        {"mark": "bar", "encoding": {"x": {"field": "category", "type": "nominal"},
                                     "y": {"field": "total", "type": "quantitative"}}}
      ]
    }"#;
    assert_eq!(invalid(pie_and_bar), ("layers-differ", "layer".to_owned()));
    let nested = r#"{"data": {"name": "spending_by_category"},
        "layer": [{"layer": [], "mark": "bar", "title": "t",
                   "encoding": {"x": {"field": "category", "type": "nominal"},
                                "y": {"field": "total", "type": "quantitative"}}}]}"#;
    assert_eq!(
        listed(&ok(nested)),
        [
            ("layer[0].layer", "composition"),
            ("layer[0].title", "option")
        ]
    );
    assert_eq!(
        invalid(r#"{"data": {"name": "spending_by_category"}, "layer": []}"#),
        ("missing", "layer".to_owned())
    );
}

#[test]
fn size_palette_and_our_own_key() {
    let spec = chart(&with(
        r#", "width": 600, "height": 300, "title": "Net by month",
           "config": {"range": {"category": ["vermillion", "sky-blue"]}},
           "kassensturz": {"vl_subset": 1}"#,
    ));
    let size = spec.size().expect("a size");
    assert_eq!((size.width(), size.height()), (Some(600), Some(300)));
    assert_eq!(
        spec.palette(),
        [PaletteName::Vermillion, PaletteName::SkyBlue]
    );
    assert_eq!(spec.title().map(SpecText::as_str), Some("Net by month"));
    assert_eq!(
        invalid(&with(r#", "width": 50"#)),
        ("bad-number", "width".to_owned())
    );
    assert_eq!(invalid(&with(r#", "height": -300"#)).0, "bad-number");
    assert_eq!(
        listed(&ok(&with(r#", "width": "container""#))),
        [("width", "value")]
    );
    assert_eq!(
        listed(&ok(&with(
            r##", "config": {"range": {"category": ["blue", "#ff0000"]}, "view": {}}"##
        ))),
        [
            ("config.range.category[1]", "value"),
            ("config.view", "option")
        ]
    );
    assert_eq!(
        invalid(&with(r#", "kassensturz": {"vl_subset": 2}"#)),
        ("bad-choice", "kassensturz.vl_subset".to_owned())
    );
}

#[test]
fn sorting_by_ascending_descending_or_a_field() {
    let sorted = |sort: &str| {
        format!(
            r#"{{"data": {{"name": "spending_by_category"}}, "mark": "bar",
                "encoding": {{"x": {{"field": "category", "type": "nominal", "sort": {sort}}},
                              "y": {{"field": "total", "type": "quantitative"}}}}}}"#
        )
    };
    let spec = chart(&sorted(r#""descending""#));
    assert_eq!(
        spec.layers()[0].encodings()[0].sort(),
        Some(Sort::Descending)
    );
    let spec = chart(&sorted(r#""total""#));
    assert!(matches!(
        spec.layers()[0].encodings()[0].sort(),
        Some(Sort::ByField(field)) if field.name() == "total"
    ));
    assert_eq!(
        listed(&ok(&sorted(r#""-y""#))),
        [("encoding.x.sort", "value")]
    );
    assert_eq!(
        listed(&ok(&sorted(r#"{"op": "sum", "field": "total"}"#))),
        [("encoding.x.sort", "value")]
    );
    assert_eq!(
        invalid(&sorted(r#""cost""#)),
        ("unknown-field", "encoding.x.sort".to_owned())
    );
}

#[test]
fn keys_that_are_not_plain_words_are_quoted_in_paths() {
    let read = ok(&with(r#", "a.b": 1, "line\nbreak": 2, "": 3"#));
    assert_eq!(
        listed(&read),
        [
            (r#"["a.b"]"#, "option"),
            (r#"["line\nbreak"]"#, "option"),
            (r#"[""]"#, "option"),
        ]
    );
}

#[test]
fn every_reason_word_is_one_of_the_design() {
    assert_eq!(
        REASONS,
        [
            "data-source",
            "expression",
            "transform",
            "mark",
            "composition",
            "channel",
            "horizontal",
            "option",
            "value"
        ]
    );
}

/// Any text, and text made of JSON's own characters, is read or refused,
/// never a panic.
fn jsonish() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::sample::select(vec![
            "{",
            "}",
            "[",
            "]",
            ",",
            ":",
            "\"",
            "\\",
            "\"mark\"",
            "\"bar\"",
            "\"data\"",
            "\"name\"",
            "\"encoding\"",
            "\"x\"",
            "\"layer\"",
            "1",
            "-2",
            "0.5",
            "null",
            "true",
            " ",
            "\u{e9}",
            "\u{1f600}",
        ]),
        0..80,
    )
    .prop_map(|parts| parts.concat())
}

/// A small JSON tree with keys of the subset and others, for the
/// invariants of [`Read`].
fn tree() -> impl Strategy<Value = serde_json::Value> {
    let leaf = prop_oneof![
        Just(serde_json::Value::Null),
        any::<bool>().prop_map(serde_json::Value::from),
        any::<i64>().prop_map(serde_json::Value::from),
        prop::sample::select(vec![
            "bar",
            "line",
            "arc",
            "nominal",
            "quantitative",
            "temporal",
            "month",
            "net",
            "income_expense_by_month",
            "count",
            "sum",
            "2026-01-01",
            "12.50",
        ])
        .prop_map(serde_json::Value::from),
    ];
    leaf.prop_recursive(5, 48, 6, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(serde_json::Value::from),
            prop::collection::btree_map(
                prop::sample::select(vec![
                    "data",
                    "name",
                    "mark",
                    "type",
                    "encoding",
                    "x",
                    "y",
                    "color",
                    "theta",
                    "field",
                    "aggregate",
                    "timeUnit",
                    "transform",
                    "filter",
                    "equal",
                    "range",
                    "layer",
                    "title",
                    "width",
                    "url",
                    "padding",
                ])
                .prop_map(str::to_owned),
                inner,
                0..5,
            )
            .prop_map(|map| serde_json::Value::Object(map.into_iter().collect())),
        ]
    })
}

proptest! {
    #[test]
    fn any_text_is_read_or_refused(text in any::<String>()) {
        let _ = read(&text);
    }

    #[test]
    fn jsonish_text_is_read_or_refused(text in jsonish()) {
        let _ = read(&text);
    }

    #[test]
    fn nothing_is_dropped_silently(value in tree()) {
        let text = value.to_string();
        if let Ok(read) = read(&text) {
            // No chart only with a list that says why, and a reason.
            prop_assert_eq!(read.chart().is_none(), read.why_not().is_some());
            if read.chart().is_none() {
                prop_assert!(!read.unsupported().is_empty());
            }
            for entry in read.unsupported() {
                prop_assert!(REASONS.contains(&entry.reason()));
            }
        }
    }

    #[test]
    fn spacing_changes_nothing(spaces in prop::collection::vec("[ \t\r\n]{0,3}", 64)) {
        // The same description with other spacing between its tokens.
        let compact: String = serde_json::from_str::<serde_json::Value>(BAR)
            .expect("JSON")
            .to_string();
        let mut spaced = String::new();
        let mut gaps = spaces.iter().cycle();
        let mut in_string = false;
        for c in compact.chars() {
            if c == '"' {
                in_string = !in_string;
            }
            spaced.push(c);
            if !in_string && matches!(c, '{' | '[' | ',' | ':' | '}' | ']') {
                spaced.push_str(gaps.next().expect("cycled"));
            }
        }
        prop_assert_eq!(read(&spaced).ok(), read(&compact).ok());
    }

    #[test]
    fn decimals_written_as_text_keep_their_digits(digits in any::<i64>(), places in 0_u8..=18) {
        let text = SpecNumberText::new(digits, places).0;
        let spec = filter(&format!(r#"{{"field": "net", "equal": "{text}"}}"#));
        let read = read(&spec).expect("read");
        let chart = read.chart().expect("a chart");
        let FilterTest::Equal(SpecValue::Number(number)) = chart.filters()[0].test() else {
            panic!("a number");
        };
        prop_assert_eq!((number.digits(), number.places()), (digits, places));
    }

    #[test]
    fn numbers_with_a_fraction_are_refused(whole in -10_000_i64..10_000, fraction in 0_u32..1000) {
        let text = filter(&format!(r#"{{"field": "net", "equal": {whole}.{fraction}}}"#));
        prop_assert_eq!(invalid(&text).0, "fraction-number");
    }

    #[test]
    fn depth_is_refused_above_ten_only(depth in 1_usize..40) {
        let text = format!(r#"{{"title": {}{}}}"#, "[".repeat(depth), "]".repeat(depth));
        let result = read(&text);
        let deep = matches!(result, Err(ReportError::SpecTooBig { limit: "depth" }));
        prop_assert_eq!(deep, depth + 1 > MAX_SPEC_DEPTH);
    }

    #[test]
    fn the_list_follows_the_text(order in Just(vec!["padding", "view", "autosize", "usermeta", "background"]).prop_shuffle()) {
        let rest: String = order.iter().map(|name| format!(r#", "{name}": 1"#)).collect();
        let read = read(&with(&rest)).expect("read");
        let paths: Vec<&str> = read.unsupported().iter().map(Unsupported::path).collect();
        prop_assert_eq!(paths, order);
    }
}

/// A number written as text with exactly `places` decimal places.
struct SpecNumberText(String);

impl SpecNumberText {
    fn new(digits: i64, places: u8) -> Self {
        let sign = if digits < 0 { "-" } else { "" };
        let magnitude = digits.unsigned_abs().to_string();
        let places = usize::from(places);
        let padded = format!("{magnitude:0>width$}", width = places + 1);
        let (whole, fraction) = padded.split_at(padded.len() - places);
        if fraction.is_empty() {
            Self(format!("{sign}{whole}"))
        } else {
            Self(format!("{sign}{whole}.{fraction}"))
        }
    }
}
