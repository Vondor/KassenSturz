// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One bar per budget (M6c.5, D-357, D-380): how much of the planned amount
// is spent and how much is left in the budget's period. It is the
// `budget.progress` component on the Overview and on dashboards, and the
// list of the Budgets page. The bars come from `BookBudgets` or `BudgetBook`
// as text (D-065); the fill only places a mark and the figures beside it are
// what is true (D-283). The words are plain and never advice (AGENTS.md
// §3.1 rule 11).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n

ColumnLayout {
    id: list

    // The bars, as `BookBudgets.view.bars` gives them.
    property var shown: []
    // How many budgets the book has at all.
    property int budgetCount: 0
    // Whether the layout is being arranged: then an empty component says
    // what it is and offers *Set a budget*.
    property bool arranging: false

    // The user wants to set a budget (from the empty component).
    signal setBudgetClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The note of a bar with this fixed word, if it has one (D-380).
    function noteOf(bar: var, word: string): var {
        return (bar.notes ?? []).find(note => note.word === word) ?? null;
    }

    function noteText(bar: var, note: var): string {
        switch (note.word) {
        case "carried-in":
            return note.negative
                ? i18nContext.i18nc("@info %1 is an amount of money", "%1 less to spend, because more was spent in the period before.", note.amount)
                : i18nContext.i18nc("@info %1 is an amount of money", "%1 carried over from the period before.", note.amount);
        case "nothing-to-spend":
            return i18nContext.i18nc("@info", "Nothing is available to spend in this period, so there is no share to show.");
        default:
            return "";
        }
    }

    spacing: Kirigami.Units.largeSpacing

    // A book with no budget: nothing at all, except while the layout is
    // arranged (D-357, Q-264).
    Kirigami.PlaceholderMessage {
        Layout.fillWidth: true
        visible: list.budgetCount === 0 && list.arranging
        text: list.i18nContext.i18nc("@info", "Your budgets appear here once you set one.")
        helpfulAction: Kirigami.Action {
            icon.name: "list-add"
            text: list.i18nContext.i18nc("@action:button", "Set a budget")
            onTriggered: {
                ActionLog.taken("BudgetBars_setBudget")
                list.setBudgetClicked()
            }
        }
    }

    Controls.Label {
        Layout.fillWidth: true
        visible: list.budgetCount > 0 && list.shown.length === 0
        text: list.i18nContext.i18nc("@info", "None of your budgets covers these days.")
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        color: Kirigami.Theme.disabledTextColor
    }

    Repeater {
        model: list.shown

        delegate: ColumnLayout {
            id: entry

            required property var modelData
            readonly property var over: list.noteOf(modelData, "over")

            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            ProgressMeter {
                Layout.fillWidth: true
                title: entry.modelData.name
                hasValue: entry.modelData.hasShare
                over: entry.modelData.over
                permille: entry.modelData.permille
                valueText: entry.modelData.hasShare
                    ? list.i18nContext.i18nc("@info %1 is the amount spent, %2 the amount available, %3 the share used in percent", "%1 of %2 (%3)", entry.modelData.spent, entry.modelData.available, entry.modelData.used)
                    : list.i18nContext.i18nc("@info %1 is the amount spent", "%1 spent", entry.modelData.spent)
            }

            Controls.Label {
                Layout.fillWidth: true
                font: Kirigami.Theme.smallFont
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: entry.over !== null
                    ? list.i18nContext.i18nc("@info %1 is an amount of money", "%1 over", entry.over.amount)
                    : list.i18nContext.i18nc("@info %1 is an amount of money", "%1 left", entry.modelData.left)
                color: entry.over !== null ? Kirigami.Theme.neutralTextColor : Kirigami.Theme.textColor
            }

            Controls.Label {
                Layout.fillWidth: true
                font: Kirigami.Theme.smallFont
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                opacity: 0.8
                text: entry.modelData.running
                    ? list.i18nContext.i18nc("@info %1 and %2 are dates, %3 a date", "%1 to %2. Spending counted up to %3.", entry.modelData.firstDay, entry.modelData.lastDay, entry.modelData.countedTo)
                    : list.i18nContext.i18nc("@info %1 and %2 are dates", "%1 to %2", entry.modelData.firstDay, entry.modelData.lastDay)
            }

            Repeater {
                model: entry.modelData.notes.filter(note => note.word !== "over")

                delegate: Controls.Label {
                    required property var modelData

                    Layout.fillWidth: true
                    font: Kirigami.Theme.smallFont
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    opacity: 0.8
                    text: list.noteText(entry.modelData, modelData)
                }
            }
        }
    }
}
