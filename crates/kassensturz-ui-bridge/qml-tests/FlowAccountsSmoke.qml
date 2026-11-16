// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: money flow with accounts colours bands by account

// Opens FlowChart.qml with two accounts in two columns, as the money flow
// with accounts draws them when one pays the other (B-356), and checks that
// no two accounts have one colour, and that a band takes the colour of the
// account it comes from, also where it passes behind the other account's
// node. `tools/check-panel.sh` runs it and fails on the first FAIL line.

import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 760
    height: 620
    visible: true
    property bool failed: false

    // Checking pays rent and puts money aside to Savings, which pays the
    // gym: a band from Checking straight to Insurance passes Savings.
    readonly property var chart: ({
        kind: "flow", dataset: "money_flow", currency: "EUR", total: "1.000,00 €", whole: 1000000, columns: 4,
        elsewhere: [],
        nodes: [
            {kind: "account", name: "Checking", column: 0, y: 0, height: 800000, label: "800,00 €", category: "", holds: []},
            {kind: "account", name: "Savings", column: 1, y: 0, height: 200000, label: "200,00 €", category: "", holds: []},
            {kind: "category", name: "Rent", column: 3, y: 0, height: 500000, label: "500,00 €", category: "a", holds: []},
            {kind: "category", name: "Insurance", column: 3, y: 520000, height: 100000, label: "100,00 €", category: "b", holds: []},
            {kind: "category", name: "Gym", column: 3, y: 640000, height: 100000, label: "100,00 €", category: "c", holds: []}
        ],
        bands: [
            {from: 0, to: 1, yFrom: 0, yTo: 0, thickness: 200000, label: "200,00 €", assumed: "", returned: ""},
            {from: 0, to: 2, yFrom: 200000, yTo: 0, thickness: 500000, label: "500,00 €", assumed: "", returned: ""},
            {from: 0, to: 3, yFrom: 700000, yTo: 520000, thickness: 100000, label: "100,00 €", assumed: "", returned: ""},
            {from: 1, to: 4, yFrom: 0, yTo: 640000, thickness: 100000, label: "100,00 €", assumed: "", returned: ""}
        ]
    })

    pageStack.initialPage: Kirigami.Page {
        FlowChart {
            id: flow

            anchors.fill: parent
            chart: window.chart
        }
    }

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    Timer {
        interval: 500
        running: true
        onTriggered: {
            const same = (a, b) => Qt.colorEqual(a, b);
            const checking = flow.nodeColors[0];
            const savings = flow.nodeColors[1];
            window.check(!same(checking, savings), "two accounts in two columns have two colours: " + checking + " " + savings);
            window.check(same(flow.bandColor(flow.bands[1]), checking), "Checking's band to Rent has Checking's colour");
            window.check(same(flow.bandColor(flow.bands[2]), checking), "Checking's band to Insurance, which passes behind Savings, has Checking's colour");
            window.check(!same(flow.bandColor(flow.bands[2]), savings), "and not Savings'");
            window.check(same(flow.bandColor(flow.bands[0]), checking), "a move between accounts has the colour of the account it comes from");
            window.check(same(flow.bandColor(flow.bands[3]), savings), "Savings' band to Gym has Savings' colour");
            console.log(window.failed ? "flow accounts: FAILED" : "flow accounts: ok");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
