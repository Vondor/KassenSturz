// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Plugin reports and automations (#791 step 5, D-926).

use kassensturz_core::{CurrencyCode, Report};
use kassensturz_export::ExportData;
use kassensturz_plugin_api::Permission;
use kassensturz_plugin_host::{HostError, Plugin};
use serde_json::{Value, json};

use crate::tests::{date, eur, module, plugin_file_asking, transactions};
use crate::{
    Cell, Column, ColumnKind, NewTransaction, PluginAutomation, PluginReport, ReportInfo,
    ReportTable, Suggestion,
};

const REPORTS: &str = r#"{"reports": [
    {"id": "monthly", "name": "Monthly summary"},
    {"id": "largest", "name": "Largest payments"}
]}"#;

fn plugin(points: &[&str], permissions: &[&str], calls: &[(&str, Option<&str>)]) -> Plugin {
    Plugin::load(
        &plugin_file_asking("org.example.reports", points, permissions, &module(calls)),
        false,
    )
    .unwrap()
}

fn report(run: Option<&str>) -> Result<PluginReport, HostError> {
    PluginReport::new(plugin(
        &["report"],
        &["read_book"],
        &[("ks_report_info", Some(REPORTS)), ("ks_report_run", run)],
    ))
}

fn accounts() -> Vec<kassensturz_export::AccountTransactions> {
    let ExportData::Transactions(accounts) = transactions() else {
        unreachable!()
    };
    accounts
}

fn table(value: &Value) -> Result<ReportTable, HostError> {
    report(Some(&value.to_string()))
        .unwrap()
        .run("monthly", &accounts())
}

fn good() -> Value {
    json!({
        "title": "Monthly summary",
        "columns": [
            {"label": "Month", "kind": "text"},
            {"label": "Day", "kind": "date"},
            {"label": "Left over", "kind": "amount"},
            {"label": "Payments", "kind": "count"}
        ],
        "rows": [
            ["November 2026", "2026-11-30", {"amount": "-123456", "currency": "EUR", "minor_digits": 2}, "12"],
            ["", null, null, null]
        ]
    })
}

#[test]
fn a_report_plugin_names_its_reports() {
    let report = report(None).unwrap();
    assert_eq!(
        report.reports(),
        [
            ReportInfo {
                id: "monthly".into(),
                name: "Monthly summary".into()
            },
            ReportInfo {
                id: "largest".into(),
                name: "Largest payments".into()
            }
        ]
    );
}

#[test]
fn a_report_gets_the_transactions_and_which_report() {
    // The echo plugin hands back what it got, which isn't a table: refused,
    // but first read what it was sent through a plugin that answers it.
    let echo = report(None).unwrap();
    assert_eq!(
        echo.run("monthly", &accounts()),
        Err(HostError::OutputInvalid)
    );
    let sent = crate::report::input("monthly", &accounts());
    assert_eq!(sent["report"], "monthly");
    assert_eq!(
        sent["accounts"],
        crate::exporter::input(&accounts())["accounts"]
    );
    assert_eq!(sent["accounts"][0]["rows"][0]["amount"], "-2345");
    // A report the plugin doesn't offer isn't asked for.
    assert_eq!(
        echo.run("weekly", &accounts()),
        Err(HostError::ExportMissing {
            name: "weekly".into()
        })
    );
}

#[test]
fn a_table_is_read_with_exact_amounts() {
    let table = table(&good()).unwrap();
    assert_eq!(table.title, "Monthly summary");
    assert_eq!(
        table.columns[2],
        Column {
            label: "Left over".into(),
            kind: ColumnKind::Amount
        }
    );
    assert_eq!(
        table.rows[0],
        [
            Cell::Text("November 2026".into()),
            Cell::Date(date("2026-11-30")),
            Cell::Amount {
                minor_units: -123_456,
                currency: CurrencyCode::new("EUR").unwrap(),
                minor_digits: 2
            },
            Cell::Count(12),
        ]
    );
    assert_eq!(
        table.rows[1],
        [
            Cell::Text(String::new()),
            Cell::Empty,
            Cell::Empty,
            Cell::Empty
        ]
    );
}

#[test]
fn a_table_kassensturz_cannot_draw_is_refused_whole() {
    let mut cases = Vec::new();
    let mut with = |change: &dyn Fn(&mut Value)| {
        let mut value = good();
        change(&mut value);
        cases.push(value);
    };
    with(&|v| v["rows"][0][2]["amount"] = json!(-123_456));
    with(&|v| v["rows"][0][2]["amount"] = json!("1.5"));
    with(&|v| v["rows"][0][2]["minor_digits"] = json!(5));
    with(&|v| v["rows"][0][2]["currency"] = json!("euro"));
    with(&|v| v["rows"][0][1] = json!("30.11.2026"));
    with(&|v| v["rows"][0][3] = json!(12));
    with(&|v| {
        v["rows"][0]
            .as_array_mut()
            .unwrap()
            .pop()
            .map(drop)
            .unwrap_or_default()
    });
    with(&|v| v["rows"][0][0] = json!("<b>bold</b>\u{202E}"));
    with(&|v| v["columns"][0]["kind"] = json!("html"));
    with(&|v| v["columns"][0]["label"] = json!(" "));
    with(&|v| v["title"] = json!(""));
    with(&|v| v["chart"] = json!({}));
    with(&|v| v["columns"] = json!([]));
    with(&|v| v["rows"] = json!(vec![json!(["a", null, null, null]); crate::MAX_ROWS + 1]));
    for case in cases {
        assert_eq!(table(&case), Err(HostError::OutputInvalid), "{case}");
    }
    // Markup is kept as the text it is: KassenSturz shows it plainly.
    let mut markup = good();
    markup["rows"][0][0] = json!("<b>bold</b>");
    assert_eq!(
        table(&markup).unwrap().rows[0][0],
        Cell::Text("<b>bold</b>".into())
    );
}

