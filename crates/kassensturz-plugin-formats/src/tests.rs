// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fmt::Write as _;

use kassensturz_core::{CurrencyCode, Date, Money, Report};
use kassensturz_export::{
    AccountTransactions, Counterpart, ExportData, ExportKind, Exporter, OriginalRow,
    TransactionRow, export,
};
use kassensturz_import::{
    EntryStatus, ImportError, ImportKind, Imported, Importer, Limits, Recognition, import,
};
use kassensturz_plugin_api::MANIFEST_SECTION;
use kassensturz_plugin_host::{HostError, Plugin};
use serde_json::json;

use crate::{PluginExporter, PluginImporter};

/// A plugin file with this manifest ID, extension points and module.
fn plugin_file(id: &str, points: &[&str], wat: &str) -> Vec<u8> {
    plugin_file_asking(id, points, &[], wat)
}

/// A plugin file that also asks for `permissions`.
pub(crate) fn plugin_file_asking(
    id: &str,
    points: &[&str],
    permissions: &[&str],
    wat: &str,
) -> Vec<u8> {
    let mut bytes = wat::parse_str(wat).unwrap();
    let manifest = json!({
        "manifest_version": 1,
        "id": id,
        "name": "Test plugin",
        "version": "1.0.0",
        "api": "1.0",
        "extension_points": points,
        "permissions": permissions,
    })
    .to_string();
    let mut content = vec![u8::try_from(MANIFEST_SECTION.len()).unwrap()];
    content.extend(MANIFEST_SECTION.as_bytes());
    content.extend(manifest.as_bytes());
    bytes.push(0);
    let mut size = content.len();
    loop {
        let byte = u8::try_from(size & 0x7F).unwrap();
        size >>= 7;
        if size == 0 {
            bytes.push(byte);
            break;
        }
        bytes.push(byte | 0x80);
    }
    bytes.extend(content);
    bytes
}

/// A module whose exports each hand back a fixed answer, or, for an answer
/// of `None`, the input itself. Answers live from address 64 Ki on; the
/// host's input is put from address 1 Mi on.
pub(crate) fn module(calls: &[(&str, Option<&str>)]) -> String {
    let mut wat = String::from(
        r#"(module
            (memory (export "memory") 32)
            (global $next (mut i32) (i32.const 1048576))
            (func (export "ks_alloc") (param $len i32) (result i32)
                (local $at i32)
                (local.set $at (global.get $next))
                (global.set $next (i32.add (global.get $next) (local.get $len)))
                (local.get $at))"#,
    );
    let mut at = 65_536_u32;
    for (name, answer) in calls {
        match answer {
            Some(answer) => {
                let mut data = String::new();
                for byte in answer.bytes() {
                    write!(data, "\\{byte:02x}").unwrap();
                }
                write!(
                    wat,
                    r#"(data (i32.const {at}) "{data}")
                    (func (export "{name}") (param i32 i32) (result i64)
                        (i64.const {result}))"#,
                    result = (u64::from(at) << 32) | answer.len() as u64,
                )
                .unwrap();
                at += u32::try_from(answer.len()).unwrap() + 16;
            }
            None => write!(
                wat,
                r#"(func (export "{name}") (param $ptr i32) (param $len i32) (result i64)
                    (i64.or
                        (i64.shl (i64.extend_i32_u (local.get $ptr)) (i64.const 32))
                        (i64.extend_i32_u (local.get $len))))"#
            )
            .unwrap(),
        }
    }
    wat.push(')');
    wat
}

fn load(id: &str, points: &[&str], calls: &[(&str, Option<&str>)]) -> Plugin {
    Plugin::load(&plugin_file(id, points, &module(calls)), false).unwrap()
}

const INFO: &str = r#"{"extensions": ["csv", "txt"]}"#;

