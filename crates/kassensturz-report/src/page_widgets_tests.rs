// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tabs, the period header and the filter bar as widgets (M6d.8, D-452,
//! D-474 to D-476; the tabs D-529, D-530): any number of each per dashboard
//! (D-532), older dashboards get what they lack on top, and the page's
//! filters apply only while a bar is on it.

use serde_json::{Value, json};

use crate::*;

fn widget(id: &str, kind: &str, row: u32) -> Value {
    json!({"id": id, "type": kind, "frame": {"col": 0, "row": row, "cols": 12, "rows": 1}})
}

fn pie(id: &str, row: u32) -> Value {
    json!({"id": id, "type": "chart.pie", "dataset": "spending_by_category",
        "frame": {"col": 0, "row": row, "cols": 6, "rows": 4}})
}

fn dashboard(version: u32, filters: &Value, components: Vec<Value>) -> String {
    json!({"format": FORMAT, "version": version, "kind": "dashboard", "name": "Mine",
        "filters": filters, "components": components})
    .to_string()
}

fn frame_of(document: &Document, id: &str) -> Frame {
    document
        .entries
        .iter()
        .find(|entry| entry.id == id)
        .and_then(|entry| entry.frame)
        .unwrap_or_else(|| panic!("{id} has a frame"))
}

fn kind_of(entry: &Entry) -> Option<ComponentType> {
    match &entry.item {
        Item::Component(component) => Some(component.kind()),
        Item::Unknown(_) => None,
    }
}

#[test]
fn the_two_widgets_show_no_data_have_no_settings_and_are_in_the_library() {
    for (id, kind) in [
        ("page.period", ComponentType::PeriodHeader),
        ("page.filters", ComponentType::FilterBar),
        ("page.tabs", ComponentType::DashboardTabs),
    ] {
        assert_eq!(ComponentType::from_id(id), Some(kind));
        assert_eq!(kind.id(), id);
        assert!(ComponentType::LIBRARY.contains(&kind));
        // The AI is not offered them (D-282): they show no figures.
        assert!(!ComponentType::ALL.contains(&kind));
        assert!(kind.shows_no_data());
        assert!(kind.properties().is_empty());
        let component = Component::new(kind, "").unwrap();
        assert_eq!(takes(&component), Takes::NOTHING);
        assert_eq!(component.to_value(), json!({"type": id, "props": {}}));
        assert_eq!(
            Component::from_value(&component.to_value()).unwrap(),
            component
        );
        // The full width, one row high; narrower is fine, down to the smallest.
        assert_eq!(kind.default_size(), grid::Size { cols: 12, rows: 1 });
        assert_eq!(kind.smallest_size().rows, 1);
        assert!(kind.smallest_size().cols <= 4);
    }
}

#[test]
fn a_dashboard_may_hold_as_many_of_each_as_the_user_wants() {
    // Read: a dashboard with two of each is fine (D-532; before it, a second
    // one was refused with `only-one`).
    let twice = dashboard(
        VERSION,
        &json!({}),
        vec![
            widget("a", "page.filters", 0),
            widget("b", "page.period", 1),
            widget("c", "page.filters", 2),
            widget("d", "page.tabs", 3),
            widget("e", "page.tabs", 4),
            widget("f", "page.period", 5),
        ],
    );
    let document = Document::read(&twice).unwrap();
    assert_eq!(document.entries.len(), 6);
    assert_eq!(Document::read(&document.to_json()).unwrap(), document);

    // Arranging: each can be added again and copied, whatever the page
    // already has, and each copy is a widget of its own.
    let page = Document::read(&dashboard(
        VERSION,
        &json!({}),
        vec![widget("bar", "page.filters", 0), pie("p", 1)],
    ))
    .unwrap();
    let mut editor = Editor::new(page).unwrap();
    for kind in [
        ComponentType::FilterBar,
        ComponentType::PeriodHeader,
        ComponentType::DashboardTabs,
    ] {
        let first = editor.add(Component::new(kind, "").unwrap()).unwrap();
        let second = editor.add(Component::new(kind, "").unwrap()).unwrap();
        assert_ne!(first, second);
        assert_eq!(frame_of(editor.document(), &second).rows, 1);
    }
    let copy = editor.duplicate("bar").unwrap();
    assert_eq!(
        kind_of(
            editor
                .document()
                .entries
                .iter()
                .find(|entry| entry.id == copy)
                .unwrap()
        ),
        Some(ComponentType::FilterBar)
    );
    // Each can be moved and made narrower like any widget.
    assert!(editor.resize_to(&copy, 6, 1).unwrap());
    assert!(editor.move_to(&copy, 6, 0).is_ok());
    let saved = Document::read(&editor.document().to_json()).unwrap();
    assert_eq!(saved, *editor.document());
    // Taking all of one kind off leaves the page without it, nothing else.
    assert!(editor.delete("bar").unwrap());
    assert!(editor.delete(&copy).unwrap());
    assert!(editor.document().holds(ComponentType::FilterBar));
}

