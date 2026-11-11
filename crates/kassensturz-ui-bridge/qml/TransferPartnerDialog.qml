// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Lists the entries of the user's other accounts that could be the other
// side of a move of money with one transaction (US3.3, D-692): the exact
// opposite amount, in the same currency, also ones the user sorted, the
// closest dates first. Picking one hands it to TransferPrompt, which shows
// the preview and joins only on "Yes" (D-030). Every text and amount comes
// from Rust (D-065).
//
//     partnerDialog.show(transactionId);
//     onPicked: partner => transferPrompt.askPicked([partner])

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.Dialog {
    id: dialog

    required property AccountRegister register
    // What register.transferPartners() returned.
    property var partners: []

    // The user picked `partner`, an entry of `partners`.
    signal picked(var partner)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // Lists the partners of the transaction with this ID and opens.
    function show(transactionId: string) {
        const found = register.transferPartners(transactionId);
        // An error comes back as an object without a length.
        if (found?.length === undefined) {
            error.result = found;
            partners = [];
        } else {
            error.result = ({});
            partners = found;
        }
        open();
    }

    // The side of `partner` that is in the other account.
    function otherSide(partner: var): var {
        return partner.partnerIsIncoming ? partner.incoming : partner.outgoing;
    }

    title: i18nContext.i18nc("@title:window", "Which entry is the other side?")
    standardButtons: Kirigami.Dialog.Cancel
    preferredWidth: Kirigami.Units.gridUnit * 26

    ColumnLayout {
        spacing: Kirigami.Units.smallSpacing

        FormError {
            id: error

            Layout.fillWidth: true
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: dialog.partners.length > 0
            text: dialog.i18nContext.i18nc("@info", "These entries in your other accounts move exactly this amount the other way. Choose the one that belongs to this move; you see what changes before anything is joined.")
            wrapMode: Text.Wrap
        }

        // No dead ends (AGENTS.md §6): say why the list is empty and what to
        // do.
        Kirigami.PlaceholderMessage {
            Layout.fillWidth: true
            visible: dialog.partners.length === 0 && error.result.code === undefined
            text: dialog.i18nContext.i18nc("@info", "No matching entry found")
            explanation: dialog.i18nContext.i18nc("@info", "None of your other accounts has an entry that moves exactly this amount the other way. Import the other account's statement, or type its side, then try again.")
        }

        Repeater {
            model: dialog.partners

            delegate: Controls.ItemDelegate {
                id: row

                required property var modelData
                readonly property var side: dialog.otherSide(modelData)

                Layout.fillWidth: true
                Accessible.name: label.text
                onClicked: {
                    ActionLog.taken("TransferPartnerDialog_pick")
                    dialog.close()
                    dialog.picked(row.modelData)
                }

                contentItem: ColumnLayout {
                    spacing: 0

                    Controls.Label {
                        id: label

                        textFormat: Text.PlainText
                        Layout.fillWidth: true
                        text: row.modelData.partnerIsIncoming
                            ? dialog.i18nContext.i18nc("@info %1 is an amount, %2 the name of an account, %3 a date", "%1 arrived in %2 on %3.", row.modelData.amount, row.side.account, row.side.date)
                            : dialog.i18nContext.i18nc("@info %1 is an amount, %2 the name of an account, %3 a date", "%1 left %2 on %3.", row.modelData.amount, row.side.account, row.side.date)
                        wrapMode: Text.Wrap
                        font.bold: row.modelData.likely
                    }

                    Controls.Label {
                        textFormat: Text.PlainText
                        Layout.fillWidth: true
                        visible: (row.side.otherParty ?? "") !== ""
                        text: dialog.i18nContext.i18nc("@info %1 is a name that account shows for the money", "There it says: %1", row.side.otherParty ?? "")
                        wrapMode: Text.Wrap
                        font: Kirigami.Theme.smallFont
                    }

                    Controls.Label {
                        textFormat: Text.PlainText
                        Layout.fillWidth: true
                        visible: (row.side.category ?? "") !== ""
                        text: row.side.sortedBy === "rule"
                            ? dialog.i18nContext.i18nc("@info %1 is a category", "Sorted as “%1” by a Rule", row.side.category ?? "")
                            : dialog.i18nContext.i18nc("@info %1 is a category", "You sorted it as “%1” yourself", row.side.category ?? "")
                        wrapMode: Text.Wrap
                        font: Kirigami.Theme.smallFont
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        visible: row.modelData.likely
                        text: dialog.i18nContext.i18nc("@info the entry is within a few days of the chosen one", "Likely: only a few days apart")
                        wrapMode: Text.Wrap
                        font: Kirigami.Theme.smallFont
                        color: Kirigami.Theme.positiveTextColor
                    }
                }
            }
        }
    }
}
