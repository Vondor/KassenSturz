// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Draws the money-flow chart (US7, #821) with made-up places, the way
// BookCharts hands them to QML, and saves a picture of it and of its table
// to the folder `out` of the working directory. `tools/check-charts.sh`
// runs it under Xvfb and looks at the picture.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Window {
    id: window

    width: 760
    height: 620
    visible: true

    // Salary 3,000.00 in; rent 1,000.00, food 500.00 and 1,500.00 left over
    // out. Places are millionths of the chart's height.
    readonly property var chart: ({
        kind: "flow", dataset: "money_flow", currency: "EUR", total: "3.000,00 €", whole: 1000000, columns: 3,
        elsewhere: ["120,00 USD"],
        nodes: [
            {kind: "middle", name: "", column: 1, y: 20000, height: 960000, label: "3.000,00 €", category: "", holds: []},
            {kind: "category", name: "Salary", column: 0, y: 20000, height: 960000, label: "3.000,00 €", category: "a", holds: []},
            {kind: "category", name: "Rent", column: 2, y: 0, height: 320000, label: "1.000,00 €", category: "b", holds: []},
            {kind: "category", name: "Food", column: 2, y: 340000, height: 160000, label: "500,00 €", category: "c", holds: []},
            {kind: "left-over", name: "", column: 2, y: 520000, height: 480000, label: "1.500,00 €", category: "", holds: []}
        ],
        bands: [
            {from: 1, to: 0, yFrom: 20000, yTo: 20000, thickness: 960000, label: "3.000,00 €", assumed: "", returned: ""},
            {from: 0, to: 4, yFrom: 500000, yTo: 520000, thickness: 480000, label: "1.500,00 €", assumed: "", returned: ""},
            {from: 0, to: 2, yFrom: 20000, yTo: 0, thickness: 320000, label: "1.000,00 €", assumed: "400,00 €", returned: ""},
            {from: 0, to: 3, yFrom: 340000, yTo: 340000, thickness: 160000, label: "500,00 €", assumed: "", returned: "100,00 €"}
        ]
    })

    // Many small nodes close together on both sides, with long names, as in
    // a real year (B-309): their labels must not cover one another.
    readonly property var crowded: {
        const nodes = [{kind: "middle", name: "", column: 1, y: 0, height: 1000000, label: "57.190,06 €", category: "", holds: []}];
        const bands = [];
        const sides = [[0, [900000, 30000, 20000, 15000, 10000, 5000]], [2, [300000, 200000, 150000, 100000, 80000, 60000, 40000, 30000, 20000, 15000, 5000]]];
        for (const [column, heights] of sides) {
            let y = 0;
            for (const height of heights) {
                nodes.push({kind: "category", name: "A very long category name " + nodes.length, column: column, y: y, height: height, label: "54.116,00 €", category: "x" + nodes.length, holds: []});
                bands.push({from: column === 0 ? nodes.length - 1 : 0, to: column === 0 ? 0 : nodes.length - 1, yFrom: y, yTo: y, thickness: height, label: "1,00 €", assumed: "", returned: ""});
                y += height;
            }
        }
        return {kind: "flow", dataset: "money_flow", currency: "EUR", total: "57.190,06 €", whole: 1000000, columns: 3, elsewhere: [], nodes: nodes, bands: bands};
    }

    // Whether the labels of `places` in a plot `plotHeight` high stay inside
    // it and apart in each column, and no column shows more than fit.
    function labelsApart(chart, places, plotHeight, labelHeight) {
        const byColumn = {};
        // A column in `oneLine` has labels of one line, half as high.
        const heightOf = column => (places.oneLine ?? {})[column] === true ? labelHeight / 2 : labelHeight;
        for (let i = 0; i < places.length; i++) {
            if (places[i] === null) {
                continue;
            }
            if (places[i] < -0.01 || places[i] + heightOf(chart.nodes[i].column) > plotHeight + 0.01) {
                return false;
            }
            (byColumn[chart.nodes[i].column] = byColumn[chart.nodes[i].column] ?? []).push(places[i]);
        }
        for (const key in byColumn) {
            const tops = byColumn[key].sort((a, b) => a - b);
            const labelHeight = heightOf(Number(key));
            if (tops.length > Math.max(1, Math.floor(plotHeight / labelHeight))) {
                return false;
            }
            for (let j = 1; j < tops.length; j++) {
                if (tops[j] < tops[j - 1] + labelHeight - 0.01) {
                    return false;
                }
            }
        }
        return true;
    }

    readonly property string outDir: "out"

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16

        FlowChart {
            id: flow

            Layout.fillWidth: true
            chart: window.chart
            title: "Where my money went"
        }

        // The crowded chart at a phone's width (360 px): not drawn, only
        // measured.
        FlowChart {
            id: crowdedFlow

            Layout.preferredWidth: 360
            visible: false
            chart: window.crowded
        }
    }

    function save(item, name, done) {
        item.grabToImage(function (result) {
            result.saveToFile(outDir + "/" + name + ".png");
            done();
        });
    }

    Timer {
        interval: 1500
        running: true
        onTriggered: {
            console.log("flow-columns " + flow.columns);
            // Hover on the biggest band: its text names both ends.
            const found = flow.bandAt(100, 300, 400, 400);
            console.log("flow-band-at " + found);
            console.log("flow-text " + flow.bandText(flow.bands[0]));
            console.log("flow-assumed " + flow.bandText(flow.bands[2]));
            console.log("flow-returned " + flow.bandText(flow.bands[3]));
            // What a screen reader gets: the chart is one focusable item (the
            // review card of the AI shows this same component), named by the
            // band being read, with the round trip in its words (B-305).
            flow.current = 3;
            console.log("flow-accessible-name " + flow.Accessible.name);
            console.log("flow-accessible-role " + (flow.Accessible.role === Accessible.Chart));
            console.log("flow-accessible-description " + (flow.Accessible.description.length > 0));
            flow.current = -1;
            // What a click opens: a band takes its outer category, a band or
            // node with none takes everything, and the scope is narrowed in
            // text only.
            console.log("flow-behind " + [flow.categoryBehind(0, 2), flow.categoryBehind(1, 0), flow.categoryBehind(3, -1), flow.categoryBehind(0, -1), flow.categoryBehind(0, 4)].join(","));
            flow.scope = '{"accounts":["x"],"takes":{"period":true}}';
            const narrowed = JSON.parse(flow.behindScope("b", []));
            console.log("flow-scope " + JSON.stringify([narrowed.categories, narrowed.categoriesOwn, narrowed.accounts, narrowed.takes.categories, narrowed.takes.period]));
            console.log("flow-scope-all " + (JSON.parse(flow.behindScope("", [])).categories === undefined));
            // A click on an account narrows to it, in text only.
            const account = JSON.parse(flow.behindScope("", ["acc-1"]));
            console.log("flow-scope-account " + JSON.stringify([account.accounts, account.accountsOwn, account.takes.accounts, account.takes.period]));
            // No label covers another or leaves the chart, at a roomy and at a
            // short height; every outer node of the roomy one has a label,
            // and a short one shows only what fits (B-309).
            const roomy = crowdedFlow.labelPlaces(600);
            const short = crowdedFlow.labelPlaces(crowdedFlow.labelHeight * 3.5);
            console.log("flow-labels-apart " + (window.labelsApart(window.crowded, roomy, 600, crowdedFlow.labelHeight)
                && window.labelsApart(window.crowded, short, crowdedFlow.labelHeight * 3.5, crowdedFlow.labelHeight)));
            console.log("flow-labels-all " + roomy.every(top => top !== null));
            console.log("flow-labels-short " + short.filter(top => top !== null).length);
            // A crowded column of the short chart shows names only, one line
            // each, so twice as many fit (7 instead of 3: all 6 on the left,
            // 7 of the 11 on the right, and the middle, 14 labels); the
            // roomy one keeps two lines (B-309, round 4).
            console.log("flow-labels-one-line " + JSON.stringify(Object.keys(short.oneLine)) + " " + JSON.stringify(Object.keys(roomy.oneLine)));
            // A label in an inner column ends before the next column's nodes,
            // so it covers neither them nor their labels (B-309, round 4).
            console.log("flow-labels-inner-fit " + (crowdedFlow.xOf(1, 360) + crowdedFlow.nodeWidth + crowdedFlow.innerLabelWidth(1, 360) <= crowdedFlow.xOf(2, 360)));
            window.save(window.contentItem, "flow-chart", function () {
                flow.showTable = true;
                tableTimer.start();
            });
        }
    }

    Timer {
        id: tableTimer

        interval: 600
        onTriggered: window.save(window.contentItem, "flow-table", function () {
            Qt.quit();
        })
    }
}
