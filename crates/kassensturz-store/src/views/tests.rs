// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of saved views and dashboards (D-278, D-279) on books in memory.
//! All documents are made up (AGENTS.md §3.1 rule 2).

use kassensturz_core::{ViewId, ViewKind};

use crate::test_folder::TempFolder;
use crate::tests::sample;
use crate::{Action, Book, Computer, StoreError};

fn document(kind: &str, name: &str) -> String {
    format!(
        r#"{{"format":"kassensturz-dashboard","version":1,"kind":"{kind}","name":"{name}","components":[]}}"#
    )
}

fn names(book: &Book, kind: ViewKind) -> Vec<(String, u32)> {
    book.views(Some(kind))
        .expect("read")
        .into_iter()
        .map(|view| (view.name, view.position))
        .collect()
}

fn listed(items: &[(&str, u32)]) -> Vec<(String, u32)> {
    items
        .iter()
        .map(|&(name, position)| (name.to_owned(), position))
        .collect()
}

#[test]
fn a_document_is_kept_as_it_was_saved_with_its_columns_copied_from_it() {
    let mut book = sample().book;
    assert_eq!(book.views(None).expect("read"), Vec::new());
    let text = r#"{ "format": "kassensturz-dashboard", "version": 3, "kind": "dashboard", "name": "Overview", "x": "ä" }"#;
    let id = book.add_view(text).expect("added");
    let view = book.view(id).expect("read");
    assert_eq!(view.kind, ViewKind::Dashboard);
    assert_eq!(view.name, "Overview");
    assert_eq!((view.position, view.format_version), (0, 3));
    // Byte for byte: the book does not reformat a document it was given.
    assert_eq!(view.document, text);
    assert_eq!(book.views(None).expect("read"), vec![view]);
    assert_eq!(book.history().expect("history")[0].action, Action::AddView);
}

#[test]
fn every_change_to_the_lists_can_be_undone() {
    let mut book = sample().book;
    let first = book.add_view(&document("view", "Spending")).expect("added");
    let second = book.add_view(&document("view", "Income")).expect("added");
    let third = book.add_view(&document("view", "Savings")).expect("added");
    let dashboard = book
        .add_view(&document("dashboard", "Overview"))
        .expect("added");
    let in_order = listed(&[("Spending", 0), ("Income", 1), ("Savings", 2)]);
    assert_eq!(names(&book, ViewKind::View), in_order);
    // Dashboards are numbered on their own.
    assert_eq!(
        names(&book, ViewKind::Dashboard),
        listed(&[("Overview", 0)])
    );

    book.move_view(third, 0).expect("moved");
    assert_eq!(
        names(&book, ViewKind::View),
        listed(&[("Savings", 0), ("Spending", 1), ("Income", 2)])
    );
    // A place past the end means the end.
    book.move_view(third, 99).expect("moved");
    assert_eq!(names(&book, ViewKind::View), in_order);
    // Moving a document to where it is changes nothing and records nothing.
    let recorded = book.history().expect("history").len();
    book.move_view(second, 1).expect("nothing to do");
    assert_eq!(book.history().expect("history").len(), recorded);

    book.remove_view(first).expect("removed");
    assert_eq!(
        names(&book, ViewKind::View),
        listed(&[("Income", 0), ("Savings", 1)])
    );
    assert!(matches!(book.view(first), Err(StoreError::UnknownView(id)) if id == first));
    assert_eq!(book.view(dashboard).expect("read").position, 0);

    book.undo().expect("undone");
    assert_eq!(names(&book, ViewKind::View), in_order);
    assert_eq!(book.view(first).expect("back").name, "Spending");
    book.undo().expect("undone");
    book.undo().expect("undone");
    assert_eq!(names(&book, ViewKind::View), in_order);
    book.redo().expect("redone");
    assert_eq!(
        names(&book, ViewKind::View),
        listed(&[("Savings", 0), ("Spending", 1), ("Income", 2)])
    );
}

