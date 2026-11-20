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

    // The dialogs, reachable by the smoke test (children of a page are redirected).
    readonly property var pauseDialogItem: pauseDialog
    readonly property var resumeDialogItem: resumeDialog

    // What the line under a budget's name says.
    function summary(budget: var): string {
        const plan = page.i18nContext.i18nc("@info %1 is how often a budget repeats, %2 an amount of money", "%1, %2", words.everyFrom(budget.every, budget.salaryDay ?? 0), budget.amount);
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
        // A paused budget (#1241): it has a last day and can go on.
        if (budget.canResume) {
            return budget.state === "ended"
                ? page.i18nContext.i18nc("@info %1 is a date", "Paused: it stopped counting on %1", budget.lastDay)
                : page.i18nContext.i18nc("@info %1 is a date", "Paused: it counts until %1, then stops", budget.lastDay);
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
            onTriggered: {
                ActionLog.taken("BudgetsPage_addBudget")
                page.addBudgetClicked()
            }
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
                    ActionLog.taken("BudgetsPage_deleteBudget")
                    removeDialog.close();
                    error.result = page.budgets.removeBudget(removeDialog.budgetId);
                }
            },
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button keep a budget", "Keep it")
                icon.name: "go-previous"
                onTriggered: {
                    ActionLog.taken("BudgetsPage_keep")
                    removeDialog.close()
                }
            }
        ]
    }

    // Pausing says what it does before it does it (#1241, D-909).
    Kirigami.PromptDialog {
        id: pauseDialog

        property string budgetId: ""
        property string budgetName: ""
        property string until: ""

        title: page.i18nContext.i18nc("@title:window", "Pause this budget?")
        subtitle: page.i18nContext.i18nc("@info %1 is the name of a category, %2 a date", "The budget for %1 counts until %2 and stops after that. Its past results stay. You can resume it later, and Undo takes the pause back.", Plain.plain(budgetName), until)
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button pause a budget", "Pause")
                icon.name: "media-playback-pause"
                onTriggered: {
                    ActionLog.taken("BudgetsPage_pauseBudget")
                    pauseDialog.close();
                    error.result = page.budgets.stopBudget(pauseDialog.budgetId);
                }
            },
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Cancel")
                icon.name: "dialog-cancel"
                onTriggered: {
                    ActionLog.taken("BudgetsPage_cancelPause")
                    pauseDialog.close()
                }
            }
        ]
    }

    // Resuming says what happens: the same budget goes on, or, once its
    // last day is past, a new one starts from this period (#1241).
    Kirigami.PromptDialog {
        id: resumeDialog

        property string budgetId: ""
        property string budgetName: ""
        // Empty when the same budget just goes on.
        property string from: ""

        title: page.i18nContext.i18nc("@title:window", "Resume this budget?")
        subtitle: from === ""
            ? page.i18nContext.i18nc("@info %1 is the name of a category", "The budget for %1 no longer has an end date and counts on after this period, as before. Undo takes it back.", Plain.plain(budgetName))
            : page.i18nContext.i18nc("@info %1 is the name of a category, %2 a date", "The budget for %1 starts again on %2 with the same categories and amount. The months in between are not counted, and what was left over before is not carried into it. Its old results stay under the paused budget, and Undo takes this back.", Plain.plain(budgetName), from)
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button resume a budget", "Resume")
                icon.name: "media-playback-start"
                onTriggered: {
                    ActionLog.taken("BudgetsPage_resumeBudget")
                    resumeDialog.close();
                    error.result = page.budgets.resumeBudget(resumeDialog.budgetId);
                }
            },
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Cancel")
                icon.name: "dialog-cancel"
                onTriggered: {
                    ActionLog.taken("BudgetsPage_cancelResume")
                    resumeDialog.close()
                }
            }
        ]
    }

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        // What a budget is, so it is never mixed up with a plan (B-388,
        // D-863): the Plans page explains plans.
        ExplanationCard {
            heading: page.i18nContext.i18nc("@title", "What is a budget?")
            explanation: page.i18nContext.i18nc("@info", "A limit for a category, for example: “I spend at most 300 € a month on food.” KassenSturz shows you how much of it is already spent. A budget doesn't change any transaction or total.")
        }

        Kirigami.PlaceholderMessage {
            Layout.fillWidth: true
            visible: page.budgetList.length === 0
            text: page.i18nContext.i18nc("@info", "You have no budgets yet")
            explanation: page.i18nContext.i18nc("@info", "Set an amount for a spending category, for example Groceries, and the Overview shows how much of it is left.")
            helpfulAction: Kirigami.Action {
                icon.name: "list-add"
                text: page.i18nContext.i18nc("@action:button", "Add a budget")
                onTriggered: {
                    ActionLog.taken("BudgetsPage_addBudget_2")
                    page.addBudgetClicked()
                }
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

                            // A budget with a name of its own still says
                            // which categories it counts (D-664).
                            Controls.Label {
                                Layout.fillWidth: true
                                visible: (card.modelData.ownName ?? "") !== ""
                                text: page.i18nContext.i18nc("@info %1 is a list of category names", "For %1", card.modelData.categoryNames ?? "")
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                                font: Kirigami.Theme.smallFont
                                opacity: 0.8
                            }

                            Controls.Label {
                                Layout.fillWidth: true
                                text: page.summary(card.modelData)
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                                font: Kirigami.Theme.smallFont
                                opacity: 0.8
                            }

                            // Only some accounts counted (D-639).
                            Controls.Label {
                                Layout.fillWidth: true
                                visible: (card.modelData.accounts ?? "") !== ""
                                text: page.i18nContext.i18nc("@info %1 is a list of account names", "Only money paid from %1", card.modelData.accounts ?? "")
                                textFormat: Text.PlainText
                                wrapMode: Text.Wrap
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
                            Accessible.name: page.i18nContext.i18nc("@action:button %1 is the name of a category", "Change the budget for %1", card.modelData.name)
                            onClicked: {
                                ActionLog.taken("BudgetsPage_change")
                                page.changeBudgetClicked(card.modelData.id)
                            }
                            Controls.ToolTip.text: text
                            Controls.ToolTip.visible: hovered
                            Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                        }

                        Controls.ToolButton {
                            icon.name: "media-playback-pause"
                            visible: card.modelData.canStop
                            text: page.i18nContext.i18nc("@action:button pause a budget after the current period", "Pause")
                            Accessible.name: page.i18nContext.i18nc("@action:button %1 is the name of a category", "Pause the budget for %1", card.modelData.name)
                            onClicked: {
                                ActionLog.taken("BudgetsPage_pause")
                                pauseDialog.budgetId = card.modelData.id;
                                pauseDialog.budgetName = card.modelData.name;
                                pauseDialog.until = card.modelData.pauseUntil;
                                pauseDialog.open();
                            }
                            Controls.ToolTip.text: page.i18nContext.i18nc("@info", "Stops counting after the current period. Its past results stay, and you can resume it later.")
                            Controls.ToolTip.visible: hovered
                            Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                        }

                        Controls.ToolButton {
                            icon.name: "media-playback-start"
                            visible: card.modelData.canResume
                            text: page.i18nContext.i18nc("@action:button resume a paused budget", "Resume")
                            Accessible.name: page.i18nContext.i18nc("@action:button %1 is the name of a category", "Resume the budget for %1", card.modelData.name)
                            onClicked: {
                                ActionLog.taken("BudgetsPage_resume")
                                resumeDialog.budgetId = card.modelData.id;
                                resumeDialog.budgetName = card.modelData.name;
                                resumeDialog.from = card.modelData.resumeFrom;
                                resumeDialog.open();
                            }
                            Controls.ToolTip.text: page.i18nContext.i18nc("@info", "Counts again from this period on. Its past results stay.")
                            Controls.ToolTip.visible: hovered
                            Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                        }

                        Controls.ToolButton {
                            icon.name: "edit-delete"
                            text: page.i18nContext.i18nc("@action:button delete a budget", "Delete")
                            Accessible.name: page.i18nContext.i18nc("@action:button %1 is the name of a category", "Delete the budget for %1", card.modelData.name)
                            onClicked: {
                                ActionLog.taken("BudgetsPage_delete")
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
