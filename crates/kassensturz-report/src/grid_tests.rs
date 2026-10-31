// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use kassensturz_core::{Report, ViewKind};
use proptest::prelude::*;
use serde_json::json;

use crate::grid::{self, COLUMNS, LAST_ROW, Size};
use crate::*;

fn component(kind: ComponentType) -> Component {
    let dataset = kind.datasets().next().expect("every type shows a dataset");
    Component::new(kind, dataset.id).unwrap()
}

fn frame(col: u32, row: u32, cols: u32, rows: u32) -> Frame {
    Frame {
        col,
        row,
        cols,
        rows,
    }
}

/// A dashboard document as the book would keep it, read the real way.
fn dashboard(components: &[(&str, &str, Frame)]) -> Document {
    try_dashboard(components).unwrap()
}

fn try_dashboard(components: &[(&str, &str, Frame)]) -> Result<Document, ReportError> {
    let components: Vec<_> = components
        .iter()
        .map(|(id, kind, frame)| {
            let kind = ComponentType::from_id(kind).unwrap();
            let mut value = json!({
                "id": id,
                "type": kind.id(),
                "frame": {"col": frame.col, "row": frame.row, "cols": frame.cols, "rows": frame.rows},
            });
            if !kind.shows_no_data() {
                value["dataset"] = kind.datasets().next().unwrap().id.into();
            }
            value
        })
        .collect();
    let text = json!({
        "format": FORMAT, "version": VERSION, "kind": "dashboard", "name": "Overview",
        "components": components,
    })
    .to_string();
    Document::read(&text)
}

fn editor(components: &[(&str, &str, Frame)]) -> Editor {
    Editor::new(dashboard(components)).unwrap()
}

fn frame_of(editor: &Editor, id: &str) -> Frame {
    editor
        .document()
        .entries
        .iter()
        .find(|entry| entry.id == id)
        .and_then(|entry| entry.frame)
        .unwrap()
}

fn invalid(entries: &[Entry]) -> (&'static str, String) {
    match grid::check(entries).unwrap_err() {
        ReportError::DocumentInvalid { reason, path } => (reason, path),
        other => panic!("not a document error: {other:?}"),
    }
}

fn refusal(error: ReportError) -> &'static str {
    match error {
        ReportError::GridEditRefused { reason } => reason,
        other => panic!("not a refusal: {other:?}"),
    }
}

#[test]
fn every_type_has_the_smallest_size_of_the_design_and_a_default_that_is_not_smaller() {
    for kind in ComponentType::ALL {
        let least = kind.smallest_size();
        let expected = match kind {
            ComponentType::KpiCard => Size { cols: 2, rows: 2 },
            ComponentType::ProgressBar => Size { cols: 3, rows: 1 },
            ComponentType::TableRows => Size { cols: 4, rows: 3 },
            _ => Size { cols: 3, rows: 3 },
        };
        assert_eq!(least, expected, "{}", kind.id());
        let default = kind.default_size();
        assert!(default.cols >= least.cols && default.rows >= least.rows);
        assert!(default.cols <= COLUMNS && default.rows <= grid::MAX_ROWS);
    }
}

#[test]
fn the_frame_checks_name_the_component_and_why() {
    let good = dashboard(&[
        ("a", "chart.line", frame(0, 0, 6, 4)),
        ("b", "kpi.card", frame(6, 0, 2, 2)),
    ]);
    assert!(grid::check(&good.entries).is_ok());
    let with = |index: usize, frame: Option<Frame>| {
        let mut entries = good.entries.clone();
        entries[index].frame = frame;
        entries
    };
    assert_eq!(
        invalid(&with(1, None)),
        ("bad-frame", "components.1.frame".into())
    );
    for outside in [
        frame(7, 0, 6, 4),
        frame(0, 0, 13, 4),
        frame(0, 0, 6, 25),
        frame(0, LAST_ROW + 1, 6, 4),
    ] {
        assert_eq!(
            invalid(&with(0, Some(outside))),
            ("frame-outside-grid", "components.0.frame".into()),
            "{outside:?}"
        );
    }
    assert_eq!(
        invalid(&with(0, Some(frame(0, 0, 2, 4)))),
        ("frame-too-small", "components.0.frame".into())
    );
    assert_eq!(
        invalid(&with(1, Some(frame(5, 3, 2, 2)))),
        ("frames-overlap", "components.1.frame".into())
    );
    // Touching edges share no cell.
    assert!(grid::check(&with(1, Some(frame(0, 4, 2, 2)))).is_ok());
    let error = grid::check(&with(1, Some(frame(5, 3, 2, 2)))).unwrap_err();
    assert_eq!(error.report().code, "report.document-invalid");
}

