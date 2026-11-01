// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use kassensturz_import::BookReportPart;
use kassensturz_report::Item;

use super::*;

/// A made-up book: two accounts, a top-level category of each kind with
/// one inside the money-out one, and an investment the book didn't take.
struct TestBook {
    ids: Vec<(&'static str, Found)>,
}

fn id(n: u8) -> AccountId {
    static IDS: std::sync::OnceLock<Vec<AccountId>> = std::sync::OnceLock::new();
    IDS.get_or_init(|| (0..32).map(|_| AccountId::generate()).collect())[usize::from(n)]
}

impl TestBook {
    fn new() -> Self {
        Self {
            ids: vec![
                ("A1", Found::Account(id(1))),
                ("A2", Found::Account(id(2))),
                ("E1", Found::Category(id(11))),
                ("E2", Found::Category(id(12))),
                ("I1", Found::Category(id(21))),
                ("S1", Found::Nothing),
            ],
        }
    }
}

impl ReportBook for TestBook {
    fn find(&self, key: &str) -> Found {
        self.ids
            .iter()
            .find(|(of, _)| *of == key)
            .map_or(Found::Nothing, |(_, found)| *found)
    }

    fn categories(&self, income: bool, top_only: bool) -> Vec<AccountId> {
        match (income, top_only) {
            (false, true) => vec![id(11)],
            (false, false) => vec![id(11), id(12)],
            (true, _) => vec![id(21)],
        }
    }
}

fn day(text: &str) -> Date {
    text.parse().expect("a date")
}

/// An income and expense report as the samples write one: a pie of the
/// top categories over the last 30 days, both kinds of rows.
fn report(settings: &[(&str, &str)], parts: &[(&str, &[(&str, &str)])]) -> BookReport {
    let mut all: Vec<(String, String)> = [
        ("id", "R1"),
        ("type", "pivottable 1.16"),
        ("name", "Spending"),
        ("convertcurrency", "1"),
        ("datelock", "currentmonth"),
        ("rowtype", "expenseincome"),
        ("detail", "top"),
        ("charttype", "pie"),
        ("chartbydefault", "1"),
        ("columntype", "months"),
    ]
    .iter()
    .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
    .collect();
    for (key, value) in settings {
        match all.iter_mut().find(|(of, _)| of == key) {
            Some(setting) => setting.1 = (*value).to_owned(),
            None => all.push(((*key).to_owned(), (*value).to_owned())),
        }
    }
    let name = all
        .iter()
        .find(|(key, _)| key == "name")
        .map(|(_, name)| name.clone())
        .unwrap_or_default();
    let mut parts: Vec<BookReportPart> = parts
        .iter()
        .map(|(name, settings)| BookReportPart {
            name: (*name).to_owned(),
            settings: settings
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
        })
        .collect();
    if parts.is_empty() {
        parts.push(BookReportPart {
            name: "ACCOUNTGROUP".into(),
            settings: vec![("group".into(), "expense".into())],
        });
    }
    BookReport {
        key: "R1".into(),
        name,
        settings: all,
        parts,
    }
}

fn mapped(report: &BookReport) -> MappedReport {
    map_report(report, &TestBook::new(), day("2026-11-01"))
}

fn component(mapped: &MappedReport) -> &Component {
    let document = mapped.document.as_ref().expect("a view");
    match &document.entries[0].item {
        Item::Component(component) => component,
        Item::Unknown(_) => panic!("a known component"),
    }
}

fn period_of(mapped: &MappedReport) -> Option<&str> {
    mapped
        .document
        .as_ref()
        .and_then(|document| document.period.as_deref())
}

#[test]
fn a_report_with_a_period_word_is_kept_as_it_was() {
    let mapped = mapped(&report(&[], &[]));
    assert_eq!(mapped.line.outcome, Outcome::Kept);
    assert!(mapped.line.reasons.is_empty());
    assert_eq!(period_of(&mapped), Some("this-month"));
    let component = component(&mapped);
    assert_eq!(component.kind(), ComponentType::ChartPie);
    assert_eq!(component.dataset().id, SPENDING_BY_CATEGORY);
    assert_eq!(
        component.get("grouping"),
        Some(&Setting::Word("top_level_category".into()))
    );
    assert_eq!(
        component.get("title"),
        Some(&Setting::Text("Spending".into()))
    );
    let document = mapped.document.as_ref().expect("a view");
    assert_eq!(
        document.origin,
        Some(ViewOrigin::kmymoney("R1", day("2026-11-01")))
    );
    // What the book keeps reads back the same.
    let read = Document::read(&document.to_json()).expect("read back");
    assert_eq!(&read, document);
}

#[test]
fn each_period_word_of_the_dialog_maps_to_its_own() {
    for (kmymoney, word) in [
        ("currentmonth", "this-month"),
        ("lastmonth", "last-month"),
        ("last12months", "last-12-months"),
        ("currentyear", "this-year"),
        ("yeartodate", "this-year"),
        ("lastyear", "last-year"),
    ] {
        let mapped = mapped(&report(&[("datelock", kmymoney)], &[]));
        assert_eq!(period_of(&mapped), Some(word), "{kmymoney}");
        assert_eq!(mapped.line.period, None, "{kmymoney}");
    }
}

#[test]
fn each_relative_period_becomes_the_days_it_covers_on_the_import_day() {
    // Printed by the code for the import day 2026-11-01 and checked by
    // hand against the calendar.
    for (kmymoney, first, last) in [
        ("today", "2026-11-01", "2026-11-01"),
        ("last7days", "2026-10-25", "2026-11-01"),
        ("last30days", "2026-10-02", "2026-11-01"),
        ("last3months", "2026-08-01", "2026-11-01"),
        ("last6months", "2026-05-01", "2026-11-01"),
        ("last11months", "2025-12-01", "2026-11-01"),
        ("last3tonext3months", "2026-08-01", "2027-02-01"),
        ("monthtodate", "2026-11-01", "2026-11-01"),
        ("currentquarter", "2026-10-01", "2026-12-31"),
        ("lastquarter", "2026-07-01", "2026-09-30"),
        ("untiltoday", "", "2026-11-01"),
        ("alldates", "", "2026-11-01"),
    ] {
        let mapped = mapped(&report(&[("datelock", kmymoney)], &[]));
        assert_eq!(mapped.line.outcome, Outcome::Changed, "{kmymoney}");
        assert_eq!(mapped.line.reasons, ["range-fixed"], "{kmymoney}");
        let fixed = mapped.line.period.expect("a fixed period");
        assert_eq!(fixed.kmymoney, kmymoney);
        assert_eq!(fixed.imported, day("2026-11-01"));
        let text = |day: Option<Date>| day.map(|day| day.to_string()).unwrap_or_default();
        assert_eq!(
            (text(fixed.first), text(fixed.last)),
            (first.to_owned(), last.to_owned()),
            "{kmymoney}"
        );
        assert_eq!(
            period_of(&mapped),
            Some(format!("{first}..{last}").as_str()),
            "{kmymoney}"
        );
        assert!(FIXED_PERIODS.contains(&kmymoney), "{kmymoney}");
    }
}

#[test]
fn a_month_back_ends_on_the_last_day_of_a_shorter_month() {
    let mapped = map_report(
        &report(&[("datelock", "last3months")], &[]),
        &TestBook::new(),
        day("2026-05-31"),
    );
    let fixed = mapped.line.period.expect("a fixed period");
    assert_eq!(fixed.first, Some(day("2026-02-28")));
    assert_eq!(fixed.last, Some(day("2026-05-31")));
    let mapped = map_report(
        &report(&[("datelock", "lastquarter")], &[]),
        &TestBook::new(),
        day("2026-02-10"),
    );
    let fixed = mapped.line.period.expect("a fixed period");
    assert_eq!(fixed.first, Some(day("2025-10-01")));
    assert_eq!(fixed.last, Some(day("2025-12-31")));
}

#[test]
fn a_custom_range_keeps_its_days_and_is_no_change() {
    let custom = mapped(&report(
        &[("datelock", "userdefined")],
        &[
            ("ACCOUNTGROUP", &[("group", "expense")]),
            ("DATES", &[("from", "2025-01-01"), ("to", "2025-03-31")]),
        ],
    ));
    assert_eq!(custom.line.outcome, Outcome::Kept);
    assert_eq!(period_of(&custom), Some("2025-01-01..2025-03-31"));
    for dates in [
        &[][..],
        &[("from", "2025-1-1")],
        &[("from", "2025-04-01"), ("to", "2025-03-31")],
    ] {
        let refused = mapped(&report(
            &[("datelock", "userdefined")],
            &[("ACCOUNTGROUP", &[("group", "expense")]), ("DATES", dates)],
        ));
        assert_eq!(refused.line.reasons, ["unclear"], "{dates:?}");
    }
}

#[test]
fn a_period_of_days_to_come_or_one_not_known_is_not_kept() {
    for (kmymoney, reason) in [
        ("next30days", "range-future"),
        ("next3months", "range-future"),
        ("nextquarter", "range-future"),
        ("currentfiscalyear", "unclear"),
        ("sometime", "unclear"),
    ] {
        let mapped = mapped(&report(&[("datelock", kmymoney)], &[]));
        assert_eq!(mapped.line.outcome, Outcome::NotKept, "{kmymoney}");
        assert_eq!(mapped.line.reasons, [reason], "{kmymoney}");
        assert!(mapped.document.is_none());
    }
}

#[test]
fn reports_that_list_transactions_or_other_figures_are_not_kept() {
    for (settings, reason) in [
        (
            &[("type", "querytable 1.15"), ("rowtype", "cashflow")][..],
            "transactions-list",
        ),
        (
            &[("type", "querytable 1.15"), ("investments", "1")],
            "investments",
        ),
        (&[("type", "querytable 1.15"), ("tax", "1")], "tax"),
        (
            &[("type", "querytable 1.15"), ("loans", "1")],
            "information",
        ),
        (&[("type", "infotable 1.0")], "information"),
        (&[("rowtype", "budget")], "budget"),
        (&[("rowtype", "budgetactual")], "budget"),
        (&[("includesforecast", "1")], "forecast"),
        (&[("investments", "1")], "investments"),
        (&[("rowtype", "something")], "unclear"),
        (&[("type", "")], "unclear"),
        (&[("detail", "most")], "unclear"),
        (&[("charttype", "radar")], "unclear"),
    ] {
        let mapped = mapped(&report(settings, &[]));
        assert_eq!(mapped.line.outcome, Outcome::NotKept, "{settings:?}");
        assert_eq!(mapped.line.reasons, [reason], "{settings:?}");
        assert_eq!(mapped.line.name, "Spending");
    }
}

#[test]
fn each_chart_type_gets_its_component() {
    for (chart, kind, dataset, reasons) in [
        (
            "pie",
            ComponentType::ChartPie,
            SPENDING_BY_CATEGORY,
            &[][..],
        ),
        ("ring", ComponentType::ChartDonut, SPENDING_BY_CATEGORY, &[]),
        ("none", ComponentType::TableRows, SPENDING_BY_CATEGORY, &[]),
        ("line", ComponentType::ChartLine, CATEGORY_OVER_TIME, &[]),
        (
            "stackedbar",
            ComponentType::ChartStackedBar,
            CATEGORY_OVER_TIME,
            &[],
        ),
        (
            "bar",
            ComponentType::ChartStackedBar,
            CATEGORY_OVER_TIME,
            &["chart-changed"],
        ),
    ] {
        let mapped = mapped(&report(&[("charttype", chart)], &[]));
        let component = component(&mapped);
        assert_eq!(
            (component.kind(), component.dataset().id),
            (kind, dataset),
            "{chart}"
        );
        assert_eq!(mapped.line.reasons, reasons, "{chart}");
    }
    // Shown as a table although a chart type is set.
    let mapped = mapped(&report(&[("chartbydefault", "0")], &[]));
    assert_eq!(component(&mapped).kind(), ComponentType::TableRows);
}

#[test]
fn a_chart_over_time_shows_the_categories_of_the_rows_it_had() {
    let both = [
        ("ACCOUNTGROUP", &[("group", "expense")][..]),
        ("ACCOUNTGROUP", &[("group", "income")]),
    ];
    let series = |mapped: &MappedReport| component(mapped).get("series").cloned();
    let ids = |ids: &[u8]| {
        Some(Setting::Ids(
            ids.iter().map(|n| id(*n).to_string()).collect(),
        ))
    };
    let top = mapped(&report(&[("charttype", "line")], &both));
    assert_eq!(series(&top), ids(&[11, 21]));
    let all = mapped(&report(&[("charttype", "line"), ("detail", "all")], &both));
    assert_eq!(series(&all), ids(&[11, 12, 21]));
    // The categories it was limited to, whatever the rows.
    let chosen = mapped(&report(
        &[("charttype", "line")],
        &[
            ("ACCOUNTGROUP", &[("group", "expense")]),
            ("CATEGORY", &[("id", "E2")]),
        ],
    ));
    assert_eq!(series(&chosen), ids(&[12]));
    // Columns other than months are shown by month.
    let weeks = mapped(&report(
        &[("charttype", "line"), ("columntype", "weeks")],
        &both,
    ));
    assert_eq!(weeks.line.reasons, ["by-month"]);
}

#[test]
fn totals_only_show_money_in_and_out_per_month() {
    for (chart, kind) in [
        ("none", ComponentType::TableRows),
        ("line", ComponentType::ChartLine),
        ("stackedbar", ComponentType::ChartStackedBar),
    ] {
        let mapped = mapped(&report(&[("detail", "total"), ("charttype", chart)], &[]));
        let component = component(&mapped);
        assert_eq!(component.kind(), kind, "{chart}");
        assert_eq!(component.dataset().id, INCOME_EXPENSE_BY_MONTH);
        assert_eq!(mapped.line.reasons, ["totals-only"]);
    }
}

#[test]
fn a_pie_of_money_in_and_out_keeps_money_out_and_says_so() {
    let mapped_both = mapped(&report(
        &[],
        &[
            ("ACCOUNTGROUP", &[("group", "expense")]),
            ("ACCOUNTGROUP", &[("group", "income")]),
        ],
    ));
    assert_eq!(mapped_both.line.reasons, ["income-left-out"]);
    // Without any rows named, KMyMoney's report shows both.
    let report_without = BookReport {
        parts: Vec::new(),
        ..report(&[], &[])
    };
    assert_eq!(mapped(&report_without).line.reasons, ["income-left-out"]);
    let income = mapped(&report(&[], &[("ACCOUNTGROUP", &[("group", "income")])]));
    assert_eq!(income.line.outcome, Outcome::NotKept);
    assert_eq!(income.line.reasons, ["income-only"]);
}

#[test]
fn net_worth_reports_show_net_worth() {
    let line = mapped(&report(
        &[("rowtype", "assetliability"), ("charttype", "line")],
        &[],
    ));
    assert_eq!(component(&line).dataset().id, NET_WORTH_OVER_TIME);
    assert_eq!(line.line.outcome, Outcome::Kept);
    let bar = mapped(&report(
        &[("rowtype", "assetliability"), ("charttype", "bar")],
        &[],
    ));
    assert_eq!(component(&bar).kind(), ComponentType::ChartLine);
    assert_eq!(bar.line.reasons, ["chart-changed"]);
    let table = mapped(&report(
        &[("rowtype", "assetliability"), ("chartbydefault", "0")],
        &[],
    ));
    assert_eq!(component(&table).dataset().id, ACCOUNT_BALANCES);
    assert_eq!(component(&table).kind(), ComponentType::TableRows);
}

#[test]
fn filters_by_account_and_category_are_kept_and_the_others_listed() {
    let mapped = mapped(&report(
        &[],
        &[
            ("ACCOUNTGROUP", &[("group", "expense")]),
            ("ACCOUNT", &[("id", "A2")]),
            ("ACCOUNT", &[("id", "S1")]),
            ("ACCOUNT", &[("id", "A2")]),
            ("CATEGORY", &[("id", "E1")]),
            ("PAYEE", &[("id", "P1")]),
            ("TAG", &[("id", "G1")]),
            ("PAYEE", &[("id", "P2")]),
            ("TEXT", &[("pattern", "rent")]),
            ("AMOUNT", &[]),
            ("STATE", &[]),
            ("TYPE", &[]),
            ("NUMBER", &[]),
            ("SOMETHING", &[]),
        ],
    ));
    let document = mapped.document.as_ref().expect("a view");
    assert_eq!(document.accounts, [id(2).to_string()]);
    assert_eq!(document.categories, [id(11).to_string()]);
    assert_eq!(
        mapped.line.reasons,
        ["account-left-out", "filter-left-out", "options-left-out"]
    );
    assert_eq!(
        mapped.line.filters,
        ["payee", "tag", "text", "amount", "state", "type", "number"]
    );
}

#[test]
fn not_converting_and_other_settings_are_listed_once() {
    let mapped = mapped(&report(
        &[
            ("convertcurrency", "0"),
            ("includeschedules", "1"),
            ("includestransfers", "1"),
            ("comment", "for the tax office"),
        ],
        &[],
    ));
    assert_eq!(mapped.line.reasons, ["currency", "options-left-out"]);
    // Chart looks change no figure and are passed over.
    let looks = mapped_with(&[("chartdatalabels", "1"), ("chartlinewidth", "3")]);
    assert!(looks.line.reasons.is_empty());
}

fn mapped_with(settings: &[(&str, &str)]) -> MappedReport {
    mapped(&report(settings, &[]))
}

#[test]
fn names_are_one_line_short_and_never_taken_twice() {
    let reports = [
        report(&[("name", "Spending")], &[]),
        report(&[("name", "SPENDING")], &[]),
        report(&[("name", "  ")], &[]),
        report(&[("name", "Two\nlines")], &[]),
        report(&[("name", &"x".repeat(300))], &[]),
        report(&[("name", "Kept elsewhere")], &[]),
    ];
    let mut taken = HashSet::from(["kept elsewhere".to_owned()]);
    let mapped = map_reports(&reports, &TestBook::new(), day("2026-11-01"), &mut taken);
    let names: Vec<&str> = mapped
        .iter()
        .map(|mapped| mapped.line.name.as_str())
        .collect();
    assert_eq!(names[..4], ["Spending", "SPENDING (2)", "R1", "Two lines"]);
    assert_eq!(names[4].chars().count(), MAX_NAME_CHARS - 6);
    assert_eq!(names[5], "Kept elsewhere (2)");
    for mapped in &mapped {
        let document = mapped.document.as_ref().expect("a view");
        assert_eq!(document.name, mapped.line.name);
        assert_eq!(
            component(mapped).get("title"),
            Some(&Setting::Text(mapped.line.name.clone()))
        );
        Document::read(&document.to_json()).expect("the book takes it");
    }
}
