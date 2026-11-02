// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Hostile chart descriptions (M6e.4, T-38, D-523): what a pasted, opened or
//! stored description can ask for, and proof that each is refused, listed as
//! "not supported here" or drawn from the book only, in bounded time and
//! memory. Every description in this file is written by hand from the public
//! Vega-Lite documentation, never copied from its examples (D-280).
//!
//! The cases go through the public ways in: `spec::read` (a paste or a file),
//! `Document::read` (a dashboard stored in the book) and
//! `IncomingDashboardFile::read` (a dashboard file from someone else). The
//! renderer's limits are in `kassensturz-charts/tests/hostile_render.rs` and
//! the panel's check in `kassensturz-ui-bridge` (`chart_spec_hostile_tests.rs`).
//!
//! This file is its own test program, so it can count what it allocates: a
//! global allocator that passes every call on to the system's and adds the
//! bytes asked for to a counter of the calling thread. The total is an upper
//! bound of the memory a check used, whatever it freed in between.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::time::{Duration, Instant};

use kassensturz_report::spec::{
    self, MAX_ONE_OF, MAX_SPEC_BYTES, MAX_TEXT_CHARS, REASONS, SPEC_DATASETS,
};
use kassensturz_report::{
    CATALOG, Document, IncomingDashboardFile, MAX_BYTES, MAX_COMPONENTS, ReportError, VERSION,
};
use proptest::prelude::*;
use serde_json::json;

struct Counting;

thread_local! {
    static ALLOCATED: Cell<usize> = const { Cell::new(0) };
}

fn count(bytes: usize) {
    // `try_with`: a thread that is ending may still free and allocate.
    let _ = ALLOCATED.try_with(|total| total.set(total.get().saturating_add(bytes)));
}

// SAFETY: every call is passed on unchanged to the system allocator, which
// upholds the contract; the counter is a thread-local `Cell` with a constant
// start, so counting never allocates or blocks.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        // SAFETY: the caller's layout, unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        // SAFETY: the caller's layout, unchanged.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` was allocated by `System` with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(new_size);
        // SAFETY: `ptr` was allocated by `System` with this layout.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// Most bytes one check of one description may allocate in total: 256 times
/// the largest description, 8 MiB. The worst case written here allocates
/// about 5.5 MB in all, a few hundred kilobytes at a time (printed by
/// `the_worst_cases_stay_within_their_bounds`): every key, path and list
/// entry is its own small allocation, and a growing list counts each size it
/// had.
const MEMORY: usize = 256 * MAX_SPEC_BYTES;
/// Longest one check may take, in a debug build on a slow machine. The worst
/// case takes a few milliseconds.
const TIME: Duration = Duration::from_secs(2);

/// Runs `work` and gives its result, the bytes it allocated in total and the
/// time it took.
fn measured<T>(work: impl FnOnce() -> T) -> (T, usize, Duration) {
    let before = ALLOCATED.with(Cell::get);
    let start = Instant::now();
    let result = work();
    let took = start.elapsed();
    let after = ALLOCATED.with(Cell::get);
    (result, after - before, took)
}

/// What `spec::read` made of `text`, in words a table can hold:
/// `too-big <limit>`, `invalid <reason> <path>`, or `draws` / `nothing to
/// draw` followed by the unsupported parts as `<path> <reason>`.
fn outcome(text: &str) -> String {
    let (result, bytes, took) = measured(|| spec::read(text));
    assert!(bytes <= MEMORY, "{bytes} bytes for {text:.80}");
    assert!(took <= TIME, "{took:?} for {text:.80}");
    match result {
        Err(ReportError::SpecTooBig { limit }) => format!("too-big {limit}"),
        Err(ReportError::SpecInvalid { reason, path }) => {
            format!("invalid {reason} {path}").trim_end().to_owned()
        }
        Err(other) => panic!("not a code of the parser: {other:?}"),
        Ok(read) => {
            let mut words = vec![if read.chart().is_some() {
                "draws".to_owned()
            } else {
                "nothing to draw".to_owned()
            }];
            words.extend(
                read.unsupported()
                    .iter()
                    .map(|part| format!("{} {}", part.path(), part.reason())),
            );
            words.join("; ")
        }
    }
}

/// A bar chart of spending by category, written for these tests.
const BAR: &str = r#"{"data":{"name":"spending_by_category"},"mark":"bar","encoding":{"x":{"field":"category","type":"nominal"},"y":{"field":"total","type":"quantitative"}}}"#;

