// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The period header widget (M6d.8, D-452, D-474): the month the page shows,
// with the way to the months before and after it and back to this one. Until
// M6d.8 it was a fixed row above the Overview's grid; now it is a widget like
// the others, so the user can move it, make it narrower or take it off. A
// page without it shows this month (the page sees to that). The month and
// what is possible come from Rust (`BookOverview`); this file shows them.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

ColumnLayout {
    id: header

    // A `BookOverview` (or anything with its `view` and the three `show…`
    // functions).
    required property var overview
    readonly property var view: overview?.view ?? ({})
    // While the page is arranged the buttons are a picture of themselves.
    property bool arranging: false
    // GridItem reads it for the page's problem count.
    readonly property bool failed: false

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    spacing: 0

    RowLayout {
        Layout.fillWidth: true
        spacing: Kirigami.Units.smallSpacing

        Controls.ToolButton {
            icon.name: "go-previous"
            text: header.i18nContext.i18nc("@action:button the month before", "Earlier month")
            display: Controls.AbstractButton.IconOnly
            enabled: !header.arranging && (header.view.hasEarlier ?? false)
            onClicked: {
                ActionLog.taken("PeriodHeader_earlierMonth")
                header.overview.showEarlierMonth()
            }

            Controls.ToolTip.text: text
            Controls.ToolTip.visible: hovered
            Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
        }

        Kirigami.Heading {
            Layout.fillWidth: true
            textFormat: Text.PlainText
            text: header.view.month ?? ""
            level: 2
            horizontalAlignment: Text.AlignHCenter
            elide: Text.ElideRight
            Accessible.role: Accessible.Heading
            Accessible.name: text
        }

        Controls.ToolButton {
            icon.name: "go-next"
            text: header.i18nContext.i18nc("@action:button the month after", "Later month")
            display: Controls.AbstractButton.IconOnly
            enabled: !header.arranging && (header.view.hasLater ?? false)
            onClicked: {
                ActionLog.taken("PeriodHeader_laterMonth")
                header.overview.showLaterMonth()
            }

            Controls.ToolTip.text: text
            Controls.ToolTip.visible: hovered
            Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
        }

        // In the same row, so the widget stays one row high.
        Controls.ToolButton {
            visible: !(header.view.thisMonth ?? true)
            enabled: !header.arranging
            icon.name: "go-jump-today"
            text: header.i18nContext.i18nc("@action:button", "Back to this month")
            display: header.width > Kirigami.Units.gridUnit * 20 ? Controls.AbstractButton.TextBesideIcon : Controls.AbstractButton.IconOnly
            onClicked: {
                ActionLog.taken("PeriodHeader_backMonth")
                header.overview.showThisMonth()
            }

            Controls.ToolTip.text: text
            Controls.ToolTip.visible: hovered && display === Controls.AbstractButton.IconOnly
            Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
        }
    }

    Controls.Label {
        Layout.fillWidth: true
        visible: header.view.empty ?? false
        text: header.i18nContext.i18nc("@info", "Nothing came in or went out this month yet. Add what you paid or received, and it shows up here.")
        horizontalAlignment: Text.AlignHCenter
        font: Kirigami.Theme.smallFont
        wrapMode: Text.Wrap
        elide: Text.ElideRight
        maximumLineCount: 2
    }
}
