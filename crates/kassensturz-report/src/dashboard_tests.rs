// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The dashboard document v1 of M6b.2 (D-289, D-312, D-313): frames checked
//! when a dashboard is read, `props.period`, the heading, the built-in
//! default document and saving a layout session in the book.

use kassensturz_core::ViewKind;
use kassensturz_store::Book;
use serde_json::{Value, json};

use crate::tests::{date, money, period, sample};
use crate::*;

fn card(id: &str, col: u32, row: u32) -> Value {
    json!({"id": id, "type": "kpi.card", "dataset": "income_expense_by_month",
        "frame": {"col": col, "row": row, "cols": 3, "rows": 2}})
}

fn dashboard(components: Vec<Value>) -> String {
    json!({"format": "kassensturz-dashboard", "version": 1, "kind": "dashboard",
        "name": "Mine", "components": components})
    .to_string()
}

fn reason(text: &str) -> (&'static str, String) {
    match Document::read(text).unwrap_err() {
        ReportError::DocumentInvalid { reason, path } => (reason, path),
        other => panic!("not a document error: {other:?}"),
    }
}

#[test]
fn the_frames_of_a_dashboard_are_checked_when_it_is_read() {
    let wide = |cols: u32, col: u32| {
        let mut chart = json!({"id": "c2", "type": "chart.pie", "dataset": "spending_by_category"});
        chart["frame"] = json!({"col": col, "row": 4, "cols": cols, "rows": 3});
        chart
    };
    let small = json!({"id": "c2", "type": "chart.pie", "dataset": "spending_by_category",
        "frame": {"col": 0, "row": 4, "cols": 2, "rows": 3}});
    let no_frame = json!({"id": "c2", "type": "chart.pie", "dataset": "spending_by_category"});
    let mut tall = wide(3, 0);
    tall["frame"]["rows"] = 25.into();
    for (second, why, path) in [
        (wide(4, 9), "frame-outside-grid", "components.1.frame"),
        (wide(13, 0), "frame-outside-grid", "components.1.frame"),
        (tall, "frame-outside-grid", "components.1.frame"),
        (small, "frame-too-small", "components.1.frame"),
        (no_frame, "bad-frame", "components.1.frame"),
        // Shares a cell with the card: the later of the two is named.
        (
            json!({"id": "c2", "type": "kpi.card", "dataset": "income_expense_by_month",
                "frame": {"col": 2, "row": 1, "cols": 3, "rows": 2}}),
            "frames-overlap",
            "components.1.frame",
        ),
    ] {
        let (got, got_path) = reason(&dashboard(vec![card("c1", 0, 0), second.clone()]));
        assert_eq!((got, got_path.as_str()), (why, path), "{second}");
    }
    // Touching edges are not an overlap; a gap is allowed.
    Document::read(&dashboard(vec![
        card("c1", 0, 0),
        card("c2", 3, 0),
        card("c3", 9, 7),
    ]))
    .unwrap();
}

#[test]
fn a_component_this_version_does_not_know_may_be_one_cell_but_still_needs_a_place() {
    let unknown = |frame: Value| {
        let mut radar = json!({"id": "c2", "type": "chart.radar", "dataset": "x"});
        if !frame.is_null() {
            radar["frame"] = frame;
        }
        dashboard(vec![card("c1", 0, 0), radar])
    };
    Document::read(&unknown(json!({"col": 3, "row": 0, "cols": 1, "rows": 1}))).unwrap();
    assert_eq!(reason(&unknown(Value::Null)).0, "bad-frame");
    assert_eq!(
        reason(&unknown(json!({"col": 2, "row": 0, "cols": 1, "rows": 1}))).0,
        "frames-overlap"
    );
}

#[test]
fn a_document_that_fails_is_refused_whole_and_never_repaired() {
    let text = dashboard(vec![card("c1", 0, 0), card("c2", 1, 0), card("c3", 6, 0)]);
    assert_eq!(reason(&text).0, "frames-overlap");
}

#[test]
fn every_edit_of_the_editor_is_a_document_the_reader_accepts() {
    let mut editor = Editor::new(Document::default_overview().unwrap()).unwrap();
    let income = "income".to_owned();
    editor.move_to(&income, 0, 6).unwrap();
    editor.resize_to("trend", 12, 6).unwrap();
    let added = editor
        .add(Component::new(ComponentType::ChartBar, "spending_by_category").unwrap())
        .unwrap();
    editor.duplicate(&added).unwrap();
    editor.delete("kept").unwrap();
    let text = editor.document().to_json();
    assert_eq!(Document::read(&text).unwrap(), *editor.document());
}

