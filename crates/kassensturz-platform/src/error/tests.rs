// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `platform.` error codes against `docs/ERROR_CODES.md`.

use std::collections::HashSet;

use super::list::{check, documented, unused};
use super::*;

/// One error of each kind. A new error needs an example here.
fn examples() -> Vec<KeychainError> {
    vec![
        KeychainError::Unavailable,
        KeychainError::Failed {
            kind: "PlatformFailure",
        },
        KeychainError::NotText,
        KeychainError::InvalidKey,
    ]
}

#[test]
fn every_code_is_listed_with_its_parameters() {
    let rows = documented();
    let mut seen = HashSet::new();
    let mut codes = HashSet::new();
    for error in examples() {
        let report = error.report();
        assert!(
            codes.insert(report.code),
            "two errors share the code {}",
            report.code
        );
        assert!(report.code.starts_with("platform."), "{}", report.code);
        check(&report, &rows, &mut seen);
    }
    let unused = unused(&rows, &seen, &["platform"]);
    assert!(
        unused.is_empty(),
        "listed in docs/ERROR_CODES.md but no error has them: {unused:?}"
    );
}

#[test]
fn no_report_carries_a_value() {
    // Every keychain error is a bare code (threat T-02).
    for error in examples() {
        let report = error.report();
        assert!(report.params.is_empty(), "{}", report.code);
        assert!(report.cause.is_none(), "{}", report.code);
    }
}

#[test]
fn display_and_debug_show_the_kind_only() {
    let error = KeychainError::Failed {
        kind: "PlatformFailure",
    };
    assert_eq!(
        error.to_string(),
        "the system keychain failed (PlatformFailure)"
    );
    assert_eq!(format!("{error:?}"), "Failed { kind: \"PlatformFailure\" }");
}
