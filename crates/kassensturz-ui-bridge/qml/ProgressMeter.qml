// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A bar that fills from 0 to 100 %, with its figure written out beside it
// (M6a.5). The fill is only a picture: `valueText` is what is true, and
// screen readers get it too (D-283).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

ColumnLayout {
    id: meter

    // What the bar is about, e.g. "Share you kept".
    property string title
    // The figure as the user's number format writes it, e.g. "65,9 %".
    property string valueText
    // How full the bar is, 0 to 1000. A number that places the fill and is
    // never shown.
    property int permille: 0
    // Whether there is a figure to show.
    property bool hasValue: true
    // Whether the figure went past what the bar stands for, e.g. a budget
    // spent beyond what was available: the fill takes the color of a warning.
    property bool over: false

    spacing: Kirigami.Units.smallSpacing
    Accessible.role: Accessible.ProgressBar
    Accessible.name: title
    Accessible.description: valueText

    RowLayout {
        Layout.fillWidth: true

        Controls.Label {
            Layout.fillWidth: true
            text: meter.title
            elide: Text.ElideRight
            textFormat: Text.PlainText
        }

        Controls.Label {
            text: meter.valueText
            font.bold: true
            textFormat: Text.PlainText
        }
    }

    Rectangle {
        Layout.fillWidth: true
        implicitHeight: Kirigami.Units.smallSpacing * 2
        radius: height / 2
        color: Kirigami.Theme.alternateBackgroundColor
        border.color: Kirigami.Theme.disabledTextColor
        border.width: 1
        visible: meter.hasValue

        Rectangle {
            width: parent.width * Math.max(0, Math.min(1000, meter.permille)) / 1000
            height: parent.height
            radius: parent.radius
            color: meter.over ? Kirigami.Theme.neutralTextColor : Kirigami.Theme.highlightColor
        }
    }
}
