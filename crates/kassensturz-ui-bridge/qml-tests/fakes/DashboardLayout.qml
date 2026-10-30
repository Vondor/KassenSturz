// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object DashboardLayout, for `tools/check-panel.sh`
// (M6b.3): the same `view` shape and calls, recording each call in `calls`.
// It does not arrange anything; the grid's own drawing and keys are what the
// smoke test looks at.

import QtQuick

QtObject {
    id: layout

    property var calls: []
    property var view: ({
        editing: false, damaged: false, hasChanges: false, canUndo: false, canRedo: false,
        undoEdit: "", redoEdit: "", builtIn: true, columns: 12, maxRows: 24, rows: 6,
        library: [],
        items: [
            item("income", "kpi.card", 0, 0, 3, 2),
            item("spending", "chart.donut", 3, 0, 6, 4),
            item("title", "text.heading", 0, 2, 3, 1),
            item("line", "text.separator", 0, 3, 12, 1),
            item("gaps", "notice.fetch_gap", 0, 4, 12, 1),
            item("moved", "notice.transfers", 0, 5, 12, 1),
            Object.assign(item("future", "", 9, 0, 3, 2), {unknown: true, subject: ({})})
        ]
    })

    function item(id, type, col, row, cols, rows) {
        const heading = type === "text.heading";
        return {
            id: id, unknown: false, type: type, col: col, row: row, cols: cols, rows: rows,
            minCols: 2, minRows: 1,
            subject: {
                type: type, dataset: heading ? "" : "spending_by_category", datasets: [],
                json: JSON.stringify({type: type, id: id}),
                fields: [{key: heading ? "text" : "title", place: "props", kind: "text", text: heading ? "Spending" : "", flag: false, word: "", words: []}],
                period: heading ? undefined : {word: "page", words: ["page", "this-month"]},
                preview: {shows: heading ? "heading" : type === "kpi.card" ? "card" : "chart", chart: "donut", grouping: "top-level", chosen: "", showHidden: false, compare: "previous_period", measure: "income"}
            }
        };
    }

    function note(call) {
        calls = calls.concat([call]);
        return ({});
    }

    function setEditing(editing) {
        view = Object.assign({}, view, {editing: editing});
    }

    function refresh() { }
    function edit() { setEditing(true); }
    function cancel() { setEditing(false); }
    function done() { setEditing(false); return ({}); }
    function add(kind) { note("add:" + kind); return ({id: "c9"}); }
    function moveTo(id, col, row) { return note("moveTo:" + id + ":" + col + ":" + row); }
    function nudge(id, cols, rows) { return note("nudge:" + id + ":" + cols + ":" + rows); }
    function resizeTo(id, cols, rows) { return note("resizeTo:" + id + ":" + cols + ":" + rows); }
    function grow(id, cols, rows) { return note("grow:" + id + ":" + cols + ":" + rows); }
    function duplicate(id) { note("duplicate:" + id); return ({id: "c9"}); }
    function remove(id) { return note("remove:" + id); }
    function moveEarlier(id) { return note("earlier:" + id); }
    function moveLater(id) { return note("later:" + id); }
    function setComponent(id, json) { return note("setComponent:" + id); }
    function undo() { return ""; }
    function redo() { return ""; }
    function reset() { return note("reset"); }
    function snap(offset, cell) { return cell <= 0 ? 0 : Math.floor((offset * 2 + cell) / (cell * 2)); }
    function narrow(width, gap, unit) { return width - gap * 11 < unit * 3 * 12; }
}