#[test]
fn a_report_must_ask_to_read_the_book_and_name_its_reports_well() {
    let without = PluginReport::new(plugin(
        &["report"],
        &[],
        &[("ks_report_info", Some(REPORTS))],
    ));
    assert_eq!(
        without.unwrap_err(),
        HostError::PermissionMissing {
            permission: Permission::ReadBook
        }
    );
    assert_eq!(
        HostError::PermissionMissing {
            permission: Permission::ReadBook
        }
        .report()
        .code,
        "plugin-host.permission-missing"
    );
    for info in [
        r#"{"reports": []}"#,
        r#"{"reports": [{"id": "Monthly", "name": "x"}]}"#,
        r#"{"reports": [{"id": "a", "name": "x"}, {"id": "a", "name": "y"}]}"#,
        r#"{"reports": [{"id": "a", "name": ""}]}"#,
        r#"{"reports": [{"id": "a", "name": "x", "chart": true}]}"#,
    ] {
        let refused = PluginReport::new(plugin(
            &["report"],
            &["read_book"],
            &[("ks_report_info", Some(info))],
        ));
        assert_eq!(refused.unwrap_err(), HostError::OutputInvalid, "{info}");
    }
}

fn new_transactions() -> Vec<NewTransaction> {
    vec![
        NewTransaction {
            id: "t1".into(),
            date: date("2026-11-12"),
            account: "Checking".into(),
            payee: Some("Example Bakery".into()),
            memo: None,
            amount: eur(-420),
            minor_digits: 2,
            uncategorized: true,
        },
        NewTransaction {
            id: "t2".into(),
            date: date("2026-11-14"),
            account: "Checking".into(),
            payee: None,
            memo: Some("Rent".into()),
            amount: eur(-90_000),
            minor_digits: 2,
            uncategorized: false,
        },
    ]
}

fn automation(answer: Option<&str>) -> PluginAutomation {
    PluginAutomation::new(plugin(
        &["automation"],
        &["read_book", "propose_changes"],
        &[("ks_automation_after_import", answer)],
    ))
    .unwrap()
}

fn categories() -> Vec<String> {
    vec!["Food".into(), "Housing".into()]
}

#[test]
fn an_automation_suggests_categories_for_uncategorized_transactions_only() {
    let sorter = automation(Some(
        r#"{"suggestions": [{"transaction": "t1", "category": "Food"}]}"#,
    ));
    assert_eq!(
        sorter.after_import(&new_transactions(), &categories()),
        Ok(vec![Suggestion {
            transaction: "t1".into(),
            category: "Food".into()
        }])
    );
    for answer in [
        // Already has a category.
        r#"{"suggestions": [{"transaction": "t2", "category": "Housing"}]}"#,
        // Not one it was given.
        r#"{"suggestions": [{"transaction": "t9", "category": "Food"}]}"#,
        // A category the book doesn't have: suggestions never add one.
        r#"{"suggestions": [{"transaction": "t1", "category": "Snacks"}]}"#,
        // Twice.
        r#"{"suggestions": [{"transaction": "t1", "category": "Food"}, {"transaction": "t1", "category": "Housing"}]}"#,
        // More than a suggestion.
        r#"{"suggestions": [{"transaction": "t1", "category": "Food", "amount": "0"}]}"#,
        r#"{"changes": []}"#,
    ] {
        assert_eq!(
            automation(Some(answer)).after_import(&new_transactions(), &categories()),
            Err(HostError::OutputInvalid),
            "{answer}"
        );
    }
}

#[test]
fn an_automation_gets_exact_amounts_as_text() {
    // The echo plugin hands back its input, which has no suggestions key.
    let echo = automation(None);
    assert_eq!(
        echo.after_import(&new_transactions(), &categories()),
        Err(HostError::OutputInvalid)
    );
    let input = crate::automation::input(&new_transactions(), &categories());
    assert_eq!(
        input,
        json!({
            "categories": ["Food", "Housing"],
            "transactions": [
                {"id": "t1", "date": "2026-11-12", "account": "Checking",
                 "payee": "Example Bakery", "memo": null, "amount": "-420",
                 "currency": "EUR", "minor_digits": 2, "uncategorized": true},
                {"id": "t2", "date": "2026-11-14", "account": "Checking",
                 "payee": null, "memo": "Rent", "amount": "-90000",
                 "currency": "EUR", "minor_digits": 2, "uncategorized": false}
            ]
        })
    );
}

#[test]
fn an_automation_must_ask_for_both_permissions() {
    for (permissions, missing) in [
        (&[][..], Permission::ReadBook),
        (&["read_book"][..], Permission::ProposeChanges),
    ] {
        let refused = PluginAutomation::new(plugin(
            &["automation"],
            permissions,
            &[("ks_automation_after_import", None)],
        ));
        assert_eq!(
            refused.unwrap_err(),
            HostError::PermissionMissing {
                permission: missing
            }
        );
    }
}
