// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object AiSwitch, for `tools/check-panel.sh`
// (M6a.9): AI is on unless a test says otherwise.

import QtQuick

QtObject {
    property bool enabled: true
    // Whether this build has AI at all (D-039).
    property bool available: true
    // Whether the settings can be written; a test sets it to false.
    property bool saves: true
    // What `switchTo` was asked for, in order.
    property var asked: []

    function switchTo(on: bool): bool {
        asked.push(on);
        if (!available || !saves) {
            return false;
        }
        enabled = on;
        return true;
    }
}