#[test]
fn a_type_this_version_does_not_know_may_be_one_cell() {
    let text = json!({
        "format": FORMAT, "version": VERSION, "kind": "dashboard", "name": "Overview",
        "components": [{"id": "x", "type": "map.world", "frame": {"col": 0, "row": 0, "cols": 1, "rows": 1}}],
    })
    .to_string();
    let mut editor = Editor::new(Document::read(&text).unwrap()).unwrap();
    assert_eq!(
        refusal(editor.change("x", |_| Ok(())).unwrap_err()),
        "not-editable"
    );
    assert!(editor.nudge("x", 1, 0).unwrap());
    let copy = editor.duplicate("x").unwrap();
    assert!(matches!(
        editor.document().entries[1].item,
        Item::Unknown(_)
    ));
    assert_eq!(frame_of(&editor, &copy), frame(2, 0, 1, 1));
}

#[test]
fn reading_order_is_by_row_then_column() {
    let editor = editor(&[
        ("low", "chart.bar", frame(0, 4, 6, 4)),
        ("right", "kpi.card", frame(9, 0, 3, 2)),
        ("left", "kpi.card", frame(0, 0, 3, 2)),
        ("middle", "kpi.card", frame(3, 1, 3, 2)),
    ]);
    assert_eq!(editor.reading_order(), ["left", "right", "middle", "low"]);
}

#[test]
fn a_drag_snaps_to_the_nearest_cell_and_a_narrow_window_is_found_exactly() {
    assert_eq!(grid::snap(0, 80), 0);
    assert_eq!(grid::snap(39, 80), 0);
    assert_eq!(grid::snap(40, 80), 1);
    assert_eq!(grid::snap(119, 80), 1);
    assert_eq!(grid::snap(-41, 80), -1);
    assert_eq!(grid::snap(-40, 80), 0);
    assert_eq!(grid::snap(500, 0), 0);
    // A grid unit of 18 pixels: a column must be at least 54 pixels.
    assert!(!grid::is_narrow(12 * 54 + 11 * 8, 8, 18));
    assert!(grid::is_narrow(12 * 54 + 11 * 8 - 1, 8, 18));
    assert!(grid::is_narrow(0, 8, 18));
}

#[test]
fn a_component_that_lands_on_others_pushes_them_down_in_reading_order() {
    let mut editor = editor(&[
        ("a", "chart.line", frame(0, 0, 6, 4)),
        ("b", "chart.bar", frame(6, 0, 6, 4)),
        ("c", "chart.area", frame(6, 4, 6, 4)),
        ("d", "kpi.card", frame(0, 6, 3, 2)),
    ]);
    // "a" lands on "b", which lands on "c"; "d" is hit by nothing.
    assert!(editor.move_to("a", 5, 1).unwrap());
    assert_eq!(frame_of(&editor, "a"), frame(5, 1, 6, 4));
    assert_eq!(frame_of(&editor, "b"), frame(6, 5, 6, 4));
    assert_eq!(frame_of(&editor, "c"), frame(6, 9, 6, 4));
    assert_eq!(frame_of(&editor, "d"), frame(0, 6, 3, 2));
    // The gap "a" left in rows 0 and 1 stays: nothing floats up.
    assert_eq!(editor.reading_order(), ["a", "b", "d", "c"]);
}

#[test]
fn a_move_is_clamped_to_the_grid_and_a_move_to_the_same_place_is_no_edit() {
    let mut editor = editor(&[("a", "chart.line", frame(0, 0, 6, 4))]);
    assert!(editor.move_to("a", 40, -3).unwrap());
    assert_eq!(frame_of(&editor, "a"), frame(6, 0, 6, 4));
    assert!(!editor.move_to("a", 6, 0).unwrap());
    assert!(!editor.nudge("a", 1, 0).unwrap());
    assert!(editor.nudge("a", -1, 2).unwrap());
    assert_eq!(frame_of(&editor, "a"), frame(5, 2, 6, 4));
    assert_eq!(editor.next_undo(), Some(Edit::Move));
    editor.undo();
    editor.undo();
    assert_eq!(editor.next_undo(), None);
    assert!(!editor.has_changes());
}

