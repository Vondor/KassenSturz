// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object BookFormula, for `tools/check-panel.sh`
// (M6b.7): the shapes the real one gives (see formulas_model.rs) for a few
// fixed formulas.

import QtQuick

QtObject {
    property var view: ({})

    function show(period, formula, typed) {
        if (formula.trim() === "") {
            view = {empty: true};
        } else if (formula.indexOf("Nothing") >= 0) {
            view = {empty: false, mistake: {
                error: {code: "report.formula-unknown-name", position: "2", name: "Nothing", separator: ";", decimal: ","},
                before: "=", after: "Nothing"
            }};
        } else if (formula.indexOf("PERCENT_OF") >= 0) {
            view = {empty: false, result: {kind: "percent", value: "42,0 %", exactValue: "42,0 %", negative: false, currency: "", permille: 420,
                counted: [{name: "Pets", inside: [], insideMore: 0, ownPeriod: true, month: "", firstDay: "", lastDay: ""}]}};
        } else {
            view = {empty: false, result: {kind: "amount", value: "312,40 €", exactValue: "312,40 €", negative: false, currency: "EUR",
                counted: [
                    {name: "Pets", inside: ["Vet", "Cat food"], insideMore: 2, ownPeriod: true, month: "", firstDay: "", lastDay: ""},
                    {name: "Travel", inside: [], insideMore: 0, ownPeriod: false, month: "September 2026", firstDay: "01.09.2026", lastDay: "30.09.2026"}
                ]}};
        }
    }

    // A bar: the same figures, with a fill and the text of a target.
    function showBar(period, formula, target) {
        show(period, formula, false);
        if (view.result !== undefined) {
            view = {empty: false, result: Object.assign({}, view.result, {
                permille: view.result.kind === "percent" ? 420 : 780,
                target: target === "" ? "" : "400,00 €"
            })};
        }
    }

    // The filters the next requests are narrowed by (M6c.7); kept to read.
    property string scope: ""
    function setScope(text) { scope = text; }

    function refresh() { }

    // Names that fit the word before the cursor; `start`/`end` as QML counts.
    function complete(text, cursor) {
        const match = /[A-Za-z]+$/.exec(text.slice(0, cursor));
        if (match === null) {
            return {start: 0, end: 0, names: []};
        }
        const names = [{insert: "Pets", label: "Pets", group: false}, {insert: "'Pet food'", label: "Pets > Pet food", group: false}]
            .filter(name => name.label.toLowerCase().indexOf(match[0].toLowerCase()) >= 0);
        return {start: match.index, end: cursor, names: names};
    }

    function functions() {
        return {separator: ";", decimal: ",", functions: [
            {name: "SUM", arguments: ["x", "…"]},
            {name: "IF", arguments: ["test", "yes", "no"]},
            {name: "PERCENT_OF", arguments: ["part", "whole"]}
        ]};
    }
}
