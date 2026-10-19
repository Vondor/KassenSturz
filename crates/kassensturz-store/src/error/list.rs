// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reads `docs/ERROR_CODES.md` and checks error reports against it (D-084).
//!
//! Shared by the error-code tests of `kassensturz-store` and
//! `kassensturz-import`, which includes this file with `#[path]`: neither
//! crate may depend on the other, and each checks the codes of its own
//! origins.

use std::collections::{HashMap, HashSet};

use kassensturz_core::{ErrorParam, ErrorReport};

const ERROR_CODES: &str = include_str!("../../../../docs/ERROR_CODES.md");

/// A code's row in the list: its parameters in order, each with whether it
/// is optional, and the keywords a `Keyword` parameter can take.
pub(super) struct Row {
    pub(super) params: Vec<(String, bool)>,
    pub(super) keywords: Vec<String>,
}

/// The words in backticks in a cell of a table row.
fn quoted(cell: &str) -> Vec<String> {
    cell.split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

/// Every row of the list's tables whose first cell is a code in backticks.
/// The second cell names the parameters (`since?` is optional), the third
/// the keywords, if any.
pub(super) fn documented() -> HashMap<String, Row> {
    let mut rows = HashMap::new();
    for line in ERROR_CODES.lines() {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        // A table row starts and ends with a bar, so there are empty cells at
        // both ends.
        let [_, code, params, keywords, _what, _] = cells[..] else {
            continue;
        };
        let [code] = &quoted(code)[..] else {
            continue;
        };
        let params = quoted(params)
            .into_iter()
            .map(|param| match param.strip_suffix('?') {
                Some(name) => (name.to_owned(), true),
                None => (param, false),
            })
            .collect();
        let row = Row {
            params,
            keywords: quoted(keywords),
        };
        assert!(
            rows.insert(code.clone(), row).is_none(),
            "{code} is listed twice in docs/ERROR_CODES.md"
        );
    }
    rows
}

/// Checks a report and its causes against the list.
pub(super) fn check(
    report: &ErrorReport,
    rows: &HashMap<String, Row>,
    seen: &mut HashSet<&'static str>,
) {
    let code = report.code;
    assert!(
        code.split('.').count() >= 2
            && code.split('.').all(|part| {
                !part.is_empty()
                    && !part.starts_with('-')
                    && !part.ends_with('-')
                    && part
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            }),
        "{code} isn't lowercase words joined by hyphens, in parts separated by dots"
    );
    let row = rows
        .get(code)
        .unwrap_or_else(|| panic!("{code} is missing from docs/ERROR_CODES.md"));
    let names: Vec<&str> = report.params.iter().map(|(name, _)| *name).collect();
    let expected: Vec<&str> = row
        .params
        .iter()
        .filter(|(name, optional)| !optional || names.contains(&name.as_str()))
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(names, expected, "the parameters of {code}");
    for (name, value) in &report.params {
        if let ErrorParam::Keyword(keyword) = value {
            assert!(
                row.keywords.iter().any(|listed| listed == keyword),
                "{code}: the keyword {keyword} of {name} isn't listed"
            );
        }
    }
    seen.insert(code);
    if let Some(cause) = &report.cause {
        check(cause, rows, seen);
    }
}

/// The listed codes from `origins`, the first parts of their codes, that
/// none of the reports checked into `seen` has, sorted.
pub(super) fn unused(
    rows: &HashMap<String, Row>,
    seen: &HashSet<&'static str>,
    origins: &[&str],
) -> Vec<String> {
    let mut unused: Vec<String> = rows
        .keys()
        .filter(|code| {
            let origin = code.split('.').next().unwrap_or_default();
            origins.contains(&origin) && !seen.contains(code.as_str())
        })
        .cloned()
        .collect();
    unused.sort();
    unused
}
