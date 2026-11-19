// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object ChangeHistory, for `tools/check-panel.sh`
// (HistorySmoke, B-395): the test sets the changes, newest first, as
// history_model.rs builds them.

import QtQuick

QtObject {
    property var undoChange: ({})
    property var redoChange: ({})
    property var changes: []
    readonly property int total: changes.length
    readonly property int shown: changes.length

    function undoAnyway() { return ({}); }
}
