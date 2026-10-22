// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One account's transactions and its balance (M3.5, D-118). The rows come
// from a Rust list model, sorted there, each with what the account held
// after it. Newest first unless the user sorts otherwise. A click opens the
// transaction to change or remove it; "Add transaction" adds one to this
// account.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    // The app's register model, shared by the screens; this page shows
    // `accountId` in it.
    required property AccountRegister register
    required property string accountId

    // Add a transaction to this account.
    signal addTransactionClicked(string accountId)
    // Open a transaction to change or remove it.
    signal transactionClicked(string accountId, string transactionId)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The balance, with `amount` as shown or exactly. A debt reads "You owe"
    // (D-067).
    function balanceText(amount: string): string {
        return register.owes
            ? i18nContext.i18nc("@label %1 is an amount of money", "You owe: %1", amount)
            : i18nContext.i18nc("@label %1 is an amount of money", "Balance: %1", amount);
    }

    function sortName(key: string): string {
        switch (key) {
        case "amount":
            return i18nContext.i18nc("@item:inmenu sort the transactions by", "Amount");
        case "other-party":
            return i18nContext.i18nc("@item:inmenu sort the transactions by whom the money came from or went to", "From or to");
        case "category":
            return i18nContext.i18nc("@item:inmenu sort the transactions by", "Category");
        default:
            return i18nContext.i18nc("@item:inmenu sort the transactions by", "Date");
        }
    }

    title: register.accountName
    Component.onCompleted: register.show(accountId)

    actions: [
        Kirigami.Action {
            icon.name: "list-add"
            text: page.i18nContext.i18nc("@action:button", "Add transaction")
            onTriggered: page.addTransactionClicked(page.accountId)
        },
        Kirigami.Action {
            icon.name: "view-sort"
            text: page.i18nContext.i18nc("@action:button %1 says what the list is sorted by", "Sorted by %1", page.sortName(page.register.sortKey))

            Kirigami.Action {
                text: page.sortName("date")
                checkable: true
                checked: page.register.sortKey === "date"
                onTriggered: page.register.sortBy("date", true)
            }
            Kirigami.Action {
                text: page.sortName("amount")
                checkable: true
                checked: page.register.sortKey === "amount"
                onTriggered: page.register.sortBy("amount", true)
            }
            Kirigami.Action {
                text: page.sortName("other-party")
                checkable: true
                checked: page.register.sortKey === "other-party"
                onTriggered: page.register.sortBy("other-party", false)
            }
            Kirigami.Action {
                text: page.sortName("category")
                checkable: true
                checked: page.register.sortKey === "category"
                onTriggered: page.register.sortBy("category", false)
            }
            Kirigami.Action {
                separator: true
            }
            Kirigami.Action {
                text: page.register.sortKey === "date"
                    ? page.i18nContext.i18nc("@action:inmenu turn the sort order around", "Oldest first")
                    : page.register.sortKey === "amount"
                        ? page.i18nContext.i18nc("@action:inmenu turn the sort order around", "Smallest first")
                        : page.i18nContext.i18nc("@action:inmenu turn the sort order around", "Z to A")
                checkable: true
                checked: page.register.sortKey === "date" || page.register.sortKey === "amount"
                    ? !page.register.descending
                    : page.register.descending
                onTriggered: page.register.sortBy(page.register.sortKey, !page.register.descending)
            }
        }
    ]

    footer: Controls.ToolBar {
        contentItem: AmountLabel {
            text: page.balanceText(page.register.balance)
            exactText: page.balanceText(page.register.exactBalance)
            font.bold: true
            horizontalAlignment: Text.AlignRight
        }
    }

    ListView {
        id: list

        model: page.register
        // The page shows the chosen account; the model also serves the
        // transaction page, which may have shown another one.
        onVisibleChanged: if (visible) { page.register.show(page.accountId); }

        Kirigami.PlaceholderMessage {
            anchors.centerIn: parent
            width: parent.width - Kirigami.Units.largeSpacing * 4
            visible: list.count === 0
            text: page.i18nContext.i18nc("@info", "Nothing here yet")
            explanation: page.i18nContext.i18nc("@info", "Add what you paid or received from this account, and it shows up here with what was left afterwards.")
            helpfulAction: Kirigami.Action {
                icon.name: "list-add"
                text: page.i18nContext.i18nc("@action:button", "Add transaction")
                onTriggered: page.addTransactionClicked(page.accountId)
            }
        }

        delegate: Controls.ItemDelegate {
            id: delegate

            required property string transactionId
            required property string date
            required property string otherParty
            required property string category
            required property string amount
            required property string exactAmount
            required property bool incoming
            required property string kind
            required property string balance
            required property string exactBalance

            width: ListView.view.width
            Accessible.name: transaction.Accessible.name
            onClicked: page.transactionClicked(page.accountId, transactionId)

            contentItem: TransactionRow {
                id: transaction

                date: delegate.date
                otherParty: delegate.otherParty
                category: delegate.category
                amount: delegate.amount
                exactAmount: delegate.exactAmount
                incoming: delegate.incoming
                kind: delegate.kind
                balance: delegate.balance
                exactBalance: delegate.exactBalance
                owes: page.register.owes
            }
        }
    }
}
