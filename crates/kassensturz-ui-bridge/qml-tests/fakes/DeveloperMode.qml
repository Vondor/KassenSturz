// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object DeveloperMode, for `tools/check-panel.sh`
// (M6c.D): it remembers the actions the handlers name, so a smoke test can
// check that a click on a button reached `ActionLog.taken`.

import QtQuick

QtObject {
    property bool on: true
    property var actions: []

    function actionTaken(name: string): void {
        actions = actions.concat([name]);
    }
}
