// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! KMyMoney's saved reports into saved views (M6d.6a, D-444, D-445).
//!
//! Clean-room (AGENTS.md §3.1 rule 5, D-044): the settings below are read
//! by the names the maintainer's checked sample files show (D-431), and
//! their meaning is taken from KMyMoney's public handbook, which names the
//! report types and the settings of its report dialog. No KMyMoney source
//! was read. Values that the samples do not show (other periods, chart
//! types, filters) are matched by the words the file format uses for them;
//! a value that matches nothing below is never guessed: the report is not
//! kept, or the setting is listed as left out (`docs/REPORT_DESIGNER_M6D.md`
//! section 5.1).
//!
//! The mapping does not know where the IDs come from: a new book made from
//! the file maps the file's keys to the new IDs ([`ReportBook`]); bringing
//! reports into an existing book (M6d.6b, `kmy_reports_into.rs`) finds them
//! by name.

use std::collections::HashSet;

use kassensturz_core::{AccountId, Date};
use kassensturz_import::BookReport;
use kassensturz_report::{
    ACCOUNT_BALANCES, CATEGORY_OVER_TIME, Component, ComponentType, Document,
    INCOME_EXPENSE_BY_MONTH, MAX_NAME_CHARS, NET_WORTH_OVER_TIME, SPENDING_BY_CATEGORY, Setting,
    ViewOrigin, range_word,
};

/// KMyMoney's words for the relative periods that become fixed days
/// ([`FixedPeriod::kmymoney`]); the debug log knows each of them as a
/// detail (`kassensturz_core::vocabulary::DETAILS`).
pub const FIXED_PERIODS: [&str; 12] = [
    "today",
    "last7days",
    "last30days",
    "last3months",
    "last6months",
    "last11months",
    "last3tonext3months",
    "monthtodate",
    "untiltoday",
    "alldates",
    "currentquarter",
    "lastquarter",
];

/// The most categories a chart over time gets as its lines, the schema's
/// limit for a list of IDs.
const MAX_SERIES: usize = 50;

/// What an ID of the file stands for in the book.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Found {
    /// One of the user's accounts.
    Account(AccountId),
    /// A category.
    Category(AccountId),
    /// Nothing the book holds, such as an investment account (D-029).
    Nothing,
    /// One of the file's accounts that the open book has no account of that
    /// name for (M6d.6b, D-450).
    AccountNotFound,
    /// One of the file's categories that the open book has no category of
    /// that path for (M6d.6b, D-450).
    CategoryNotFound,
}

/// The book the views are made for, as the mapping needs to see it.
pub trait ReportBook {
    /// What the file's ID `key` stands for.
    fn find(&self, key: &str) -> Found;
    /// The categories of money out (`income` false) or money in, in the
    /// book's order: only the top-level ones, or all of them.
    fn categories(&self, income: bool, top_only: bool) -> Vec<AccountId>;
    /// Whether [`ReportBook::categories`] leaves out categories of the file
    /// that the book lacks (M6d.6b); a book made from the file lacks none.
    fn lacks_categories(&self, _income: bool, _top_only: bool) -> bool {
        false
    }
}

/// What became of a report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// A saved view shows what the report showed.
    Kept,
    /// A saved view, with the changes its reasons list.
    Changed,
    /// No saved view; the reason says why.
    NotKept,
}

impl Outcome {
    /// The keyword the UI words it from (D-034, D-445).
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Kept => "kept",
            Self::Changed => "changed",
            Self::NotKept => "not-kept",
        }
    }
}

/// A relative period that became fixed days on the import day (Q-319 = A):
/// shown beside KMyMoney's period so it can be checked at a glance, and
/// written to the debug log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixedPeriod {
    /// KMyMoney's word for the period, e.g. `last30days`.
    pub kmymoney: &'static str,
    /// The first day it became, if it has one.
    pub first: Option<Date>,
    /// The last day it became.
    pub last: Option<Date>,
    /// The day of the import.
    pub imported: Date,
}