#[test]
fn a_period_is_the_page_or_one_of_the_five_fixed_periods() {
    let make = |period: Value| {
        let mut chart = card("c1", 0, 0);
        chart["props"] = json!({"period": period});
        dashboard(vec![chart])
    };
    let mut seen = Vec::new();
    for name in PERIODS {
        let document = Document::read(&make(name.into())).unwrap();
        let Item::Component(component) = &document.entries[0].item else {
            panic!("a known component");
        };
        assert_eq!(component.get("period"), Some(&Setting::Word(name.into())));
        // It is written and read back.
        assert_eq!(Document::read(&document.to_json()).unwrap(), document);
        seen.push(name);
    }
    // The schema offers exactly the periods the reader knows, after `page`.
    let words = ComponentType::KpiCard
        .properties()
        .iter()
        .find(|property| property.key == "period")
        .map(|property| property.words(dataset("income_expense_by_month").unwrap()))
        .unwrap();
    assert_eq!(&words[1..], PERIODS.as_slice());
    assert_eq!(words[0], "page");
    // Left out, it is `page`.
    let plain = Document::read(&dashboard(vec![card("c1", 0, 0)])).unwrap();
    let Item::Component(component) = &plain.entries[0].item else {
        panic!("a known component");
    };
    assert_eq!(component.get("period"), Some(&Setting::Word("page".into())));
    for bad in [json!("forever"), json!("This-Month"), json!(7), json!(null)] {
        assert_eq!(reason(&make(bad.clone())).1, "props.period", "{bad}");
    }
}

#[test]
fn a_component_takes_the_page_period_or_its_own() {
    let page = period(date(2026, 8, 1), date(2026, 8, 31));
    let today = date(2026, 10, 15);
    let of = |name| Period::for_component(name, page, today).unwrap().unwrap();
    assert_eq!(of("page"), page);
    assert_eq!(
        of("this-month"),
        period(date(2026, 10, 1), date(2026, 10, 31))
    );
    assert_eq!(
        of("last-month"),
        period(date(2026, 9, 1), date(2026, 9, 30))
    );
    assert_eq!(
        of("last-12-months"),
        period(date(2025, 11, 1), date(2026, 10, 31))
    );
    assert_eq!(
        of("this-year"),
        period(date(2026, 1, 1), date(2026, 12, 31))
    );
    assert_eq!(
        of("last-year"),
        period(date(2025, 1, 1), date(2025, 12, 31))
    );
    assert!(Period::for_component("someday", page, today).is_none());
    for name in PERIODS {
        assert!(Period::for_component(name, page, today).is_some(), "{name}");
    }
}

fn heading(text: &str) -> Value {
    json!({"id": "h1", "type": "text.heading", "props": {"text": text},
        "frame": {"col": 0, "row": 0, "cols": 12, "rows": 1}})
}

#[test]
fn a_heading_is_plain_text_without_data_and_without_a_period() {
    let document = Document::read(&dashboard(vec![heading("Spending"), card("c1", 0, 1)])).unwrap();
    let Item::Component(component) = &document.entries[0].item else {
        panic!("a known component");
    };
    assert_eq!(component.kind(), ComponentType::Heading);
    assert_eq!(
        component.get("text"),
        Some(&Setting::Text("Spending".into()))
    );
    assert!(component.get("period").is_none());
    // Written without a dataset or params, and read back the same.
    let written: Value = serde_json::from_str(&document.to_json()).unwrap();
    let object = written["components"][0].as_object().unwrap();
    assert!(!object.contains_key("dataset") && !object.contains_key("params"));
    assert_eq!(object["props"], json!({"text": "Spending"}));
    assert_eq!(Document::read(&document.to_json()).unwrap(), document);
    // Markup stays text: nothing reads it.
    let markup = "<b>Spending</b> & [link](http://example.com) {{x}}";
    let kept = Document::read(&dashboard(vec![heading(markup)])).unwrap();
    assert!(kept.to_json().contains("<b>Spending</b>"));
}

