// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// What the warnings tile shows (US4.9, D-721): one line for each rule that
// applies today, is about something removed or can't be checked, then how
// many more are fine; "All fine" when none applies; and a hint when the tile
// has no rule yet. The rows come from `BookAlarms` as text (D-065); the
// words are in AlarmWords.qml.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n

ColumnLayout {
    id: list

    // `BookAlarms.view`.
    property var shown: ({})
    // Whether the layout is being arranged.
    property bool arranging: false

    // A line about a budget was clicked: the screen opens the budgets.
    signal budgetClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    readonly property var rows: shown?.rows ?? []
    readonly property var lines: rows.filter(row => row.state !== "fine")
    readonly property int fine: rows.length - lines.length

    spacing: Kirigami.Units.smallSpacing

    AlarmWords {
        id: words
    }

    // No rule yet.
    Controls.Label {
        Layout.fillWidth: true
        visible: list.rows.length === 0 && list.shown?.error === undefined
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        color: Kirigami.Theme.disabledTextColor
        text: list.i18nContext.i18nc("@info", "No warnings set yet. Add one in this tile's settings, for example: tell me when the shared account falls under 500.")
    }

    // The rules could not be read.
    Controls.Label {
        Layout.fillWidth: true
        visible: list.shown?.error !== undefined
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        text: list.i18nContext.i18nc("@info", "The warnings can't be checked right now. Open the tile's settings to check its rules, or try again after reopening the book.")
    }

    // Rules, none of them a line of its own.
    RowLayout {
        Layout.fillWidth: true
        visible: list.rows.length > 0 && list.lines.length === 0

        Kirigami.Icon {
            source: "checkmark"
            color: Kirigami.Theme.positiveTextColor
            implicitWidth: Kirigami.Units.iconSizes.small
            implicitHeight: implicitWidth
            Accessible.ignored: true
        }

        Controls.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: list.rows.length === 1
                ? list.i18nContext.i18nc("@info", "All fine. Your warning doesn't apply today.")
                : list.i18nContext.i18nc("@info %1 is how many warnings, always more than one", "All fine. None of your %1 warnings applies today.", list.rows.length)
        }
    }

    Repeater {
        model: list.lines

        delegate: Controls.ItemDelegate {
            id: line

            required property var modelData

            readonly property bool aboutBudget: modelData.reason === "budget_over" || modelData.reason === "budget_under"

            Layout.fillWidth: true
            // Only a line about a budget leads somewhere: the budgets.
            enabled: aboutBudget && modelData.state === "triggered"
            Accessible.name: words.sentence(modelData)
            onClicked: {
                ActionLog.taken("AlarmList_openBudgets")
                list.budgetClicked();
            }

            contentItem: RowLayout {
                spacing: Kirigami.Units.smallSpacing

                Kirigami.Icon {
                    Layout.alignment: Qt.AlignTop
                    source: line.modelData.state === "triggered" ? "dialog-warning" : "dialog-information"
                    color: line.modelData.state === "triggered" ? Kirigami.Theme.neutralTextColor : Kirigami.Theme.textColor
                    implicitWidth: Kirigami.Units.iconSizes.small
                    implicitHeight: implicitWidth
                    Accessible.ignored: true
                }

                Controls.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    // A disabled delegate greys its text; the sentence stays readable.
                    color: Kirigami.Theme.textColor
                    text: words.sentence(line.modelData)
                }
            }
        }
    }

    // How many more are set and fine.
    Controls.Label {
        Layout.fillWidth: true
        visible: list.lines.length > 0 && list.fine > 0
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        font: Kirigami.Theme.smallFont
        opacity: 0.75
        text: list.i18nContext.i18ncp("@info %1 is how many", "%1 more warning is set and fine.", "%1 more warnings are set and fine.", list.fine)
    }
}