/// One line of the import summary: a report and what became of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportLine {
    /// The report's ID in the file.
    pub key: String,
    /// The name of its view, or the report's name if it has none.
    pub name: String,
    /// What became of it.
    pub outcome: Outcome,
    /// Why it changed or wasn't kept, as keywords the UI words (D-445),
    /// each once, in the order of the mapping table. A filter that was left
    /// out is `filter-left-out` with its kind in [`ReportLine::filters`].
    pub reasons: Vec<&'static str>,
    /// The kinds of the filters left out (`payee`, `tag`, …), each once.
    pub filters: Vec<&'static str>,
    /// The fixed days a relative period became.
    pub period: Option<FixedPeriod>,
}

/// A report mapped: its summary line and, if kept, the view's document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappedReport {
    /// The summary line.
    pub line: ReportLine,
    /// The view, unless the report was not kept.
    pub document: Option<Document>,
}

/// What a report's settings say it is.
enum ReportKind {
    IncomeExpense,
    NetWorth,
    NotKept(&'static str),
}

/// The period a report's `datelock` asks for.
enum AskedPeriod {
    /// One of KassenSturz's period words.
    Word(&'static str),
    /// Fixed days: a custom range, or a relative period on the import day.
    Range(Option<Date>, Option<Date>, Option<&'static str>),
    /// Only days to come: nothing a saved view can show yet.
    Future,
    /// Nothing this mapping knows.
    Unclear,
}

/// The reasons collected for one report.
#[derive(Default)]
struct Reasons {
    reasons: Vec<&'static str>,
    filters: Vec<&'static str>,
}

impl Reasons {
    fn add(&mut self, reason: &'static str) {
        if !self.reasons.contains(&reason) {
            self.reasons.push(reason);
        }
    }

    fn filter(&mut self, kind: &'static str) {
        self.add("filter-left-out");
        if !self.filters.contains(&kind) {
            self.filters.push(kind);
        }
    }
}

/// Maps the reports of a file onto saved views for `book`, in the file's
/// order, on the import day `today`. A view's name taken by an earlier one
/// (ignoring capitals) gets ` (2)`, ` (3)`, …; `taken` holds the names the
/// book has already, in lower case.
pub fn map_reports(
    reports: &[BookReport],
    book: &dyn ReportBook,
    today: Date,
    taken: &mut HashSet<String>,
) -> Vec<MappedReport> {
    reports
        .iter()
        .map(|report| {
            let mut mapped = map_report(report, book, today);
            if let Some(document) = &mut mapped.document {
                let name = unique_name(&document.name, taken);
                rename(document, &name);
                mapped.line.name = name;
            }
            mapped
        })
        .collect()
}

/// Maps one report onto a saved view for `book` on the import day `today`.
pub fn map_report(report: &BookReport, book: &dyn ReportBook, today: Date) -> MappedReport {
    let name = view_name(report);
    let mut reasons = Reasons::default();
    let not_kept = |reason: &'static str, period: Option<FixedPeriod>| MappedReport {
        line: ReportLine {
            key: report.key.clone(),
            name: name.clone(),
            outcome: Outcome::NotKept,
            reasons: vec![reason],
            filters: Vec::new(),
            period,
        },
        document: None,
    };
    let kind = match report_kind(report) {
        ReportKind::NotKept(reason) => return not_kept(reason, None),
        kind => kind,
    };
    let (period, fixed) = match asked_period(report, today) {
        AskedPeriod::Word(word) => (word.to_owned(), None),
        AskedPeriod::Range(first, last, relative) => match range_word(first, last) {
            Ok(word) => {
                let fixed = relative.map(|kmymoney| FixedPeriod {
                    kmymoney,
                    first,
                    last,
                    imported: today,
                });
                if fixed.is_some() {
                    reasons.add("range-fixed");
                }
                (word, fixed)
            }
            Err(_) => return not_kept("unclear", None),
        },
        AskedPeriod::Future => return not_kept("range-future", None),
        AskedPeriod::Unclear => return not_kept("unclear", None),
    };

    // The rows: which kinds of categories, and the filters.
    let mut expense = false;
    let mut income = false;
    let mut accounts: Vec<String> = Vec::new();
    let mut categories: Vec<AccountId> = Vec::new();
    // Whether the report narrowed its accounts or categories at all: a
    // filter that loses every one of them shows more than it did.
    let mut asked_accounts = false;
    let mut asked_categories = false;
    for part in &report.parts {
        match part.name.as_str() {
            "ACCOUNTGROUP" => match part.setting("group") {
                Some("expense") => expense = true,
                Some("income") => income = true,
                // Accounts as rows: what a net worth report shows anyway.
                Some("asset" | "liability") => {}
                _ => reasons.add("options-left-out"),
            },
            "ACCOUNT" | "CATEGORY" => match part.setting("id").map(|key| book.find(key)) {
                Some(Found::Account(id)) => {
                    asked_accounts = true;
                    let id = id.to_string();
                    if !accounts.contains(&id) {
                        accounts.push(id);
                    }
                }
                Some(Found::Category(id)) => {
                    asked_categories = true;
                    if !categories.contains(&id) {
                        categories.push(id);
                    }
                }
                Some(Found::Nothing) => {
                    asked_accounts = true;
                    reasons.add("account-left-out");
                }
                Some(Found::AccountNotFound) => {
                    asked_accounts = true;
                    reasons.add("account-not-found");
                }
                Some(Found::CategoryNotFound) => {
                    asked_categories = true;
                    reasons.add("category-not-found");
                }
                None => reasons.add("options-left-out"),
            },
            "PAYEE" => reasons.filter("payee"),
            "TAG" => reasons.filter("tag"),
            "TEXT" => reasons.filter("text"),
            "AMOUNT" => reasons.filter("amount"),
            "STATE" => reasons.filter("state"),
            "TYPE" => reasons.filter("type"),
            "NUMBER" => reasons.filter("number"),
            // The days of a custom range, read with the period.
            "DATES" => {}
            _ => reasons.add("options-left-out"),
        }
    }
    if !expense && !income {
        expense = true;
        income = true;
    }

    let chart = (report.setting("chartbydefault") == Some("1"))
        .then(|| report.setting("charttype").unwrap_or("none"))
        .filter(|chart| *chart != "none");
    let top_only = match report.setting("detail") {
        Some("top") => Some(true),
        Some("all") => Some(false),
        Some("group" | "total") => None,
        _ => return not_kept("unclear", fixed),
    };

    let made = match kind {
        ReportKind::IncomeExpense => income_expense(
            chart,
            top_only,
            expense,
            income,
            &categories,
            book,
            &mut reasons,
        ),
        ReportKind::NetWorth => {
            if !categories.is_empty() {
                reasons.add("options-left-out");
            }
            categories.clear();
            net_worth(chart, &mut reasons)
        }
        ReportKind::NotKept(reason) => Err(reason),
    };
    let mut component = match made {
        Ok(component) => component,
        Err(reason) => return not_kept(reason, fixed),
    };
    if matches!(kind, ReportKind::IncomeExpense)
        && chart.is_some_and(|chart| !matches!(chart, "pie" | "ring"))
        && (report.setting("columntype") != Some("months")
            || report.setting("columnsaredays") == Some("1"))
    {
        reasons.add("by-month");
    }
    if (asked_accounts && accounts.is_empty())
        || (matches!(kind, ReportKind::IncomeExpense) && asked_categories && categories.is_empty())
    {
        reasons.add("shows-more");
    }
    if report.setting("convertcurrency") == Some("0") {
        reasons.add("currency");
    }
    let options = [
        "includeschedules",
        "includestransfers",
        "includesmovingaverage",
        "includesaverageprice",
        "includesprice",
        "favorite",
    ];
    if options
        .iter()
        .any(|option| report.setting(option) == Some("1"))
        || report
            .setting("comment")
            .is_some_and(|comment| !comment.trim().is_empty())
    {
        reasons.add("options-left-out");
    }

    if component.set("title", Setting::Text(name.clone())).is_err() {
        return not_kept("unclear", fixed);
    }
    let mut document = Document::view(&name, Some(&period), component);
    document.accounts = accounts;
    document.categories = categories.iter().map(ToString::to_string).collect();
    document.origin = Some(ViewOrigin::kmymoney(&cut(&report.key, 64), today));
    // Read back what the book would keep, so nothing it refuses is offered.
    if Document::read(&document.to_json()).is_err() {
        return not_kept("unclear", fixed);
    }
    let outcome = if reasons.reasons.is_empty() {
        Outcome::Kept
    } else {
        Outcome::Changed
    };
    MappedReport {
        line: ReportLine {
            key: report.key.clone(),
            name,
            outcome,
            reasons: reasons.reasons,
            filters: reasons.filters,
            period: fixed,
        },
        document: Some(document),
    }
}

/// What the report's type and rows say it is (`type`, `rowtype`; the
/// handbook's report types).
fn report_kind(report: &BookReport) -> ReportKind {
    let flag = |name: &str| report.setting(name) == Some("1");
    let kind = report
        .setting("type")
        .and_then(|kind| kind.split_whitespace().next())
        .unwrap_or("");
    match kind {
        "querytable" if flag("investments") => ReportKind::NotKept("investments"),
        "querytable" if flag("tax") => ReportKind::NotKept("tax"),
        "querytable" if flag("loans") => ReportKind::NotKept("information"),
        "querytable" => ReportKind::NotKept("transactions-list"),
        "infotable" => ReportKind::NotKept("information"),
        "pivottable" if flag("investments") => ReportKind::NotKept("investments"),
        "pivottable" if flag("includesforecast") => ReportKind::NotKept("forecast"),
        "pivottable" => match report.setting("rowtype") {
            Some("expenseincome") => ReportKind::IncomeExpense,
            Some("assetliability") => ReportKind::NetWorth,
            Some("budget" | "budgetactual") => ReportKind::NotKept("budget"),
            _ => ReportKind::NotKept("unclear"),
        },
        _ => ReportKind::NotKept("unclear"),
    }
}

/// How far from the import day: days and calendar months.
type Shift = (i64, i64);

/// The relative periods of KMyMoney's report dialog (`datelock`) that
/// become fixed days on the import day: the word, how far back the first
/// day is and how far ahead the last one, from the import day.
fn relative(word: &str) -> Option<(&'static str, Shift, Shift)> {
    const RELATIVE: [(&str, i64, i64, i64, i64); 7] = [
        // word, days back, months back, days ahead, months ahead
        ("today", 0, 0, 0, 0),
        ("last7days", 7, 0, 0, 0),
        ("last30days", 30, 0, 0, 0),
        ("last3months", 0, 3, 0, 0),
        ("last6months", 0, 6, 0, 0),
        ("last11months", 0, 11, 0, 0),
        ("last3tonext3months", 0, 3, 0, 3),
    ];
    RELATIVE.iter().find(|(name, ..)| *name == word).map(
        |&(name, days_back, months_back, days_ahead, months_ahead)| {
            (name, (days_back, months_back), (days_ahead, months_ahead))
        },
    )
}

/// The period the report's `datelock` asks for, seen from `today`.
fn asked_period(report: &BookReport, today: Date) -> AskedPeriod {
    let Some(word) = report.setting("datelock") else {
        return AskedPeriod::Unclear;
    };
    let range = |first: Option<Date>, last: Option<Date>, relative| {
        AskedPeriod::Range(first, last, relative)
    };
    match word {
        "currentmonth" => AskedPeriod::Word("this-month"),
        "lastmonth" => AskedPeriod::Word("last-month"),
        "last12months" => AskedPeriod::Word("last-12-months"),
        "currentyear" | "yeartodate" => AskedPeriod::Word("this-year"),
        "lastyear" => AskedPeriod::Word("last-year"),
        "userdefined" => {
            let dates = report.parts.iter().find(|part| part.name == "DATES");
            let day = |key: &str| -> Result<Option<Date>, ()> {
                match dates.and_then(|part| part.setting(key)) {
                    None | Some("") => Ok(None),
                    Some(text) if text.len() == 10 => text.parse().map(Some).map_err(|_| ()),
                    Some(_) => Err(()),
                }
            };
            match (day("from"), day("to")) {
                (Ok(first), Ok(last)) if first.is_some() || last.is_some() => {
                    range(first, last, None)
                }
                _ => AskedPeriod::Unclear,
            }
        }
        "monthtodate" => range(
            Some(today.first_of_month()),
            Some(today),
            Some("monthtodate"),
        ),
        "untiltoday" | "alldates" => {
            let kmymoney = if word == "untiltoday" {
                "untiltoday"
            } else {
                "alldates"
            };
            range(None, Some(today), Some(kmymoney))
        }
        "currentquarter" | "lastquarter" => {
            let quarter_start = |day: Date| -> Option<Date> {
                let month = (day.month() - 1) / 3 * 3 + 1;
                Date::new(day.year(), month, 1).ok()
            };
            let start = quarter_start(today).and_then(|start| {
                if word == "lastquarter" {
                    months_from(start, -3)
                } else {
                    Some(start)
                }
            });
            let end = start
                .and_then(|start| months_from(start, 3))
                .and_then(|next| next.yesterday().ok());
            let kmymoney = if word == "lastquarter" {
                "lastquarter"
            } else {
                "currentquarter"
            };
            match (start, end) {
                (Some(start), Some(end)) => range(Some(start), Some(end), Some(kmymoney)),
                _ => AskedPeriod::Unclear,
            }
        }
        _ if word.starts_with("next") => AskedPeriod::Future,
        _ => match relative(word) {
            Some((kmymoney, back, ahead)) => {
                let first = months_from(today, -back.1).and_then(|day| days_from(day, -back.0));
                let last = months_from(today, ahead.1).and_then(|day| days_from(day, ahead.0));
                match (first, last) {
                    (Some(first), Some(last)) => range(Some(first), Some(last), Some(kmymoney)),
                    _ => AskedPeriod::Unclear,
                }
            }
            None => AskedPeriod::Unclear,
        },
    }
}

/// The day `months` calendar months after `day` (before it, if negative),
/// on the same day of the month or the month's last day if it is shorter.
fn months_from(day: Date, months: i64) -> Option<Date> {
    let index = i64::from(day.year()) * 12 + i64::from(day.month()) - 1 + months;
    let year = i16::try_from(index.div_euclid(12)).ok()?;
    let month = i8::try_from(index.rem_euclid(12) + 1).ok()?;
    let first = Date::new(year, month, 1).ok()?;
    let last = first.last_of_month().day();
    Date::new(year, month, day.day().min(last)).ok()
}

/// The day `days` days after `day` (before it, if negative).
fn days_from(mut day: Date, days: i64) -> Option<Date> {
    for _ in 0..days.unsigned_abs() {
        day = if days < 0 {
            day.yesterday().ok()?
        } else {
            day.tomorrow().ok()?
        };
    }
    Some(day)
}

/// The component of an income and expense report.
fn income_expense(
    chart: Option<&str>,
    top_only: Option<bool>,
    expense: bool,
    income: bool,
    chosen: &[AccountId],
    book: &dyn ReportBook,
    reasons: &mut Reasons,
) -> Result<Component, &'static str> {
    let new = |kind, dataset| Component::new(kind, dataset).map_err(|_| "unclear");
    let Some(top_only) = top_only else {
        // Groups or totals only: the money in and out per month.
        reasons.add("totals-only");
        let kind = match chart {
            None => ComponentType::TableRows,
            Some("line") => ComponentType::ChartLine,
            Some("bar" | "stackedbar") => ComponentType::ChartStackedBar,
            Some(_) => return Err("unclear"),
        };
        return new(kind, INCOME_EXPENSE_BY_MONTH);
    };
    let grouping = if top_only {
        "top_level_category"
    } else {
        "category"
    };
    let spending = |kind, reasons: &mut Reasons| -> Result<Component, &'static str> {
        if !expense {
            return Err("income-only");
        }
        if income {
            reasons.add("income-left-out");
        }
        let mut component = new(kind, SPENDING_BY_CATEGORY)?;
        component
            .set("grouping", Setting::Word(grouping.to_owned()))
            .map_err(|_| "unclear")?;
        Ok(component)
    };
    let kind = match chart {
        None => return spending(ComponentType::TableRows, reasons),
        Some("pie") => return spending(ComponentType::ChartPie, reasons),
        Some("ring") => return spending(ComponentType::ChartDonut, reasons),
        Some("line") => ComponentType::ChartLine,
        Some("stackedbar") => ComponentType::ChartStackedBar,
        Some("bar") => {
            reasons.add("chart-changed");
            ComponentType::ChartStackedBar
        }
        Some(_) => return Err("unclear"),
    };
    // A chart over time: the report's categories as lines, or else the
    // categories of the kinds it shows.
    let mut series: Vec<AccountId> = if chosen.is_empty() {
        let mut all = Vec::new();
        for (wanted, is_income) in [(expense, false), (income, true)] {
            if wanted {
                all.extend(book.categories(is_income, top_only));
                if book.lacks_categories(is_income, top_only) {
                    reasons.add("category-not-found");
                }
            }
        }
        all
    } else {
        chosen.to_vec()
    };
    if series.len() > MAX_SERIES && chosen.is_empty() && !top_only {
        reasons.add("top-only");
        series = [(expense, false), (income, true)]
            .into_iter()
            .filter(|(wanted, _)| *wanted)
            .flat_map(|(_, is_income)| book.categories(is_income, true))
            .collect();
    }
    if series.is_empty() || series.len() > MAX_SERIES {
        return Err("unclear");
    }
    let mut component = new(kind, CATEGORY_OVER_TIME)?;
    component
        .set(
            "series",
            Setting::Ids(series.iter().map(ToString::to_string).collect()),
        )
        .map_err(|_| "unclear")?;
    Ok(component)
}

