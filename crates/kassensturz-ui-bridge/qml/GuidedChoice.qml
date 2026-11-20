// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One choice on a page of the guided setup (#1031, docs/GUIDED_SETUP.md): an
// icon, a heading and a line that says what clicking it does. A page that
// offers one of several marks the one that holds now with `selected` and can
// say so in a few words next to the heading (`badge`).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Controls.AbstractButton {
    id: choice

    property string iconName
    property string heading
    property string line
    // Whether this is the choice that holds now.
    property bool selected: false
    // A few words next to the heading, or none.
    property string badge

    Layout.fillWidth: true
    Accessible.name: badge === "" ? heading : heading + ", " + badge
    Accessible.description: line
    implicitHeight: content.implicitHeight + Kirigami.Units.largeSpacing * 2
    background: Rectangle {
        radius: Kirigami.Units.cornerRadius
        color: choice.hovered || choice.visualFocus || choice.selected ? Qt.alpha(Kirigami.Theme.highlightColor, 0.12) : Kirigami.Theme.backgroundColor
        border.color: choice.visualFocus || choice.selected ? Kirigami.Theme.highlightColor : Qt.alpha(Kirigami.Theme.textColor, 0.2)
        border.width: choice.visualFocus ? 2 : 1
    }
    contentItem: RowLayout {
        id: content

        spacing: Kirigami.Units.largeSpacing

        Kirigami.Icon {
            Layout.leftMargin: Kirigami.Units.largeSpacing
            implicitWidth: Kirigami.Units.iconSizes.large
            implicitHeight: Kirigami.Units.iconSizes.large
            source: choice.iconName
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.rightMargin: Kirigami.Units.largeSpacing
            spacing: 2

            Flow {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing

                Kirigami.Heading {
                    width: Math.min(implicitWidth, parent.width)
                    level: 4
                    text: choice.heading
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }

                Rectangle {
                    visible: choice.badge !== ""
                    width: Math.min(badgeLabel.implicitWidth + Kirigami.Units.largeSpacing, parent.width)
                    height: badgeLabel.implicitHeight + 2
                    radius: Kirigami.Units.cornerRadius
                    color: Qt.alpha(Kirigami.Theme.positiveTextColor, 0.2)

                    Controls.Label {
                        id: badgeLabel

                        anchors.fill: parent
                        horizontalAlignment: Text.AlignHCenter
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                        text: choice.badge
                        textFormat: Text.PlainText
                    }
                }
            }

            Controls.Label {
                Layout.fillWidth: true
                text: choice.line
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                opacity: 0.8
            }
        }
    }
}