/// A line over the months with a level and labels on top, written for these
/// tests.
const LAYERS: &str = r#"{"data":{"name":"income_expense_by_month"},
 "transform":[{"filter":{"field":"net","range":[-500,"12.50"]}}],
 "layer":[
  {"mark":"line","encoding":{"x":{"field":"month","type":"temporal","timeUnit":"yearmonth"},
   "y":{"field":"net","type":"quantitative"},"tooltip":[{"field":"income","type":"quantitative"}]}},
  {"mark":"text","encoding":{"x":{"field":"month","type":"temporal","timeUnit":"yearmonth"},
   "y":{"field":"net","type":"quantitative"},"text":{"field":"net","type":"quantitative"}}}]}"#;

/// A pie of the budgets, written for these tests.
const PIE: &str = r#"{"$schema":"https://vega.github.io/schema/vega-lite/v5.json",
 "data":{"name":"budget_utilization"},"mark":{"type":"arc","innerRadius":40},
 "title":"Where the plan goes","width":300,"height":200,
 "encoding":{"theta":{"field":"planned","type":"quantitative","aggregate":"sum"},
 "color":{"field":"category","type":"nominal"}}}"#;

/// `BAR` with `rest` added at the end of its top level.
fn bar_with(rest: &str) -> String {
    format!("{},{rest}}}", &BAR[..BAR.len() - 1])
}

/// `BAR` with its dataset written as `name`.
fn bar_of(name: &str) -> String {
    BAR.replace("spending_by_category", name)
}

