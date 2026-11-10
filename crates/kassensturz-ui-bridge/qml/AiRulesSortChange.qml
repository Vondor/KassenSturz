// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The AI's "sort with all Rules", as the review page lists it (US2.9,
// D-682): the same run and the same words as "Apply all" and "Apply now" on
// the Rules page (RulesApply.qml, D-630, D-631). It says which Rules sort,
// how many transactions are sorted, how many a Rule fits that stay as they
// are and why, and the newest of them as they will look. The switch "Also
// sort again what Rules sorted before" is the user's own correction of the
// AI's call (D-293): the bridge prepares the run again with it. Every text
// from the model or the book is plain text (T-08).

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.AbstractCard {
    id: card

    required property AiSwitch ai
    // The change object the bridge made (`sortWithRules`).
    required property var change
    // The review page, for the switch and for leaving the change out.
    required property var review
    required property var choices

    visible: ai.enabled

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    readonly property int count: parseInt(change.changed ?? 0)
    readonly property bool oneRule: (change.ruleName ?? "") !== ""

    // Why some transactions a Rule fits stay as they are, one line each.
    function skippedText(): string {
        const lines = [];
        const add = (count, line) => {
            if (count > 0) {
                lines.push(line);
            }
        };
        const transfers = parseInt(change.transfers ?? 0);
        const noRate = parseInt(change.noRate ?? 0);
        const closed = parseInt(change.closed ?? 0);
        const other = parseInt(change.other ?? 0);
        add(transfers, i18nContext.i18ncp("@info %1 is a number", "%1 transaction looks like money moved between your own accounts. Decide that in the account first; it stays as it is.", "%1 transactions look like money moved between your own accounts. Decide that in the account first; they stay as they are.", transfers));
        add(noRate, i18nContext.i18ncp("@info %1 is a number", "%1 transaction stays as it is because an exchange rate for its day is missing.", "%1 transactions stay as they are because an exchange rate for their day is missing.", noRate));
        add(closed, i18nContext.i18ncp("@info %1 is a number", "%1 transaction stays as it is because an account it would go to or come from is closed.", "%1 transactions stay as they are because an account they would go to or come from is closed.", closed));
        add(other, i18nContext.i18ncp("@info %1 is a number", "%1 more transaction a Rule fits can't be sorted and stays as it is.", "%1 more transactions a Rule fits can't be sorted and stay as they are.", other));
        return lines.join("\n");
    }

    contentItem: ColumnLayout {
        Controls.Label {
            Layout.fillWidth: true
            font.bold: true
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: card.oneRule
                ? card.i18nContext.i18nc("@title:window %1 is the name of a Rule", "Sort your transactions with %1", card.change.ruleName)
                : card.i18nContext.i18nc("@title:window", "Sort your transactions with all Rules")
        }

        Controls.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: card.oneRule
                ? card.i18nContext.i18nc("@info", "This Rule sorts the transactions already in your book that it decides. A Rule before it in the list still takes a transaction first. Transactions you sorted or changed yourself are never touched.")
                : card.i18nContext.i18nc("@info", "Your Rules sort the transactions already in your book, in the order of the list: the first Rule that fits a transaction decides. Transactions you sorted or changed yourself are never touched.")
        }

        Controls.CheckBox {
            id: sortAgainBox
            Layout.fillWidth: true
            checked: card.change.sortAgain === true
            text: card.i18nContext.i18nc("@option:check", "Also sort again what Rules sorted before")
            onToggled: {
                ActionLog.taken("AiRulesSortChange_alsoSortAgain")
                // The set is prepared again with the switch; when the
                // checks refuse that, the switch shows the set as it is.
                if (!card.review.editChange(card.change.slot, { sort_again: checked })) {
                    checked = Qt.binding(() => card.change.sortAgain === true);
                }
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
            textFormat: Text.PlainText
            text: card.i18nContext.i18nc("@info", "Without this, only transactions without a category are sorted. With it, what a Rule sorted earlier is sorted again as the Rules say now, for example after you changed a Rule or the order. What no Rule fits anymore stays as it is.")
        }

        Controls.Label {
            Layout.fillWidth: true
            font.bold: true
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: card.i18nContext.i18ncp("@info %1 is a number", "%1 transaction will be sorted.", "%1 transactions will be sorted.", card.count)
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: text !== ""
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: card.skippedText()
        }

        Controls.Label {
            Layout.fillWidth: true
            font: Kirigami.Theme.smallFont
            textFormat: Text.PlainText
            text: card.count > (card.change.examples ?? []).length
                ? card.i18nContext.i18nc("@info", "The newest of them, as they will look:")
                : card.i18nContext.i18nc("@info", "As they will look:")
        }

        Repeater {
            model: card.change.examples ?? []

            ColumnLayout {
                required property var modelData

                Layout.fillWidth: true
                spacing: 0

                RowLayout {
                    Layout.fillWidth: true

                    Controls.Label {
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        textFormat: Text.PlainText
                        text: modelData.otherParty !== "" ? modelData.otherParty : card.i18nContext.i18nc("@info a transaction without a name", "(no name)")
                    }

                    Controls.Label {
                        textFormat: Text.PlainText
                        text: modelData.exactAmount
                    }
                }

                Controls.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    opacity: 0.8
                    textFormat: Text.PlainText
                    text: card.i18nContext.i18nc("@info %1 is a date, %2 the name of an account", "%1 · %2", modelData.date, modelData.account)
                }

                Controls.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    opacity: 0.8
                    textFormat: Text.PlainText
                    text: modelData.kind === "transfer"
                        ? card.i18nContext.i18nc("@info a Transfer in the book, %1 is the other account", "Transfer with %1", modelData.category)
                        : card.i18nContext.i18nc("@info %1 is the path of a category", "Category: %1", modelData.category)
                }
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            opacity: 0.8
            textFormat: Text.PlainText
            text: card.i18nContext.i18nc("@info", "Your own Rules decide where each transaction goes, not the AI. If your book changes before you confirm, nothing is sorted and you are asked to prepare it again. Undo takes the whole run back.")
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
