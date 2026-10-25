// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `ai.` error codes against `docs/ERROR_CODES.md`.

use std::collections::HashSet;

use super::list::{check, documented, unused};
use super::*;

/// One error of each kind. A new error needs an example here.
fn examples() -> Vec<AiError> {
    vec![
        AiError::Off,
        AiError::NoProvider,
        AiError::BadAddress,
        AiError::PlainHttp,
        AiError::NoConsent,
        AiError::SettingsChanged,
        AiError::RequestTooLarge,
        AiError::KeyNeeded,
        AiError::KeyRejected,
        AiError::Keychain,
        AiError::KeePassXc,
        AiError::Unreachable,
        AiError::UntrustedServer,
        AiError::Timeout,
        AiError::Redirect,
        AiError::NotFound,
        AiError::RateLimited,
        AiError::ProviderFailed { status: 500 },
        AiError::ReplyTooLarge,
        AiError::BadReply,
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
        assert!(report.code.starts_with("ai."), "{}", report.code);
        check(&report, &rows, &mut seen);
    }
    let unused = unused(&rows, &seen, &["ai"]);
    assert!(
        unused.is_empty(),
        "listed in docs/ERROR_CODES.md but no error has them: {unused:?}"
    );
}

#[test]
fn no_report_carries_text() {
    // A key, a question or the provider's text never reaches a report
    // (threats T-25, T-31): the only parameter is a status number.
    for error in examples() {
        let report = error.report();
        assert!(report.cause.is_none(), "{}", report.code);
        for (name, value) in &report.params {
            assert!(
                matches!(value, ErrorParam::Number(_)),
                "{}: {name} isn't a number",
                report.code
            );
        }
    }
}

#[test]
fn display_is_the_code() {
    assert_eq!(AiError::PlainHttp.to_string(), "ai.plain-http");
    assert_eq!(
        AiError::ProviderFailed { status: 502 }.to_string(),
        "the provider failed (HTTP 502)"
    );
}
