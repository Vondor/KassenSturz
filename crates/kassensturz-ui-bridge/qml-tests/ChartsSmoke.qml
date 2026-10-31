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
            total: "1 100,00 €", paletteSize: 8, count: 4, unsortedIndex: -1, hidden: ({}), slices: ({other: -1, holds: [], shares: [], named: [], undrawn: []}), 
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
            total: "1 330,00 €", paletteSize: 8, count: 4, unsortedIndex: 2, hidden: ({}), slices: ({other: -1, holds: [], shares: [], named: [], undrawn: []}), 
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

    // Eleven categories: seven slices, "Other" and a refund that can't be one.
    function spendingWithOther(kind) {
        const names = ["Rent", "Food", "Travel", "Phone", "Power", "Fun", "Pets", "", "Refund"];
        const values = [800, 320, 150, 90, 80, 60, 50, 140, -20];
        const chart = spending(kind);
        chart.categories = names;
        chart.count = names.length;
        chart.unsortedIndex = -1;
        chart.series = [{key: "spending", name: "", color: 0, marker: 0, points: values.map((v, i) => point(v + ",00 €", v, i))}];
        chart.table = {currency: "EUR", columns: ["category", "total"], rows: names.map((n, i) => [n, values[i] + ",00 €"])};
        chart.slices = {
            other: 7,
            holds: [{name: "Gifts", label: "90,00 €"}, {name: "", label: "50,00 €"}],
            shares: ["37,2 %", "14,9 %", "7,0 %", "4,2 %", "3,7 %", "2,8 %", "2,3 %", "6,5 %", ""],
            named: [true, true, true, false, false, false, false, true, false],
            undrawn: [false, false, false, false, false, false, false, false, true]
        };
        return chart;
    }

    // Eight slices of about the same size with long names, all big enough to
    // carry their name: the names are crowded and must not cover each other.
    function spendingCrowded(kind) {
        const names = ["Miscellaneous › Everyday things", "Taxes and Fees › Colony land use", "Clothing and Gear › Helmets", "Food › Groceries", "Housing › Rent", "Travel › Long journeys", "Health › Dentist", "Fun › Games"];
        const values = [200, 150, 140, 130, 120, 110, 90, 60];
        const whole = values.reduce((sum, v) => sum + v, 0);
        const chart = spending(kind);
        chart.categories = names;
        chart.count = names.length;
        chart.unsortedIndex = -1;
        chart.series = [{key: "spending", name: "", color: 0, marker: 0, points: values.map((v, i) => point(v + ",00 €", v, i))}];
        chart.table = {currency: "EUR", columns: ["category", "total"], rows: names.map((n, i) => [n, values[i] + ",00 €"])};
        chart.slices = {
            other: -1,
            holds: [],
            shares: values.map(v => Math.round(v / whole * 1000) / 10 + " %"),
            named: values.map(() => true),
            undrawn: values.map(() => false)
        };
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

    // What you are worth at the end of each month, one line (M6c.2).
    function netWorth() {
        const assets = [1700, 1900, 3350, 3300];
        const owed = [200, 200, 250, 400];
        const worth = assets.map((v, i) => v - owed[i]);
        return {
            kind: "line", dataset: "net_worth_over_time", currency: "EUR", round: false,
            total: "2.900,00 €", paletteSize: 8, count: 4, unsortedIndex: -1, hidden: ({}), slices: ({other: -1, holds: [], shares: [], named: [], undrawn: []}),
            categories: months(),
            series: [{key: "net_worth", name: "", color: 0, marker: 0, points: worth.map(v => point(v + ",00 €", v, 0))}],
            ticks: [0, 1000, 2000, 3000, 4000].map(v => ({label: v + " €", minorUnits: String(v * 100), value: v * 100})),
            table: {currency: "EUR", columns: ["month", "assets", "liabilities", "net_worth"],
                    rows: months().map((m, i) => [m, assets[i] + ",00 €", owed[i] + ",00 €", worth[i] + ",00 €"])}
        };
    }

    // Money in and out of the accounts as bars, with the table that has what
    // they held at the start and at the end (M6c.2).
    function cashFlow() {
        const start = [1700, 2400, 2450, 3100];
        const inn = [2500, 2500, 2650, 2500];
        const out = [1800, 2450, 2000, 2750];
        const chart = monthly("bar");
        chart.dataset = "cash_flow_by_month";
        chart.total = "850,00 €";
        chart.series = [
            {key: "money_in", name: "", color: 0, marker: 0, points: inn.map(v => point(v + ",00 €", v, 0))},
            {key: "money_out", name: "", color: 1, marker: 1, points: out.map(v => point(v + ",00 €", v, 1))}
        ];
        chart.table = {currency: "EUR", columns: ["month", "start", "money_in", "money_out", "starting", "end"],
                       rows: months().map((m, i) => [m, start[i] + ",00 €", inn[i] + ",00 €", out[i] + ",00 €", "0,00 €", (start[i] + inn[i] - out[i]) + ",00 €"])};
        return chart;
    }

    readonly property var charts: [
        monthly("line"), monthly("area"), monthly("bar"), monthly("stacked-bar"),
        spending("bar"), spending("pie"), spending("donut"), spendingWithHidden("pie"), spendingWithOther("pie"), spendingWithOther("donut"), overTime(), netWorth(), cashFlow(),
        spendingCrowded("pie"), spendingCrowded("donut")
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
            if (!view.showTable) {
                // tools/check-charts.sh fails on any slice names that cover each other.
                console.log("name-overlaps", name, view.nameOverlaps());
            }
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
