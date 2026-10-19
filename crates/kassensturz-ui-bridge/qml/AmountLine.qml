// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A name and an amount on one line, e.g. a category and its total.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts

RowLayout {
    id: line

    // name and amount, both text.
    required property var modelData

    Accessible.role: Accessible.StaticText
    Accessible.name: modelData.name + ", " + modelData.amount

    Controls.Label {
        Layout.fillWidth: true
        text: line.modelData.name
        elide: Text.ElideRight
    }

    Controls.Label {
        text: line.modelData.amount
    }
}