#[test]
fn a_resize_stops_at_the_smallest_size_and_the_grid_and_pushes_down() {
    let mut editor = editor(&[
        ("a", "table.rows", frame(0, 0, 6, 4)),
        ("b", "kpi.card", frame(0, 4, 3, 2)),
    ]);
    assert!(editor.resize_to("a", 1, 1).unwrap());
    assert_eq!(frame_of(&editor, "a"), frame(0, 0, 4, 3));
    assert!(editor.resize_to("a", 99, 99).unwrap());
    assert_eq!(frame_of(&editor, "a"), frame(0, 0, 12, 24));
    assert_eq!(frame_of(&editor, "b"), frame(0, 24, 3, 2));
    assert!(editor.grow("a", -8, -20).unwrap());
    assert_eq!(frame_of(&editor, "a"), frame(0, 0, 4, 4));
    // "b" was pushed and stays down: nothing moves up by itself.
    assert_eq!(frame_of(&editor, "b"), frame(0, 24, 3, 2));
    assert_eq!(editor.next_undo(), Some(Edit::Resize));
}

#[test]
fn add_takes_the_first_free_place_in_reading_order_or_the_row_below_everything() {
    let mut editor = editor(&[
        ("c1", "chart.line", frame(0, 0, 6, 4)),
        ("c3", "kpi.card", frame(9, 0, 3, 2)),
    ]);
    let card = editor.add(component(ComponentType::KpiCard)).unwrap();
    assert_eq!(card, "c2");
    assert_eq!(frame_of(&editor, "c2"), frame(6, 0, 3, 2));
    let chart = editor.add(component(ComponentType::ChartPie)).unwrap();
    assert_eq!(chart, "c4");
    assert_eq!(frame_of(&editor, "c4"), frame(6, 2, 6, 4));
    let table = editor.add(component(ComponentType::TableRows)).unwrap();
    assert_eq!(frame_of(&editor, &table), frame(0, 4, 6, 4));
    assert_eq!(editor.document().entries.len(), 5);
    assert_eq!(editor.next_undo(), Some(Edit::Add));
}

#[test]
fn a_period_box_lies_under_widgets_but_boxes_and_widgets_never_overlap_their_own_kind() {
    // A box over two widgets is a valid layout.
    let mut editor = editor(&[
        ("box", "group.period", frame(0, 0, 12, 5)),
        ("a", "chart.pie", frame(0, 0, 6, 4)),
        ("b", "kpi.card", frame(6, 0, 3, 2)),
    ]);
    assert_eq!(frame_of(&editor, "box"), frame(0, 0, 12, 5));
    // Two widgets still don't overlap, nor two boxes.
    assert!(
        try_dashboard(&[
            ("a", "chart.pie", frame(0, 0, 6, 4)),
            ("b", "chart.bar", frame(5, 0, 6, 4)),
        ])
        .is_err()
    );
    assert!(
        try_dashboard(&[
            ("x", "group.period", frame(0, 0, 6, 4)),
            ("y", "group.period", frame(5, 2, 6, 4)),
        ])
        .is_err()
    );
    // A widget added to a box's area takes the first free place among
    // widgets, so it can land inside the box; a new box keeps clear of widgets too, since
    // under their cards it could not be seen (M6-B12).
    let card = editor.add(component(ComponentType::KpiCard)).unwrap();
    assert_eq!(frame_of(&editor, &card), frame(9, 0, 3, 2));
    let second = editor
        .add(Component::new(ComponentType::PeriodBox, "").unwrap())
        .unwrap();
    assert_eq!(frame_of(&editor, &second), frame(0, 5, 12, 5));
    // Moving a widget over a box pushes no one.
    editor.move_to("b", 0, 1).unwrap_or(false);
    assert_eq!(frame_of(&editor, "box"), frame(0, 0, 12, 5));
}