/// The component of a net worth report.
fn net_worth(chart: Option<&str>, reasons: &mut Reasons) -> Result<Component, &'static str> {
    let (kind, dataset) = match chart {
        None => (ComponentType::TableRows, ACCOUNT_BALANCES),
        Some(chart) => {
            if chart != "line" {
                reasons.add("chart-changed");
            }
            (ComponentType::ChartLine, NET_WORTH_OVER_TIME)
        }
    };
    Component::new(kind, dataset).map_err(|_| "unclear")
}

/// The name a report's view gets: its name on one line, at most
/// [`MAX_NAME_CHARS`] characters, or its ID if it has none.
pub(crate) fn view_name(report: &BookReport) -> String {
    let one_line: String = report
        .name
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let name = cut(one_line.trim(), MAX_NAME_CHARS - 6);
    let name = name.trim();
    if name.is_empty() {
        cut(report.key.trim(), MAX_NAME_CHARS - 6)
    } else {
        name.to_owned()
    }
}

/// The first `max` characters of `text`.
pub(crate) fn cut(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// `name`, or `name (2)`, `name (3)`, … if `taken` has it, ignoring
/// capitals; the result is added to `taken`.
pub(crate) fn unique_name(name: &str, taken: &mut HashSet<String>) -> String {
    let mut candidate = name.to_owned();
    let mut number = 2;
    while taken.contains(&candidate.to_lowercase()) {
        candidate = format!("{name} ({number})");
        number += 1;
    }
    taken.insert(candidate.to_lowercase());
    candidate
}

/// Gives the view `document` and its one component the name `name`.
pub(crate) fn rename(document: &mut Document, name: &str) {
    document.name = name.to_owned();
    if let Some(entry) = document.entries.first_mut()
        && let kassensturz_report::Item::Component(component) = &mut entry.item
    {
        let _ = component.set("title", Setting::Text(name.to_owned()));
    }
}

#[cfg(test)]
mod tests;
