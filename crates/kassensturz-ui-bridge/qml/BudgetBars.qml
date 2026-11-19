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

    // Whether the empty component offers *Set a budget*: only where something
    // is listening, so a preview shows no button that does nothing (B-412).
    property bool offersSet: true

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
        case "counts-changed":
            return i18nContext.i18nc("@info", "From this period on, the budget counts other categories or accounts.");
        default:
            return "";
        }
    }

    // Whether `point` (in this item) lies on a button that works while the
    // layout is arranged too: there the grid lets the pointer through to it,
    // where it otherwise selects and drags the tile (B-412, D-904).
    function liveAt(point: point): bool {
        return setButton.visible && setButton.contains(mapToItem(setButton, point));
    }

    spacing: Kirigami.Units.largeSpacing

    // A book with no budget: what the bars are and how to get them, also
    // after the layout is arranged, so a tile the user just added is still
    // there to be filled (B-412, D-904, replacing "nothing at all" of D-357).
    // The note wraps, so no window width or text size cuts it off.
    Controls.Label {
        Layout.fillWidth: true
        visible: list.budgetCount === 0
        text: list.i18nContext.i18nc("@info", "Your budgets appear here once you set one.")
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
    }

    Controls.Button {
        id: setButton

        visible: list.budgetCount === 0 && list.offersSet
        icon.name: "list-add"
        text: list.i18nContext.i18nc("@action:button", "Set a budget")
        onClicked: {
            ActionLog.taken("BudgetBars_setBudget")
            list.setBudgetClicked()
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
                // A budget that has not started yet shows an empty bar with
                // what it plans left; one that has ended its last period
                // (D-710).
                text: entry.modelData.startsLater
                    ? list.i18nContext.i18nc("@info %1 and %2 are the first and last day of a budget that has not started yet", "%1 to %2. Starts on %1, nothing spent yet.", entry.modelData.firstDay, entry.modelData.lastDay)
                    : entry.modelData.ended
                    ? list.i18nContext.i18nc("@info %1 and %2 are the first and last day of the last period of a budget that has ended", "%1 to %2. Ended on %2.", entry.modelData.firstDay, entry.modelData.lastDay)
                    : entry.modelData.running
                    ? list.i18nContext.i18nc("@info %1 and %2 are dates, %3 a date", "%1 to %2. Spending counted up to %3.", entry.modelData.firstDay, entry.modelData.lastDay, entry.modelData.countedTo)
                    : list.i18nContext.i18nc("@info %1 and %2 are dates", "%1 to %2", entry.modelData.firstDay, entry.modelData.lastDay)
            }

            // A budget may count only some of the user's accounts (D-639).
            Controls.Label {
                Layout.fillWidth: true
                visible: (entry.modelData.accounts ?? "") !== ""
                font: Kirigami.Theme.smallFont
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                opacity: 0.8
                text: list.i18nContext.i18nc("@info %1 is a list of account names", "Only money paid from %1", entry.modelData.accounts ?? "")
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