#[test]
fn a_hostile_heading_is_refused_with_the_path_of_the_setting() {
    for (patch, why, path) in [
        (heading(&"x".repeat(201)), "too-long", "props.text"),
        (heading("two\nlines"), "bad-text", "props.text"),
        (heading("bell\u{7}"), "bad-text", "props.text"),
        (
            json!({"id": "h1", "type": "text.heading", "dataset": "spending_by_category",
                "props": {"text": "x"}, "frame": {"col": 0, "row": 0, "cols": 12, "rows": 1}}),
            "unknown-property",
            "dataset",
        ),
        (
            json!({"id": "h1", "type": "text.heading", "props": {"text": 7},
                "frame": {"col": 0, "row": 0, "cols": 12, "rows": 1}}),
            "wrong-type",
            "props.text",
        ),
        (
            json!({"id": "h1", "type": "text.heading", "props": {"period": "page"},
                "frame": {"col": 0, "row": 0, "cols": 12, "rows": 1}}),
            "unknown-property",
            "props.period",
        ),
        (
            json!({"id": "h1", "type": "text.heading", "props": {"onclick": "x"},
                "frame": {"col": 0, "row": 0, "cols": 12, "rows": 1}}),
            "unknown-property",
            "props.onclick",
        ),
        (
            json!({"id": "h1", "type": "text.heading", "params": {"grouping": "category"},
                "frame": {"col": 0, "row": 0, "cols": 12, "rows": 1}}),
            "unknown-property",
            "params.grouping",
        ),
    ] {
        let (got, got_path) = reason(&dashboard(vec![patch.clone()]));
        assert_eq!((got, got_path.as_str()), (why, path), "{patch}");
    }
    // 200 characters is the limit, and a heading may be one row of three
    // cells but not smaller.
    Document::read(&dashboard(vec![heading(&"x".repeat(200))])).unwrap();
    let mut narrow = heading("x");
    narrow["frame"]["cols"] = 2.into();
    assert_eq!(reason(&dashboard(vec![narrow])).0, "frame-too-small");
    let mut slim = heading("x");
    slim["frame"]["cols"] = 3.into();
    Document::read(&dashboard(vec![slim])).unwrap();
}

#[test]
fn the_heading_is_in_the_library_and_not_among_what_the_ai_may_pick() {
    assert_eq!(
        ComponentType::from_id("text.heading"),
        Some(ComponentType::Heading)
    );
    assert!(ComponentType::LIBRARY.contains(&ComponentType::Heading));
    assert!(!ComponentType::ALL.contains(&ComponentType::Heading));
    assert_eq!(
        ComponentType::LIBRARY[..ComponentType::ALL.len()],
        ComponentType::ALL
    );
    // It shows no data, so no dataset is offered or accepted for it.
    assert_eq!(ComponentType::Heading.datasets().count(), 0);
    assert!(Component::new(ComponentType::Heading, "spending_by_category").is_err());
    let heading = Component::new(ComponentType::Heading, "").unwrap();
    assert_eq!(heading.get("text"), Some(&Setting::Text(String::new())));
    // A chart can't be changed into it, and the other way round either.
    let mut chart = Component::new(ComponentType::ChartPie, "spending_by_category").unwrap();
    assert!(chart.change_kind(ComponentType::Heading).is_err());
    let mut heading = heading;
    heading.change_kind(ComponentType::ChartPie).unwrap();
    assert_eq!(heading.kind(), ComponentType::ChartPie);
    assert_eq!(
        ComponentType::Heading.smallest_size(),
        grid::Size { cols: 3, rows: 1 }
    );
    assert_eq!(
        ComponentType::Heading.default_size(),
        grid::Size { cols: 12, rows: 1 }
    );
}

#[test]
fn the_default_overview_is_a_valid_dashboard_that_writes_back_the_same() {
    let document = Document::default_overview().unwrap();
    assert_eq!(document.kind, ViewKind::Dashboard);
    assert_eq!(document.name, OVERVIEW_NAME);
    assert_eq!(document.period, None);
    assert_eq!(document.entries.len(), 9);
    grid::check(&document.entries).unwrap();
    assert_eq!(Document::read(&document.to_json()).unwrap(), document);
    // Its reading order is the order the Overview shows things in.
    let editor = Editor::new(document).unwrap();
    assert_eq!(
        editor.reading_order(),
        [
            "fetch-gaps",
            "income",
            "expenses",
            "left",
            "kept",
            "spending",
            "biggest",
            "trend",
            "transfers"
        ]
    );
}