fn importer(read: &str) -> PluginImporter {
    PluginImporter::new(load(
        "org.example.bank-csv",
        &["importer"],
        &[
            ("ks_importer_info", Some(INFO)),
            ("ks_importer_recognize", Some(r#"{"recognition": "maybe"}"#)),
            ("ks_importer_read", Some(read)),
        ],
    ))
    .unwrap()
}

pub(crate) fn date(text: &str) -> Date {
    text.parse().unwrap()
}

pub(crate) fn eur(minor: i64) -> Money {
    Money::from_minor_units(minor, CurrencyCode::new("EUR").unwrap())
}

const STATEMENTS: &str = r#"{"statements": [{
    "account": {"iban": "DE89370400440532013000"},
    "currency": "EUR",
    "opening_balance": {"amount": "100000", "date": "2026-10-31"},
    "closing_balance": {"amount": "97655", "date": "2026-11-02"},
    "entries": [
        {"status": "booked", "booking_date": "2026-11-01", "amount": "-2345",
         "other_party_name": "Example Bakery", "remittance_information": "Bread"},
        {"status": "pending", "bank_id": "T-2", "booking_date": "2026-11-02",
         "value_date": "2026-11-03", "amount": "0"}
    ]
}]}"#;

#[test]
fn a_plugin_importer_reads_statements_through_the_common_import() {
    let importer = importer(STATEMENTS);
    let info = importer.info();
    assert_eq!(info.id, "org.example.bank-csv");
    assert_eq!(info.kind, ImportKind::Statements);
    assert_eq!(info.extensions, ["csv", "txt"]);
    assert_eq!(importer.recognize(b"Date;Amount"), Recognition::Maybe);

    let Imported::Statements(statements) =
        import(&importer, b"any bytes", &Limits::default()).expect("statements")
    else {
        panic!("statements expected");
    };
    assert_eq!(statements.len(), 1);
    let statement = &statements[0];
    assert_eq!(
        statement.account.iban.as_deref(),
        Some("DE89370400440532013000")
    );
    assert_eq!(statement.account.number, None);
    assert_eq!(statement.currency.minor_digits(), 2);
    let opening = statement.opening_balance.unwrap();
    assert_eq!(
        (opening.amount, opening.date),
        (eur(100_000), date("2026-10-31"))
    );
    assert_eq!(statement.closing_balance.unwrap().amount, eur(97_655));
    let [first, second] = statement.entries.as_slice() else {
        panic!("two entries");
    };
    assert_eq!(first.status, EntryStatus::Booked);
    assert_eq!(first.amount, eur(-2345));
    assert_eq!(first.booking_date, date("2026-11-01"));
    assert_eq!(first.other_party_name.as_deref(), Some("Example Bakery"));
    assert_eq!(first.remittance_information.as_deref(), Some("Bread"));
    assert_eq!(first.value_date, None);
    assert_eq!(second.status, EntryStatus::Pending);
    assert_eq!(second.bank_id.as_deref(), Some("T-2"));
    assert_eq!(second.value_date, Some(date("2026-11-03")));
}

