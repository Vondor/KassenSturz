// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `export.` error codes against `docs/ERROR_CODES.md`.

use std::collections::HashSet;

use super::list::{check, documented, unused};
use super::*;

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};
const USD: CurrencyCode = match CurrencyCode::new("USD") {
    Ok(code) => code,
    Err(_) => panic!("USD is a valid code"),
};

/// One error of each kind. A new error needs an example here.
fn examples() -> Vec<ExportError> {
    vec![
        ExportError::Io(ErrorKind::NotFound),
        ExportError::AlreadyExists {
            file: "transactions.csv".to_owned(),
        },
        ExportError::BadSettings,
        ExportError::CurrencyMismatch {
            expected: EUR,
            found: USD,
        },
        ExportError::BadDecimalPlaces {
            currency: EUR,
            digits: 40,
        },
        ExportError::WrongKind,
        ExportError::UnlistedCurrency {
            currency: USD,
            path: Some("transactions[0].parts[1].amount".into()),
        },
        ExportError::MarkWithoutPart(SplitId::generate()),
        ExportError::UnlistedTag {
            split: SplitId::generate(),
            tag: kassensturz_core::TagId::generate(),
        },
        ExportError::RulesTooDeep { max: 32 },
        ExportError::TooLarge { max: 1024 },
        ExportError::NotJson { line: 3, column: 7 },
        ExportError::NotABook,
        ExportError::NewerVersion { version: 2 },
        ExportError::MissingValue {
            path: "accounts[0].name".into(),
        },
        ExportError::UnknownValue {
            path: "accounts[0].colour".into(),
        },
        ExportError::BadValue {
            path: "accounts[0].closed".into(),
            expected: Expected::YesOrNo,
        },
        ExportError::Duplicate {
            path: "accounts[1].id".into(),
        },
        ExportError::InvalidTransaction {
            path: "transactions[4]".into(),
            error: TransactionError::TooFewSplits { found: 1 },
        },
    ]
}

#[test]
fn every_expected_value_is_a_listed_keyword() {
    let rows = documented();
    let listed = &rows["export.bad-value"].keywords;
    for expected in [
        Expected::Object,
        Expected::List,
        Expected::Text,
        Expected::Number,
        Expected::YesOrNo,
        Expected::Null,
        Expected::Id,
        Expected::Date,
        Expected::Amount,
        Expected::Currency,
        Expected::Rate,
        Expected::Keyword,
    ] {
        assert!(
            listed.iter().any(|listed| listed == expected.keyword()),
            "{}",
            expected.keyword()
        );
    }
    assert_eq!(listed.len(), 12, "every keyword has a value");
}

#[test]
fn an_invalid_transaction_names_its_cause() {
    let report = ExportError::InvalidTransaction {
        path: "transactions[4]".into(),
        error: TransactionError::TooFewSplits { found: 1 },
    }
    .report();
    assert_eq!(
        report.cause.map(|cause| cause.code),
        Some("transaction.too-few-splits")
    );
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
        assert!(report.code.starts_with("export."), "{}", report.code);
        check(&report, &rows, &mut seen);
    }
    let unused = unused(&rows, &seen, &["export"]);
    assert!(
        unused.is_empty(),
        "listed in docs/ERROR_CODES.md but no error has them: {unused:?}"
    );
}

#[test]
fn every_file_system_keyword_is_listed() {
    let rows = documented();
    let listed = &rows["export.io"].keywords;
    for kind in [
        ErrorKind::NotFound,
        ErrorKind::PermissionDenied,
        ErrorKind::IsADirectory,
        ErrorKind::StorageFull,
        ErrorKind::Interrupted,
    ] {
        let report = ExportError::Io(kind).report();
        let Some(ErrorParam::Keyword(keyword)) = report.param("kind") else {
            panic!("export.io has a keyword");
        };
        assert!(listed.iter().any(|listed| listed == keyword), "{keyword}");
    }
}

#[test]
fn an_io_error_keeps_its_kind() {
    let error = ExportError::from(std::io::Error::from(ErrorKind::PermissionDenied));
    assert_eq!(error, ExportError::Io(ErrorKind::PermissionDenied));
}
