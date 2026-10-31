// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object AiChat, for `tools/check-panel.sh`
// (M6a.9): one prepared set, with a new chart and a new budget as the bridge
// writes them (`ChangeView::NewView` and `ChangeView::NewBudget` in a
// `ChangeRow`, M6c.11), and the calls the review page
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
        }, {
            kind: "newBudget",
            slot: "new_budgets/0",
            edited: false,
            leftOut: false,
            fields: [],
            form: "",
            category: "Food › Groceries",
            every: "month",
            from: "01.11.2026",
            to: "30.11.2026",
            amount: "390,00 €",
            carryOver: true,
            basisPeriods: 6,
            basisFrom: "01.05.2026",
            basisTo: "31.10.2026",
            basisSpent: "2.324,40 €",
            basisAverage: "387,40 €",
            basisStep: "10,00 €"
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
