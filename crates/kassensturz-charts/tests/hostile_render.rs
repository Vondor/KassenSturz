// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The renderer's side of hostile chart descriptions (M6e.4, T-38, D-523):
//! the most costly descriptions the parser accepts, read by the real
//! `spec::read` and drawn from the largest tables the renderer takes
//! ([`MAX_ROWS`] rows, [`MAX_SERIES`] series, labels of the longest
//! length), finish in bounded time and memory; one row or one series more is
//! refused whole. Every description here is written by hand (D-280).
//!
//! Like `kassensturz-report/tests/hostile_specs.rs`, this test program counts
//! the bytes each thread allocates, as an upper bound of the memory a chart
//! used.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell as Counter;
use std::time::{Duration, Instant};

use kassensturz_charts::{CustomError, MAX_ROWS, MAX_SERIES, custom_chart};
use kassensturz_core::{CurrencyCode, Date, Money};
use kassensturz_report::spec::{self, Cell, ChartSpec, FlatTable, MAX_TEXT_CHARS, spec_dataset};
use kassensturz_report::{Percent, TextFormat};

struct Counting;

thread_local! {
    static ALLOCATED: Counter<usize> = const { Counter::new(0) };
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

/// Most bytes one chart may allocate in total: 192 MiB, for a table of
/// [`MAX_ROWS`] rows whose labels are as long as they may be. The worst
/// case here allocates about 115 MB in all, a little at a time (printed by
/// the tests): every label is copied for its place, its series and its tip.
const MEMORY: usize = 192 << 20;
/// Longest one chart may take, in a debug build on a slow machine.
const TIME: Duration = Duration::from_secs(10);

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};

/// Words for the tests.
struct Words;

impl TextFormat for Words {
    fn amount(&self, money: Money) -> String {
        format!("{} cents", money.minor_units())
    }

    fn month(&self, first_day: Date) -> String {
        format!("{}-{:02}", first_day.year(), first_day.month())
    }

    fn percent(&self, percent: Percent) -> String {
        format!("{} tenths", percent.tenths())
    }

    fn number(&self, hundredths: i64) -> String {
        format!("{hundredths} hundredths")
    }

    fn unsorted(&self) -> String {
        "(unsorted)".into()
    }
}

/// The chart of `text`, which the parser must read in full.
fn parsed(text: &str) -> ChartSpec {
    let read = spec::read(text).expect("read");
    assert!(read.unsupported().is_empty(), "{:?}", read.unsupported());
    read.into_parts().0.expect("a chart")
}

/// A label of the longest length, different for every `n`.
fn label(n: usize) -> String {
    let head = format!("{n:05} ");
    format!("{head}{}", "\u{e9}".repeat(MAX_TEXT_CHARS - head.len()))
}

/// Draws `spec` from `table` and gives the result with the bytes and time
/// it took, within the bounds.
fn drawn(
    what: &str,
    spec: &ChartSpec,
    table: &FlatTable,
) -> Result<Option<kassensturz_charts::CustomChart>, CustomError> {
    let before = ALLOCATED.with(Counter::get);
    let start = Instant::now();
    let result = custom_chart(spec, table, &Words);
    let took = start.elapsed();
    let bytes = ALLOCATED.with(Counter::get) - before;
    println!("{what}: {bytes} bytes, {took:?}");
    assert!(bytes <= MEMORY, "{what}: {bytes} bytes");
    assert!(took <= TIME, "{what}: {took:?}");
    result
}

/// Spending by category: `rows` different categories of the longest
/// labels.
fn spending(rows: usize) -> FlatTable {
    let dataset = spec_dataset("spending_by_category").expect("dataset");
    let rows = (0..rows)
        .map(|n| {
            vec![
                Cell::Text(label(n)),
                Cell::Text(label(n % 7)),
                Cell::Amount(Money::from_minor_units(i64::try_from(n).unwrap_or(0), EUR)),
                Cell::Text("EUR".into()),
            ]
        })
        .collect();
    FlatTable::new(dataset, EUR, rows).expect("table")
}

