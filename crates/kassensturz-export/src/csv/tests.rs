// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Unit tests for the CSV exporter. Names and amounts are made up
//! (AGENTS.md §3.1 rule 2).

use kassensturz_core::{CurrencyCode, Date, Money};
use proptest::prelude::*;

use super::*;
use crate::export;

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};
const JPY: CurrencyCode = match CurrencyCode::new("JPY") {
    Ok(code) => code,
    Err(_) => panic!("JPY is a valid code"),
};

fn row(minor: i64, payee: Option<&str>) -> TransactionRow {
    TransactionRow {
        date: Date::constant(2026, 10, 5),
        payee: payee.map(str::to_owned),
        counterpart: Counterpart::Uncategorized,
        memo: None,
        amount: Money::from_minor_units(minor, EUR),
    }
}

fn account(rows: Vec<TransactionRow>) -> ExportData {
    ExportData::Transactions(vec![AccountTransactions {
        account: "Checking".to_owned(),
        currency: EUR,
        minor_digits: 2,
        rows,
    }])
}

fn csv(settings: CsvExportSettings, data: &ExportData) -> String {
    let exporter = CsvExport::new(settings).expect("valid settings");
    String::from_utf8(export(&exporter, data).expect("exported")).expect("UTF-8")
}

#[test]
fn amounts_are_exact_with_the_currencys_decimal_places() {
    let exporter = CsvExport::default();
    let text = |minor, digits| {
        exporter
            .amount(Money::from_minor_units(minor, EUR), digits)
            .expect("written")
    };
    assert_eq!(text(1234, 2), "12.34");
    assert_eq!(text(-5, 2), "-0.05");
    assert_eq!(text(0, 2), "0.00");
    assert_eq!(text(-7, 0), "-7");
    assert_eq!(text(12345, 3), "12.345");
    // The smallest amount does not overflow on the way to text.
    assert_eq!(text(i64::MIN, 2), "-92233720368547758.08");
    assert_eq!(text(i64::MAX, 0), "9223372036854775807");
}

#[test]
fn a_decimal_comma_replaces_the_point_only() {
    let settings = CsvExportSettings {
        separator: Separator::Semicolon,
        decimal_mark: DecimalMark::Comma,
        ..CsvExportSettings::default()
    };
    let text = csv(settings, &account(vec![row(-1250, Some("Shop; Co"))]));
    assert!(
        text.ends_with("2026-10-05;Checking;\"Shop; Co\";;;;-12,50;EUR;\r\n"),
        "{text}"
    );
}

#[test]
fn a_comma_cannot_be_both_separator_and_decimal_mark() {
    let settings = CsvExportSettings {
        decimal_mark: DecimalMark::Comma,
        ..CsvExportSettings::default()
    };
    assert_eq!(CsvExport::new(settings), Err(ExportError::BadSettings));
}

#[test]
fn fields_with_quotes_and_line_breaks_are_quoted_as_rfc_4180_says() {
    let mut transaction = row(100, Some("The \"Corner\" Shop"));
    transaction.memo = Some("line one\nline two".to_owned());
    let text = csv(CsvExportSettings::default(), &account(vec![transaction]));
    assert!(
        text.ends_with(
            "2026-10-05,Checking,\"The \"\"Corner\"\" Shop\",,,\"line one\nline two\",1.00,EUR,\r\n"
        ),
        "{text:?}"
    );
}

#[test]
fn texts_a_spreadsheet_would_run_as_a_formula_get_a_quote_in_front() {
    for start in ["=", "+", "-", "@", "\t", "\r"] {
        let payee = format!("{start}SUM(A1)");
        let text = csv(
            CsvExportSettings {
                header: false,
                ..CsvExportSettings::default()
            },
            &account(vec![row(-100, Some(&payee))]),
        );
        assert!(
            text.contains(&format!("'{start}SUM(A1)")),
            "{start:?}: {text:?}"
        );
    }
    // The amount is a number and keeps its minus sign.
    let text = csv(
        CsvExportSettings::default(),
        &account(vec![row(-100, None)]),
    );
    assert!(text.contains(",-1.00,EUR,"), "{text}");
    // Other texts stay as they are.
    assert_eq!(guard_formula("Rent = 500"), "Rent = 500");
}

