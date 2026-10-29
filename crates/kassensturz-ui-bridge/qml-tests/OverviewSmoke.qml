// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Loads OverviewMonth.qml (M6a.8) on stand-ins for BookCards and BookCharts
// and checks what the overview month does: it asks for the month the overview
// view names, shows a card for every card it was given and a chart for every
// chart, and asks again for the new month when the view changes.
// `tools/check-overview.sh` runs it; it exits 1 and prints why when something
// is wrong.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts

Window {
    id: window

    width: 900
    height: 900
    visible: true

    property var asked: []
    property bool failed: false

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    function card(measure, value) {
        return {measure: measure, currency: "EUR", value: value, previous: value, change: "0,00 €",
                changePercent: "0,0 %", direction: "same", exactValue: value, negative: false};
    }

    // The shapes BookCards.view and BookCharts.view have (cards_model.rs,
    // charts_model.rs).
    QtObject {
        id: fakeCards

        property var view: ({
            empty: false,
            sets: [{currency: "EUR", cards: [
                card("income", "2.500,00 €"), card("expenses", "853,20 €"),
                card("net", "1.646,80 €"), Object.assign(card("savings-rate", "65,9 %"), {permille: 659})
            ]}]
        })

        function show(period, compare) { window.asked.push("cards:" + period + ":" + compare); }
    }

    QtObject {
        id: fakeCharts

        property var view: ({
            empty: false,
            charts: [{
                kind: "donut", dataset: "spending_by_category", currency: "EUR", round: true,
                total: "1.330,00 €", paletteSize: 8, count: 2, unsortedIndex: -1,
                categories: ["Rent", "Groceries", "Fuel", "Phone", "Gym", "Books", "Gifts"],
                series: [{key: "spending", color: 0, marker: 0, points: [
                    {label: "800,00 €", minorUnits: "80000", value: 80000, color: 0, marker: 0},
                    {label: "530,00 €", minorUnits: "53000", value: 53000, color: 1, marker: 1}
                ]}],
                ticks: [],
                table: {currency: "EUR", columns: ["category", "total"],
                        rows: [["Rent", "800,00 €"], ["Groceries", "530,00 €"], ["Fuel", "90,00 €"],
                               ["Phone", "40,00 €"], ["Gym", "30,00 €"], ["Books", "20,00 €"], ["Gifts", "10,00 €"]]}
            }]
        })

        function show(dataset, kind, period, grouping) {
            window.asked.push("chart:" + dataset + ":" + kind + ":" + period + ":" + grouping);
        }
    }

    QtObject {
        id: fakeTrend

        property var view: ({
            empty: false,
            charts: [{
                kind: "line", dataset: "income_expense_by_month", currency: "EUR", round: false,
                total: "100,00 €", paletteSize: 8, count: 2, unsortedIndex: -1,
                categories: ["Aug 2026", "Sep 2026"],
                series: [
                    {key: "income", color: 0, marker: 0, points: [
                        {label: "500,00 €", minorUnits: "50000", value: 50000, color: 0, marker: 0},
                        {label: "600,00 €", minorUnits: "60000", value: 60000, color: 0, marker: 0}]},
                    {key: "expenses", color: 1, marker: 1, points: [
                        {label: "400,00 €", minorUnits: "40000", value: 40000, color: 1, marker: 1},
                        {label: "500,00 €", minorUnits: "50000", value: 50000, color: 1, marker: 1}]}
                ],
                ticks: [{label: "0,00 €", minorUnits: "0", value: 0}, {label: "600,00 €", minorUnits: "60000", value: 60000}],
                table: {currency: "EUR", columns: ["month", "income", "expenses", "net"],
                        rows: [["Aug 2026", "500,00 €", "400,00 €", "100,00 €"], ["Sep 2026", "600,00 €", "500,00 €", "100,00 €"]]}
            }]
        })

        function show(dataset, kind, period, grouping) {
            window.asked.push("trend:" + dataset + ":" + kind + ":" + period + ":" + grouping);
        }
    }

    property var overview: ({monthKey: "2026-09", empty: false})

    OverviewMonth {
        id: month

        anchors.fill: parent
        view: window.overview
        cards: fakeCards
        charts: fakeCharts
        trend: fakeTrend
    }

    function find(item, typeName) {
        const found = [];
        const walk = node => {
            if (String(node).indexOf(typeName) === 0) {
                found.push(node);
            }
            for (const child of node.children) {
                walk(child);
            }
        };
        walk(item);
        return found;
    }

    function count(item, typeName) {
        return find(item, typeName).length;
    }

    function months(key) {
        return "cards:" + key + ":previous_period,chart:spending_by_category:donut:" + key
            + ":top-level,trend:income_expense_by_month:line:last-12-months:each";
    }

    Timer {
        interval: 600
        running: true
        onTriggered: {
            window.check(window.asked.join() === window.months("2026-09"),
                         "asked for the month shown, got " + window.asked.join());
            window.check(window.count(month, "KpiCard") === 4, "a card for each of the four, found " + window.count(month, "KpiCard"));
            window.check(window.count(month, "ChartView") === 2, "the donut and the trend line, found " + window.count(month, "ChartView"));
            // Four cards fit one row or a 2 x 2 block, never one column (M6-B2).
            const grids = window.find(month, "KpiGrid");
            window.check(grids.length === 1 && grids[0].columns >= 2, "cards side by side, columns: " + (grids.length > 0 ? grids[0].columns : "no grid"));
            // The table of the largest spending lists five of the seven rows.
            const tables = window.find(month, "ChartTable");
            window.check(tables.length === 1 && tables[0].rows.length === 5, "five largest categories, found " + (tables.length > 0 ? tables[0].rows.length : "no table"));
            // Going to another month asks again for that month.
            window.asked = [];
            window.overview = {monthKey: "2026-08", empty: false};
            next.start();
        }
    }

    Timer {
        id: next

        interval: 300
        onTriggered: {
            window.check(window.asked.join() === window.months("2026-08"),
                         "asked for the next month, got " + window.asked.join());
            if (window.failed) {
                Qt.exit(1);
            }
            console.log("overview month: ok");
            Qt.quit();
        }
    }
}