#[test]
fn moving_a_period_box_takes_the_widgets_inside_it_along() {
    let mut editor = editor(&[
        ("box", "group.period", frame(0, 1, 6, 4)),
        ("in", "kpi.card", frame(1, 2, 3, 2)),
        ("half", "kpi.card", frame(5, 2, 3, 2)),
        ("out", "kpi.card", frame(9, 1, 3, 2)),
    ]);
    assert!(editor.move_to("box", 3, 4).unwrap());
    assert_eq!(frame_of(&editor, "box"), frame(3, 4, 6, 4));
    // Inside: shifted by the same cells; the one that only touched the box
    // and the one outside stay.
    assert_eq!(frame_of(&editor, "in"), frame(4, 5, 3, 2));
    assert_eq!(frame_of(&editor, "half"), frame(5, 2, 3, 2));
    assert_eq!(frame_of(&editor, "out"), frame(9, 1, 3, 2));
    assert_eq!(editor.next_undo(), Some(Edit::Move));
    // One undo takes the box and what moved with it back.
    assert_eq!(editor.undo(), Some(Edit::Move));
    assert_eq!(frame_of(&editor, "box"), frame(0, 1, 6, 4));
    assert_eq!(frame_of(&editor, "in"), frame(1, 2, 3, 2));
    // Moving a widget alone never moves a box.
    editor.move_to("in", 2, 2).unwrap();
    assert_eq!(frame_of(&editor, "box"), frame(0, 1, 6, 4));
}

#[test]
fn a_full_dashboard_takes_no_more_components() {
    let mut editor = editor(&[]);
    for _ in 0..MAX_COMPONENTS {
        editor.add(component(ComponentType::KpiCard)).unwrap();
    }
    let before = editor.document().clone();
    let error = editor.add(component(ComponentType::KpiCard)).unwrap_err();
    assert_eq!(refusal(error), "too-many-components");
    assert_eq!(
        refusal(editor.duplicate("c1").unwrap_err()),
        "too-many-components"
    );
    assert_eq!(editor.document(), &before);
    // The undo stack keeps the newest 200 edits; 100 adds all fit.
    let mut undone = 0;
    while editor.undo().is_some() {
        undone += 1;
    }
    assert_eq!(undone, MAX_COMPONENTS);
}

#[test]
fn duplicate_goes_right_of_the_original_if_it_fits_else_below_it() {
    let mut editor = editor(&[
        ("a", "chart.line", frame(0, 0, 4, 4)),
        ("b", "kpi.card", frame(8, 0, 3, 2)),
        ("c", "chart.bar", frame(0, 4, 6, 4)),
    ]);
    let right = editor.duplicate("a").unwrap();
    assert_eq!(frame_of(&editor, &right), frame(4, 0, 4, 4));
    // Right of the copy is "b", so the next copy goes below and pushes "c".
    let below = editor.duplicate(&right).unwrap();
    assert_eq!(frame_of(&editor, &below), frame(4, 4, 4, 4));
    assert_eq!(frame_of(&editor, "c"), frame(0, 8, 6, 4));
    let copied = editor
        .document()
        .entries
        .iter()
        .find(|entry| entry.id == below)
        .unwrap();
    assert_eq!(copied.item, editor.document().entries[0].item);
    assert_eq!(editor.next_undo(), Some(Edit::Duplicate));
}

#[test]
fn delete_moves_nothing_else() {
    let mut editor = editor(&[
        ("a", "chart.line", frame(0, 0, 6, 4)),
        ("b", "chart.bar", frame(0, 4, 6, 4)),
    ]);
    assert!(editor.delete("a").unwrap());
    assert_eq!(editor.reading_order(), ["b"]);
    assert_eq!(frame_of(&editor, "b"), frame(0, 4, 6, 4));
    assert_eq!(
        refusal(editor.delete("a").unwrap_err()),
        "unknown-component"
    );
    assert_eq!(editor.next_undo(), Some(Edit::Delete));
}

