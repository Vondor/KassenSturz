// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A name and an amount on one line, e.g. a category and its total.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts

RowLayout {
    id: line

    // name, amount and exactAmount, all text.
    required property var modelData
    // The name shown, if it isn't the model's as it is, e.g. one the UI
    // translates.
    property string name: modelData.name

    Accessible.role: Accessible.StaticText
    Accessible.name: name + ", " + modelData.exactAmount

    Controls.Label {
        Layout.fillWidth: true
        text: line.name
        elide: Text.ElideRight
    }

    AmountLabel {
        text: line.modelData.amount
        exactText: line.modelData.exactAmount
    }
}
