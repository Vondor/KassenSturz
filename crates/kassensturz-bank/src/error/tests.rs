// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `bank.` error codes against `docs/ERROR_CODES.md`.

use std::collections::HashSet;

use kassensturz_core::CurrencyCode;
use kassensturz_import::ImportError;

use super::list::{check, documented, unused};
use super::*;
use crate::availability::{AQBANKING_MINIMUM, Library, Version};
use crate::backend::{BankAccountId, LoginId};

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};
const USD: CurrencyCode = match CurrencyCode::new("USD") {
    Ok(code) => code,
    Err(_) => panic!("USD is a valid code"),
};

/// One error of each kind. A new error needs an example here.
fn examples() -> Vec<BankError> {
    vec![
        BankError::NotInstalled,
        BankError::TooOld {
            library: Library::AqBanking,
            found: Version::new(6, 5, 0),
            minimum: AQBANKING_MINIMUM,
        },
        BankError::NotRegistered,
        BankError::MalformedIban,
        BankError::UnknownLogin(LoginId::new("login-1")),
        BankError::UnknownAccount(BankAccountId::new("account-1")),
        BankError::PinNeeded,
        BankError::PinRejected,
        BankError::Unreachable,
        BankError::Refused {
            text: Some("9010: made-up bank message".to_owned()),
        },
        BankError::Aborted,
        BankError::Keychain,
        BankError::TanNeeded,
        BankError::TanMethodUnsupported,
        BankError::UntrustedServer,
        BankError::NeedsSetupDialog,
        BankError::SetupFailed {
            text: Some("ERROR: Could not parse arguments".to_owned()),
        },
        BankError::Folder,
        BankError::Busy,
        BankError::MalformedAmount {
            amount: "12.34".to_owned(),
        },
        BankError::InexactAmount {
            amount: "1/3".to_owned(),
        },
        BankError::AmountTooLarge {
            amount: "1".repeat(20),
        },
        BankError::MissingCurrency,
        BankError::CurrencyMismatch {
            expected: EUR,
            found: USD,
        },
        BankError::MissingDate,
        BankError::Statement(ImportError::TooManyEntries { max: 1 }),
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
        assert!(report.code.starts_with("bank."), "{}", report.code);
        check(&report, &rows, &mut seen);
    }
    let unused = unused(&rows, &seen, &["bank"]);
    assert!(
        unused.is_empty(),
        "listed in docs/ERROR_CODES.md but no error has them: {unused:?}"
    );
}

#[test]
fn a_refusal_without_text_leaves_the_parameter_out() {
    let report = BankError::Refused { text: None }.report();
    assert_eq!(report.code, "bank.refused");
    assert!(report.params.is_empty());
}

#[test]
fn every_library_keyword_is_listed() {
    let rows = documented();
    let listed = &rows["bank.too-old"].keywords;
    for library in [Library::AqBanking, Library::Gwenhywfar] {
        assert!(
            listed.contains(&library.keyword().to_owned()),
            "{library:?}"
        );
    }
    assert_eq!(listed.len(), 2);
}

#[test]
fn a_failed_statement_names_its_cause() {
    let report = BankError::Statement(ImportError::TooManyEntries { max: 1 }).report();
    assert_eq!(report.code, "bank.statement");
    assert_eq!(
        report.cause.map(|cause| cause.code),
        Some("import.too-many-entries")
    );
}

#[test]
fn no_report_carries_a_pin_or_login_name() {
    // Reports hold IDs, codes and the bank's text, never what the user
    // typed to log in (threat T-02). The examples cover every variant.
    for error in examples() {
        for (name, _) in error.report().params {
            assert!(
                !["pin", "tan", "login_name", "customer_id", "password"].contains(&name),
                "{name}"
            );
        }
    }
}
