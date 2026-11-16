// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One plan the AI prepared, new or changed, as the review page lists it
// (US7, B-357, D-814): its name, the amount out of or into its account and
// how often, the first due day and the next three, when it ends, how much
// an entry may differ, the category a matching entry gets and the other
// party. A change shows the plan as it is and as it will be, and names what
// changes. It says where the amount came from: what the user wrote, or an
// entry of the book, never a figure the AI worked out. Every text from the
// model or the book is plain text (T-08), every amount and day written by
// Rust (D-065).

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.AbstractCard {
    id: card

    required property AiSwitch ai
    // The change object the bridge made (`newPlan` or `editPlan`).
    required property var change
    // The review page, for leaving the change out.
    required property var review
    required property var choices

    visible: ai.enabled

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    readonly property bool isNew: change.kind === "newPlan"
    // The plan as it will be, and for a change as it is now.
    readonly property var plan: isNew ? change.plan : change.after
    readonly property var before: isNew ? null : change.before

    // "750,00 € out of Checking account, every month", the words of the
    // Planned payments window.
    function describe(plan: var): string {
        const how = plan.unit === "week"
            ? i18nContext.i18ncp("@info", "every week", "every %1 weeks", plan.every)
            : plan.unit === "year"
                ? i18nContext.i18ncp("@info", "every year", "every %1 years", plan.every)
                : i18nContext.i18ncp("@info", "every month", "every %1 months", plan.every);
        return plan.outgoing
            ? i18nContext.i18nc("@info %1 is an amount, %2 the name of an account, %3 how often, e.g. every month", "%1 out of %2, %3", plan.amount, plan.account, how)
            : i18nContext.i18nc("@info %1 is an amount, %2 the name of an account, %3 how often, e.g. every month", "%1 into %2, %3", plan.amount, plan.account, how);
    }

    function endsText(plan: var): string {
        return plan.ends !== ""
            ? i18nContext.i18nc("@info %1 is the first due day, %2 the last day a plan may be due", "From %1 until %2", plan.firstDue, plan.ends)
            : i18nContext.i18nc("@info %1 is the first due day of a plan that doesn't end", "From %1, with no end", plan.firstDue);
    }

    function details(plan: var): string {
        const parts = [];
        if (plan.party !== "") {
            parts.push(plan.outgoing
                ? i18nContext.i18nc("@info %1 is who is paid", "Paid to %1", plan.party)
                : i18nContext.i18nc("@info %1 is who pays", "Received from %1", plan.party));
        }
        if (plan.tolerance !== "") {
            parts.push(i18nContext.i18nc("@info %1 is an amount", "may differ by up to %1", plan.tolerance));
        }
        if (plan.category !== "") {
            parts.push(i18nContext.i18nc("@info %1 is a category", "matching entries go to %1", plan.category));
        }
        return parts.join(" · ");
    }

    contentItem: ColumnLayout {
        RowLayout {
            Layout.fillWidth: true

            Controls.Label {
                Layout.fillWidth: true
                elide: Text.ElideRight
                font.bold: true
                textFormat: Text.PlainText
                text: card.plan.name
            }

            Controls.Label {
                visible: card.plan.paused
                textFormat: Text.PlainText
                text: card.i18nContext.i18nc("@info a plan that is paused", "Paused")
            }
        }

        // For a change: the plan as it is now, then as it will be.
        Controls.Label {
            Layout.fillWidth: true
            visible: !card.isNew
            wrapMode: Text.Wrap
            opacity: 0.8
            textFormat: Text.PlainText
            text: visible
                ? card.i18nContext.i18nc("@info %1 describes a plan as it is now, %2 when it is due", "Now: %1. %2.", card.describe(card.before), card.endsText(card.before)) + (card.before.paused ? " " + card.i18nContext.i18nc("@info", "It is paused.") : "")
                : ""
        }

        Controls.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            font.bold: !card.isNew
            textFormat: Text.PlainText
            text: card.isNew
                ? card.describe(card.plan)
                : card.i18nContext.i18nc("@info %1 describes a plan after a change", "After the change: %1", card.describe(card.plan))
        }

        Controls.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: card.endsText(card.plan)
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: text !== ""
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: card.details(card.plan)
        }

        // The name, the category, the party or the tolerance before, when
        // the change takes them away or replaces them.
        Controls.Label {
            Layout.fillWidth: true
            visible: !card.isNew && text !== ""
            wrapMode: Text.Wrap
            opacity: 0.8
            textFormat: Text.PlainText
            text: !card.isNew && card.before.name !== card.plan.name
                // The heading shows the new name; "now" read as after the
                // change, so the old name says "until now" (B-369).
                ? card.i18nContext.i18nc("@info %1 is the name a plan has before the change", "Called %1 until now.", card.before.name)
                : ""
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: !card.isNew && card.before.paused !== card.plan.paused
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: card.plan.paused
                ? card.i18nContext.i18nc("@info", "It is paused: nothing is expected or counted until you let it go on.")
                : card.i18nContext.i18nc("@info", "It goes on again.")
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: card.plan.next.length > 0 && !card.plan.paused
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: card.i18nContext.i18nc("@info %1 is a list of days", "Next due: %1", card.plan.next.join(", "))
        }

        Controls.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            opacity: 0.8
            textFormat: Text.PlainText
            text: card.plan.fromEntry
                ? card.i18nContext.i18nc("@info", "KassenSturz took the amount from an entry in your book, not the AI. A plan only says what you expect: it moves no money and makes no entry, and the forecast counts it. Undo takes it back.")
                : card.isNew || card.before.amount !== card.plan.amount
                ? card.i18nContext.i18nc("@info", "The AI took this amount from what you wrote. A plan only says what you expect: it moves no money and makes no entry, and the forecast counts it. Undo takes it back.")
                : card.i18nContext.i18nc("@info", "A plan only says what you expect: it moves no money and makes no entry, and the forecast counts it. Undo takes it back.")
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