/// `BAR` with the field of `x` written as `field` (JSON text).
fn bar_field(field: &str) -> String {
    BAR.replacen(r#""field":"category""#, &format!(r#""field":{field}"#), 1)
}

/// `BAR` with a filter.
fn bar_filter(filter: &str) -> String {
    bar_with(&format!(r#""transform":[{{"filter":{filter}}}]"#))
}

/// The cases of T-38 with what must come of them. `what` names the threat.
fn cases() -> Vec<(&'static str, String, &'static str)> {
    let padded = format!("{BAR}{}", " ".repeat(MAX_SPEC_BYTES - BAR.len()));
    vec![
        // Deep: refused by the raw scan before anything is built.
        ("open brackets only", "[".repeat(MAX_SPEC_BYTES), "too-big depth"),
        ("open objects only", "{\"a\":".repeat(MAX_SPEC_BYTES / 5), "too-big depth"),
        ("eleven closed brackets", format!("{}{}", "[".repeat(11), "]".repeat(11)), "too-big depth"),
        (
            "deep inside an option",
            BAR.replacen(
                r#""type":"nominal""#,
                r#""type":"nominal","axis":{"a":{"b":{"c":{"d":{"e":{"f":[[1]]}}}}}}"#,
                1,
            ),
            "too-big depth",
        ),
        // Huge: the size is the first check.
        ("one byte over", format!("{padded} "), "too-big bytes"),
        ("exactly the limit", padded, "draws"),
        // Repeated keys: refused, because serde_json would keep the last one.
        ("the mark twice", bar_with(r#""mark":"line""#), "invalid duplicate-key mark"),
        ("the mark twice, escaped", bar_with(r#""mark":"line""#), "invalid duplicate-key mark"),
        (
            "a field twice",
            BAR.replacen(r#""type":"quantitative""#, r#""type":"quantitative","field":"category""#, 1),
            "invalid duplicate-key encoding.y.field",
        ),
        // Data from anywhere but the book.
        (
            "a file URL",
            r#"{"data":{"url":"file:///etc/passwd"},"mark":"bar"}"#.into(),
            "nothing to draw; data.url data-source",
        ),
        (
            "a URL beside the dataset",
            BAR.replacen(r#""name":"spending_by_category""#, r#""name":"spending_by_category","url":"https://example.invalid/x.json""#, 1),
            "draws; data.url data-source",
        ),
        (
            "values in the text",
            BAR.replacen(r#""name":"spending_by_category""#, r#""name":"spending_by_category","values":[{"category":"Rent","total":1}]"#, 1),
            "draws; data.values data-source",
        ),
        (
            "a generated sequence",
            BAR.replacen(r#""name":"spending_by_category""#, r#""name":"spending_by_category","sequence":{"start":0,"stop":1000000000}"#, 1),
            "draws; data.sequence data-source",
        ),
        ("a schema elsewhere", bar_with(r#""$schema":"https://example.invalid/vega-lite/v5.json""#), "invalid bad-schema $schema"),
        // Expressions: never run, listed.
        ("a filter expression", bar_filter(r#""datum.total > 0""#), "draws; transform[0].filter expression"),
        ("a calculation", bar_with(r#""transform":[{"calculate":"datum.total * 2","as":"t"}]"#), "draws; transform[0].calculate expression"),
        (
            "an expression as a color",
            BAR.replacen(r#""mark":"bar""#, r#""mark":{"type":"bar","color":{"expr":"'red'"}}"#, 1),
            "draws; mark.color option",
        ),
        (
            "a condition with a test",
            BAR.replacen(r#""type":"quantitative"}"#, r#""type":"quantitative"},"opacity":{"condition":{"test":"datum.total > 0","value":1},"value":0}"#, 1),
            "draws; encoding.opacity channel",
        ),
        (
            "a link per mark",
            BAR.replacen(r#""type":"quantitative"}"#, r#""type":"quantitative"},"href":{"field":"category","type":"nominal"}"#, 1),
            "draws; encoding.href channel",
        ),
        ("a parameter with an expression", bar_with(r#""params":[{"name":"p","expr":"now()"}]"#), "draws; params composition"),
        ("a title from an expression", bar_with(r#""title":{"text":{"expr":"datum"}}"#), "draws; title value"),
        ("an image mark", BAR.replacen(r#""mark":"bar""#, r#""mark":"image""#, 1), "nothing to draw; mark mark"),
        // Datasets and fields that aren't in the catalog.
        ("a table of the book", bar_of("transactions"), "invalid unknown-dataset data.name"),
        ("a path as the dataset", bar_of("../spending_by_category"), "invalid unknown-dataset data.name"),
        ("the dataset in other letters", bar_of("Spending_By_Category"), "invalid unknown-dataset data.name"),
        ("a field the dataset hasn't got", bar_field(r#""iban""#), "invalid unknown-field encoding.x.field"),
        ("a field as an expression", bar_field(r#""datum.category""#), "invalid unknown-field encoding.x.field"),
        ("a field of the language", bar_field(r#""__proto__""#), "invalid unknown-field encoding.x.field"),
        (
            "a field from a repeat",
            bar_field(r#"{"repeat":"row"}"#),
            "nothing to draw; encoding.x.field composition",
        ),
        // Numbers are never floating point.
        ("a width with a fraction", bar_with(r#""width":100.5"#), "invalid fraction-number width"),
        ("a width with an exponent", bar_with(r#""width":1e3"#), "invalid fraction-number width"),
        ("a width beyond 64 bits", bar_with(r#""width":99999999999999999999"#), "invalid fraction-number width"),
        ("minus zero", bar_with(r#""width":-0.0"#), "invalid fraction-number width"),
        ("a number beyond floating point", bar_with(r#""width":1e400"#), "invalid not-json"),
        (
            "a fraction in a range",
            bar_filter(r#"{"field":"total","range":[0,0.1]}"#),
            "invalid fraction-number transform[0].filter.range[1]",
        ),
        ("not a number", bar_filter(r#"{"field":"total","equal":"NaN"}"#), "invalid bad-number transform[0].filter.equal"),
        ("an exponent as text", bar_filter(r#"{"field":"total","equal":"1e3"}"#), "invalid bad-number transform[0].filter.equal"),
        (
            "four hundred digits",
            bar_filter(&format!(r#"{{"field":"total","equal":"{}"}}"#, "9".repeat(400))),
            "invalid bad-number transform[0].filter.equal",
        ),
        // Text that isn't one JSON object, or isn't text.
        ("nothing", String::new(), "invalid not-json"),
        ("a list", "[1,2,3]".into(), "invalid not-an-object"),
        ("two objects", format!("{BAR}{BAR}"), "invalid not-json"),
        ("a comment", format!("// a note\n{BAR}"), "invalid not-json"),
        ("a byte-order mark in a paste", format!("\u{feff}{BAR}"), "invalid not-json"),
        ("an unclosed key in a long letter", "{\"\u{e9}".into(), "invalid not-json"),
        ("half a surrogate as a key", bar_with(r#""\ud800":1"#), "invalid not-json"),
        ("half a surrogate as a title", bar_with(r#""title":"\ud800""#), "invalid not-json"),
        // Keys that hide or turn the text are shown escaped (D-522).
        ("a control character in a key", bar_with(r#""a\u0000b":1"#), r#"draws; ["a\u0000b"] option"#),
        ("a key that turns the text", bar_with("\"\u{202e}fdp.\":1"), r#"draws; ["\u202efdp."] option"#),
        // Labels are capped in characters, not bytes.
        (
            "a title of too many letters",
            bar_with(&format!(r#""title":"{}""#, "\u{e9}".repeat(MAX_TEXT_CHARS + 1))),
            "too-big label",
        ),
        (
            "escapes that make a long title",
            bar_with(&format!(r#""description":"{}""#, "\\u0041".repeat(MAX_TEXT_CHARS + 1))),
            "too-big label",
        ),
        (
            "too many values to pick",
            bar_filter(&format!(
                r#"{{"field":"category","oneOf":[{}"x"]}}"#,
                "\"x\",".repeat(MAX_ONE_OF)
            )),
            "too-big values",
        ),
    ]
}

#[test]
fn every_threat_of_t38_is_refused_or_listed_within_its_bounds() {
    for (what, text, expected) in cases() {
        assert_eq!(outcome(&text), expected, "{what}");
    }
}

#[test]
fn the_hand_written_descriptions_are_read_in_full() {
    for text in [BAR, LAYERS, PIE] {
        assert_eq!(outcome(text), "draws", "{text}");
    }
}

/// The largest descriptions of the cheapest parts, each as many times as
/// the limit allows, so they cost the most work there is per byte.
fn worst_cases() -> Vec<(&'static str, String)> {
    let fit = |head: &str, item: &str, tail: &str| {
        let room = MAX_SPEC_BYTES - head.len() - tail.len();
        let items = item.repeat(room / item.len());
        format!("{head}{}{tail}", items.trim_end_matches(','))
    };
    let open = &BAR[..BAR.len() - 1];
    // `BAR` cut inside its `x` encoding.
    let cut = BAR.find(r#""type":"nominal""#).map_or(0, |at| at + 16);
    let (inside_x, outside_x) = BAR.split_at(cut);
    // Different short keys, as many as fit between `head` and `tail`.
    let keys = |head: &str, tail: &str| {
        let mut text = head.to_owned();
        let mut n = 0_u32;
        while text.len() + 16 + tail.len() < MAX_SPEC_BYTES {
            text.push_str(&format!("\"{n:x}\":0,"));
            n += 1;
        }
        text.push_str(&format!("\"z\":0{tail}"));
        text
    };
    vec![
        // As many unsupported keys as fit: each is an entry with its path.
        ("short unknown keys", keys("{", "}")),
        (
            "unknown keys deep in an encoding",
            keys(&format!("{inside_x},"), outside_x),
        ),
        // Many small values the parser never looks into.
        (
            "a long list of data",
            fit(&format!(r#"{open},"usermeta":["#), "0,", "]}"),
        ),
        (
            "a long list of lists",
            fit(&format!(r#"{open},"usermeta":["#), "[],", "]}"),
        ),
        (
            "a long list of objects",
            fit(&format!(r#"{open},"usermeta":["#), "{},", "]}"),
        ),
        (
            "a long list of strings",
            fit(&format!(r#"{open},"usermeta":["#), "\"\",", "]}"),
        ),
        // One huge key and one huge string.
        ("a huge key", fit(&format!("{open},\""), "k", "\":1}")),
        (
            "a huge string",
            fit(&format!(r#"{open},"usermeta":""#), "\\n", "\"}"),
        ),
        // Objects nested to the limit, side by side.
        (
            "nests side by side",
            fit(
                &format!(r#"{open},"usermeta":["#),
                r#"{"a":{"a":{"a":{"a":{"a":{"a":{"a":[0]}}}}}}},"#,
                "]}",
            ),
        ),
        // Different keys that start alike, in one object.
        (
            "many keys alike",
            keys(
                &format!(r#"{open},"usermeta":{{"aaaaaaaaaaaaaaaa":0,"#),
                "}}",
            ),
        ),
    ]
}

#[test]
fn the_worst_cases_stay_within_their_bounds() {
    for (what, text) in worst_cases() {
        assert!(text.len() <= MAX_SPEC_BYTES, "{what}: {}", text.len());
        assert!(text.len() + 64 > MAX_SPEC_BYTES, "{what} fills the limit");
        let (result, bytes, took) = measured(|| spec::read(&text));
        println!("{what}: {bytes} bytes, {took:?}");
        if let Ok(read) = result {
            // Every part not read is listed with a fixed reason word.
            for part in read.unsupported() {
                assert!(REASONS.contains(&part.reason()), "{what}");
            }
        }
    }
    // The list of unknown keys holds each of them, in the order written.
    let (_, text) = &worst_cases()[0];
    let read = spec::read(text).expect("read");
    let paths: Vec<&str> = read.unsupported().iter().map(|part| part.path()).collect();
    assert!(paths.len() > 3_000, "{}", paths.len());
    assert_eq!(paths[..3], ["0", "1", "2"]);
    assert_eq!(paths.last(), Some(&"z"));
}

/// A dashboard document with these components, in the book's format.
fn document(components: serde_json::Value) -> String {
    json!({
        "format": "kassensturz-dashboard", "version": VERSION, "kind": "dashboard",
        "name": "Hostile", "filters": {}, "components": components,
    })
    .to_string()
}

/// A custom chart component with this description.
fn custom(n: usize, dataset: &str, spec: &str, partial: bool) -> serde_json::Value {
    json!({
        "id": format!("c{n}"), "type": "chart.custom", "dataset": dataset,
        "params": {}, "props": {"spec": spec, "partial": partial},
        "frame": {"col": 0, "row": n * 4, "cols": 6, "rows": 4},
    })
}

/// The reason of a refused document, or `ok`.
fn stored(text: &str) -> String {
    let (result, bytes, took) = measured(|| Document::read(text));
    assert!(took <= TIME, "{took:?}");
    assert!(bytes <= 64 * MAX_BYTES, "{bytes} bytes");
    match result {
        Ok(_) => "ok".to_owned(),
        Err(ReportError::DocumentInvalid { reason, path }) => format!("{reason} {path}"),
        Err(other) => format!("{other:?}"),
    }
}

#[test]
fn a_dashboard_in_the_book_checks_every_description_it_holds() {
    for (what, text, expected) in cases() {
        let dataset = spec::read(&text)
            .ok()
            .and_then(|read| read.chart().map(|chart| chart.dataset().id()))
            .unwrap_or("spending_by_category");
        // A description the parser reads is kept, even with nothing to draw:
        // the chart then shows what is not supported instead (D-508).
        let kept = expected.starts_with("draws") || expected.starts_with("nothing");
        let result = stored(&document(json!([custom(0, dataset, &text, false)])));
        let expected = if kept { "ok" } else { "bad-spec props.spec" };
        assert_eq!(result, expected, "{what}");
    }
    // A description stored as an object instead of its text is refused.
    let mut object = custom(0, "spending_by_category", BAR, false);
    object["props"]["spec"] = serde_json::from_str(BAR).expect("json");
    assert_eq!(stored(&document(json!([object]))), "wrong-type props.spec");
    // The description says one dataset and the component another.
    assert_eq!(
        stored(&document(json!([custom(
            0,
            "account_balances",
            BAR,
            false
        )]))),
        "spec-dataset dataset"
    );
}

#[test]
fn a_full_dashboard_of_the_worst_descriptions_is_read_in_bounds() {
    // As many custom charts of the most costly description as one document
    // holds, each listing thousands of parts that are not supported here.
    let (_, worst) = worst_cases().swap_remove(1);
    let one = custom(0, "spending_by_category", &worst, true).to_string();
    let count = (MAX_BYTES / one.len()).min(MAX_COMPONENTS) - 1;
    let components: Vec<_> = (0..count)
        .map(|n| custom(n, "spending_by_category", &worst, true))
        .collect();
    let text = document(json!(components));
    assert!(text.len() <= MAX_BYTES);
    let (result, bytes, took) = measured(|| Document::read(&text));
    println!("{count} charts: {bytes} bytes, {took:?}");
    let document = result.expect("a document of drawable charts");
    assert_eq!(document.entries.len(), count);
    // Each description is read twice, by the check of its property and by
    // the check that its dataset agrees.
    assert!(bytes <= count * 2 * MEMORY, "{bytes} bytes");
    assert!(took <= TIME * 4, "{took:?}");
}

/// A dashboard file around these components.
fn dashboard_file(components: serde_json::Value) -> String {
    json!({
        "format": "kassensturz-dashboard-file", "version": 1,
        "application": "KassenSturz 0.261102.0", "references": {},
        "dashboard": serde_json::from_str::<serde_json::Value>(&document(components))
            .expect("json"),
    })
    .to_string()
}

#[test]
fn a_dashboard_file_from_someone_else_checks_every_description_it_holds() {
    let read = |text: &str| match IncomingDashboardFile::read(text) {
        Ok(_) => "ok".to_owned(),
        Err(
            ReportError::DashboardFileInvalid { reason, path }
            | ReportError::DocumentInvalid { reason, path },
        ) => format!("{reason} {path}"),
        Err(other) => format!("{other:?}"),
    };
    let ok = dashboard_file(json!([custom(0, "spending_by_category", BAR, false)]));
    assert_eq!(read(&ok), "ok");
    for (what, text, expected) in cases() {
        if expected.starts_with("draws") || expected.starts_with("nothing") {
            continue;
        }
        let file = dashboard_file(json!([custom(0, "spending_by_category", &text, false)]));
        let (result, bytes, took) = measured(|| read(&file));
        assert!(took <= TIME, "{what}: {took:?}");
        assert!(bytes <= 64 * MAX_BYTES, "{what}: {bytes} bytes");
        assert_eq!(result, "bad-spec dashboard.props.spec", "{what}");
    }
    // A description with a URL can't make the file fetch anything: it is
    // read, and the part is listed when the chart is shown.
    let url = r#"{"data":{"name":"spending_by_category","url":"https://example.invalid/x.json"},"mark":"bar","encoding":{"x":{"field":"category","type":"nominal"},"y":{"field":"total","type":"quantitative"}}}"#;
    let file = dashboard_file(json!([custom(0, "spending_by_category", url, false)]));
    assert_eq!(read(&file), "ok");
}

#[test]
fn a_description_can_name_only_what_the_built_in_charts_show() {
    // Every dataset of a description is one of the catalog's, so a
    // description reads nothing a built-in chart couldn't show (T-38).
    for dataset in SPEC_DATASETS {
        assert!(
            CATALOG.iter().any(|info| info.id == dataset.id()),
            "{}",
            dataset.id()
        );
        for field in dataset.fields() {
            let name = field.name();
            for secret in [
                "iban", "bic", "memo", "note", "payee", "number", "owner", "pin", "password",
                "token", "key", "path", "file",
            ] {
                assert!(!name.contains(secret), "{}.{name}", dataset.id());
            }
        }
    }
}

/// Small changes that turn a good description into a hostile one.
const FRAGMENTS: &[&str] = &[
    "{",
    "}",
    "[",
    "]",
    "\"",
    "\\",
    ",",
    ":",
    "\u{e9}",
    "\u{1f600}",
    "\"\u{e9}",
    "\\u",
    "\\ud800",
    "1e400",
    "0.5",
    "-0",
    "null",
    r#","mark":"bar""#,
    r#","data":{"url":"x"}"#,
    r#"{"expr":"x"}"#,
    r#""layer":[{"mark":"line"}],"#,
    r#""mark""#,
    "\u{202e}",
];

/// A good description and one change to it: cut at a character, a fragment
/// put in, a piece taken out, or a piece written twice.
fn mutated() -> impl Strategy<Value = String> {
    (
        prop::sample::select(vec![BAR, LAYERS, PIE]),
        any::<prop::sample::Index>(),
        any::<prop::sample::Index>(),
        prop::sample::select(FRAGMENTS),
        0_u8..4,
    )
        .prop_map(|(seed, at, len, fragment, how)| {
            let bounds: Vec<usize> = seed
                .char_indices()
                .map(|(at, _)| at)
                .chain([seed.len()])
                .collect();
            let start = bounds[at.index(bounds.len())];
            let end = bounds[(at.index(bounds.len()) + len.index(32)).min(bounds.len() - 1)];
            match how {
                0 => seed[..start].to_owned(),
                1 => format!("{}{fragment}{}", &seed[..start], &seed[start..]),
                2 => format!("{}{}", &seed[..start], &seed[end..]),
                _ => format!("{}{}", &seed[..end], &seed[start..]),
            }
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    /// Fuzz-style: a changed description is read or refused the same way
    /// every time, within its bounds, and never panics.
    #[test]
    fn a_changed_description_is_read_or_refused_within_bounds(text in mutated()) {
        let first = outcome(&text);
        prop_assert_eq!(&first, &outcome(&text));
        let stored = document(json!([custom(0, "spending_by_category", &text, true)]));
        let (_, bytes, took) = measured(|| Document::read(&stored));
        prop_assert!(bytes <= 64 * MAX_BYTES);
        prop_assert!(took <= TIME);
    }
}
