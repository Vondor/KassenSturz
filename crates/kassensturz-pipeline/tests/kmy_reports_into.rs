// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! KMyMoney's saved reports into a book that already exists, again and
//! again (M6d.6b, D-450, D-491). The file is the maintainer's checked sample
//! (D-431, a made-up Space Colony book with seven reports): a book is made
//! from it as the app makes one (M6d.6a), then the same file's reports are
//! brought in again after the user changed one view and deleted another,
//! so each step meets the real output of the one before (AGENTS.md §9.4).

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use kassensturz_core::{AccountKind, Date, ViewKind};
use kassensturz_import::{Imported, ImportedBook, Kmy, KmySqlite, Limits, import};
use kassensturz_pipeline::{
    ReportState, ReportsPlan, RuleScope, add_reports, fill_book_from_on, plan_reports,
};
use kassensturz_report::{Component, ComponentType, Document, SPENDING_BY_CATEGORY};
use kassensturz_store::Book;

fn golden(folder: &str, name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../kassensturz-import/tests/golden")
        .join(folder)
        .join(name);
    fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn day(text: &str) -> Date {
    text.parse().expect("a date")
}

fn sample() -> ImportedBook {
    let Imported::Book(imported) =
        import(&Kmy, &golden("kmy", "reports-anon.kmy"), &Limits::default()).expect("read")
    else {
        panic!("a book");
    };
    *imported
}

/// A book made from the sample on the day the maintainer saved it.
fn made_from_sample(imported: &ImportedBook) -> Book {
    let mut book = Book::create_in_memory().expect("book created");
    fill_book_from_on(
        &mut book,
        imported,
        &HashMap::new(),
        RuleScope::default(),
        day("2026-11-01"),
    )
    .expect("filled");
    book
}

fn plan(book: &Book, imported: &ImportedBook, on: &str) -> ReportsPlan {
    plan_reports(book, &imported.accounts, &imported.reports, day(on)).expect("planned")
}

/// Each report as the preview lists it: name, state and reasons.
fn lines(plan: &ReportsPlan) -> Vec<String> {
    plan.reports
        .iter()
        .map(|report| {
            format!(
                "{} {} {}",
                report.line.name,
                report.state.keyword(),
                report.line.reasons.join(",")
            )
        })
        .collect()
}

/// The book's views, as stored, in their order.
fn views(book: &Book) -> Vec<(String, String)> {
    book.views(Some(ViewKind::View))
        .expect("views")
        .into_iter()
        .map(|view| (view.name, view.document))
        .collect()
}

fn view_id(book: &Book, name: &str) -> kassensturz_core::ViewId {
    book.views(Some(ViewKind::View))
        .expect("views")
        .into_iter()
        .find(|view| view.name == name)
        .unwrap_or_else(|| panic!("no view {name}"))
        .id
}

#[test]
fn the_same_file_again_finds_every_view_already_here() {
    let imported = sample();
    let book = made_from_sample(&imported);
    // Days later: the periods that became days are judged on the day they
    // were imported, so they are no change (D-491).
    let again = plan(&book, &imported, "2026-11-20");
    assert_eq!(
        lines(&again),
        [
            "Report 1 already-here range-fixed,income-left-out",
            "Report 2 already-here range-fixed",
            "Report 3 already-here range-fixed",
            "Report 4 not-kept range-future",
            "Report 5 not-kept range-future",
            "Report 6 not-kept transactions-list",
            "Report 7 not-kept transactions-list",
        ]
    );
    assert!(again.no_longer_in_file.is_empty());
    assert!(
        again
            .reports
            .iter()
            .all(|report| !report.state.can_choose())
    );
}

#[test]
fn both_formats_of_the_sample_plan_alike() {
    let imported = sample();
    let Imported::Book(from_sqlite) = import(
        &KmySqlite,
        &golden("kmy-sqlite", "reports-anon.sqlite"),
        &Limits::default(),
    )
    .expect("read") else {
        panic!("a book");
    };
    let book = made_from_sample(&imported);
    assert_eq!(
        lines(&plan(&book, &imported, "2026-11-20")),
        lines(&plan(&book, &from_sqlite, "2026-11-20"))
    );
}

#[test]
fn a_changed_view_is_kept_a_deleted_one_is_offered_again_and_undo_takes_it_all_back() {
    let imported = sample();
    let mut book = made_from_sample(&imported);
    // The user changes Report 2's period and deletes Report 3.
    let second = view_id(&book, "Report 2");
    let mut changed = Document::read(&book.view(second).expect("view").document).expect("reads");
    changed.period = Some("last-year".into());
    book.edit_view(second, &changed.to_json()).expect("edited");
    book.remove_view(view_id(&book, "Report 3"))
        .expect("removed");
    let before = views(&book);
    let undo_steps = book.history().expect("history").len();

    let again = plan(&book, &imported, "2026-11-20");
    assert_eq!(
        lines(&again)[..3],
        [
            "Report 1 already-here range-fixed,income-left-out",
            "Report 2 kept-yours range-fixed",
            "Report 3 deleted-earlier range-fixed",
        ]
    );
    let states: Vec<bool> = again
        .reports
        .iter()
        .map(|report| report.state.chosen_by_default())
        .collect();
    assert_eq!(states, [false, false, true, false, false, false, false]);

    // As the preview leaves it: Report 3 comes back, Report 2 stays yours.
    let done = add_reports(&mut book, &again, &states).expect("added");
    assert_eq!((done.added, done.replaced), (1, 0));
    let after = views(&book);
    assert_eq!(after.len(), 3);
    assert_eq!(after[..2], before[..]);
    assert_eq!(after[2].0, "Report 3");
    let third = Document::read(&after[2].1).expect("reads");
    // Mapped on the day of this import.
    assert_eq!(
        third.origin.as_ref().map(|origin| origin.imported),
        Some(day("2026-11-20"))
    );
    // One history entry, one Undo.
    assert_eq!(book.history().expect("history").len(), undo_steps + 1);
    book.undo().expect("undone");
    assert_eq!(views(&book), before);
}

#[test]
fn replace_keeps_the_view_its_id_name_and_place() {
    let imported = sample();
    let mut book = made_from_sample(&imported);
    let second = view_id(&book, "Report 2");
    book.rename_view(second, "My bars").expect("renamed");
    let mut changed = Document::read(&book.view(second).expect("view").document).expect("reads");
    changed.period = Some("last-year".into());
    book.edit_view(second, &changed.to_json()).expect("edited");
    let position = book.view(second).expect("view").position;

    let again = plan(&book, &imported, "2026-11-20");
    assert_eq!(again.reports[1].state, ReportState::Differs);
    assert_eq!(again.reports[1].line.name, "My bars");
    assert_eq!(again.reports[1].view, Some(second));
    let mut chosen: Vec<bool> = again
        .reports
        .iter()
        .map(|report| report.state.chosen_by_default())
        .collect();
    chosen[1] = true;
    let done = add_reports(&mut book, &again, &chosen).expect("replaced");
    assert_eq!((done.added, done.replaced), (0, 1));
    let replaced = book.view(second).expect("same ID");
    assert_eq!(
        (replaced.name.as_str(), replaced.position),
        ("My bars", position)
    );
    let document = Document::read(&replaced.document).expect("reads");
    // KMyMoney's period again, as the days it covers on this import's day.
    assert_ne!(document.period, Some("last-year".into()));
    assert_eq!(
        document.origin.map(|origin| origin.imported),
        Some(day("2026-11-20"))
    );
    // Now it is what the report gives.
    assert_eq!(
        plan(&book, &imported, "2026-11-25").reports[1].state,
        ReportState::AlreadyHere
    );
}

#[test]
fn a_report_changed_in_kmymoney_is_kept_as_the_user_has_it() {
    let imported = sample();
    let book = made_from_sample(&imported);
    // The golden copy's records, changed as KMyMoney would after the user
    // picked another chart for Report 1: a ring instead of a pie.
    let mut changed = imported.clone();
    let report = &mut changed.reports[0];
    let chart = report
        .settings
        .iter_mut()
        .find(|(name, _)| name == "charttype")
        .expect("a chart type");
    assert_eq!(chart.1, "pie");
    chart.1 = "ring".into();
    let again = plan(&book, &changed, "2026-11-20");
    assert_eq!(again.reports[0].state, ReportState::Differs);
    assert!(!again.reports[0].state.chosen_by_default());
}

#[test]
fn a_report_gone_from_the_file_leaves_its_view_and_is_listed() {
    let imported = sample();
    let mut book = made_from_sample(&imported);
    let before = views(&book);
    let mut fewer = imported.clone();
    fewer.reports.remove(1);
    let again = plan(&book, &fewer, "2026-11-20");
    assert_eq!(again.no_longer_in_file, ["Report 2"]);
    let chosen: Vec<bool> = again
        .reports
        .iter()
        .map(|report| report.state.chosen_by_default())
        .collect();
    let done = add_reports(&mut book, &again, &chosen).expect("nothing to do");
    assert_eq!((done.added, done.replaced), (0, 0));
    assert_eq!(views(&book), before);
}

#[test]
fn two_reports_with_the_same_id_are_both_refused() {
    let imported = sample();
    let book = made_from_sample(&imported);
    let mut broken = imported.clone();
    let mut twin = broken.reports[0].clone();
    twin.name = "Twin".into();
    broken.reports.push(twin);
    let again = plan(&book, &broken, "2026-11-20");
    assert_eq!(again.reports[0].state, ReportState::SameId);
    assert_eq!(again.reports[7].state, ReportState::SameId);
    assert_eq!(again.reports[7].line.reasons, ["same-id"]);
}

/// A book of its own, which has the sample's categories (it was made from
/// it) but whose accounts have other names.
#[test]
fn into_another_book_names_are_matched_and_what_it_lacks_is_listed() {
    let imported = sample();
    let mut other = imported.clone();
    other.reports.clear();
    for account in other
        .accounts
        .iter_mut()
        .filter(|account| account.kind.is_account())
    {
        account.name = format!("Other {}", account.name);
    }
    let mut book = made_from_sample(&other);
    let again = plan(&book, &imported, "2026-11-20");
    assert_eq!(
        lines(&again)[..3],
        [
            "Report 1 new range-fixed,account-not-found,income-left-out,shows-more",
            "Report 2 new range-fixed",
            "Report 3 new range-fixed",
        ]
    );
    let chosen: Vec<bool> = again
        .reports
        .iter()
        .map(|report| report.state.chosen_by_default())
        .collect();
    let done = add_reports(&mut book, &again, &chosen).expect("added");
    assert_eq!((done.added, done.replaced), (3, 0));
    let first = Document::read(&views(&book)[0].1).expect("reads");
    // The filter lost both accounts: the view shows all of them.
    assert!(first.accounts.is_empty());
    // Brought in once, the same file again finds them here.
    let twice = plan(&book, &imported, "2026-11-21");
    assert!(
        twice.reports[..3]
            .iter()
            .all(|report| report.state == ReportState::AlreadyHere),
        "{:?}",
        lines(&twice)
    );
}

#[test]
fn a_category_the_book_lacks_is_listed_and_a_name_taken_gets_a_number() {
    let imported = sample();
    // A book made from the sample, whose top-level spending categories have
    // other names, and which has a view called "Report 2" already.
    let mut other = imported.clone();
    other.reports.clear();
    for account in other
        .accounts
        .iter_mut()
        .filter(|account| account.kind == AccountKind::Expense && account.parent.is_none())
    {
        account.name = format!("Other {}", account.name);
    }
    let mut book = made_from_sample(&other);
    let taken = Document::view(
        "Report 2",
        None,
        Component::new(ComponentType::TableRows, SPENDING_BY_CATEGORY).expect("a table"),
    );
    book.add_view(&taken.to_json()).expect("a view");
    let again = plan(&book, &imported, "2026-11-20");
    assert_eq!(again.reports[1].line.name, "Report 2 (2)");
    assert!(
        again.reports[1]
            .line
            .reasons
            .contains(&"category-not-found"),
        "{:?}",
        lines(&again)
    );
}
