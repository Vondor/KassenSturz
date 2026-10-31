// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The Budgets page (M6c.5, D-357): what the user plans to spend on a
// category, with how much of it is spent and left in the current period. A
// budget never changes a transaction, a balance or a total; it is only a way
// of looking at the spending (D-344). *Stop* ends a budget after the current
// period and keeps its past; *Delete* removes it with its past results, says
// so first, and one Undo brings it back (D-345).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property BudgetBook budgets
    readonly property var budgetList: budgets.view.budgets ?? []

    signal addBudgetClicked()
    signal changeBudgetClicked(budgetId: string)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    BudgetWords {
        id: words
    }

    // What the line under a budget's name says.
    function summary(budget: var): string {
        const plan = page.i18nContext.i18nc("@info %1 is how often a budget repeats, %2 an amount of money", "%1, %2", words.every(budget.every), budget.amount);
        const carry = budget.every !== "range" && budget.carryOver
            ? " · " + page.i18nContext.i18nc("@info", "what is left is carried over")
            : "";
        return plan + carry;
    }

    // Where the budget stands in time, if it is not simply running.
    function stateText(budget: var): string {
        if (budget.state === "upcoming") {
            return page.i18nContext.i18nc("@info %1 is a date", "Starts %1", budget.firstDay);
        }
        if (budget.state === "ended") {
            return page.i18nContext.i18nc("@info %1 is a date", "Ended %1", budget.lastDay);
        }
        return budget.lastDay !== ""
            ? page.i18nContext.i18nc("@info %1 is a date", "Runs until %1", budget.lastDay)
            : "";
    }

    title: i18nContext.i18nc("@title", "Budgets")
    header: FormError {
        id: error
    }

    actions: [
        Kirigami.Action {
            icon.name: "list-add"
            text: page.i18nContext.i18nc("@action:button", "Add a budget")
            onTriggered: page.addBudgetClicked()
        }
    ]

    Kirigami.PromptDialog {
        id: removeDialog

        property string budgetId: ""
        property string budgetName: ""

        title: page.i18nContext.i18nc("@title:window", "Delete this budget?")
        subtitle: page.i18nContext.i18nc("@info %1 is the name of a category", "This deletes the budget for %1 with its past results. Your transactions and categories stay as they are, and Undo brings the budget back.", Plain.plain(budgetName))
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Delete the budget")
                icon.name: "edit-delete"
                onTriggered: {
                    removeDialog.close();
                    error.result = page.budgets.removeBudget(removeDialog.budgetId);
                }
            },
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button keep a budget", "Keep it")
                icon.name: "go-previous"
                onTriggered: removeDialog.close()
            }
        ]
    }

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        Controls.Label {
            Layout.fillWidth: true
            text: page.i18nContext.i18nc("@info", "A budget is what you plan to spend on a category in a month, a quarter, a year or some days. The bar shows how much of it is spent and how much is left. A budget doesn't change any transaction or total.")
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
        }

        Kirigami.PlaceholderMessage {
            Layout.fillWidth: true
            visible: page.budgetList.length === 0
            text: page.i18nContext.i18nc("@info", "You have no budgets yet")
            explanation: page.i18nContext.i18nc("@info", "Set an amount for a spending category, for example Groceries, and the Overview shows how much of it is left.")
            helpfulAction: Kirigami.Action {
                icon.name: "list-add"
                text: page.i18nContext.i18nc("@action:button", "Add a budget")
                onTriggered: page.addBudgetClicked()
            }
        }

        Repeater {
            model: page.budgetList

            delegate: Kirigami.AbstractCard {
                id: card

                required property var modelData

                Layout.fillWidth: true

                contentItem: ColumnLayout {
                    spacing: Kirigami.Units.smallSpacing

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 0

                            Controls.Label {
                                Layout.fillWidth: true
                                text: card.modelData.name
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                                font.bold: true
                            }

                            Controls.Label {
                                Layout.fillWidth: true
                                text: page.summary(card.modelData)
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                                font: Kirigami.Theme.smallFont
                                opacity: 0.8
                            }

                            Controls.Label {
                                Layout.fillWidth: true
                                visible: text !== ""
                                text: page.stateText(card.modelData)
                                textFormat: Text.PlainText
                                font: Kirigami.Theme.smallFont
                                opacity: 0.8
                            }
                        }

                        Controls.ToolButton {
                            icon.name: "document-edit"
                            text: page.i18nContext.i18nc("@action:button change a budget", "Change")
                            display: Controls.AbstractButton.IconOnly
                            Accessible.name: page.i18nContext.i18nc("@action:button %1 is the name of a category", "Change the budget for %1", card.modelData.name)
                            onClicked: page.changeBudgetClicked(card.modelData.id)
                            Controls.ToolTip.text: text
                            Controls.ToolTip.visible: hovered
                            Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                        }

                        Controls.ToolButton {
                            icon.name: "media-playback-stop"
                            visible: card.modelData.canStop
                            text: page.i18nContext.i18nc("@action:button end a budget after the current period", "Stop")
                            display: Controls.AbstractButton.IconOnly
                            Accessible.name: page.i18nContext.i18nc("@action:button %1 is the name of a category", "Stop the budget for %1", card.modelData.name)
                            onClicked: error.result = page.budgets.stopBudget(card.modelData.id)
                            Controls.ToolTip.text: page.i18nContext.i18nc("@info", "Ends the budget after the current period. Its past results stay.")
                            Controls.ToolTip.visible: hovered
                            Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                        }

                        Controls.ToolButton {
                            icon.name: "edit-delete"
                            text: page.i18nContext.i18nc("@action:button delete a budget", "Delete")
                            display: Controls.AbstractButton.IconOnly
                            Accessible.name: page.i18nContext.i18nc("@action:button %1 is the name of a category", "Delete the budget for %1", card.modelData.name)
                            onClicked: {
                                removeDialog.budgetId = card.modelData.id;
                                removeDialog.budgetName = card.modelData.name;
                                removeDialog.open();
                            }
                            Controls.ToolTip.text: page.i18nContext.i18nc("@info", "Deletes the budget with its past results. Undo brings it back.")
                            Controls.ToolTip.visible: hovered
                            Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                        }
                    }

                    BudgetBars {
                        Layout.fillWidth: true
                        visible: card.modelData.bar !== undefined
                        shown: card.modelData.bar !== undefined ? [card.modelData.bar] : []
                        budgetCount: 1
                    }
                }
            }
        }
    }
}
