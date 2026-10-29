// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object BookCharts, for `tools/check-panel.sh`
// (M6a.7): one pie-like chart object in the shape the real one gives.

import QtQuick

QtObject {
    property var view: ({empty: true, charts: []})
    property var asked: []

    function point(label, value, color) {
        return {label: label, minorUnits: String(value * 100), value: value * 100, color: color, marker: color < 4 ? color : -1};
    }

    function show(dataset, kind, period, grouping) {
        asked.push([dataset, kind, period, grouping].join(" "));
        const names = ["Rent", "Groceries", ""];
        const values = [800, 320, 60];
        view = {empty: false, charts: [{
            kind: kind, dataset: dataset, currency: "EUR", round: kind === "pie" || kind === "donut",
            total: "1.180,00 €", paletteSize: 8, count: 3, unsortedIndex: 2,
            categories: names,
            series: [{key: "spending", color: 0, marker: 0, points: values.map((v, i) => point(v + ",00 €", v, i))}],
            ticks: [], 
            table: {currency: "EUR", columns: ["category", "total"], rows: names.map((n, i) => [n, values[i] + ",00 €"])}
        }]};
    }

    function refresh() { }
}