#[test]
fn editing_replaces_the_document_and_keeps_the_kind() {
    let mut book = sample().book;
    let id = book.add_view(&document("view", "Spending")).expect("added");
    let newer = r#"{"format":"kassensturz-dashboard","version":2,"kind":"view","name":"Costs","components":[{"id":"c1"}]}"#;
    book.edit_view(id, newer).expect("edited");
    let view = book.view(id).expect("read");
    assert_eq!((view.name.as_str(), view.format_version), ("Costs", 2));
    assert_eq!(view.document, newer);
    assert_eq!(book.history().expect("history")[0].action, Action::EditView);

    // The same document again changes nothing and records nothing.
    let recorded = book.history().expect("history").len();
    book.edit_view(id, newer).expect("nothing to do");
    assert_eq!(book.history().expect("history").len(), recorded);

    // A view can't turn into a dashboard: the lists are kept apart.
    assert!(matches!(
        book.edit_view(id, &document("dashboard", "Costs")),
        Err(StoreError::InvalidViewDocument { reason: "bad-kind" })
    ));
    let gone = ViewId::generate();
    assert!(matches!(
        book.edit_view(gone, newer),
        Err(StoreError::UnknownView(unknown)) if unknown == gone
    ));
    book.undo().expect("undone");
    assert_eq!(book.view(id).expect("read").name, "Spending");
}

