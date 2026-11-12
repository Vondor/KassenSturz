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

    // Spending compared with the average of the 3 months before (M6-B229):
    // with long names the bars go in rows; one category whose spending is
    // zero keeps a narrow average bar beside a zero line under its name.
    function compared(names, spent, average, currency) {
        const ticks = [0, 1000, 2000, 3000, 4000, 5000].filter(v => v <= Math.max(...spent, ...average) + 1000);
        return {
            kind: "bar", dataset: "spending_by_category", currency: currency, round: false,
            total: "", paletteSize: 8, count: names.length, unsortedIndex: -1, hidden: ({}), slices: ({other: -1, holds: [], shares: [], named: [], undrawn: []}),
            categories: names,
            series: [
                {key: "spending", name: "", color: 0, marker: 0, points: spent.map(v => point(v + ",00", v, 0))},
                {key: "average_3", name: "", color: 1, marker: 1, points: average.map(v => point(v + ",00", v, 1))}
            ],
            ticks: ticks.map(v => ({label: String(v), minorUnits: String(v * 100), value: v * 100})),
            table: {currency: currency, columns: ["category", "spending", "average_3"],
                    rows: names.map((n, i) => [n, spent[i] + ",00", average[i] + ",00"])}
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

    // The balance as candles (US4.10): two weeks up (hollow, blue) and two
    // down (filled, orange), on an axis that does not start at zero.
    function candles() {
        const open = [5000, 5200, 5600, 5300];
        const high = [5300, 5700, 5650, 5400];
        const low = [4900, 5100, 5200, 5000];
        const close = [5200, 5600, 5300, 5050];
        const chart = monthly("bar");
        chart.kind = "candlestick";
        chart.dataset = "cash_flow_by_month";
        chart.total = "5050,00 €";
        chart.categories = ["2 Mar 2026", "9 Mar 2026", "16 Mar 2026", "23 Mar 2026"];
        const four = (key, values, color) => ({key: key, name: "", color: color, marker: color, points: values.map(v => point(v + ",00 €", v, color))});
        chart.series = [four("start", open, 0), four("highest", high, 1), four("lowest", low, 2), four("end", close, 3)];
        chart.ticks = [4800, 5000, 5200, 5400, 5600, 5800].map(v => ({label: v + " €", minorUnits: String(v * 100), value: v * 100}));
        chart.table = {currency: "EUR", columns: ["period", "start", "highest", "lowest", "end"],
                       rows: chart.categories.map((c, i) => [c, open[i] + ",00 €", high[i] + ",00 €", low[i] + ",00 €", close[i] + ",00 €"])};
        return chart;
    }

    function item(category, value, label, text, size) {
        return {category: category, hasValue: value !== null, value: value === null ? 0 : value * 100, label: label, text: text ?? "", size: size ?? 2};
    }

    function customTable(rows) {
        return {currency: "EUR", columns: ["category", "series", "value"], rows: rows};
    }

    // A custom chart (M6e.2): bars of the net, a level at the mean, dots at the
    // income with their size, and texts.
    function customLayers() {
        const net = [700, 300, 750, -250];
        const income = [2500, 2500, 2650, 2500];
        const chart = monthly("bar");
        chart.dataset = "income_expense_by_month";
        chart.total = "";
        chart.series = [{key: "series-0", name: "Net", color: 0, marker: 0, points: net.map(v => point(v + ",00 €", v, 0))}];
        chart.ticks = [-1000, 0, 1000, 2000, 3000].map(v => ({label: v + " €", minorUnits: String(v * 100), value: v * 100}));
        chart.overlays = [
            {mark: "rule-level", series: [{name: "Mean net", color: 1, marker: 1, items: [item(-1, 375, "375,00 €")]}]},
            {mark: "point", series: [{name: "Income", color: 2, marker: 2, items: income.map((v, i) => item(i, v, v + ",00 €", "", 1 + i))}]},
            {mark: "text", series: [{name: "Notes", color: 3, marker: 3, items: [item(0, 2500, "2.500,00 €", "Best month")]}]}
        ];
        chart.table = customTable(months().map((m, i) => [m, "Net", net[i] + ",00 €"]));
        return chart;
    }

    // Stacked areas of two colored series.
    function stackedArea() {
        const chart = overTime();
        chart.kind = "stacked-area";
        chart.dataset = "category_over_time";
        chart.total = "";
        chart.ticks = [0, 500, 1000, 1500].map(v => ({label: v + " €", minorUnits: String(v * 100), value: v * 100}));
        return chart;
    }

    // Only dots, a line and a rule at one month: an empty chart carries them.
    function marksOnly() {
        const chart = monthly("bar");
        chart.total = "";
        chart.series = [];
        chart.overlays = [
            {mark: "line", series: [{name: "Income", color: 0, marker: 0, items: [2500, 2500, 2650, 2500].map((v, i) => item(i, v, v + ",00 €"))}]},
            {mark: "area", series: [{name: "Floor", color: 1, marker: 1, items: [500, 600, 700, 800].map((v, i) => item(i, v, v + ",00 €"))}]},
            {mark: "bar", series: [{name: "Spent", color: 2, marker: 2, items: [1800, 2200, 1900, 2750].map((v, i) => item(i, v, v + ",00 €"))}]},
            {mark: "rule-category", series: [{name: "Move", color: 3, marker: 3, items: [{category: 2, hasValue: false, value: 0, label: "March 2026", text: "", size: 1}]}]}
        ];
        chart.table = customTable([["March 2026", "Move", ""]]);
        return chart;
    }

    readonly property var charts: [
        monthly("line"), monthly("area"), monthly("bar"), monthly("stacked-bar"),
        spending("bar"), spending("pie"), spending("donut"), spendingWithHidden("pie"), spendingWithOther("pie"), spendingWithOther("donut"), overTime(), netWorth(), cashFlow(),
        spendingCrowded("pie"), spendingCrowded("donut"),
        customLayers(), stackedArea(), marksOnly(),
        compared(["Miscellaneous", "Subspace Communications", "Taxes and Fees", "Clothing and Gear", "Food"],
                 [40, 300, 1255, 1110, 900], [120, 280, 4020, 75, 0], "EUR"),
        compared(["Travel"], [0], [2433], "USD"),
        candles()
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
                // tools/check-charts.sh checks which bar charts are rows.
                console.log("bar-rows", name, view.horizontal);
                // tools/check-charts.sh checks that a lone bar beside a series
                // of zeros sits in the middle of its name (US4 P3 R3b).
                console.log("bar-middle", name, Math.round(view.barMiddle(view.chart.series.length - 1, 0, 1000)));
                // tools/check-charts.sh checks that the candle legend shows
                // the boxes each fill draws (D-748): rising, then falling.
                if (view.candles) {
                    for (const fill of ["up-hollow", "down-hollow", "all-filled", "all-hollow"]) {
                        view.candleFill = fill;
                        console.log("candle-legend", fill, view.legendEntries.map(entry => entry.hollow ? "hollow" : "filled").join(","));
                    }
                    view.candleFill = "up-hollow";
                    // *Show the legend* switched off hides only the legend.
                    view.showLegend = false;
                    console.log("legend-shown", "off", view.legendShown);
                    view.showLegend = true;
                    console.log("legend-shown", "on", view.legendShown);
                }
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