#[test]
fn the_plugin_can_refuse_a_file_it_cant_read() {
    let error = importer(r#"{"error": "malformed", "line": 3}"#)
        .read(b"x", &Limits::default())
        .unwrap_err();
    assert_eq!(error, ImportError::Malformed { line: Some(3) });
    let error = importer(r#"{"error": "malformed"}"#)
        .read(b"x", &Limits::default())
        .unwrap_err();
    assert_eq!(error, ImportError::Malformed { line: None });
}

#[test]
fn an_answer_kassensturz_cant_use_is_refused_whole() {
    let statement = |entry: &str| {
        format!(
            r#"{{"statements": [{{"account": {{}}, "currency": "EUR", "entries": [{entry}]}}]}}"#
        )
    };
    let fine = r#"{"status": "booked", "booking_date": "2026-11-01", "amount": "5"}"#;
    assert!(
        importer(&statement(fine))
            .read(b"x", &Limits::default())
            .is_ok()
    );
    let wrong = [
        "not json".to_owned(),
        "[]".to_owned(),
        r#"{"statements": [], "more": 1}"#.to_owned(),
        r#"{"error": "something else"}"#.to_owned(),
        r#"{"error": "malformed", "line": 0}"#.to_owned(),
        statement(r#"{"status": "booked", "booking_date": "2026-11-01", "amount": 5}"#),
        statement(r#"{"status": "booked", "booking_date": "2026-11-01", "amount": "5.00"}"#),
        statement(r#"{"status": "booked", "booking_date": "2026-11-01", "amount": "5e2"}"#),
        statement(r#"{"status": "done", "booking_date": "2026-11-01", "amount": "5"}"#),
        statement(r#"{"status": "booked", "booking_date": "2026-02-30", "amount": "5"}"#),
        statement(
            r#"{"status": "booked", "booking_date": "2026-11-01", "amount": "5", "memo": "x"}"#,
        ),
        statement(
            r#"{"status": "booked", "booking_date": "2026-11-01", "amount": "5", "bank_id": null}"#,
        ),
        r#"{"statements": [{"account": {}, "currency": "eur", "entries": []}]}"#.to_owned(),
    ];
    for answer in wrong {
        let error = importer(&answer)
            .read(b"x", &Limits::default())
            .unwrap_err();
        assert_eq!(
            error.report().code,
            "plugin-host.output-invalid",
            "{answer}"
        );
    }
}

#[test]
fn a_currency_that_isnt_current_is_named() {
    let error = importer(r#"{"statements": [{"account": {}, "currency": "XYZ", "entries": []}]}"#)
        .read(b"x", &Limits::default())
        .unwrap_err();
    assert_eq!(error.report().code, "import.unknown-currency");
}

#[test]
fn the_common_checks_hold_for_a_plugin_too() {
    // An entry text longer than the limit is caught by `import`, not
    // trusted because a plugin made it.
    let long = "x".repeat(Limits::default().max_text_bytes + 1);
    let answer = format!(
        r#"{{"statements": [{{"account": {{}}, "currency": "EUR", "entries": [
            {{"status": "booked", "booking_date": "2026-11-01", "amount": "5",
              "remittance_information": "{long}"}}]}}]}}"#
    );
    let error = import(&importer(&answer), b"x", &Limits::default()).unwrap_err();
    assert_eq!(error.report().code, "import.text-too-long");
}

#[test]
fn a_plugin_that_fails_reports_the_hosts_code() {
    let importer = PluginImporter::new(load(
        "org.example.no-read",
        &["importer"],
        &[("ks_importer_info", Some(INFO))],
    ))
    .unwrap();
    let error = importer.read(b"x", &Limits::default()).unwrap_err();
    assert_eq!(error.report().code, "plugin-host.export-missing");
    // Recognizing never fails: a plugin that can't answer says no.
    assert_eq!(importer.recognize(b"x"), Recognition::No);
}

#[test]
fn bad_info_or_another_extension_point_is_refused() {
    let with_info = |info: &str| {
        PluginImporter::new(load(
            "org.example.info",
            &["importer"],
            &[("ks_importer_info", Some(info))],
        ))
        .unwrap_err()
    };
    for info in [
        r#"{"extensions": ["CSV"]}"#,
        r#"{"extensions": ["csv", "csv"]}"#,
        r#"{"extensions": [".csv"]}"#,
        r#"{"extensions": ["a","b","c","d","e","f","g","h","i"]}"#,
        r#"{"extensions": [], "name": "x"}"#,
    ] {
        assert_eq!(with_info(info), HostError::OutputInvalid, "{info}");
    }
    let exporter_only = load(
        "org.example.export-only",
        &["exporter"],
        &[("ks_importer_info", Some(INFO))],
    );
    assert!(matches!(
        PluginImporter::new(exporter_only),
        Err(HostError::ExportMissing { .. })
    ));
}

pub(crate) fn transactions() -> ExportData {
    let usd = CurrencyCode::new("USD").unwrap();
    ExportData::Transactions(vec![AccountTransactions {
        account: "Checking".to_owned(),
        currency: CurrencyCode::new("EUR").unwrap(),
        minor_digits: 2,
        rows: vec![
            TransactionRow {
                date: date("2026-11-01"),
                payee: Some("Example Bakery".to_owned()),
                counterpart: Counterpart::Category("Food".to_owned()),
                memo: None,
                note: Some("Sunday".to_owned()),
                amount: eur(-2345),
                original: Some(OriginalRow {
                    amount: Money::from_minor_units(-2500, usd),
                    minor_digits: 2,
                }),
            },
            TransactionRow {
                date: date("2026-11-02"),
                payee: None,
                counterpart: Counterpart::Parts(3),
                memo: Some("Shop".to_owned()),
                note: None,
                amount: eur(9_007_199_254_740_993),
                original: None,
            },
        ],
    }])
}

fn echo_exporter() -> PluginExporter {
    PluginExporter::new(load(
        "org.example.echo-export",
        &["exporter"],
        &[
            ("ks_exporter_info", Some(r#"{"extension": "json"}"#)),
            ("ks_exporter_write", None),
        ],
    ))
    .unwrap()
}

#[test]
fn a_plugin_exporter_gets_the_transactions_with_amounts_as_text() {
    let exporter = echo_exporter();
    let info = exporter.info();
    assert_eq!(info.id, "org.example.echo-export");
    assert_eq!(info.kind, ExportKind::Transactions);
    assert_eq!(info.extension, "json");

    // The echo plugin hands its input back, so the file is what it got.
    let bytes = export(&exporter, &transactions()).unwrap();
    let got: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        got,
        json!({"accounts": [{
            "name": "Checking",
            "currency": "EUR",
            "minor_digits": 2,
            "rows": [
                {
                    "date": "2026-11-01",
                    "payee": "Example Bakery",
                    "counterpart": {"kind": "category", "name": "Food"},
                    "memo": null,
                    "note": "Sunday",
                    "amount": "-2345",
                    "original": {"amount": "-2500", "currency": "USD", "minor_digits": 2},
                },
                {
                    "date": "2026-11-02",
                    "payee": null,
                    "counterpart": {"kind": "parts", "count": 3},
                    "memo": "Shop",
                    "note": null,
                    // Larger than a JSON number keeps exactly in many languages.
                    "amount": "9007199254740993",
                    "original": null,
                },
            ],
        }]})
    );
}

#[test]
fn every_kind_of_counterpart_is_named() {
    let kinds = [
        (Counterpart::Uncategorized, json!({"kind": "uncategorized"})),
        (
            Counterpart::Transfer("Savings".to_owned()),
            json!({"kind": "transfer", "account": "Savings"}),
        ),
    ];
    for (counterpart, expected) in kinds {
        let ExportData::Transactions(mut accounts) = transactions() else {
            unreachable!()
        };
        accounts[0].rows[0].counterpart = counterpart;
        let input = crate::exporter::input(&accounts);
        assert_eq!(input["accounts"][0]["rows"][0]["counterpart"], expected);
    }
}

#[test]
fn a_plugin_exporter_reports_failures() {
    let failing = PluginExporter::new(load(
        "org.example.no-write",
        &["exporter"],
        &[("ks_exporter_info", Some(r#"{"extension": "csv"}"#))],
    ))
    .unwrap();
    let error = export(&failing, &transactions()).unwrap_err();
    assert_eq!(error.report().code, "plugin-host.export-missing");

    for info in [r#"{"extension": "CSV"}"#, r#"{"extension": ["csv"]}"#, "{}"] {
        let plugin = load(
            "org.example.bad-info",
            &["exporter"],
            &[("ks_exporter_info", Some(info))],
        );
        assert_eq!(
            PluginExporter::new(plugin).unwrap_err(),
            HostError::OutputInvalid,
            "{info}"
        );
    }
}
