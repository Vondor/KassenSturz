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
        ExportError::AlreadyExists,
        ExportError::BadSettings,
        ExportError::CurrencyMismatch {
            expected: EUR,
            found: USD,
        },
        ExportError::BadDecimalPlaces {
            currency: EUR,
            digits: 40,
        },
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
fn a_file_that_exists_is_not_an_io_error() {
    let error = ExportError::from(std::io::Error::from(ErrorKind::AlreadyExists));
    assert_eq!(error, ExportError::AlreadyExists);
    let error = ExportError::from(std::io::Error::from(ErrorKind::PermissionDenied));
    assert_eq!(error, ExportError::Io(ErrorKind::PermissionDenied));
}
