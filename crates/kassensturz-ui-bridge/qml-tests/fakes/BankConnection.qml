// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object BankConnection, for `tools/check-panel.sh`
// (M6d.7): only what the Update all accounts widget uses, with the same
// shapes, recording each call in `calls`.

import QtQuick

QtObject {
    id: fake

    property var calls: []
    property int linked: 0
    property bool busy: false
    property bool updating: false
    property var updateSummary: ({})

    signal fetchStarted(string account)
    signal updateAllFinished(var result)

    function updateAllState() {
        return {linked: fake.linked, lastUpdated: "2026-10-09", never: false};
    }

    function updateAll() {
        calls.push("updateAll");
        busy = true;
        updating = true;
        updateSummary = ({});
        return {accounts: [{id: "a1", name: "Current account"}, {id: "a2", name: "Card"}]};
    }

    function stopUpdateAll() {
        calls.push("stopUpdateAll");
    }

    // What the worker and the GUI thread do when the fetch has ended.
    function finish(summary) {
        busy = false;
        updateSummary = summary;
        updating = false;
        updateAllFinished(summary);
    }
}
