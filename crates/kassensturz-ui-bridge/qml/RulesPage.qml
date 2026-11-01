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

Kirigami.Page {
    id: page

    required property RuleBook rules
    readonly property var rows: rules.view.rows ?? []
    readonly property var suggestions: rules.view.suggestions ?? []

    // The Rule whose transactions are shown, by ID; the first one to start
    // with. A Rule that is gone shows nothing.
    property string selectedId: ""
    // Many suggestions would push the Rules off the page: from two on they
    // are folded behind one line until the user asks to see them.
    property bool suggestionsOpen: false
    readonly property var selected: rows.find(row => row.id === selectedId) ?? null
    // What the selected Rule matches now, newest first. Read again whenever
    // the Rules or what they act on change, which refreshes `rules.view`.
    readonly property var matchRows: {
        rules.view;
        return selected === null ? [] : rules.matches(selected.id);
    }
    // Side by side where there is room, one above the other otherwise.
    readonly property bool wide: width >= Kirigami.Units.gridUnit * 45

    signal addRuleClicked()
    signal ruleClicked(string ruleId)
    signal createFromSuggestion(string payee, string category)
    // Open a transaction, seen from the account with `accountId`.
    signal transactionClicked(string accountId, string transactionId)

    Component.onCompleted: if (rows.length > 0) selectedId = rows[0].id

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Rules")

    actions: [
        Kirigami.Action {
            icon.name: "list-add"
            text: page.i18nContext.i18nc("@action:button", "Add a Rule")
            onTriggered: {
                ActionLog.taken("RulesPage_addRule")
                page.addRuleClicked()
            }
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
            does.push(i18nContext.i18nc("@info %1 is a name", "Shows the name %1 as From or To", row.payee));
        }
        return does.length > 0 ? does.join(", ") : i18nContext.i18nc("@info", "Leaves it as it is");
    }

    // Rules on one side, what the selected one matches on the other: click
    // through the Rules and see their transactions.
    GridLayout {
        anchors.fill: parent
        // The list, the line between, and the transactions: three cells side
        // by side, one under the other on a narrow page.
        columns: page.wide ? 3 : 1
        columnSpacing: 0
        rowSpacing: 0

        Controls.ScrollView {
            id: master

            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: Kirigami.Units.gridUnit * 22
            contentWidth: availableWidth

            ColumnLayout {
                width: master.availableWidth
                spacing: Kirigami.Units.smallSpacing

                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: page.suggestions.length > 1
                    type: Kirigami.MessageType.Information
                    text: page.i18nContext.i18ncp("@info %1 is a number", "KassenSturz has %1 suggestion for a Rule.", "KassenSturz has %1 suggestions for Rules.", page.suggestions.length)
                    actions: [
                        Kirigami.Action {
                            icon.name: page.suggestionsOpen ? "go-up" : "go-down"
                            text: page.suggestionsOpen
                                ? page.i18nContext.i18nc("@action:button hide the list of suggested Rules", "Hide them")
                                : page.i18nContext.i18nc("@action:button show the list of suggested Rules", "Show them")
                            onTriggered: {
                                ActionLog.taken("RulesPage_suggestionsOpen")
                                page.suggestionsOpen = !page.suggestionsOpen
                            }
                        }
                    ]
                }

                Repeater {
                    model: page.suggestions.length > 1 && !page.suggestionsOpen ? [] : page.suggestions

                    delegate: Kirigami.InlineMessage {
                        id: suggestion

                        required property var modelData

                        Layout.fillWidth: true
                        visible: true
                        type: Kirigami.MessageType.Information
                        text: Plain.plain(page.i18nContext.i18ncp("@info %1 is a number, %2 a name, %3 a category", "You have sorted %2 into %3 by hand %1 time. Want KassenSturz to do it for you from now on?", "You have sorted %2 into %3 by hand %1 times. Want KassenSturz to do it for you from now on?", parseInt(modelData.count), modelData.payee, modelData.categoryPath))
                        actions: [
                            Kirigami.Action {
                                icon.name: "list-add"
                                text: page.i18nContext.i18nc("@action:button", "Create a Rule")
                                onTriggered: {
                                    ActionLog.taken("RulesPage_createRule")
                                    page.createFromSuggestion(suggestion.modelData.payee, suggestion.modelData.category)
                                }
                            },
                            Kirigami.Action {
                                text: page.i18nContext.i18nc("@action:button", "No, thanks")
                                onTriggered: {
                                    ActionLog.taken("RulesPage_noThanks")
                                    page.rules.dismissSuggestion(suggestion.modelData.payee, suggestion.modelData.category)
                                }
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
                        onTriggered: {
                            ActionLog.taken("RulesPage_addRule_2")
                            page.addRuleClicked()
                        }
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
                            // A click shows what the Rule matches; the pencil opens it.
                            highlighted: entry.modelData.id === page.selectedId
                            onClicked: {
                                ActionLog.taken("RulesPage_selectedId")
                                page.selectedId = entry.modelData.id
                            }
                        }

                        Controls.ToolButton {
                            icon.name: "document-edit"
                            text: page.i18nContext.i18nc("@action:button %1 is the name of a Rule", "Edit %1", entry.modelData.name)
                            display: Controls.AbstractButton.IconOnly
                            onClicked: {
                                ActionLog.taken("RulesPage_edit")
                                page.ruleClicked(entry.modelData.id)
                            }
                            Controls.ToolTip.text: text
                            Controls.ToolTip.visible: hovered
                        }

                        Controls.ToolButton {
                            icon.name: "go-up"
                            enabled: entry.modelData.position > 0
                            text: page.i18nContext.i18nc("@action:button %1 is the name of a Rule", "Move %1 up", entry.modelData.name)
                            display: Controls.AbstractButton.IconOnly
                            onClicked: {
                                ActionLog.taken("RulesPage_moveUp")
                                page.rules.moveRule(entry.modelData.id, true)
                            }
                            Controls.ToolTip.text: text
                            Controls.ToolTip.visible: hovered
                        }

                        Controls.ToolButton {
                            icon.name: "go-down"
                            enabled: entry.modelData.position < entry.modelData.siblings - 1
                            text: page.i18nContext.i18nc("@action:button %1 is the name of a Rule", "Move %1 down", entry.modelData.name)
                            display: Controls.AbstractButton.IconOnly
                            onClicked: {
                                ActionLog.taken("RulesPage_moveDown")
                                page.rules.moveRule(entry.modelData.id, false)
                            }
                            Controls.ToolTip.text: text
                            Controls.ToolTip.visible: hovered
                        }
                    }
                }
            }
        }

        Kirigami.Separator {
            Layout.fillWidth: !page.wide
            Layout.fillHeight: page.wide
        }

        ColumnLayout {
            id: detail

            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: Kirigami.Units.gridUnit * 28
            spacing: 0

            Kirigami.PlaceholderMessage {
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.margins: Kirigami.Units.largeSpacing
                visible: page.selected === null
                text: page.i18nContext.i18nc("@info", "Pick a Rule to see its transactions")
                explanation: page.i18nContext.i18nc("@info", "Click a Rule in the list, and the transactions it fits show up here.")
            }

            RowLayout {
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.largeSpacing
                visible: page.selected !== null
                spacing: Kirigami.Units.largeSpacing

                Kirigami.Heading {
                    Layout.fillWidth: true
                    level: 3
                    // A name could contain anything.
                    text: page.selected === null
                        ? ""
                        : page.i18nContext.i18ncp("@title %2 is the name of a Rule, %1 how many transactions it fits", "%2 fits %1 transaction", "%2 fits %1 transactions", page.matchRows.length, page.selected.name)
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }

                Controls.Button {
                    icon.name: "document-edit"
                    text: page.i18nContext.i18nc("@action:button", "Edit this Rule")
                    onClicked: {
                        ActionLog.taken("RulesPage_editRule")
                        page.ruleClicked(page.selected.id)
                    }
                }
            }

            Controls.Label {
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.largeSpacing
                visible: page.selected !== null
                text: page.i18nContext.i18nc("@info", "These are the transactions this Rule fits now, whether or not it sorted them, except the ones an earlier Rule takes first.")
                wrapMode: Text.Wrap
                font: Kirigami.Theme.smallFont
            }

            Kirigami.PlaceholderMessage {
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.largeSpacing
                visible: page.selected !== null && page.matchRows.length === 0
                text: page.i18nContext.i18nc("@info", "No transactions fit this Rule yet")
            }

            ListView {
                id: matchList

                Layout.fillWidth: true
                Layout.fillHeight: true
                visible: page.selected !== null && page.matchRows.length > 0
                clip: true
                model: page.matchRows

                Controls.ScrollBar.vertical: Controls.ScrollBar {}

                delegate: Controls.ItemDelegate {
                    id: matchRow

                    required property var modelData
                    required property int index

                    width: ListView.view.width
                    Accessible.name: transaction.Accessible.name
                    onClicked: {
                        ActionLog.taken("RulesPage_transactionClicked")
                        page.transactionClicked(modelData.accountId, modelData.id)
                    }

                    background: Rectangle {
                        readonly property bool lit: matchRow.down || matchRow.hovered || matchRow.visualFocus

                        color: lit
                            ? Kirigami.Theme.highlightColor
                            : matchRow.index % 2 === 1 ? Kirigami.Theme.alternateBackgroundColor : "transparent"
                        opacity: lit ? 0.3 : 1
                    }

                    contentItem: TransactionRow {
                        id: transaction

                        date: matchRow.modelData.date
                        otherParty: matchRow.modelData.otherParty
                        category: matchRow.modelData.category
                        amount: matchRow.modelData.amount
                        exactAmount: matchRow.modelData.exactAmount
                        incoming: matchRow.modelData.incoming
                        kind: matchRow.modelData.kind
                        account: matchRow.modelData.account
                    }
                }
            }
        }
    }
}
