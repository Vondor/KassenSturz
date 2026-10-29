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
                categories: ["Rent", "Groceries"],
                series: [{key: "spending", color: 0, marker: 0, points: [
                    {label: "800,00 €", minorUnits: "80000", value: 80000, color: 0, marker: 0},
                    {label: "530,00 €", minorUnits: "53000", value: 53000, color: 1, marker: 1}
                ]}],
                ticks: [],
                table: {currency: "EUR", columns: ["category", "total"],
                        rows: [["Rent", "800,00 €"], ["Groceries", "530,00 €"]]}
            }]
        })

        function show(dataset, kind, period, grouping) {
            window.asked.push("chart:" + dataset + ":" + kind + ":" + period + ":" + grouping);
        }
    }

    property var overview: ({monthKey: "2026-09", empty: false})

    OverviewMonth {
        id: month

        anchors.fill: parent
        view: window.overview
        cards: fakeCards
        charts: fakeCharts
    }

    function count(item, typeName) {
        let found = 0;
        const walk = node => {
            if (String(node).indexOf(typeName) === 0) {
                found += 1;
            }
            for (const child of node.children) {
                walk(child);
            }
        };
        walk(item);
        return found;
    }

    Timer {
        interval: 600
        running: true
        onTriggered: {
            window.check(window.asked.join() === "cards:2026-09:previous_period,chart:spending_by_category:donut:2026-09:top-level",
                         "asked for the month shown, got " + window.asked.join());
            window.check(window.count(month, "KpiCard") === 4, "a card for each of the four, found " + window.count(month, "KpiCard"));
            window.check(window.count(month, "ChartView") === 1, "a chart, found " + window.count(month, "ChartView"));
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
            window.check(window.asked.join() === "cards:2026-08:previous_period,chart:spending_by_category:donut:2026-08:top-level",
                         "asked for the next month, got " + window.asked.join());
            if (window.failed) {
                Qt.exit(1);
            }
            console.log("overview month: ok");
            Qt.quit();
        }
    }
}
