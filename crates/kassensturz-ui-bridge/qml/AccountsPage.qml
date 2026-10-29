// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The accounts side by side (D-021) with their balances. Amounts of
// different currencies are never added up: with more than one currency, each
// has its own group and total. A debt reads "You owe" (D-067). Every amount
// and total comes from Rust (D-065).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property AccountList accounts
    readonly property var sections: accounts.view.sections ?? []
    readonly property var closedAccounts: accounts.view.closed ?? []
    property bool showClosed: false

    signal addAccountClicked()
    signal accountClicked(accountId: string)
    // Fetch the new transactions of the accounts from the bank (M4.7).
    signal getTransactionsClicked()
    // Whether bank connections can be used here, so the action is offered.
    property bool bankReady: false

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Accounts")

    actions: [
        Kirigami.Action {
            icon.name: "list-add"
            text: page.i18nContext.i18nc("@action:button", "Add account")
            onTriggered: page.addAccountClicked()
        },
        Kirigami.Action {
            icon.name: "mail-receive"
            visible: page.bankReady && page.sections.length > 0
            text: page.i18nContext.i18nc("@action:button get the accounts' new transactions from the bank over the internet", "Get new transactions")
            onTriggered: page.getTransactionsClicked()
        },
        Kirigami.Action {
            icon.name: "view-visible"
            text: page.i18nContext.i18nc("@action:button", "Show closed accounts")
            checkable: true
            checked: page.showClosed
            visible: page.closedAccounts.length > 0
            onTriggered: page.showClosed = checked
        }
    ]

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        Kirigami.PlaceholderMessage {
            Layout.fillWidth: true
            visible: page.sections.length === 0
            text: page.i18nContext.i18nc("@info", "You have no accounts yet")
            explanation: page.i18nContext.i18nc("@info", "An account is a place where your money is: your bank account, your wallet or a credit card.")
            helpfulAction: Kirigami.Action {
                icon.name: "list-add"
                text: page.i18nContext.i18nc("@action:button", "Add account")
                onTriggered: page.addAccountClicked()
            }
        }

        Repeater {
            model: page.sections

            delegate: ColumnLayout {
                id: section

                required property var modelData

                Layout.fillWidth: true
                spacing: Kirigami.Units.largeSpacing

                // Only a book with several currencies needs to say which.
                Kirigami.Heading {
                    textFormat: Text.PlainText
                    visible: page.sections.length > 1
                    text: section.modelData.currency
                    level: 2
                }

                AccountGrid {
                    accounts: section.modelData.accounts
                    onAccountClicked: accountId => page.accountClicked(accountId)
                }

                Balance {
                    textFormat: Text.PlainText
                    id: together

                    Layout.fillWidth: true
                    visible: section.modelData.accounts.length > 1
                    amount: section.modelData.together
                    exactAmount: section.modelData.exactTogether
                    owes: section.modelData.togetherOwes
                    font.bold: true
                    wrapMode: Text.Wrap
                    text: sentence(amount)
                    exactText: sentence(exactAmount)

                    function sentence(amount: string): string {
                        return owes
                            ? page.i18nContext.i18nc("@label %1 is an amount of money", "All together you owe %1", amount)
                            : page.i18nContext.i18nc("@label %1 is an amount of money", "All together: %1", amount);
                    }
                }
            }
        }

        Kirigami.Heading {
            Layout.topMargin: Kirigami.Units.largeSpacing
            visible: page.showClosed && page.closedAccounts.length > 0
            text: page.i18nContext.i18nc("@title", "Closed accounts")
            level: 2
        }

        AccountGrid {
            visible: page.showClosed
            accounts: page.closedAccounts
            onAccountClicked: accountId => page.accountClicked(accountId)
        }
    }
}