#[test]
fn a_change_of_type_grows_the_frame_to_the_new_smallest_size() {
    let mut editor = editor(&[
        ("card", "kpi.card", frame(10, 0, 2, 2)),
        ("below", "kpi.card", frame(8, 2, 2, 2)),
    ]);
    assert!(
        editor
            .change("card", |component| component
                .change_kind(ComponentType::TableRows))
            .unwrap()
    );
    // A table needs 4 × 3: it moves left only as far as it must.
    assert_eq!(frame_of(&editor, "card"), frame(8, 0, 4, 3));
    assert_eq!(frame_of(&editor, "below"), frame(8, 3, 2, 2));
    assert!(
        editor
            .change("card", |component| {
                component.set("title", Setting::Text("Largest".into()))
            })
            .unwrap()
    );
    assert_eq!(editor.next_undo(), Some(Edit::Change));
    // A refused change keeps everything as it was.
    let before = editor.document().clone();
    assert!(
        editor
            .change("card", |component| component
                .set("title", Setting::Flag(true)))
            .is_err()
    );
    assert_eq!(editor.document(), &before);
    assert_eq!(
        refusal(editor.change("gone", |_| Ok(())).unwrap_err()),
        "unknown-component"
    );
}

#[test]
fn move_earlier_and_later_change_places_in_reading_order() {
    let mut editor = editor(&[
        ("a", "chart.line", frame(0, 0, 6, 4)),
        ("b", "kpi.card", frame(6, 0, 3, 2)),
        ("c", "chart.bar", frame(0, 4, 12, 4)),
    ]);
    assert!(!editor.move_earlier("a").unwrap());
    assert!(!editor.move_later("c").unwrap());
    assert!(editor.move_earlier("b").unwrap());
    assert_eq!(editor.reading_order(), ["b", "a", "c"]);
    assert_eq!(frame_of(&editor, "b"), frame(0, 0, 3, 2));
    assert_eq!(frame_of(&editor, "a"), frame(6, 0, 6, 4));
    // The wide "c" changes places with "a" and stays inside the grid.
    assert!(editor.move_later("a").unwrap());
    assert_eq!(editor.reading_order(), ["b", "c", "a"]);
    assert_eq!(frame_of(&editor, "c"), frame(0, 2, 12, 4));
    assert_eq!(frame_of(&editor, "a"), frame(0, 6, 6, 4));
    assert_eq!(editor.next_undo(), Some(Edit::Move));
}

#[test]
fn reset_is_one_edit_that_undo_takes_back() {
    let built_in = dashboard(&[("c1", "chart.donut", frame(0, 0, 6, 4))]);
    let mut editor = editor(&[("a", "chart.line", frame(0, 0, 6, 4))]);
    let start = editor.document().clone();
    assert!(editor.reset(&built_in.entries).unwrap());
    assert_eq!(editor.document().entries, built_in.entries);
    assert!(!editor.reset(&built_in.entries).unwrap());
    assert_eq!(editor.undo(), Some(Edit::Reset));
    assert_eq!(editor.document(), &start);
    assert_eq!(editor.redo(), Some(Edit::Reset));
    assert_eq!(editor.document().entries, built_in.entries);
    let mut overlapping = built_in.entries.clone();
    overlapping.extend(built_in.entries.iter().cloned().map(|mut entry| {
        entry.id = "c2".into();
        entry
    }));
    assert!(editor.reset(&overlapping).is_err());
    let mut twice = built_in.entries.clone();
    twice.extend(built_in.entries.iter().cloned().map(|mut entry| {
        entry.frame = Some(frame(6, 0, 6, 4));
        entry
    }));
    assert!(matches!(
        editor.reset(&twice).unwrap_err(),
        ReportError::DocumentInvalid {
            reason: "duplicate-id",
            ..
        }
    ));
    assert_eq!(editor.document().entries, built_in.entries);
}

#[test]
fn undo_and_redo_walk_the_stack_and_a_new_edit_drops_the_redo() {
    let mut editor = editor(&[("a", "chart.line", frame(0, 0, 6, 4))]);
    assert_eq!(editor.undo(), None);
    assert_eq!(editor.redo(), None);
    editor.nudge("a", 0, 1).unwrap();
    editor.grow("a", 1, 0).unwrap();
    assert!(editor.has_changes());
    assert_eq!(editor.undo(), Some(Edit::Resize));
    assert_eq!(editor.next_redo(), Some(Edit::Resize));
    assert_eq!(frame_of(&editor, "a"), frame(0, 1, 6, 4));
    editor.nudge("a", 1, 0).unwrap();
    assert_eq!(editor.next_redo(), None);
    assert_eq!(editor.redo(), None);
    assert_eq!(editor.undo(), Some(Edit::Move));
    assert_eq!(editor.undo(), Some(Edit::Move));
    assert!(!editor.has_changes());
}

