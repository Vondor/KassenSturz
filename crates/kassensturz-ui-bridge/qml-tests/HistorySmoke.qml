// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: history page groups

// Opens HistoryPage.qml on a stand-in history and checks the grouped list
// (B-395, D-870): changes are grouped by day and by sitting (a gap of more
// than 30 minutes starts a new one), the same step done several times in a
// row is one row, the kind filter and the search keep "Undo up to here"
// counting over all changes, and an undone change on top isn't counted.
// `tools/check-panel.sh` runs it and fails on the first FAIL line or on any
// QML warning about a binding or a type.

import QtQuick
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 900
    height: 1200
    visible: true
    property bool failed: false

    // 15 Jan 2026, 12:00 UTC: far from today, in every time zone one day.
    readonly property int noon: 1768478400

    ChangeHistory {
        id: history

        changes: [
            { action: "add-budget", madeAtSeconds: String(window.noon + 600), undone: true, fileName: "", filedBills: 0 },
            { action: "add-transaction", madeAtSeconds: String(window.noon + 300), undone: false, fileName: "", filedBills: 0 },
            { action: "add-transaction", madeAtSeconds: String(window.noon + 200), undone: false, fileName: "", filedBills: 0 },
            { action: "add-transaction", madeAtSeconds: String(window.noon + 100), undone: false, fileName: "", filedBills: 0 },
            { action: "edit-budget", madeAtSeconds: String(window.noon), undone: false, fileName: "", filedBills: 0 },
            // Two hours earlier: a sitting of its own.
            { action: "add-rule", madeAtSeconds: String(window.noon - 7200), undone: false, fileName: "", filedBills: 0 },
        ]
    }

    pageStack.initialPage: HistoryPage {
        id: page

        history: history
    }

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    Timer {
        interval: 300
        running: true
        onTriggered: {
            let groups = page.groups;
            window.check(groups.length === 2, "two sittings, got " + groups.length);
            window.check(groups[0].count === 5 && groups[0].rows.length === 3, "the three same steps are one row: " + JSON.stringify(groups[0].rows));
            const merged = groups[0].rows[1];
            window.check(merged.times === 3 && merged.steps === 3, "three times, and Undo takes back three, got " + merged.times + "/" + merged.steps);
            window.check(groups[0].rows[0].undone && groups[0].rows[2].steps === 4, "the undone change isn't counted: " + groups[0].rows[2].steps);
            window.check(groups[1].rows[0].steps === 5, "the older sitting's change is the fifth to undo");

            page.kind = "rules";
            groups = page.groups;
            window.check(groups.length === 1 && groups[0].rows[0].steps === 5, "a filter keeps the count over all changes");
            page.kind = "all";
            page.searchText = "budget";
            groups = page.groups;
            window.check(groups.length === 1 && groups[0].count === 2, "the search finds the two budget changes, got " + JSON.stringify(groups));
            page.searchText = "nothing like this";
            window.check(page.groups.length === 0, "no group when nothing matches");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
