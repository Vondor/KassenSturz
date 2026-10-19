// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading files within the limits, and checking what an importer returns.

use std::fs;
use std::path::PathBuf;

use kassensturz_core::{Currency, CurrencyCode, Date, Money, Report};
use kassensturz_import::{
    Entry, ImportError, ImportKind, Imported, Importer, ImporterInfo, Limits, Recognition,
    Statement, StatementAccount, import, import_file, read_file,
};

use crate::test_format::TestFormat;

/// A file of its own for a test, in Cargo's folder for test files.
fn file(name: &str, content: &[u8]) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("files-{name}"));
    fs::write(&path, content).expect("test file written");
    path
}

const SAMPLE: &[u8] = b"KSTEST\n\
    account;DE89370400440532013000;;EUR\n\
    entry;booked;2026-10-01;;-25.00;;;;\n";

#[test]
fn a_file_within_the_limits_is_read() {
    let path = file("within", SAMPLE);
    let limits = Limits {
        max_file_bytes: SAMPLE.len().try_into().expect("a small file"),
        ..Limits::default()
    };
    assert_eq!(read_file(&path, &limits).as_deref(), Ok(SAMPLE));
    let Ok(Imported::Statements(statements)) = import_file(&TestFormat, &path, &limits) else {
        panic!("the sample is imported");
    };
    assert_eq!(statements.len(), 1);
}

#[test]
fn a_file_one_byte_too_large_is_refused() {
    let path = file("too-large", SAMPLE);
    let limits = Limits {
        max_file_bytes: u64::try_from(SAMPLE.len()).expect("a small file") - 1,
        ..Limits::default()
    };
    let too_large = ImportError::FileTooLarge {
        max_bytes: limits.max_file_bytes,
    };
    assert_eq!(read_file(&path, &limits), Err(too_large.clone()));
    assert_eq!(
        import_file(&TestFormat, &path, &limits),
        Err(too_large.clone())
    );
    // Also when the bytes don't come from a file.
    assert_eq!(import(&TestFormat, SAMPLE, &limits), Err(too_large));
}

#[test]
fn a_missing_file_says_so() {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("files-never-written");
    let error = read_file(&path, &Limits::default()).expect_err("there is no file");
    assert_eq!(error, ImportError::Io(std::io::ErrorKind::NotFound));
    assert_eq!(error.report().code, "import.io");
}

#[test]
fn too_many_entries_stop_the_reader() {
    let limits = Limits {
        max_entries: 1,
        ..Limits::default()
    };
    let two = [SAMPLE, b"entry;booked;2026-10-02;;-1.00;;;;\n"].concat();
    assert_eq!(
        import(&TestFormat, &two, &limits),
        Err(ImportError::TooManyEntries { max: 1 })
    );
}

/// An importer that returns whatever it is given, whatever the file says,
/// as a careless plugin might.
struct Careless(Statement);

impl Importer for Careless {
    fn info(&self) -> ImporterInfo {
        ImporterInfo {
            id: "careless",
            kind: ImportKind::Statements,
            extensions: &[],
        }
    }

    fn recognize(&self, _start: &[u8]) -> Recognition {
        Recognition::Maybe
    }

    fn read(&self, _input: &[u8], _limits: &Limits) -> Result<Imported, ImportError> {
        Ok(Imported::Statements(vec![self.0.clone()]))
    }
}

#[test]
fn what_an_importer_returns_is_checked() {
    let code = |text| CurrencyCode::new(text).expect("a valid code");
    let date = Date::new(2026, 10, 5).expect("a valid date");
    let statement = |entries| Statement {
        account: StatementAccount::default(),
        currency: Currency::new(code("EUR"), 2).expect("valid decimal places"),
        opening_balance: None,
        closing_balance: None,
        entries,
    };
    let dollars = Entry::booked(date, Money::from_minor_units(100, code("USD")));
    assert_eq!(
        import(&Careless(statement(vec![dollars])), b"", &Limits::default()),
        Err(ImportError::CurrencyMismatch {
            expected: code("EUR"),
            found: code("USD"),
        })
    );

    let mut long = Entry::booked(date, Money::from_minor_units(100, code("EUR")));
    long.other_party_name = Some("x".repeat(17));
    let limits = Limits {
        max_text_bytes: 16,
        ..Limits::default()
    };
    assert_eq!(
        import(&Careless(statement(vec![long.clone()])), b"", &limits),
        Err(ImportError::TextTooLong {
            max_bytes: 16,
            line: None,
        })
    );

    let limits = Limits {
        max_entries: 1,
        ..Limits::default()
    };
    assert_eq!(
        import(&Careless(statement(vec![long.clone(), long])), b"", &limits),
        Err(ImportError::TooManyEntries { max: 1 })
    );
}
