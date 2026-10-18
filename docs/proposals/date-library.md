# Proposal: date library

**Status:** Decided on 2026-10-18: `jiff` (D-061 in `docs/DECISIONS.md`, answers Q-030). Kept as the background for that decision.

## Why this needs deciding

The core needs calendar dates from M0's walking skeleton onward (D-068): booking and value dates (D-030), the 3-business-day window for Transfer matching (D-030), monthly budgets (D-029) and date conditions in Rules (D-014). AGENTS.md §5 says transaction dates are **calendar dates without time zone**. Writing a date type of our own is possible but means re-solving leap years, month lengths and ISO 8601 parsing, so an established library is the better choice. It is a direct dependency, so the maintainer approves it (D-026).

## What the library has to handle

- A date type without time or time zone, plus parsing and printing ISO 8601 (`2026-10-18`).
- The weekday of a date, so business days can be counted (weekends don't count, D-030).
- Adding days and months, and comparing and sorting dates.
- No floating point in anything we use (AGENTS.md §3.1 rule 1).

Dates are stored in SQLite as ISO 8601 text, whichever library is chosen: it sorts correctly, and other programs can read it (D-046).

## The candidates (crates.io, checked 2026-10-18)

| | `jiff` | `time` | `chrono` |
|---|---|---|---|
| Newest release | 0.2.38 (2026-10-06) | 0.3.55 (2026-08-01) | 0.4.45 (2026-06-04) |
| License | Unlicense OR MIT | MIT OR Apache-2.0 | MIT OR Apache-2.0 |
| Minimum Rust version | 1.70 | 1.88 | 1.62 |
| Date type without time zone | `jiff::civil::Date` | `time::Date` | `chrono::NaiveDate` |
| Time-zone data | Bundled by default; left out with `default-features = false, features = ["std"]` | Not included | Separate crate (`chrono-tz`) |
| Version 1.0 yet? | No | No | No |

All three are maintained, widely used and on the license allowlist. Where a library offers conversions to floating point (for example `jiff`'s `Span::total` or `time`'s `Duration::as_seconds_f64`), the same Clippy lint that blocks `rust_decimal`'s (D-051) blocks them.

## Recommendation: `jiff`

- Its `civil::Date` is explicitly a date without a time zone, which is exactly the rule in AGENTS.md §5, and the time-zone parts can be left out of the build entirely.
- Its main author also wrote `regex` (D-015), whose care for correctness and documentation the project already relies on.
- It is the most recently released of the three.

Risks: it is still before 1.0, so a later major release may need small changes, and it depends heavily on one main maintainer. `chrono` would be the fallback: the oldest of the three and very widely used, with `NaiveDate` covering the same needs. `time` raises its minimum Rust version often (1.88 at the moment), which would push ours up as a side effect of updates, something D-025 wants to avoid.

## What you need to decide

1. `jiff`, `time` or `chrono`.