/// What a component of the default document asks the reporting layer for.
fn asks(entry: &Entry) -> (&'static str, String, String, String, String) {
    let Item::Component(component) = &entry.item else {
        panic!("a known component");
    };
    let word = |key: &str| match component.get(key) {
        Some(Setting::Word(word)) => word.clone(),
        _ => String::new(),
    };
    (
        component.kind().id(),
        component.dataset().id.to_owned(),
        if component.kind() == ComponentType::KpiCard {
            word("measure")
        } else {
            word("grouping")
        },
        word("compare"),
        word("period"),
    )
}

#[test]
fn the_default_overview_asks_for_what_the_overview_page_asks_for() {
    // OverviewMonth.qml: the cards of the month against the month before
    // (income, expenses, what is left, savings rate), the donut and its
    // table by top-level category for the month, and the line of money in
    // and out for the last 12 months.
    let document = Document::default_overview().unwrap();
    let asked: Vec<_> = document.entries.iter().map(asks).collect();
    let card = |measure: &str| {
        (
            "kpi.card",
            "income_expense_by_month".to_owned(),
            measure.to_owned(),
            "previous_period".to_owned(),
            "page".to_owned(),
        )
    };
    // The notices ask the book for nothing: the page fills them.
    let notice = |kind| {
        (
            kind,
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        )
    };
    let spending = |kind| {
        (
            kind,
            "spending_by_category".to_owned(),
            "top_level_category".to_owned(),
            String::new(),
            "page".to_owned(),
        )
    };
    assert_eq!(
        asked,
        [
            notice("notice.fetch_gap"),
            card("income"),
            card("expenses"),
            card("net"),
            card("savings-rate"),
            spending("chart.donut"),
            spending("table.rows"),
            (
                "chart.line",
                "income_expense_by_month".to_owned(),
                "month".to_owned(),
                String::new(),
                "last-12-months".to_owned(),
            ),
            notice("notice.transfers"),
        ]
    );
}

#[test]
fn the_default_overview_shows_the_same_figures_as_the_overview_on_the_sample_book() {
    let sample = sample();
    let page = period(date(2026, 10, 1), date(2026, 10, 31));
    let today = date(2026, 10, 15);
    let document = Document::default_overview().unwrap();
    let every_component = || {
        document.entries.iter().filter_map(|entry| {
            let Item::Component(component) = &entry.item else {
                panic!("a known component");
            };
            // The notices show no figures of the book.
            if component.kind().shows_no_data() {
                return None;
            }
            let Some(Setting::Word(name)) = component.get("period") else {
                panic!("a period");
            };
            let shown = Period::for_component(name, page, today).unwrap().unwrap();
            Some((component, shown))
        })
    };
    let mut figures = 0;
    for (component, shown) in every_component() {
        let span = match component.get("compare") {
            Some(Setting::Word(name)) => {
                comparison_span(shown, Compare::from_id(name).unwrap()).unwrap()
            }
            _ => shown,
        };
        let source = Source::read(&sample.book, span).unwrap();
        match component.dataset().id {
            SPENDING_BY_CATEGORY => {
                // What the Overview's donut and table are made from.
                let by_category =
                    spending_by_category(&source, shown, &Filter::default(), Level::TopLevel)
                        .unwrap();
                assert!(!by_category.is_empty());
                figures += 1;
            }
            INCOME_EXPENSE_BY_MONTH => {
                let months = income_expense_by_month(&source, shown, &Filter::default()).unwrap();
                let first = months.first().expect("euros");
                if component.kind() == ComponentType::ChartLine {
                    // The line covers the twelve months, whatever the page shows.
                    assert_eq!(first.rows.len(), 12);
                    assert_eq!(first.rows[0].month, date(2025, 11, 1));
                } else {
                    // The cards: October against September, as the page shows them.
                    let (earlier, changes) = compare_income_expense(
                        &source,
                        shown,
                        &Filter::default(),
                        Compare::PreviousPeriod,
                    )
                    .unwrap();
                    assert_eq!(earlier, period(date(2026, 9, 1), date(2026, 9, 30)));
                    assert_eq!(changes[0].income.current, money(250_700));
                }
                figures += 1;
            }
            other => panic!("the Overview does not use {other}"),
        }
    }
    assert_eq!(figures, 7);
}

