// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object AiSwitch, for `tools/check-panel.sh`
// (M6a.9): AI is on.

import QtQuick

QtObject {
    property bool enabled: true
}
