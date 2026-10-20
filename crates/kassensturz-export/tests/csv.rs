// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Golden-file tests for the CSV exporter, and writing files. All data is
//! made up (AGENTS.md §3.1 rule 2).

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use kassensturz_core::{CurrencyCode, Date, Money};
use kassensturz_export::{
    AccountTransactions, Counterpart, CsvExport, CsvExportSettings, DecimalMark, ExportData,
    ExportError, ExportKind, Exporter, ExporterInfo, Separator, TransactionRow, export,
    export_file,
};

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};
const JPY: CurrencyCode = match CurrencyCode::new("JPY") {
    Ok(code) => code,
    Err(_) => panic!("JPY is a valid code"),
};

fn row(
    day: i8,
    payee: Option<&str>,
    counterpart: Counterpart,
    memo: Option<&str>,
    amount: Money,
) -> TransactionRow {
    TransactionRow {
        date: Date::new(2026, 10, day).expect("a valid date"),
        payee: payee.map(str::to_owned),
        counterpart,
        memo: memo.map(str::to_owned),
        amount,
    }
}

/// Two accounts in two currencies, with every kind of counterpart and the
/// texts that need care: separators, quotes, line breaks, umlauts and a
/// formula.
fn sample() -> ExportData {
    let eur = |minor| Money::from_minor_units(minor, EUR);
    ExportData::Transactions(vec![
        AccountTransactions {
            account: "Girokonto; privat".to_owned(),
            currency: EUR,
            minor_digits: 2,
            rows: vec![
                row(
                    1,
                    Some("Arbeitgeber GmbH"),
                    Counterpart::Category("Gehalt".to_owned()),
                    Some("Gehalt Oktober"),
                    eur(250_000),
                ),
                row(
                    5,
                    Some("Bäckerei \"Zum Hörnchen\""),
                    Counterpart::Category("Essen/Lebensmittel".to_owned()),
                    Some("Karte 4711\nBrötchen"),
                    eur(-1_250),
                ),
                row(
                    6,
                    None,
                    Counterpart::Transfer("Sparkonto".to_owned()),
                    None,
                    eur(-30_000),
                ),
                row(7, Some("Markt"), Counterpart::Parts(2), None, eur(-4_000)),
                row(
                    8,
                    Some("=1+1"),
                    Counterpart::Uncategorized,
                    Some("@home"),
                    eur(-1),
                ),
                row(9, Some("Refund"), Counterpart::Uncategorized, None, eur(0)),
            ],
        },
        AccountTransactions {
            account: "Reisekasse".to_owned(),
            currency: JPY,
            minor_digits: 0,
            rows: vec![row(
                10,
                Some("Ramen"),
                Counterpart::Uncategorized,
                None,
                Money::from_minor_units(-980, JPY),
            )],
        },
    ])
}

fn golden(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/csv")
        .join(name);
    fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

#[test]
fn the_default_settings_write_rfc_4180_with_a_point() {
    let bytes = export(&CsvExport::default(), &sample()).expect("exported");
    assert_eq!(
        String::from_utf8_lossy(&bytes),
        String::from_utf8_lossy(&golden("default.csv"))
    );
}

#[test]
fn excel_for_german_users_gets_semicolons_a_comma_and_a_byte_order_mark() {
    let exporter = CsvExport::new(CsvExportSettings {
        separator: Separator::Semicolon,
        decimal_mark: DecimalMark::Comma,
        header: true,
        byte_order_mark: true,
    })
    .expect("valid settings");
    let bytes = export(&exporter, &sample()).expect("exported");
    assert_eq!(
        String::from_utf8_lossy(&bytes),
        String::from_utf8_lossy(&golden("excel-german.csv"))
    );
}

#[test]
fn tabs_separate_without_quoting_semicolons() {
    let exporter = CsvExport::new(CsvExportSettings {
        separator: Separator::Tab,
        header: false,
        ..CsvExportSettings::default()
    })
    .expect("valid settings");
    let bytes = export(&exporter, &sample()).expect("exported");
    assert_eq!(
        String::from_utf8_lossy(&bytes),
        String::from_utf8_lossy(&golden("tab.csv"))
    );
}

#[test]
fn the_exporter_says_what_it_writes() {
    assert_eq!(
        CsvExport::default().info(),
        ExporterInfo {
            id: "csv",
            kind: ExportKind::Transactions,
            extension: "csv"
        }
    );
}

/// A folder of its own for a test.
fn folder(name: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("export-{name}"));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("test folder");
    path
}

#[test]
fn a_file_is_written_once_and_never_overwritten() {
    let path = folder("once").join("transactions.csv");
    export_file(&CsvExport::default(), &sample(), &path).expect("exported");
    assert_eq!(fs::read(&path).expect("written"), golden("default.csv"));
    fs::write(&path, b"mine").expect("changed");
    assert_eq!(
        export_file(&CsvExport::default(), &sample(), &path),
        Err(ExportError::AlreadyExists)
    );
    assert_eq!(fs::read(&path).expect("kept"), b"mine");
}

#[test]
fn a_missing_folder_is_an_io_error() {
    let path = folder("missing").join("nowhere/transactions.csv");
    assert_eq!(
        export_file(&CsvExport::default(), &sample(), &path),
        Err(ExportError::Io(std::io::ErrorKind::NotFound))
    );
}

#[test]
fn data_that_cannot_be_written_exactly_leaves_no_file() {
    let path = folder("bad-data").join("transactions.csv");
    let ExportData::Transactions(mut accounts) = sample() else {
        unreachable!()
    };
    accounts[1].minor_digits = 30;
    let result = export_file(
        &CsvExport::default(),
        &ExportData::Transactions(accounts),
        &path,
    );
    assert_eq!(
        result,
        Err(ExportError::BadDecimalPlaces {
            currency: JPY,
            digits: 30
        })
    );
    assert!(!path.exists());
}

/// An exporter that fails half-way, as a full disk would.
struct Failing;

impl Exporter for Failing {
    fn info(&self) -> ExporterInfo {
        ExporterInfo {
            id: "failing",
            kind: ExportKind::Transactions,
            extension: "txt",
        }
    }

    fn write(&self, _data: &ExportData, out: &mut dyn Write) -> Result<(), ExportError> {
        out.write_all(b"half a file")?;
        Err(ExportError::Io(std::io::ErrorKind::StorageFull))
    }
}

#[test]
fn a_failed_export_removes_the_file_it_began() {
    let path = folder("failing").join("transactions.txt");
    assert_eq!(
        export_file(&Failing, &sample(), &path),
        Err(ExportError::Io(std::io::ErrorKind::StorageFull))
    );
    assert!(!path.exists());
}