#[test]
fn a_session_is_saved_in_the_book_and_read_back_to_the_same_layout() {
    let mut book = Book::create_in_memory().unwrap();
    // Until the user changes it, nothing is stored: the layout is built in.
    assert!(book.views(Some(ViewKind::Dashboard)).unwrap().is_empty());
    let mut editor = Editor::new(Document::default_overview().unwrap()).unwrap();
    editor.move_to("trend", 0, 0).unwrap();
    editor
        .add(Component::new(ComponentType::Heading, "").unwrap())
        .unwrap();
    // The first save adds the dashboard ...
    let first = editor.document().to_json();
    let id = book.add_view(&first).unwrap();
    let stored = book.view(id).unwrap();
    assert_eq!(
        (stored.kind, stored.name.as_str()),
        (ViewKind::Dashboard, "Overview")
    );
    assert_eq!(
        Document::read(&stored.document).unwrap(),
        *editor.document()
    );
    // ... a later one edits it, and a session that changed nothing records nothing.
    book.edit_view(id, &first).unwrap();
    editor.resize_to("trend", 6, 3).unwrap();
    editor.nudge("spending", 0, 1).unwrap();
    let second = editor.document().to_json();
    book.edit_view(id, &second).unwrap();
    let reread = Document::read(&book.view(id).unwrap().document).unwrap();
    assert_eq!(reread, *editor.document());
    assert_eq!(reread.entries.len(), 10);
    // A new session on the saved layout starts from it, and undo works in it.
    let mut again = Editor::new(reread).unwrap();
    assert!(!again.has_changes());
    again.delete("trend").unwrap();
    assert!(again.undo().is_some());
    assert_eq!(again.document().to_json(), second);
}

#[test]
fn the_book_keeps_a_broken_layout_and_the_reader_refuses_it() {
    // The book checks only the header (like a document from a copied book
    // or a file), so the frames are the reader's job (T-37).
    let mut book = Book::create_in_memory().unwrap();
    let overlapping = dashboard(vec![card("c1", 0, 0), card("c2", 1, 0)]);
    let id = book.add_view(&overlapping).unwrap();
    assert_eq!(
        reason(&book.view(id).unwrap().document),
        ("frames-overlap", "components.1.frame".to_owned())
    );
}

#[test]
fn the_separator_and_the_two_notices_show_no_data_and_survive_a_save() {
    for (id, kind) in [
        ("text.separator", ComponentType::Separator),
        ("notice.fetch_gap", ComponentType::FetchNotice),
        ("notice.transfers", ComponentType::TransfersNote),
    ] {
        assert_eq!(ComponentType::from_id(id), Some(kind));
        assert!(ComponentType::LIBRARY.contains(&kind));
        assert!(!ComponentType::ALL.contains(&kind));
        assert!(kind.shows_no_data());
        assert_eq!(kind.datasets().count(), 0);
        assert!(kind.properties().is_empty());
        assert!(Component::new(kind, "spending_by_category").is_err());
        let component = Component::new(kind, "").unwrap();
        // Its document has a type and props, and no dataset or params.
        let value = component.to_value();
        assert_eq!(value["type"], id);
        assert!(value.get("dataset").is_none() && value.get("params").is_none());
        assert_eq!(Component::from_json(&value.to_string()).unwrap(), component);
        assert!(Component::from_json(&format!(r#"{{"type":"{id}","dataset":"x"}}"#)).is_err());
        let size = kind.default_size();
        assert_eq!(size.rows, 1);
        assert!(kind.smallest_size().cols <= size.cols);
    }
    // The period box shows no data either, and has a title and a period of
    // its own, which is never `page` (D-333).
    let boxed = Component::new(ComponentType::PeriodBox, "").unwrap();
    assert!(ComponentType::PeriodBox.shows_no_data());
    assert_eq!(
        boxed.get("period"),
        Some(&Setting::Word("this-month".into()))
    );
    assert!(
        boxed
            .clone()
            .set("period", Setting::Word("page".into()))
            .is_err()
    );
    assert!(
        boxed
            .clone()
            .set("period", Setting::Word("last-year".into()))
            .is_ok()
    );
    assert_eq!(
        Component::from_json(&boxed.to_value().to_string()).unwrap(),
        boxed
    );
    // Placed on a grid next to the others, they pass the same checks.
    let mut editor = Editor::new(Document::default_overview().unwrap()).unwrap();
    editor
        .add(Component::new(ComponentType::Separator, "").unwrap())
        .unwrap();
    grid::check(&editor.document().entries).unwrap();
}
