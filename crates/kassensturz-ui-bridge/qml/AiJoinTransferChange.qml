// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Two entries the AI wants joined as one move of money between the user's
// own accounts, as the review page lists it (US3.3, D-693): the same words
// as the app's own question (TransferPrompt.qml, D-692). Both sides come
// from the book, with the account, the date, what that account shows for
// the money, the category that goes and who sorted it, also a side the
// user sorted themselves: confirming the set is the user's pick (D-031).
// Nothing of a pair can be edited, only left out. Every text from the book
// is plain text (T-08).

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.AbstractCard {
    id: card

    required property AiSwitch ai
    // The change object the bridge made (`joinTransfer`).
    required property var change
    // The review page, for leaving the change out.
    required property var review
    required property var choices

    visible: ai.enabled

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    readonly property var outgoing: change.outgoing ?? ({})
    readonly property var incoming: change.incoming ?? ({})

    // Who sorted the side and that its category goes, as the prompt says it.
    function sortedText(side: var): string {
        const category = side.category ?? "";
        if (category === "") {
            return "";
        }
        if (side.sortedBy === "rule") {
            return side.rule !== ""
                ? i18nContext.i18nc("@info %1 is a category, %2 the name of a Rule", "Sorted as “%1” by the Rule “%2”. Joined, it no longer counts in “%1”.", category, side.rule)
                : i18nContext.i18nc("@info %1 is a category", "Sorted as “%1” by a Rule. Joined, it no longer counts in “%1”.", category);
        }
        return i18nContext.i18nc("@info %1 is a category", "You sorted it as “%1” yourself. Joined, it no longer counts in “%1”.", category);
    }

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.smallSpacing

        Repeater {
            model: [
                { side: card.outgoing, out: true },
                { side: card.incoming, out: false }
            ]

            delegate: ColumnLayout {
                id: sideBlock

                required property var modelData

                Layout.fillWidth: true
                spacing: 0

                Controls.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    font.bold: true
                    textFormat: Text.PlainText
                    text: sideBlock.modelData.out
                        ? card.i18nContext.i18nc("@info %1 is an amount, %2 the name of an account, %3 a date", "%1 left %2 on %3.", card.change.amount ?? "", sideBlock.modelData.side.account ?? "", sideBlock.modelData.side.date ?? "")
                        : card.i18nContext.i18nc("@info %1 is an amount, %2 the name of an account, %3 a date", "%1 arrived in %2 on %3.", card.change.amount ?? "", sideBlock.modelData.side.account ?? "", sideBlock.modelData.side.date ?? "")
                }

                Controls.Label {
                    Layout.fillWidth: true
                    visible: (sideBlock.modelData.side.otherParty ?? "") !== ""
                    wrapMode: Text.Wrap
                    opacity: 0.8
                    textFormat: Text.PlainText
                    text: card.i18nContext.i18nc("@info %1 is a name that account shows for the money", "There it says: %1", sideBlock.modelData.side.otherParty ?? "")
                }

                Controls.Label {
                    Layout.fillWidth: true
                    visible: text !== ""
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    text: card.sortedText(sideBlock.modelData.side)
                }

                Controls.Label {
                    Layout.fillWidth: true
                    visible: sideBlock.modelData.side.movesNote === true
                    wrapMode: Text.Wrap
                    opacity: 0.8
                    textFormat: Text.PlainText
                    text: card.i18nContext.i18nc("@info %1 is the name of an account", "What you wrote on it stays, on the entry in %1.", sideBlock.modelData.side.account ?? "")
                }
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            opacity: 0.8
            textFormat: Text.PlainText
            text: card.i18nContext.i18nc("@info", "If you join them, they count as moving your own money around, not as spending and income. Both accounts keep their balance and the dates they show. Undo takes it apart again.")
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
