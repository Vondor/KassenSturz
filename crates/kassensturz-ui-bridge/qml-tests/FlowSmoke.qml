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
            {from: 1, to: 0, yFrom: 20000, yTo: 20000, thickness: 960000, label: "3.000,00 €"},
            {from: 0, to: 4, yFrom: 500000, yTo: 520000, thickness: 480000, label: "1.500,00 €"},
            {from: 0, to: 2, yFrom: 20000, yTo: 0, thickness: 320000, label: "1.000,00 €"},
            {from: 0, to: 3, yFrom: 340000, yTo: 340000, thickness: 160000, label: "500,00 €"}
        ]
    })

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
