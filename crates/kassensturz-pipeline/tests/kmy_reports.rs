// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! KMyMoney's saved reports into saved views, with a new book made from the
//! file (M6d.6a, D-444, D-445). The files are the maintainer's checked
//! sample files (D-431): a made-up Space Colony book with seven reports,
//! saved by KMyMoney as `.kmy` and as SQLite. The importer reads them, the
//! pipeline fills the book, and the views are read back from the book and
//! computed against it, so each step meets the real output of the one
//! before (AGENTS.md §9.4).

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use kassensturz_core::{AccountId, AccountKind, Date, GroupMember, ViewKind};
use kassensturz_import::{Imported, Kmy, KmySqlite, Limits, SkippedData, import};
use kassensturz_pipeline::{
    BookImportReport, Outcome, RuleScope, fill_book_from_on, fill_book_from_with,
};
use kassensturz_report::{
    ComponentType, Document, Item, Level, Overlap, Period, Source, ViewOrigin, category_over_time,
    spending_by_category,
};
use kassensturz_store::Book;

fn golden(folder: &str, name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../kassensturz-import/tests/golden")
        .join(folder)
        .join(name);
    fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The day the maintainer saved the samples, as the import day.
fn import_day() -> Date {
    "2026-11-01".parse().expect("a date")
}

fn filled(imported: Imported) -> (Book, BookImportReport) {
    let Imported::Book(imported) = imported else {
        panic!("a book");
    };
    let mut book = Book::create_in_memory().expect("book created");
    let report = fill_book_from_on(
        &mut book,
        &imported,
        &HashMap::new(),
        RuleScope::default(),
        import_day(),
    )
    .expect("filled");
    (book, report)
}

fn from_kmy() -> (Book, BookImportReport) {
    filled(import(&Kmy, &golden("kmy", "reports-anon.kmy"), &Limits::default()).expect("read"))
}

fn from_sqlite() -> (Book, BookImportReport) {
    filled(
        import(
            &KmySqlite,
            &golden("kmy-sqlite", "reports-anon.sqlite"),
            &Limits::default(),
        )
        .expect("read"),
    )
}

/// What became of each report, as the summary lists it: name, outcome,
/// reasons and the fixed days.
fn summary(report: &BookImportReport) -> Vec<String> {
    report
        .reports
        .iter()
        .map(|line| {
            let days = line.period.map_or(String::new(), |period| {
                let day = |day: Option<Date>| day.map(|day| day.to_string()).unwrap_or_default();
                format!(
                    " {} {}..{} on {}",
                    period.kmymoney,
                    day(period.first),
                    day(period.last),
                    period.imported
                )
            });
            format!(
                "{} {} {}{days}",
                line.name,
                line.outcome.keyword(),
                line.reasons.join(",")
            )
        })
        .collect()
}

fn views(book: &Book) -> Vec<Document> {
    book.views(Some(ViewKind::View))
        .expect("views")
        .iter()
        .map(|view| Document::read(&view.document).expect("the view reads"))
        .collect()
}

fn account(book: &Book, name: &str) -> AccountId {
    book.accounts(true)
        .expect("accounts")
        .into_iter()
        .find(|account| account.name == name)
        .unwrap_or_else(|| panic!("no account {name}"))
        .id
}

#[test]
fn every_part_a_report_of_the_samples_is_named_with_what_became_of_it() {
    let (book, report) = from_kmy();
    // Report 1: a pie of the top categories of the last 30 days, of two
    // accounts. Reports 2 and 3: stacked bars from three months back to
    // three months ahead. Reports 4 and 5: lines of the next 30 days. Reports
    // 6 and 7: cash flow lists of transactions this year.
    assert_eq!(
        summary(&report),
        [
            "Report 1 changed range-fixed,income-left-out last30days 2026-10-02..2026-11-01 on 2026-11-01",
            "Report 2 changed range-fixed last3tonext3months 2026-08-01..2027-02-01 on 2026-11-01",
            "Report 3 changed range-fixed last3tonext3months 2026-08-01..2027-02-01 on 2026-11-01",
            "Report 4 not-kept range-future",
            "Report 5 not-kept range-future",
            "Report 6 not-kept transactions-list",
            "Report 7 not-kept transactions-list",
        ]
    );
    let skipped = |what| {
        report
            .skipped
            .iter()
            .find(|skipped| skipped.what == what)
            .map(|skipped| skipped.count)
    };
    assert_eq!(skipped(SkippedData::Reports), Some(4));
    let views = views(&book);
    let names: Vec<&str> = views.iter().map(|view| view.name.as_str()).collect();
    assert_eq!(names, ["Report 1", "Report 2", "Report 3"]);
    for (view, key) in views.iter().zip(["R000001", "R000002", "R000003"]) {
        assert_eq!(
            view.origin,
            Some(ViewOrigin::kmymoney(key, import_day())),
            "{key}"
        );
    }
    // Report 1 keeps its two accounts as the view's filter.
    let mut filtered = views[0].accounts.clone();
    filtered.sort();
    let mut wanted = vec![
        account(&book, "Pressurised Rover").to_string(),
        account(&book, "Dome Credit Union").to_string(),
    ];
    wanted.sort();
    assert_eq!(filtered, wanted);
    let kinds: Vec<ComponentType> = views
        .iter()
        .map(|view| match &view.entries[0].item {
            Item::Component(component) => component.kind(),
            Item::Unknown(_) => panic!("known"),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            ComponentType::ChartPie,
            ComponentType::ChartStackedBar,
            ComponentType::ChartStackedBar
        ]
    );
}

#[test]
fn both_formats_of_the_samples_give_the_same_views() {
    let (kmy_book, kmy) = from_kmy();
    let (sqlite_book, sqlite) = from_sqlite();
    assert_eq!(summary(&kmy), summary(&sqlite));
    // The IDs differ, since every import gives new ones: compare the views
    // with each ID replaced by the name of what it stands for.
    let named = |book: &Book| -> Vec<String> {
        let mut names: Vec<(String, String)> = book
            .all_accounts()
            .expect("accounts")
            .into_iter()
            .map(|account| (account.id.to_string(), account.name))
            .collect();
        names.sort_by_key(|(id, _)| std::cmp::Reverse(id.len()));
        views(book)
            .iter()
            .map(|view| {
                names
                    .iter()
                    .fold(view.to_json(), |text, (id, name)| text.replace(id, name))
            })
            .collect()
    };
    assert_eq!(named(&kmy_book), named(&sqlite_book));
}

#[test]
fn the_figures_of_each_kept_view_are_those_of_the_book() {
    let (book, _) = from_kmy();
    let views = views(&book);
    let accounts = book.all_accounts().expect("accounts");
    let kind_of = |id: AccountId| {
        accounts
            .iter()
            .find(|account| account.id == id)
            .map(|account| (account.kind, account.parent))
    };
    // The top-level category a category sits in.
    let top = |mut id: AccountId| loop {
        match kind_of(id) {
            Some((_, Some(parent))) => id = parent,
            _ => return id,
        }
    };
    let transactions = book.transactions().expect("transactions");
    // Money out per top-level category in `period`, of the transactions
    // that touch `filtered` (all of them if it is empty), added up here from
    // the book's own parts.
    let by_hand = |period: Period, filtered: &[AccountId], kind: AccountKind| {
        let mut totals: Vec<(AccountId, i64)> = Vec::new();
        for transaction in &transactions {
            let splits = transaction.splits();
            let touches = filtered.is_empty()
                || splits
                    .iter()
                    .any(|split| split.account.is_some_and(|id| filtered.contains(&id)));
            for split in splits {
                let Some(id) = split.account else { continue };
                let day = transaction.date_of(split);
                if !touches || day < period.first() || day > period.last() {
                    continue;
                }
                if kind_of(id).map(|(of, _)| of) != Some(kind) {
                    continue;
                }
                let amount = split.amount.minor_units();
                let amount = if kind == AccountKind::Income {
                    -amount
                } else {
                    amount
                };
                match totals.iter_mut().find(|(of, _)| *of == top(id)) {
                    Some((_, total)) => *total += amount,
                    None => totals.push((top(id), amount)),
                }
            }
        }
        totals.retain(|(_, total)| *total != 0);
        totals.sort();
        totals
    };
    let period_of = |view: &Document| {
        let word = view.period.as_deref().expect("a period");
        Period::for_component(
            word,
            Period::this_month(import_day()).expect("p"),
            import_day(),
        )
        .expect("a period word")
        .expect("a period")
    };

    // Report 1: money out by top-level category of its two accounts.
    let view = &views[0];
    let period = period_of(view);
    let asked = view.asked("c1", &view.filters()).expect("asked");
    let source = Source::read(&book, period).expect("source");
    let applied = asked.apply(&source).expect("applied");
    let shown =
        spending_by_category(&source, period, &applied.filter, Level::TopLevel).expect("figures");
    let mut figures: Vec<(AccountId, i64)> = shown
        .iter()
        .flat_map(|currency| &currency.rows)
        .filter_map(|row| row.category.map(|id| (id, row.total.minor_units())))
        .collect();
    figures.sort();
    let filtered: Vec<AccountId> = view
        .accounts
        .iter()
        .map(|id| {
            accounts
                .iter()
                .find(|account| account.id.to_string() == *id)
                .expect("an account")
                .id
        })
        .collect();
    assert_eq!(figures, by_hand(period, &filtered, AccountKind::Expense));

    // Reports 2 and 3: each top-level category over the months.
    for view in &views[1..] {
        let period = period_of(view);
        let Item::Component(component) = &view.entries[0].item else {
            panic!("known");
        };
        let Some(kassensturz_report::Setting::Ids(series)) = component.get("series") else {
            panic!("series");
        };
        let members: Vec<GroupMember> = series
            .iter()
            .map(|id| {
                GroupMember::Category(
                    accounts
                        .iter()
                        .find(|account| account.id.to_string() == *id)
                        .expect("a category")
                        .id,
                )
            })
            .collect();
        let source = Source::read(&book, period).expect("source");
        let shown = category_over_time(
            &source,
            period,
            &Default::default(),
            &members,
            Overlap::Refused,
        )
        .expect("figures");
        let mut figures: Vec<(AccountId, i64)> = Vec::new();
        for currency in &shown {
            for (series, total) in currency.series.iter().zip(&currency.totals) {
                let Some(GroupMember::Category(id)) = series.of else {
                    panic!("a category");
                };
                if total.minor_units() != 0 {
                    figures.push((id, total.minor_units()));
                }
            }
        }
        figures.sort();
        let mut wanted = by_hand(period, &[], AccountKind::Expense);
        wanted.extend(by_hand(period, &[], AccountKind::Income));
        wanted.sort();
        assert_eq!(figures, wanted, "{}", view.name);
        assert!(!figures.is_empty(), "{}", view.name);
    }
}

#[test]
fn without_the_import_day_the_reports_are_counted_as_left_out() {
    let Imported::Book(imported) =
        import(&Kmy, &golden("kmy", "reports-anon.kmy"), &Limits::default()).expect("read")
    else {
        panic!("a book");
    };
    let mut book = Book::create_in_memory().expect("book created");
    let report = fill_book_from_with(&mut book, &imported, &HashMap::new(), RuleScope::default())
        .expect("filled");
    assert!(report.reports.is_empty());
    assert!(book.views(None).expect("views").is_empty());
    assert!(
        report
            .skipped
            .iter()
            .any(|skipped| skipped.what == SkippedData::Reports && skipped.count == 7)
    );
    let _ = Outcome::Kept;
}
