// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object AiChat, for `tools/check-panel.sh`
// (M6a.9): one prepared set, with a new chart as the bridge writes it
// (`ChangeView::NewView` in a `ChangeRow`), and the calls the review page
// makes, among them the user's corrections (M6b.8, D-293), which the real
// object checks in Rust.

import QtQuick

QtObject {
    property var confirmed: []
    property var discarded: []
    property var edits: []
    readonly property var aiView: ({
        title: "Where the money goes",
        chart: "chart.pie",
        period: "this-year",
        dataset: "spending_by_category"
    })
    property var view: aiView
    property bool edited: false
    property bool leftOut: false

    function changes(setId) {
        return [{
            kind: "newView",
            slot: "new_views/0",
            edited: edited,
            leftOut: leftOut,
            fields: leftOut ? [] : ["title", "chart", "period"],
            form: leftOut ? "" : JSON.stringify(view),
            title: view.title,
            period: view.period,
            charts: ["chart.pie", "chart.donut", "chart.bar"],
            component: JSON.stringify({type: view.chart, dataset: "spending_by_category", params: {grouping: "category"}, props: {title: view.title}})
        }];
    }

    function changeChoices(setId) {
        return [{id: "1b0c", key: "", path: "Food", income: false, currency: "EUR"}];
    }

    function editChange(setId, slot, fields) {
        const changed = JSON.parse(fields);
        edits = edits.concat([fields]);
        if (changed.title === "") {
            return {code: "ai-changes.edit-text"};
        }
        view = Object.assign({}, view, changed);
        edited = true;
        return changes(setId);
    }

    function leaveOutChange(setId, slot) {
        leftOut = true;
        return changes(setId);
    }

    function backToAi(setId, slot) {
        view = aiView;
        edited = false;
        leftOut = false;
        return changes(setId);
    }

    function confirm(setId) {
        confirmed = confirmed.concat([setId]);
        return {ok: true};
    }

    function discard(setId) { discarded = discarded.concat([setId]); }
}