#[test]
fn the_stack_keeps_the_newest_two_hundred_edits() {
    let mut editor = editor(&[("a", "kpi.card", frame(0, 0, 2, 2))]);
    for step in 0..250 {
        assert!(editor.nudge("a", 0, 1).unwrap(), "{step}");
    }
    let mut undone = 0;
    while editor.undo().is_some() {
        undone += 1;
    }
    assert_eq!(undone, MAX_UNDO);
    // The 50 oldest moves can no longer be taken back.
    assert_eq!(frame_of(&editor, "a"), frame(0, 50, 2, 2));
}

#[test]
fn an_edit_that_would_push_a_component_below_the_last_row_is_refused() {
    let mut editor = editor(&[
        ("a", "chart.line", frame(0, 0, 6, 24)),
        ("b", "chart.bar", frame(0, LAST_ROW, 6, 4)),
    ]);
    let before = editor.document().clone();
    assert_eq!(
        refusal(
            editor
                .move_to("a", 0, i64::from(LAST_ROW - 10))
                .unwrap_err()
        ),
        "no-room"
    );
    assert_eq!(editor.document(), &before);
    assert_eq!(editor.next_undo(), None);
}

#[test]
fn only_a_dashboard_with_good_frames_is_edited() {
    let view = Document::view("One", None, component(ComponentType::ChartBar));
    let error = Editor::new(view).unwrap_err();
    assert_eq!(error.report().code, "report.grid-edit-refused");
    assert_eq!(refusal(error), "not-a-dashboard");
    let mut overlapping = dashboard(&[
        ("a", "chart.line", frame(0, 0, 6, 4)),
        ("b", "chart.bar", frame(6, 0, 6, 4)),
    ]);
    overlapping.entries[1].frame = Some(frame(3, 2, 6, 4));
    assert!(matches!(
        Editor::new(overlapping).unwrap_err(),
        ReportError::DocumentInvalid {
            reason: "frames-overlap",
            ..
        }
    ));
}

#[test]
fn an_edited_dashboard_is_written_and_read_back_as_it_was() {
    let mut editor = editor(&[
        ("a", "chart.line", frame(0, 0, 6, 4)),
        ("b", "kpi.card", frame(6, 0, 3, 2)),
    ]);
    editor.add(component(ComponentType::ProgressBar)).unwrap();
    editor.duplicate("a").unwrap();
    editor.move_to("b", 0, 1).unwrap();
    let document = editor.into_document();
    assert_eq!(document.kind, ViewKind::Dashboard);
    let read = Document::read(&document.to_json()).unwrap();
    assert_eq!(read, document);
    assert!(grid::check(&read.entries).is_ok());
}

/// One edit a test session makes: which, on which component (by reading
/// order), with two numbers and a type.
#[derive(Clone, Debug)]
struct Op {
    which: u8,
    target: usize,
    a: i64,
    b: i64,
    kind: usize,
}

fn op() -> impl Strategy<Value = Op> {
    (0u8..10, 0usize..100, -4i64..16, -4i64..30, 0usize..9).prop_map(
        |(which, target, a, b, kind)| Op {
            which,
            target,
            a,
            b,
            kind,
        },
    )
}

