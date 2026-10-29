// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Draws every kind of chart with made-up values, the way BookCharts hands
// them to QML, and saves a picture of each to the folder in the environment
// variable CHARTS_OUT (M6a.4). `tools/check-charts.sh` runs it under Xvfb with
// Mesa and checks the pictures: under Qt's `offscreen` platform KQuickCharts
// draws nothing.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.quickcharts as Charts

Window {
    id: window

    width: 640
    height: 560
    visible: true

    // The pictures go to the folder `out` of the working directory.
    readonly property string outDir: "out"

    function point(label, value, color) {
        return {label: label, minorUnits: String(value * 100), value: value * 100, color: color, marker: color < 4 ? color : -1};
    }

    function months() {
        return ["January 2026", "February 2026", "March 2026", "April 2026"];
    }

    function monthly(kind) {
        const income = [2500, 2500, 2650, 2500];
        const expenses = [1800, 2200, 1900, 2750];
        const top = kind === "stacked-bar" ? 5000 : 3000;
        return {
            kind: kind, dataset: "income_expense_by_month", currency: "EUR", round: false,
            total: "1 100,00 €", paletteSize: 8, count: 4, unsortedIndex: -1, hidden: ({}),
            categories: months(),
            series: [
                {key: "income", name: "", color: 0, marker: 0, points: income.map(v => point(v + ",00 €", v, 0))},
                {key: "expenses", name: "", color: 1, marker: 1, points: expenses.map(v => point(v + ",00 €", v, 1))}
            ],
            ticks: [0, 1000, 2000, 3000, 4000, 5000].filter(v => v <= top).map(v => ({label: v + " €", minorUnits: String(v * 100), value: v * 100})),
            table: {currency: "EUR", columns: ["month", "income", "expenses", "net"],
                    rows: months().map((m, i) => [m, income[i] + ",00 €", expenses[i] + ",00 €", (income[i] - expenses[i]) + ",00 €"])}
        };
    }

    function spending(kind) {
        const names = ["Rent", "Food › Groceries", "", kind === "bar" ? "Travel › Long journeys by train and plane" : "Travel"];
        const values = [800, 320, 60, 150];
        return {
            kind: kind, dataset: "spending_by_category", currency: "EUR", round: kind !== "bar",
            total: "1 330,00 €", paletteSize: 8, count: 4, unsortedIndex: 2, hidden: ({}),
            categories: names,
            series: [{key: "spending", name: "", color: 0, marker: 0, points: values.map((v, i) => point(v + ",00 €", v, i))}],
            ticks: kind === "bar" ? [0, 400, 800].map(v => ({label: v + " €", minorUnits: String(v * 100), value: v * 100})) : [],
            table: {currency: "EUR", columns: ["category", "total"], rows: names.map((n, i) => [n, values[i] + ",00 €"])}
        };
    }

    // Rent and one more category are hidden: the note stays under the chart.
    function spendingWithHidden(kind) {
        const chart = spending(kind);
        chart.hidden = {first: "Rent", more: 1, total: "1.250,00 €"};
        return chart;
    }

    // Two series the user chose: named by the book, told apart by key.
    function overTime() {
        const chart = monthly("line");
        chart.dataset = "category_over_time";
        chart.series = [
            {key: "series-0", name: "Fixed costs", color: 0, marker: 0, points: [800, 800, 800, 800].map(v => point(v + ",00 €", v, 0))},
            {key: "series-1", name: "Food › Groceries", color: 1, marker: 1, points: [300, 320, 280, 310].map(v => point(v + ",00 €", v, 1))}
        ];
        return chart;
    }

    readonly property var charts: [
        monthly("line"), monthly("area"), monthly("bar"), monthly("stacked-bar"),
        spending("bar"), spending("pie"), spending("donut"), spendingWithHidden("pie"), overTime()
    ]
    property int shown: 0

    ChartView {
        id: view

        anchors.fill: parent
        anchors.margins: 12
        chart: window.charts[window.shown]
        title: "Chart " + window.shown
    }

    // Every chart as a picture and then as a table: take the picture, then
    // change what is shown, then wait for it to be drawn.
    Timer {
        interval: 900
        running: true
        repeat: true
        onTriggered: {
            const name = window.shown + "-" + view.chart.kind + (view.showTable ? "-table" : "");
            view.grabToImage(function (result) {
                result.saveToFile(window.outDir + "/" + name + ".png");
                if (!view.showTable) {
                    view.showTable = true;
                } else {
                    view.showTable = false;
                    if (window.shown + 1 < window.charts.length) {
                        window.shown += 1;
                    } else {
                        finish.start();
                    }
                }
            });
        }
    }

    Timer {
        id: finish

        interval: 800
        onTriggered: Qt.quit()
    }
}
