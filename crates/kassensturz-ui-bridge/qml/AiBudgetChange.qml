// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One budget the AI prepared, new or changed, as the review page lists it
// (M6c.11, D-358, D-395): the category, how often it starts again, the
// first period it applies to, the amount, whether it carries over and, for
// an amount KassenSturz computed, how it came about. Every text from the
// model or the book is plain text (T-08).

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

    function everyText(every: string): string {
        switch (every) {
        case "month":
            return i18nContext.i18nc("@info how often a budget starts again", "Every month");
        case "quarter":
            return i18nContext.i18nc("@info how often a budget starts again", "Every quarter");
        case "year":
            return i18nContext.i18nc("@info how often a budget starts again", "Every year");
        }
        return i18nContext.i18nc("@info how often a budget starts again", "Once, for some days");
    }

    // How the amount came about, in words, or "" for one the user named.
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
                text: card.change.category
            }

            Controls.Label {
                textFormat: Text.PlainText
                text: card.isNew ? card.change.amount : card.i18nContext.i18nc("@info %1 is what a budget plans now, %2 what it plans from then on", "%1 → %2", card.change.before, card.change.amount)
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            opacity: 0.8
            textFormat: Text.PlainText
            text: card.isNew
                ? card.i18nContext.i18nc("@info %1 says how often a budget starts again, %2 and %3 are the days of its first period", "%1, first period %2 to %3", card.everyText(card.change.every), card.change.from, card.change.to)
                : card.i18nContext.i18nc("@info %1 says how often a budget starts again, %2 and %3 are the days of the period a change applies from", "%1. The change applies from the period %2 to %3 on; earlier periods stay as they were.", card.everyText(card.change.every), card.change.from, card.change.to)
        }

        Controls.Label {
            Layout.fillWidth: true
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

        Controls.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            opacity: 0.8
            textFormat: Text.PlainText
            text: card.change.basisPeriods > 0
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