fn full_width(row: u32) -> Frame {
    Frame {
        col: 0,
        row,
        cols: 12,
        rows: 1,
    }
}

#[test]
fn an_older_dashboard_gets_the_tabs_the_header_and_the_bar_on_top() {
    for version in [1, 2] {
        let old = dashboard(
            version,
            &json!({"categories": ["c-1"]}),
            vec![pie("chart", 0), pie("other", 4)],
        );
        let document = Document::read(&old).unwrap();
        let kinds: Vec<_> = document.entries.iter().map(kind_of).collect();
        assert_eq!(
            kinds,
            [
                Some(ComponentType::DashboardTabs),
                Some(ComponentType::PeriodHeader),
                Some(ComponentType::FilterBar),
                Some(ComponentType::ChartPie),
                Some(ComponentType::ChartPie)
            ]
        );
        assert_eq!(frame_of(&document, DASHBOARD_TABS_ID), full_width(0));
        assert_eq!(frame_of(&document, PERIOD_HEADER_ID), full_width(1));
        assert_eq!(frame_of(&document, FILTER_BAR_ID), full_width(2));
        // Everything else moved down three rows, and nothing else changed.
        assert_eq!(frame_of(&document, "chart").row, 3);
        assert_eq!(frame_of(&document, "other").row, 7);
        assert_eq!(document.categories, ["c-1"]);
        // Written as the current version, it reads back the same, with no
        // second set.
        let written = document.to_json();
        assert!(written.contains(&format!("\"version\":{VERSION}")));
        assert_eq!(Document::read(&written).unwrap(), document);
    }

    // A dashboard of version 3 has the header and the bar (or not, if the
    // user took them off) and gets only the tabs, on row 0 (D-530).
    let with_both = Document::read(&dashboard(
        3,
        &json!({}),
        vec![
            widget(PERIOD_HEADER_ID, "page.period", 0),
            widget(FILTER_BAR_ID, "page.filters", 1),
            pie("chart", 2),
        ],
    ))
    .unwrap();
    let kinds: Vec<_> = with_both.entries.iter().map(kind_of).collect();
    assert_eq!(
        kinds,
        [
            Some(ComponentType::DashboardTabs),
            Some(ComponentType::PeriodHeader),
            Some(ComponentType::FilterBar),
            Some(ComponentType::ChartPie)
        ]
    );
    assert_eq!(frame_of(&with_both, DASHBOARD_TABS_ID), full_width(0));
    assert_eq!(frame_of(&with_both, PERIOD_HEADER_ID).row, 1);
    assert_eq!(frame_of(&with_both, FILTER_BAR_ID).row, 2);
    assert_eq!(frame_of(&with_both, "chart").row, 3);

    let no_bar = Document::read(&dashboard(
        3,
        &json!({}),
        vec![widget(PERIOD_HEADER_ID, "page.period", 0), pie("chart", 1)],
    ))
    .unwrap();
    assert!(no_bar.holds(ComponentType::DashboardTabs));
    assert!(no_bar.holds(ComponentType::PeriodHeader));
    assert!(!no_bar.holds(ComponentType::FilterBar));
    assert_eq!(frame_of(&no_bar, "chart").row, 2);

    // A current dashboard without them stays without them: the user took
    // them off.
    let bare = Document::read(&dashboard(VERSION, &json!({}), vec![pie("chart", 0)])).unwrap();
    assert_eq!(bare.entries.len(), 1);
    assert_eq!(frame_of(&bare, "chart").row, 0);

    // A component that already uses an ID keeps it; the widget takes the next.
    let taken = Document::read(&dashboard(2, &json!({}), vec![pie(PERIOD_HEADER_ID, 0)])).unwrap();
    assert_eq!(
        kind_of(&taken.entries[1]),
        Some(ComponentType::PeriodHeader)
    );
    assert_eq!(taken.entries[1].id, format!("{PERIOD_HEADER_ID}-2"));
    assert_eq!(kind_of(&taken.entries[3]), Some(ComponentType::ChartPie));
    assert_eq!(taken.entries[3].id, PERIOD_HEADER_ID);
    let taken = Document::read(&dashboard(3, &json!({}), vec![pie(DASHBOARD_TABS_ID, 0)])).unwrap();
    assert_eq!(taken.entries[0].id, format!("{DASHBOARD_TABS_ID}-2"));
    assert_eq!(taken.entries[1].id, DASHBOARD_TABS_ID);

    // A full dashboard has no room for more and stays as it was.
    let full: Vec<Value> = (0..MAX_COMPONENTS)
        .map(|n| {
            let row = u32::try_from(n).unwrap();
            json!({"id": format!("h{n}"), "type": "text.heading",
                "frame": {"col": 0, "row": row, "cols": 4, "rows": 1}})
        })
        .collect();
    for version in [2, 3] {
        let full = Document::read(&dashboard(version, &json!({}), full.clone())).unwrap();
        assert_eq!(full.entries.len(), MAX_COMPONENTS);
        assert!(!full.holds(ComponentType::DashboardTabs));
        assert_eq!(frame_of(&full, "h0").row, 0);
    }

    // So does one with a component on the grid's last row: moving it down
    // would make the dashboard unreadable.
    let low = Document::read(&dashboard(3, &json!({}), vec![pie("low", grid::LAST_ROW)])).unwrap();
    assert!(!low.holds(ComponentType::DashboardTabs));
    assert_eq!(frame_of(&low, "low").row, grid::LAST_ROW);

    // A view is one component and gets nothing.
    let view = json!({"format": FORMAT, "version": 1, "kind": "view", "name": "V",
        "components": [{"id": "c1", "type": "chart.pie", "dataset": "spending_by_category"}]})
    .to_string();
    assert_eq!(Document::read(&view).unwrap().entries.len(), 1);
}

