// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The period header and the filter bar as widgets (M6d.8, D-452, D-474 to
//! D-476): one of each per dashboard, older dashboards get them on top, and
//! the page's filters apply only while the bar is on it.

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
    ] {
        assert_eq!(ComponentType::from_id(id), Some(kind));
        assert_eq!(kind.id(), id);
        assert!(ComponentType::LIBRARY.contains(&kind));
        // The AI is not offered them (D-282): they show no figures.
        assert!(!ComponentType::ALL.contains(&kind));
        assert!(kind.shows_no_data());
        assert!(kind.one_per_page());
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
    let only_page_wide: Vec<_> = ComponentType::LIBRARY
        .into_iter()
        .filter(|kind| kind.one_per_page())
        .collect();
    assert_eq!(
        only_page_wide,
        [ComponentType::PeriodHeader, ComponentType::FilterBar]
    );
}

#[test]
fn a_dashboard_holds_at_most_one_of_each() {
    // Read: the second one is refused and named.
    let twice = dashboard(
        VERSION,
        &json!({}),
        vec![
            widget("a", "page.filters", 0),
            widget("b", "page.period", 1),
            widget("c", "page.filters", 2),
        ],
    );
    match Document::read(&twice) {
        Err(ReportError::DocumentInvalid { reason, path }) => {
            assert_eq!((reason, path.as_str()), ("only-one", "components.2.type"));
        }
        other => panic!("refused, not {other:?}"),
    }

    // Arranging: a second one is refused before anything changes, as is a
    // copy; after the first is removed, one can be added again.
    let page = Document::read(&dashboard(
        VERSION,
        &json!({}),
        vec![widget("bar", "page.filters", 0), pie("p", 1)],
    ))
    .unwrap();
    let mut editor = Editor::new(page).unwrap();
    let before = editor.document().clone();
    let refused = editor.add(Component::new(ComponentType::FilterBar, "").unwrap());
    let only_one = |result| {
        matches!(
            result,
            Err(ReportError::GridEditRefused { reason: "only-one" })
        )
    };
    assert!(only_one(refused));
    assert!(only_one(editor.duplicate("bar")));
    assert_eq!(*editor.document(), before);
    assert!(!editor.has_changes());

    // The other one is still welcome, at the first free place.
    let header = editor
        .add(Component::new(ComponentType::PeriodHeader, "").unwrap())
        .unwrap();
    assert_eq!(frame_of(editor.document(), &header).rows, 1);
    assert!(editor.delete("bar").unwrap());
    let again = editor
        .add(Component::new(ComponentType::FilterBar, "").unwrap())
        .unwrap();
    assert!(editor.document().holds(ComponentType::FilterBar));
    // Each can be moved and made narrower like any widget.
    assert!(editor.resize_to(&again, 6, 1).unwrap());
    assert!(editor.move_to(&again, 6, 0).is_ok());
    let saved = Document::read(&editor.document().to_json()).unwrap();
    assert_eq!(saved, *editor.document());
}

#[test]
fn an_older_dashboard_gets_the_header_and_the_bar_on_top() {
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
                Some(ComponentType::PeriodHeader),
                Some(ComponentType::FilterBar),
                Some(ComponentType::ChartPie),
                Some(ComponentType::ChartPie)
            ]
        );
        assert_eq!(
            frame_of(&document, PERIOD_HEADER_ID),
            Frame {
                col: 0,
                row: 0,
                cols: 12,
                rows: 1
            }
        );
        assert_eq!(
            frame_of(&document, FILTER_BAR_ID),
            Frame {
                col: 0,
                row: 1,
                cols: 12,
                rows: 1
            }
        );
        // Everything else moved down two rows, and nothing else changed.
        assert_eq!(frame_of(&document, "chart").row, 2);
        assert_eq!(frame_of(&document, "other").row, 6);
        assert_eq!(document.categories, ["c-1"]);
        // Written as the current version, it reads back the same, with no
        // second pair.
        let written = document.to_json();
        assert!(written.contains(&format!("\"version\":{VERSION}")));
        assert_eq!(Document::read(&written).unwrap(), document);
    }

    // A current dashboard without them stays without them: the user took
    // them off.
    let bare = Document::read(&dashboard(VERSION, &json!({}), vec![pie("chart", 0)])).unwrap();
    assert_eq!(bare.entries.len(), 1);
    assert_eq!(frame_of(&bare, "chart").row, 0);

    // A component that already uses the ID keeps it; the widget takes the next.
    let taken = Document::read(&dashboard(2, &json!({}), vec![pie(PERIOD_HEADER_ID, 0)])).unwrap();
    assert_eq!(
        kind_of(&taken.entries[0]),
        Some(ComponentType::PeriodHeader)
    );
    assert_eq!(taken.entries[0].id, format!("{PERIOD_HEADER_ID}-2"));
    assert_eq!(kind_of(&taken.entries[2]), Some(ComponentType::ChartPie));
    assert_eq!(taken.entries[2].id, PERIOD_HEADER_ID);

    // A full dashboard has no room for two more and stays as it was.
    let full: Vec<Value> = (0..MAX_COMPONENTS)
        .map(|n| {
            let row = u32::try_from(n).unwrap();
            json!({"id": format!("h{n}"), "type": "text.heading",
                "frame": {"col": 0, "row": row, "cols": 4, "rows": 1}})
        })
        .collect();
    let full = Document::read(&dashboard(2, &json!({}), full)).unwrap();
    assert_eq!(full.entries.len(), MAX_COMPONENTS);
    assert!(!full.holds(ComponentType::FilterBar));

    // A view is one component and gets nothing.
    let view = json!({"format": FORMAT, "version": 1, "kind": "view", "name": "V",
        "components": [{"id": "c1", "type": "chart.pie", "dataset": "spending_by_category"}]})
    .to_string();
    assert_eq!(Document::read(&view).unwrap().entries.len(), 1);
}

#[test]
fn the_built_in_overview_of_an_earlier_build_reads_as_todays() {
    // The built-in Overview as an earlier build wrote it into a book: no
    // header and no bar, everything two rows higher, version 1.
    let mut old: Value = serde_json::from_str(DEFAULT_OVERVIEW).unwrap();
    old["version"] = 1.into();
    let components = old["components"].as_array_mut().unwrap();
    components.retain(|component| {
        !matches!(
            component["type"].as_str(),
            Some("page.period" | "page.filters")
        )
    });
    assert_eq!(components.len(), 12);
    for component in components.iter_mut() {
        let row = component["frame"]["row"].as_u64().unwrap();
        component["frame"]["row"] = (row - 2).into();
    }
    let read = Document::read(&old.to_string()).unwrap();
    assert_eq!(read, Document::default_overview().unwrap());
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