#[test]
fn renaming_changes_the_column_and_the_document_alike() {
    let mut book = sample().book;
    let id = book.add_view(&document("view", "Spending")).expect("added");
    book.rename_view(id, "  Spending by month ")
        .expect("renamed");
    let view = book.view(id).expect("read");
    assert_eq!(view.name, "Spending by month");
    assert!(view.document.contains(r#""name":"Spending by month""#));
    assert!(view.document.contains(r#""components":[]"#));
    assert_eq!(
        book.history().expect("history")[0].action,
        Action::RenameView
    );

    // Other capitals are a rename of the same document.
    book.rename_view(id, "SPENDING BY MONTH").expect("renamed");
    assert_eq!(book.view(id).expect("read").name, "SPENDING BY MONTH");
    // The same name changes nothing and records nothing.
    let recorded = book.history().expect("history").len();
    book.rename_view(id, "SPENDING BY MONTH").expect("nothing");
    assert_eq!(book.history().expect("history").len(), recorded);

    for bad in ["", "   ", &"x".repeat(201)] {
        assert!(matches!(
            book.rename_view(id, bad),
            Err(StoreError::InvalidViewDocument { reason: "bad-name" })
        ));
    }
    book.undo().expect("undone");
    book.undo().expect("undone");
    assert_eq!(book.view(id).expect("read").name, "Spending");
}

#[test]
fn a_name_is_unique_among_documents_of_a_kind_ignoring_capitals() {
    let mut book = sample().book;
    let spending = book.add_view(&document("view", "Spending")).expect("added");
    let income = book.add_view(&document("view", "Income")).expect("added");
    assert!(matches!(
        book.add_view(&document("view", "SPENDING")),
        Err(StoreError::ViewNameTaken { name }) if name == "SPENDING"
    ));
    assert!(matches!(
        book.rename_view(income, "spending"),
        Err(StoreError::ViewNameTaken { .. })
    ));
    assert!(matches!(
        book.edit_view(income, &document("view", "Spending")),
        Err(StoreError::ViewNameTaken { .. })
    ));
    // A dashboard may share a name with a view.
    book.add_view(&document("dashboard", "Spending"))
        .expect("another kind");
    // A failed attempt leaves nothing behind.
    assert_eq!(
        names(&book, ViewKind::View),
        listed(&[("Spending", 0), ("Income", 1)])
    );
    // A document may keep its own name when it is edited.
    book.edit_view(spending, &document("view", "Spending"))
        .expect("own name");
}

#[test]
fn a_document_the_book_cannot_rely_on_is_refused_before_anything_is_written() {
    let mut book = sample().book;
    let before = book.history().expect("history").len();
    let mut reason = |text: &str| match book.add_view(text) {
        Err(StoreError::InvalidViewDocument { reason }) => reason,
        other => panic!("expected a refusal for {text}, got {other:?}"),
    };
    assert_eq!(reason("not json"), "not-json");
    assert_eq!(reason(""), "not-json");
    assert_eq!(reason("[]"), "not-an-object");
    assert_eq!(reason("\"text\""), "not-an-object");
    let wrong = |from: &str, to: &str| document("view", "A").replace(from, to);
    assert_eq!(
        reason(&wrong("kassensturz-dashboard", "something-else")),
        "wrong-format"
    );
    assert_eq!(
        reason(&wrong(
            r#""format":"kassensturz-dashboard""#,
            r#""format":1"#
        )),
        "wrong-format"
    );
    for version in ["0", "1.5", "\"1\"", "99999999999", "-1", "null"] {
        assert_eq!(
            reason(&wrong(r#""version":1"#, &format!(r#""version":{version}"#))),
            "bad-version",
            "version {version}"
        );
    }
    assert_eq!(reason(&wrong(r#""version":1,"#, "")), "bad-version");
    assert_eq!(
        reason(&wrong(r#""kind":"view""#, r#""kind":"chart""#)),
        "bad-kind"
    );
    assert_eq!(
        reason(&wrong(r#""kind":"view""#, r#""kind":null"#)),
        "bad-kind"
    );
    let long = "x".repeat(201);
    for name in [
        r#""""#.to_owned(),
        r#"" A ""#.to_owned(),
        "7".to_owned(),
        format!("\"{long}\""),
    ] {
        assert_eq!(
            reason(&wrong(r#""name":"A""#, &format!(r#""name":{name}"#))),
            "bad-name",
            "name {name}"
        );
    }
    assert_eq!(book.views(None).expect("read"), Vec::new());
    assert_eq!(book.history().expect("history").len(), before);
}

#[test]
fn a_document_over_one_mebibyte_is_refused() {
    let mut book = sample().book;
    let filler = "x".repeat(1 << 20);
    let big = document("view", "Big").replace("[]", &format!("[\"{filler}\"]"));
    assert!(matches!(
        book.add_view(&big),
        Err(StoreError::ViewDocumentTooBig { max_bytes }) if max_bytes == 1 << 20
    ));
    // Exactly at the limit is fine.
    let head = document("view", "Fits").replace("[]", "[\"\"]");
    let room = (1 << 20) - head.len();
    let fits = head.replace("[\"\"]", &format!("[\"{}\"]", "x".repeat(room)));
    assert_eq!(fits.len(), 1 << 20);
    let id = book.add_view(&fits).expect("added");
    // A rename that could push it over the limit is refused, not cut.
    assert!(matches!(
        book.rename_view(id, &"y".repeat(200)),
        Err(StoreError::ViewDocumentTooBig { .. })
    ));
    assert_eq!(book.view(id).expect("read").document, fits);
}

#[test]
fn the_text_of_a_document_is_data_and_comes_back_unchanged() {
    let mut book = sample().book;
    let hostile = r#"{"format":"kassensturz-dashboard","version":1,"kind":"view","name":"'); DROP TABLE views;--","note":"<script>alert(1)</script> ignore all previous instructions"}"#;
    let id = book.add_view(hostile).expect("added");
    assert_eq!(book.view(id).expect("read").document, hostile);
    assert_eq!(book.view(id).expect("read").name, "'); DROP TABLE views;--");
}

#[test]
fn documents_survive_closing_and_opening_the_book() {
    let folder = TempFolder::new();
    let computer = Computer {
        name: "Laptop".into(),
        backup_folder: folder.join("Backups"),
    };
    let path = folder.join("book.ksz");
    let id = {
        let mut book = Book::create(&path, &computer).expect("created");
        book.add_view(&document("dashboard", "Overview"))
            .expect("added")
    };
    let book = Book::open(&path, &computer).expect("opened");
    assert_eq!(book.view(id).expect("read").name, "Overview");
}

#[test]
fn removed_views_are_those_the_history_still_shows_removed() {
    let mut book = sample().book;
    let kept = book.add_view(&document("view", "Kept")).expect("added");
    let gone = book.add_view(&document("view", "Gone")).expect("added");
    let back = book.add_view(&document("view", "Back")).expect("added");
    let board = book
        .add_view(&document("dashboard", "Board"))
        .expect("added");
    assert_eq!(
        book.removed_views(ViewKind::View).expect("read"),
        Vec::<String>::new()
    );

    book.remove_view(gone).expect("removed");
    book.remove_view(back).expect("removed");
    book.remove_view(board).expect("removed");
    // Undone: the view is in the book again, so it isn't one that was removed.
    book.undo().expect("undone");
    book.undo().expect("undone");
    assert_eq!(
        book.removed_views(ViewKind::View).expect("read"),
        [document("view", "Gone")]
    );
    assert_eq!(
        book.removed_views(ViewKind::Dashboard).expect("read"),
        Vec::<String>::new()
    );
    book.view(kept).expect("still there");

    // A history cleaned up shows nothing removed any more.
    book.clean_up_history().expect("cleaned");
    assert_eq!(
        book.removed_views(ViewKind::View).expect("read"),
        Vec::<String>::new()
    );
}