/// Category over time: `months` months of `series` series each.
fn over_time(months: usize, series: usize) -> FlatTable {
    let dataset = spec_dataset("category_over_time").expect("dataset");
    let mut rows = Vec::new();
    for month in 0..months {
        let year = i16::try_from(1900 + month / 12).expect("a year");
        let day =
            Date::new(year, i8::try_from(month % 12 + 1).expect("a month"), 1).expect("a day");
        for s in 0..series {
            rows.push(vec![
                Cell::Day(day),
                Cell::Text(label(s)),
                Cell::Amount(Money::from_minor_units(i64::MAX / 1_000_000, EUR)),
                Cell::Text("EUR".into()),
            ]);
        }
    }
    FlatTable::new(dataset, EUR, rows).expect("table")
}

/// Bars of every category with their name written on top, a tip and a
/// level at the mean: three layers on one axis, with all eight encodings a
/// description may have and two filters on every row.
const EVERY_CATEGORY: &str = r#"{"data":{"name":"spending_by_category"},
 "transform":[{"filter":{"field":"total","range":[0,null]}},
  {"filter":{"field":"currency","equal":"EUR"}}],
 "layer":[
  {"mark":"bar","encoding":{"x":{"field":"category","type":"nominal","sort":"total"},
   "y":{"field":"total","type":"quantitative","aggregate":"sum"},
   "tooltip":[{"field":"top_level","type":"nominal"},{"field":"total","type":"quantitative"}]}},
  {"mark":"text","encoding":{"x":{"field":"category","type":"nominal","sort":"total"},
   "y":{"field":"total","type":"quantitative","aggregate":"sum"},
   "text":{"field":"category","type":"nominal"}}},
  {"mark":"rule","encoding":{"y":{"field":"total","type":"quantitative","aggregate":"mean"}}}]}"#;

/// A line per series over the months, with dots and a tip.
const EVERY_SERIES: &str = r#"{"data":{"name":"category_over_time"},
 "layer":[
  {"mark":"line","encoding":{"x":{"field":"month","type":"temporal","timeUnit":"yearmonth"},
   "y":{"field":"total","type":"quantitative","aggregate":"sum"},
   "color":{"field":"series","type":"nominal"},
   "tooltip":[{"field":"series","type":"nominal"},{"field":"total","type":"quantitative"}]}},
  {"mark":"point","encoding":{"x":{"field":"month","type":"temporal","timeUnit":"yearmonth"},
   "y":{"field":"total","type":"quantitative","aggregate":"sum"},
   "color":{"field":"series","type":"nominal"}}}]}"#;

#[test]
fn the_most_costly_chart_of_every_row_is_drawn_within_bounds() {
    let spec = parsed(EVERY_CATEGORY);
    let chart = drawn("every category", &spec, &spending(MAX_ROWS))
        .expect("drawn")
        .expect("something to show");
    assert_eq!(
        chart.chart.series.first().map(|s| s.points.len()),
        Some(MAX_ROWS)
    );
    // One row more is refused whole, before anything is drawn.
    assert_eq!(
        drawn("one row more", &spec, &spending(MAX_ROWS + 1)),
        Err(CustomError::TooBig("rows"))
    );
}

#[test]
fn the_most_series_there_may_be_are_drawn_within_bounds() {
    let spec = parsed(EVERY_SERIES);
    let months = MAX_ROWS / MAX_SERIES;
    let chart = drawn("every series", &spec, &over_time(months, MAX_SERIES))
        .expect("drawn")
        .expect("something to show");
    assert_eq!(chart.chart.series.len(), MAX_SERIES);
    // One series more is refused whole.
    assert_eq!(
        drawn("one series more", &spec, &over_time(2, MAX_SERIES + 1)),
        Err(CustomError::TooBig("series"))
    );
}