fn frames_by_id(editor: &Editor) -> Vec<(String, Frame)> {
    editor
        .document()
        .entries
        .iter()
        .map(|entry| (entry.id.clone(), entry.frame.unwrap()))
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// No edit, made or refused, leaves an overlap or a frame outside the
    /// grid; the others only ever move down; a refused edit changes nothing;
    /// and undo takes the session back to its start, redo to its end.
    #[test]
    fn no_edit_leaves_an_overlap_and_undo_and_redo_restore_every_state(
        ops in prop::collection::vec(op(), 1..60),
    ) {
        let mut editor = editor(&[]);
        let start = editor.document().clone();
        for op in &ops {
            let order: Vec<String> = editor.reading_order().iter().map(|id| (*id).to_owned()).collect();
            let id = if order.is_empty() { String::new() } else { order[op.target % order.len()].clone() };
            let kind = ComponentType::ALL[op.kind];
            let before = frames_by_id(&editor);
            let document = editor.document().clone();
            let undo = editor.next_undo();
            let at = order.iter().position(|other| *other == id);
            let result = match op.which {
                0 | 1 => editor.add(component(kind)).map(|_| true),
                2 => editor.move_to(&id, op.a, op.b),
                3 => editor.nudge(&id, op.a % 2, op.b % 2),
                4 => editor.resize_to(&id, op.a, op.b),
                5 => editor.grow(&id, op.a % 3, op.b % 3),
                6 => editor.duplicate(&id).map(|_| true),
                7 => editor.delete(&id),
                8 => editor.change(&id, |component| component.change_kind(kind)),
                _ => if op.a % 2 == 0 { editor.move_earlier(&id) } else { editor.move_later(&id) },
            };
            prop_assert!(grid::check(&editor.document().entries).is_ok());
            match result {
                Err(_) | Ok(false) => {
                    prop_assert_eq!(editor.document(), &document);
                    prop_assert_eq!(editor.next_undo(), undo);
                }
                Ok(true) => {
                    let after = frames_by_id(&editor);
                    // Only the edited component (and, for a change of
                    // place in reading order, its neighbour) may move other
                    // than down.
                    if (2..=8).contains(&op.which) {
                        for (other, old) in before.iter().filter(|(other, _)| *other != id) {
                            if let Some((_, new)) = after.iter().find(|(new, _)| new == other) {
                                prop_assert_eq!((new.col, new.cols, new.rows), (old.col, old.cols, old.rows));
                                prop_assert!(new.row >= old.row);
                            }
                        }
                    }
                    if op.which == 7 {
                        prop_assert_eq!(after.len() + 1, before.len());
                        for (other, new) in &after {
                            prop_assert!(before.contains(&(other.clone(), *new)));
                        }
                    }
                    if op.which == 9 && op.a % 2 == 0 {
                        let now = editor.reading_order().iter().position(|other| *other == id);
                        prop_assert_eq!(now, at.and_then(|at| at.checked_sub(1)));
                    }
                }
            }
        }
        let end = editor.document().clone();
        let mut undone = 0;
        while editor.undo().is_some() {
            prop_assert!(grid::check(&editor.document().entries).is_ok());
            undone += 1;
        }
        prop_assert!(undone <= ops.len());
        prop_assert_eq!(editor.document(), &start);
        prop_assert!(!editor.has_changes());
        while editor.redo().is_some() {}
        prop_assert_eq!(editor.document(), &end);
    }

    /// Every layout the push-down rule makes from frames that may overlap
    /// has none, keeps every frame's column and size, and moves none up.
    #[test]
    fn push_down_never_leaves_an_overlap(
        frames in prop::collection::vec((0u32..12, 0u32..40, 1u32..12, 1u32..10), 1..30),
        first in 0usize..30,
    ) {
        let mut frames: Vec<Frame> = frames
            .into_iter()
            .map(|(col, row, cols, rows)| Frame { col: col.min(COLUMNS - cols.min(COLUMNS)), row, cols, rows })
            .collect();
        let before = frames.clone();
        let first = [first % frames.len()];
        let boxes = vec![false; frames.len()];
        grid::push_down(&mut frames, &boxes, &first).unwrap();
        prop_assert_eq!(frames[first[0]], before[first[0]]);
        for (index, (new, old)) in frames.iter().zip(&before).enumerate() {
            prop_assert_eq!((new.col, new.cols, new.rows), (old.col, old.cols, old.rows));
            prop_assert!(new.row >= old.row);
            for other in &frames[..index] {
                prop_assert!(!other.overlaps(*new));
            }
        }
    }
}

#[test]
fn a_new_period_box_lands_where_no_widget_is() {
    let mut editor = editor(&[
        ("a", "chart.pie", frame(0, 0, 6, 4)),
        ("b", "chart.bar", frame(6, 0, 6, 4)),
    ]);
    let added = editor
        .add(Component::new(ComponentType::PeriodBox, "").unwrap())
        .unwrap();
    assert_eq!(frame_of(&editor, &added), frame(0, 4, 12, 5));
}
