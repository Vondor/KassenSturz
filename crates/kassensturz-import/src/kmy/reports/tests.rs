// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[test]
fn a_row_holds_one_report_with_its_settings_and_filters() {
    let xml = br#"<REPORTS>
 <REPORT id="R1" type="pivottable 1.16" name="Spending" datelock="last30days">
  <ACCOUNTGROUP group="expense" />
  <ACCOUNT id="A1" />
 </REPORT>
</REPORTS>
"#;
    let reports = from_row(xml, &Limits::default()).expect("read");
    assert_eq!(reports.len(), 1);
    let report = &reports[0];
    assert_eq!(report.key, "R1");
    assert_eq!(report.name, "Spending");
    assert_eq!(report.setting("datelock"), Some("last30days"));
    let parts: Vec<(&str, Option<&str>)> = report
        .parts
        .iter()
        .map(|part| (part.name.as_str(), part.setting("id")))
        .collect();
    assert_eq!(parts, [("ACCOUNTGROUP", None), ("ACCOUNT", Some("A1"))]);
}

#[test]
fn a_row_that_is_not_a_report_record_is_refused() {
    for xml in [
        &br#"<REPORT id="R1" />"#[..],
        br#"<REPORTS><BUDGET id="B1" /></REPORTS>"#,
        br#"<REPORTS><REPORT name="no id" /></REPORTS>"#,
        br#"<!DOCTYPE x [<!ENTITY a "b">]><REPORTS />"#,
        b"not xml",
    ] {
        assert!(from_row(xml, &Limits::default()).is_err(), "{xml:?}");
    }
}
