// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Step 2 of the guided setup (#1031, docs/GUIDED_SETUP.md): is this balance
// right? (D-911) One box per account with the balance the book holds, as
// exact text from Rust (D-065), and, where the user stated a balance of
// their bank before, that one (D-117). A balance that isn't right is
// corrected on the existing "Check against my bank" page, which compares,
// explains and offers the correction of the starting balance (D-116); this
// page changes nothing itself.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property AccountList accounts
    // This page's step and how many there are; 0 when opened from Help.
    property int step: 0
    property int count: 0

    // The user wants `accountId` compared with what their bank shows, typed
    // as `bank` (may be empty: the check page asks for it).
    signal compareClicked(string accountId, string bank)
    signal nextClicked()
    signal skipClicked()
    signal endClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The book's accounts in detail. Read again whenever the list changes,
    // as after a correction.
    readonly property var ownAccounts: (page.accounts.view.sections ?? [])
        .reduce((all, section) => all.concat(section.accounts ?? []), [])
        .map(card => page.accounts.details(card.id) ?? ({}))
        .filter(details => details.id !== undefined && details.closed !== true)

    title: i18nContext.i18nc("@title a step of the guided setup", "Is this balance right?")
    header: GuidedBar {
        step: page.step
        count: page.count
        onEndClicked: page.endClicked()
    }
    footer: GuidedFooter {
        nextText: page.step > 0
            ? page.i18nContext.i18nc("@action:button the balances shown are right", "Yes, that's right")
            : page.i18nContext.i18nc("@action:button closes a step opened from Help", "Done")
        skipVisible: page.step > 0
        onSkipClicked: page.skipClicked()
        onNextClicked: page.nextClicked()
    }

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        ColumnLayout {
            Layout.alignment: Qt.AlignHCenter
            Layout.topMargin: Kirigami.Units.gridUnit
            Layout.maximumWidth: Kirigami.Units.gridUnit * 36
            Layout.preferredWidth: Kirigami.Units.gridUnit * 36
            spacing: Kirigami.Units.largeSpacing

            Controls.Label {
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignHCenter
                textFormat: Text.PlainText
                text: page.i18nContext.i18nc("@info", "Compare with your banking app or your statement. KassenSturz is only useful when these figures are the ones your bank shows.")
                wrapMode: Text.Wrap
            }

            Repeater {
                model: page.ownAccounts

                delegate: Kirigami.AbstractCard {
                    id: card

                    required property var modelData

                    Layout.fillWidth: true

                    contentItem: ColumnLayout {
                        spacing: Kirigami.Units.smallSpacing

                        RowLayout {
                            Layout.fillWidth: true

                            Controls.Label {
                                Layout.fillWidth: true
                                textFormat: Text.PlainText
                                text: Plain.plain(card.modelData.name)
                                elide: Text.ElideRight
                            }

                            Kirigami.Heading {
                                level: 2
                                textFormat: Text.PlainText
                                text: card.modelData.owes
                                    ? Plain.plain(page.i18nContext.i18nc("@info %1 is an amount of money", "You owe %1", card.modelData.exactBalance))
                                    : Plain.plain(card.modelData.exactBalance)
                            }
                        }

                        Controls.Label {
                            Layout.fillWidth: true
                            visible: card.modelData.stated !== undefined && card.modelData.stated !== null
                            textFormat: Text.PlainText
                            text: visible
                                ? Plain.plain(card.modelData.stated.matches
                                    ? page.i18nContext.i18nc("@info %1 is an amount of money, %2 a date", "Your bank showed %1 on %2, and that is what KassenSturz has for that day.", card.modelData.stated.exactBalance, card.modelData.stated.date)
                                    : page.i18nContext.i18nc("@info %1 and %3 are amounts of money, %2 is a date", "Your bank showed %1 on %2, but KassenSturz has %3 for that day.", card.modelData.stated.exactBalance, card.modelData.stated.date, card.modelData.stated.exactHeld))
                                : ""
                            wrapMode: Text.Wrap
                            opacity: 0.8
                        }

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.smallSpacing

                            Controls.Label {
                                textFormat: Text.PlainText
                                text: page.i18nContext.i18nc("@label:textbox", "Not right? What does your bank show today?")
                            }

                            Controls.TextField {
                                id: bankField

                                Layout.preferredWidth: Kirigami.Units.gridUnit * 8
                                Accessible.name: page.i18nContext.i18nc("@label:textbox", "Not right? What does your bank show today?")
                                horizontalAlignment: Text.AlignRight
                                inputMethodHints: Qt.ImhFormattedNumbersOnly
                                placeholderText: page.accounts.amountExample(card.modelData.currency)
                                onAccepted: {
                                    ActionLog.taken("GuidedBalancePage_compareTyped")
                                    page.compareClicked(card.modelData.id, text)
                                }
                            }

                            Controls.Button {
                                text: page.i18nContext.i18nc("@action:button compares the account with what the bank shows", "Compare…")
                                onClicked: {
                                    ActionLog.taken("GuidedBalancePage_compare")
                                    page.compareClicked(card.modelData.id, bankField.text)
                                }
                            }
                        }

                        Controls.Label {
                            Layout.fillWidth: true
                            textFormat: Text.PlainText
                            text: page.i18nContext.i18nc("@info", "KassenSturz then shows the difference and offers to correct the account's starting balance. No transaction is changed, and you can take it back with Undo.")
                            wrapMode: Text.Wrap
                            opacity: 0.8
                            font: Kirigami.Theme.smallFont
                        }
                    }
                }
            }

            Controls.Label {
                Layout.fillWidth: true
                visible: page.ownAccounts.length === 0
                horizontalAlignment: Text.AlignHCenter
                textFormat: Text.PlainText
                text: page.i18nContext.i18nc("@info", "Your book has no account yet, so there is no balance to check.")
                wrapMode: Text.Wrap
            }
        }
    }
}
