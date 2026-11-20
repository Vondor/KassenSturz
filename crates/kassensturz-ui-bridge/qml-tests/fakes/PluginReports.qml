// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Stands in for the Rust PluginReports in the QML checks (#791 step 5,
// D-926): no report plugin was added, so there are no reports to list or
// run.

import QtQuick

QtObject {
    function reports() { return []; }
    function run(file, id, account, period) { return {code: "plugin-host.start-failed"}; }
}
