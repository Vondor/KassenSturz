// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for AccountTypes.qml, for `tools/check-panel.sh` (M6a.9).

import QtQuick

QtObject {
    // The types the wizard's account rows offer (#1031).
    readonly property var all: [
        { key: "checking", name: "Checking account", hint: "" },
        { key: "savings", name: "Savings account", hint: "" },
        { key: "cash", name: "Cash", hint: "" },
        { key: "credit-card", name: "Credit card", hint: "" },
    ]

    function nameOf(type) { return type; }
}