#[test]
fn the_header_and_byte_order_mark_are_optional() {
    let data = account(vec![row(100, None)]);
    let plain = csv(CsvExportSettings::default(), &data);
    assert!(
        plain.starts_with("date,account,payee,category,transfer,memo,amount,currency,parts\r\n")
    );
    let bare = csv(
        CsvExportSettings {
            header: false,
            byte_order_mark: true,
            ..CsvExportSettings::default()
        },
        &data,
    );
    assert_eq!(bare, "\u{feff}2026-10-05,Checking,,,,,1.00,EUR,\r\n");
}

#[test]
fn counterparts_fill_their_own_columns() {
    let rows = [
        (Counterpart::Uncategorized, ",,,"),
        (
            Counterpart::Category("Food/Groceries".to_owned()),
            "Food/Groceries,,,",
        ),
        (Counterpart::Transfer("Savings".to_owned()), ",Savings,,"),
        (Counterpart::Parts(3), ",,,"),
    ];
    for (counterpart, columns) in rows {
        let mut transaction = row(-100, Some("Shop"));
        transaction.counterpart = counterpart.clone();
        let text = csv(
            CsvExportSettings {
                header: false,
                ..CsvExportSettings::default()
            },
            &account(vec![transaction]),
        );
        let parts = if let Counterpart::Parts(count) = counterpart {
            count.to_string()
        } else {
            String::new()
        };
        assert_eq!(
            text,
            format!("2026-10-05,Checking,Shop,{columns}-1.00,EUR,{parts}\r\n"),
            "{counterpart:?}"
        );
    }
}

#[test]
fn accounts_follow_each_other_and_keep_their_own_currency() {
    let data = ExportData::Transactions(vec![
        AccountTransactions {
            account: "Checking".to_owned(),
            currency: EUR,
            minor_digits: 2,
            rows: vec![row(100, None)],
        },
        AccountTransactions {
            account: "Wallet".to_owned(),
            currency: JPY,
            minor_digits: 0,
            rows: vec![TransactionRow {
                amount: Money::from_minor_units(-500, JPY),
                ..row(0, None)
            }],
        },
    ]);
    let text = csv(
        CsvExportSettings {
            header: false,
            ..CsvExportSettings::default()
        },
        &data,
    );
    assert_eq!(
        text,
        "2026-10-05,Checking,,,,,1.00,EUR,\r\n2026-10-05,Wallet,,,,,-500,JPY,\r\n"
    );
}

#[test]
fn an_account_without_rows_gives_only_the_header() {
    let text = csv(CsvExportSettings::default(), &account(Vec::new()));
    assert_eq!(
        text,
        "date,account,payee,category,transfer,memo,amount,currency,parts\r\n"
    );
}

#[test]
fn data_that_contradicts_itself_is_refused() {
    let mut data = account(vec![row(100, None)]);
    let ExportData::Transactions(accounts) = &mut data;
    accounts[0].rows[0].amount = Money::from_minor_units(100, JPY);
    assert_eq!(
        export(&CsvExport::default(), &data),
        Err(ExportError::CurrencyMismatch {
            expected: EUR,
            found: JPY
        })
    );
    let ExportData::Transactions(accounts) = &mut data;
    accounts[0].rows[0].amount = Money::from_minor_units(100, EUR);
    accounts[0].minor_digits = 19;
    assert_eq!(
        export(&CsvExport::default(), &data),
        Err(ExportError::BadDecimalPlaces {
            currency: EUR,
            digits: 19
        })
    );
}

proptest! {
    /// Taking the decimal mark out of the text gives the minor units back, so
    /// no digit is lost, moved or rounded.
    #[test]
    fn an_amount_written_reads_back_to_the_same_minor_units(
        minor in any::<i64>(),
        digits in 0_u8..=18,
    ) {
        let exporter = CsvExport::default();
        let text = exporter
            .amount(Money::from_minor_units(minor, EUR), digits)
            .expect("written");
        let digits_only: String = text.chars().filter(|c| *c != '.').collect();
        prop_assert_eq!(digits_only.parse::<i128>().expect("a number"), i128::from(minor));
        let decimals = text.split('.').nth(1).map_or(0, str::len);
        prop_assert_eq!(decimals, usize::from(digits));
    }
}
