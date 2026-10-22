// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The Rules (M3.11, D-088): they sort new transactions into categories for
// you. The order matters, because the first Rule that fits decides (D-031),
// so each Rule has buttons to move it up or down. Rules inside a Rule are
// indented. At the top, KassenSturz offers a Rule for a payee you sorted
// into the same category by hand several times (D-071); that is plain
// counting, and works with AI off.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property RuleBook rules
    readonly property var rows: rules.view.rows ?? []
    readonly property var suggestions: rules.view.suggestions ?? []

    signal addRuleClicked()
    signal ruleClicked(string ruleId)
    signal createFromSuggestion(string payee, string category)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Rules")

    actions: [
        Kirigami.Action {
            icon.name: "list-add"
            text: page.i18nContext.i18nc("@action:button", "Add a Rule")
            onTriggered: page.addRuleClicked()
        }
    ]

    // What a Rule does, in a few words.
    function doesText(row: var): string {
        const does = [];
        if (row.bookToKind === "category") {
            does.push(i18nContext.i18nc("@info %1 is the name of a category", "Sorts into %1", row.bookTo));
        } else if (row.bookToKind === "transfer") {
            does.push(i18nContext.i18nc("@info %1 is the name of an account", "Moves the money to %1", row.bookTo));
        }
        if (row.payee !== "") {
            does.push(i18nContext.i18nc("@info %1 is a name", "Sets the payee to %1", row.payee));
        }
        return does.length > 0 ? does.join(", ") : i18nContext.i18nc("@info", "Leaves it as it is");
    }

    ColumnLayout {
        spacing: Kirigami.Units.smallSpacing

        Repeater {
            model: page.suggestions

            delegate: Kirigami.InlineMessage {
                id: suggestion

                required property var modelData

                Layout.fillWidth: true
                visible: true
                type: Kirigami.MessageType.Information
                text: page.i18nContext.i18ncp("@info %1 is a number, %2 a name, %3 a category", "You have sorted %2 into %3 by hand %1 time. Want KassenSturz to do it for you from now on?", "You have sorted %2 into %3 by hand %1 times. Want KassenSturz to do it for you from now on?", parseInt(modelData.count), modelData.payee, modelData.categoryPath)
                actions: [
                    Kirigami.Action {
                        icon.name: "list-add"
                        text: page.i18nContext.i18nc("@action:button", "Create a Rule")
                        onTriggered: page.createFromSuggestion(suggestion.modelData.payee, suggestion.modelData.category)
                    },
                    Kirigami.Action {
                        text: page.i18nContext.i18nc("@action:button", "No, thanks")
                        onTriggered: page.rules.dismissSuggestion(suggestion.modelData.payee, suggestion.modelData.category)
                    }
                ]
            }
        }

        Kirigami.PlaceholderMessage {
            Layout.fillWidth: true
            visible: page.rows.length === 0
            text: page.i18nContext.i18nc("@info", "You have no Rules yet")
            explanation: page.i18nContext.i18nc("@info", "A Rule sorts new transactions for you, for example everything from your supermarket into Groceries. You can also start one from a transaction.")
            helpfulAction: Kirigami.Action {
                icon.name: "list-add"
                text: page.i18nContext.i18nc("@action:button", "Add a Rule")
                onTriggered: page.addRuleClicked()
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.largeSpacing
            visible: page.rows.length > 1
            text: page.i18nContext.i18nc("@info", "The first Rule that fits a transaction decides. Use the arrows to change the order.")
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
        }

        Repeater {
            model: page.rows

            delegate: RowLayout {
                id: entry

                required property var modelData

                Layout.fillWidth: true
                spacing: 0

                Controls.ItemDelegate {
                    Layout.fillWidth: true
                    // Inside another Rule: indented one step per level.
                    leftPadding: Kirigami.Units.largeSpacing + entry.modelData.depth * Kirigami.Units.gridUnit * 1.5
                    contentItem: ColumnLayout {
                        spacing: 0

                        Controls.Label {
                            Layout.fillWidth: true
                            // A name could contain anything.
                            text: entry.modelData.name
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                        }

                        Controls.Label {
                            Layout.fillWidth: true
                            text: page.doesText(entry.modelData)
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                            opacity: 0.7
                            font: Kirigami.Theme.smallFont
                        }
                    }
                    Accessible.name: entry.modelData.name + ", " + page.doesText(entry.modelData)
                    onClicked: page.ruleClicked(entry.modelData.id)
                }

                Controls.ToolButton {
                    icon.name: "go-up"
                    enabled: entry.modelData.position > 0
                    text: page.i18nContext.i18nc("@action:button %1 is the name of a Rule", "Move %1 up", entry.modelData.name)
                    display: Controls.AbstractButton.IconOnly
                    onClicked: page.rules.moveRule(entry.modelData.id, true)
                    Controls.ToolTip.text: text
                    Controls.ToolTip.visible: hovered
                }

                Controls.ToolButton {
                    icon.name: "go-down"
                    enabled: entry.modelData.position < entry.modelData.siblings - 1
                    text: page.i18nContext.i18nc("@action:button %1 is the name of a Rule", "Move %1 down", entry.modelData.name)
                    display: Controls.AbstractButton.IconOnly
                    onClicked: page.rules.moveRule(entry.modelData.id, false)
                    Controls.ToolTip.text: text
                    Controls.ToolTip.visible: hovered
                }
            }
        }
    }
}
