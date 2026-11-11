// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One budget the AI prepared, new or changed, as the review page lists it
// (M6c.11, D-358, D-395): its name, if it has one (D-664), the category, how
// often it starts again, the first period it applies to, the amount,
// whether it carries over and, for an amount KassenSturz computed, how it
// came about. A change of only the name says that no amount changes. A
// change of what a budget counts shows the categories and accounts before
// and after, and that earlier periods keep counting what they counted
// (D-690). Every text from the model or the book is plain text (T-08).

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.AbstractCard {
    id: card

    required property AiSwitch ai
    // The change object the bridge made (`newBudget` or `editBudget`).
    required property var change
    // The review page, for leaving the change out.
    required property var review
    required property var choices

    visible: ai.enabled

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    readonly property bool isNew: change.kind === "newBudget"
    // A change of the name only leaves every amount as it was (D-664).
    readonly property bool changesAmount: isNew || (change.changesAmount ?? true)
    // What it counts changes from the period of the change on (D-690).
    readonly property bool changesCounts: !isNew && (change.changesCounts ?? false)
    readonly property string ownName: change.name ?? ""

    function everyText(every: string): string {
        switch (every) {
        case "month":
            return i18nContext.i18nc("@info how often a budget starts again", "Every month");
        case "salary-month":
            // A month from a salary day, the same words as on the budget
            // page (D-651).
            return i18nContext.i18nc("@info how often a budget repeats; %1 is a day of the month, 1 to 31", "Every salary month, from day %1", change.salaryDay ?? 1);
        case "quarter":
            return i18nContext.i18nc("@info how often a budget starts again", "Every quarter");
        case "year":
            return i18nContext.i18nc("@info how often a budget starts again", "Every year");
        }
        return i18nContext.i18nc("@info how often a budget starts again", "Once, for some days");
    }

    // How the amount came about, in words, or "" for one the user named.
    // What a budget counts, in words: categories and, when chosen, accounts.
    function countsText(now: bool, categories: string, accounts: string): string {
        if (now) {
            return accounts === ""
                ? i18nContext.i18nc("@info %1 is a list of categories", "Now it counts %1.", categories)
                : i18nContext.i18nc("@info %1 is a list of categories, %2 a list of account names", "Now it counts %1, only money paid from %2.", categories, accounts);
        }
        return accounts === ""
            ? i18nContext.i18nc("@info %1 is a list of categories", "From this change on it counts %1.", categories)
            : i18nContext.i18nc("@info %1 is a list of categories, %2 a list of account names", "From this change on it counts %1, only money paid from %2.", categories, accounts);
    }

    function basisText(): string {
        const periods = change.basisPeriods;
        if (periods === 0) {
            return "";
        }
        if (periods === 1) {
            return i18nContext.i18nc("@info %1 and %2 are dates, %3 an amount, %4 the step it is rounded up to", "Based on what was spent in the last full period, %1 to %2: %3, rounded up to a multiple of %4.", change.basisFrom, change.basisTo, change.basisSpent, change.basisStep);
        }
        return i18nContext.i18ncp("@info %1 is a number of periods, %2 and %3 are dates, %4 and %5 amounts, %6 the step it is rounded up to", "Average of the last full period, %2 to %3: %5 per period (%4 in all), rounded up to a multiple of %6.", "Average of the last %1 full periods, %2 to %3: %5 per period (%4 in all), rounded up to a multiple of %6.", periods, change.basisFrom, change.basisTo, change.basisSpent, change.basisAverage, change.basisStep);
    }

    contentItem: ColumnLayout {
        RowLayout {
            Layout.fillWidth: true

            Controls.Label {
                Layout.fillWidth: true
                elide: Text.ElideRight
                font.bold: true
                textFormat: Text.PlainText
                text: card.ownName !== "" ? card.ownName : card.change.category
            }

            Controls.Label {
                visible: card.changesAmount
                textFormat: Text.PlainText
                text: card.isNew ? card.change.amount : card.i18nContext.i18nc("@info %1 is what a budget plans now, %2 what it plans from then on", "%1 → %2", card.change.before, card.change.amount)
            }
        }

        // A budget with a name still says which categories it counts.
        Controls.Label {
            Layout.fillWidth: true
            visible: card.ownName !== ""
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: card.i18nContext.i18nc("@info %1 is a list of category names", "For %1", card.change.category)
        }

        // Its new name, or that it loses its name (D-664).
        Controls.Label {
            Layout.fillWidth: true
            visible: !card.isNew && (card.change.renames ?? false)
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: (card.change.newName ?? "") !== ""
                ? card.i18nContext.i18nc("@info %1 is the new name of a budget", "New name: %1", card.change.newName)
                : card.i18nContext.i18nc("@info", "The name is taken away, so the budget is called by its categories again.")
        }

        // What it counts now and from the change on (D-690).
        Controls.Label {
            Layout.fillWidth: true
            visible: card.changesCounts
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: visible ? card.countsText(true, card.change.category, card.change.accounts ?? "") : ""
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: card.changesCounts
            wrapMode: Text.Wrap
            font.bold: true
            textFormat: Text.PlainText
            text: visible ? card.countsText(false, card.change.newCategory ?? "", card.change.newAccounts ?? "") : ""
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: card.changesCounts
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: card.changesAmount
                ? card.i18nContext.i18nc("@info", "Earlier periods keep counting what they counted, so their results don't change.")
                : card.i18nContext.i18nc("@info", "Earlier periods keep counting what they counted, so their results don't change. Every amount stays as it was.")
        }

        // A budget may count only some of the user's accounts (D-641).
        Controls.Label {
            Layout.fillWidth: true
            visible: !card.changesCounts && (card.change.accounts ?? "") !== ""
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: card.i18nContext.i18nc("@info %1 is a list of account names", "Only money paid from %1", card.change.accounts ?? "")
        }

        Controls.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            opacity: 0.8
            textFormat: Text.PlainText
            text: !card.changesAmount && !card.changesCounts
                ? card.i18nContext.i18nc("@info", "Only the name changes. Every amount stays as it was.")
                : card.isNew && card.change.every === "range"
                // A stretch of days the user named has one period (D-688).
                ? card.i18nContext.i18nc("@info %1 and %2 are the first and last day of a budget for one stretch of days", "Once, from %1 to %2", card.change.from, card.change.to)
                : card.isNew
                ? card.i18nContext.i18nc("@info %1 says how often a budget starts again, %2 and %3 are the days of its first period", "%1, first period %2 to %3", card.everyText(card.change.every), card.change.from, card.change.to)
                : card.i18nContext.i18nc("@info %1 says how often a budget starts again, %2 and %3 are the days of the period a change applies from", "%1. The change applies from the period %2 to %3 on; earlier periods stay as they were.", card.everyText(card.change.every), card.change.from, card.change.to)
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: card.changesAmount
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: card.change.carryOver
                ? card.i18nContext.i18nc("@info", "What is left, or spent too much, moves into the next period.")
                : card.i18nContext.i18nc("@info", "Each period starts afresh.")
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: text !== ""
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: card.basisText()
        }

        // The empty newest period the count moved back past (D-669).
        Controls.Label {
            Layout.fillWidth: true
            visible: card.change.basisLeftOutFrom !== ""
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: visible
                ? card.i18nContext.i18nc("@info %1 and %2 are dates", "Nothing was spent from %1 to %2, perhaps because a payment was booked after that period ended. So that period is left out, and one period before it is counted instead.", card.change.basisLeftOutFrom, card.change.basisLeftOutTo)
                : ""
        }

        Controls.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            opacity: 0.8
            textFormat: Text.PlainText
            text: !card.changesAmount
                ? card.i18nContext.i18nc("@info", "A budget only shows how your spending compares; it changes no booking. Undo takes it back.")
                : card.change.basisPeriods > 0
                ? card.i18nContext.i18nc("@info", "KassenSturz worked out this amount from your book, not the AI. A budget only shows how your spending compares; it changes no booking. Undo takes it back.")
                : card.i18nContext.i18nc("@info", "The AI took this amount from what you wrote. A budget only shows how your spending compares; it changes no booking. Undo takes it back.")
        }

        AiChangeCorrection {
            Layout.fillWidth: true
            ai: card.ai
            change: card.change
            choices: card.choices
            review: card.review
        }
    }
}