#[test]
fn the_built_in_overview_of_an_earlier_build_reads_as_todays() {
    // The built-in Overview as an earlier build wrote it into a book: the
    // first without tabs, header and bar (version 1, everything three rows
    // higher), the second with header and bar but no tabs (version 3,
    // everything one row higher).
    for (version, removed, up) in [
        (1, &["page.tabs", "page.period", "page.filters"][..], 3),
        (3, &["page.tabs"][..], 1),
    ] {
        let mut old: Value = serde_json::from_str(DEFAULT_OVERVIEW).unwrap();
        old["version"] = version.into();
        let components = old["components"].as_array_mut().unwrap();
        components.retain(|component| !removed.contains(&component["type"].as_str().unwrap()));
        assert_eq!(components.len(), 15 - removed.len());
        for component in components.iter_mut() {
            let row = component["frame"]["row"].as_u64().unwrap();
            component["frame"]["row"] = (row - up).into();
        }
        let read = Document::read(&old.to_string()).unwrap();
        assert_eq!(
            read,
            Document::default_overview().unwrap(),
            "version {version}"
        );
    }
}

#[test]
fn the_page_filters_apply_only_while_the_bar_is_on_the_page() {
    let filters = json!({"period": "last-year", "categories": ["c-1"]});
    let with_bar = Document::read(&dashboard(
        VERSION,
        &filters,
        vec![widget("bar", "page.filters", 0), pie("chart", 1)],
    ))
    .unwrap();
    let asked = with_bar.asked("chart", &with_bar.filters()).unwrap();
    assert_eq!(asked.period, "last-year");
    assert_eq!(asked.period_origin, Origin::FilterBar);
    assert_eq!(
        asked.categories.map(|chosen| chosen.ids),
        Some(vec!["c-1".to_owned()])
    );

    // Taken off, the page is not narrowed, neither by what was saved nor by
    // a choice for now; the saved filters stay in the document.
    let mut editor = Editor::new(with_bar).unwrap();
    assert!(editor.delete("bar").unwrap());
    let without = editor.document().clone();
    assert_eq!(without.categories, ["c-1"]);
    let for_now = Filters {
        accounts: vec!["a-1".into()],
        ..Filters::default()
    };
    for bar in [without.filters(), for_now] {
        let asked = without.asked("chart", &bar).unwrap();
        assert_eq!(asked.period, "page");
        assert_eq!(asked.period_origin, Origin::Default);
        assert_eq!(asked.accounts, None);
        assert_eq!(asked.categories, None);
    }

    // Added back, the saved filters apply again.
    editor
        .add(Component::new(ComponentType::FilterBar, "").unwrap())
        .unwrap();
    let asked = editor.document().asked("chart", &editor.filters()).unwrap();
    assert_eq!(asked.period, "last-year");

    // A saved view has no bar and keeps its own period (D-326).
    let view = Document::view(
        "V",
        Some("last-month"),
        Component::new(ComponentType::ChartPie, "spending_by_category").unwrap(),
    );
    let asked = view.asked("c1", &view.filters()).unwrap();
    assert_eq!(asked.period, "last-month");
}
