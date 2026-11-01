// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One account's transactions and its balance (M3.5, D-118), laid out like
// the lines of a household book where the page is wide enough (round 1): date,
// from or to, category, payment, deposit and what the account held after it.
// The rows come from a Rust list model, sorted there. Newest first unless the
// user sorts otherwise. A click opens the transaction to change or remove it;
// "Add transaction" adds one to this account, and "About this account" opens
// its name, its closing and the check against the bank.

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
    // The Rules, which suggest one when the same payee was sorted into the
    // same category by hand several times (M3.11, D-071).
    required property RuleBook rules

    // The lines were read and a picture with them was drawn.
    signal ready()

    // Add a transaction to this account.
    signal addTransactionClicked(string accountId)
    // Import a file from the bank into this account (M3.10).
    signal importClicked(string accountId)
    // Fetch this account's new transactions from the bank (M4.7).
    signal getTransactionsClicked(string accountId)
    // Whether bank connections can be used here, so the action is offered.
    property bool bankReady: false
    // Whether the action that sorts this account's transactions with help
    // is offered; Main.qml binds it to the switch that turns that help off
    // (M5.9, D-222).
    property bool suggestionsShown: false
    // The user wants categories suggested for this account's transactions.
    signal suggestCategoriesClicked(string accountId)
    // The account's name, closing and check against the bank.
    signal detailsClicked(string accountId)
    // Open a transaction to change or remove it.
    signal transactionClicked(string accountId, string transactionId)
    // The user wants the Rule that is suggested.
    signal createRuleFromSuggestion(string payee, string category)

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

    // Undo or redo may have removed this account, in which case the page
    // closes (M3.8). The transactions in it refresh by themselves.
    function survivesUndo(): bool {
        return register.accountChoices().findIndex(choice => choice.id === accountId) >= 0;
    }

    // Columns need room: the row's two lines are used on a narrow page.
    readonly property bool wide: width >= Kirigami.Units.gridUnit * 50

    // Staged loading (M4.U, D-156). The page opens at once with the account's
    // name, and its transactions are read when the list has its size. A list
    // that is filled while the page is still being made has no size yet and
    // makes a row for every transaction instead of the few in view: 500
    // transactions took seconds to open, 3,000 took a minute. Until then
    // the list has no model, so it shows nothing of an account seen before.
    property bool loaded: false

    // The account's name from the list of accounts, for the title until the
    // register is read.
    readonly property string chosenName: {
        const choices = register.accountChoices();
        const found = choices.findIndex(choice => choice.id === accountId);
        return found >= 0 ? choices[found].name : "";
    }

    function load() {
        if (loaded) {
            return;
        }
        register.show(accountId);
        loaded = true;
        // The first picture with the lines in it, for the developer mode's
        // timing of how long the register takes.
        const window = Window.window;
        if (window) {
            const drawn = () => {
                window.frameSwapped.disconnect(drawn);
                page.ready();
            };
            window.frameSwapped.connect(drawn);
        }
    }

    title: loaded ? register.accountName : chosenName
    Component.onCompleted: {
        // The filter box of a new page starts empty.
        register.setFilter("");
        if (list.height > 0) {
            load();
        }
    }

    // Should the list never get a size, e.g. in a window that is hidden, the
    // account is read anyway, so nothing waits for a size forever.
    Timer {
        interval: 500
        running: !page.loaded
        onTriggered: {
            ActionLog.taken("RegisterPage_load")
            page.load()
        }
    }

    actions: [
        Kirigami.Action {
            icon.name: "list-add"
            text: page.i18nContext.i18nc("@action:button", "Add transaction")
            onTriggered: {
                ActionLog.taken("RegisterPage_addTransaction")
                page.addTransactionClicked(page.accountId)
            }
        },
        Kirigami.Action {
            icon.name: "mail-receive"
            visible: page.bankReady
            text: page.i18nContext.i18nc("@action:button get this account's new transactions from the bank over the internet", "Get new transactions")
            onTriggered: {
                ActionLog.taken("RegisterPage_getNewTransactions")
                page.getTransactionsClicked(page.accountId)
            }
        },
        Kirigami.Action {
            icon.name: "document-import"
            text: page.i18nContext.i18nc("@action:button", "Import from my bank")
            onTriggered: {
                ActionLog.taken("RegisterPage_importBank")
                page.importClicked(page.accountId)
            }
        },
        Kirigami.Action {
            icon.name: "tag-new"
            visible: page.suggestionsShown
            text: page.i18nContext.i18nc("@action:button asks for a category for each transaction of this account that has none; the answer is shown for you to check", "Suggest categories")
            onTriggered: {
                ActionLog.taken("RegisterPage_suggestCategories")
                page.suggestCategoriesClicked(page.accountId)
            }
        },
        Kirigami.Action {
            icon.name: "configure"
            text: page.i18nContext.i18nc("@action:button settings of one account: its name, closing it, checking it against the bank", "About this account")
            onTriggered: {
                ActionLog.taken("RegisterPage_aboutAccount")
                page.detailsClicked(page.accountId)
            }
        },
        Kirigami.Action {
            icon.name: "view-sort"
            text: page.i18nContext.i18nc("@action:button %1 says what the list is sorted by", "Sorted by %1", page.sortName(page.register.sortKey))

            Kirigami.Action {
                text: page.sortName("date")
                checkable: true
                checked: page.register.sortKey === "date"
                onTriggered: {
                    ActionLog.taken("RegisterPage_date")
                    page.register.sortBy("date", true)
                }
            }
            Kirigami.Action {
                text: page.sortName("amount")
                checkable: true
                checked: page.register.sortKey === "amount"
                onTriggered: {
                    ActionLog.taken("RegisterPage_amount")
                    page.register.sortBy("amount", true)
                }
            }
            Kirigami.Action {
                text: page.sortName("other-party")
                checkable: true
                checked: page.register.sortKey === "other-party"
                onTriggered: {
                    ActionLog.taken("RegisterPage_otherParty")
                    page.register.sortBy("other-party", false)
                }
            }
            Kirigami.Action {
                text: page.sortName("category")
                checkable: true
                checked: page.register.sortKey === "category"
                onTriggered: {
                    ActionLog.taken("RegisterPage_category")
                    page.register.sortBy("category", false)
                }
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
                onTriggered: {
                    ActionLog.taken("RegisterPage_date_2")
                    page.register.sortBy(page.register.sortKey, !page.register.descending)
                }
            }
        }
    ]

    // The suggestion that has been waiting longest, if any.
    readonly property var suggestion: (rules.view.suggestions ?? [])[0]

    header: ColumnLayout {
        spacing: 0

        // The quick filter, like the search above a mailbox: it narrows the
        // lines to those with every typed word in them (round 2). The balance
        // below still counts all of the account.
        Kirigami.SearchField {
            id: quickFilter

            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.smallSpacing
            visible: page.loaded && page.register.unfiltered > 0
            placeholderText: page.i18nContext.i18nc("@info:placeholder", "Filter these transactions")
            Accessible.name: page.i18nContext.i18nc("@label", "Filter transactions")
            onTextChanged: page.register.setFilter(text)
            // A page for another account starts empty; text kept here would
            // filter nothing.
            Connections {
                target: page.register
                function onFilterChanged() {
                    if (quickFilter.text !== page.register.filter) {
                        quickFilter.text = page.register.filter;
                    }
                }
            }
        }

        Kirigami.InlineMessage {
            id: ruleSuggestion

            Layout.fillWidth: true
            position: Kirigami.InlineMessage.Position.Header
            visible: page.suggestion !== undefined
            type: Kirigami.MessageType.Information
            text: page.suggestion === undefined
                ? ""
                : Plain.plain(page.i18nContext.i18ncp("@info %1 is a number, %2 a name, %3 a category", "You have sorted %2 into %3 by hand %1 time. Want KassenSturz to do it for you from now on?", "You have sorted %2 into %3 by hand %1 times. Want KassenSturz to do it for you from now on?", parseInt(page.suggestion.count), page.suggestion.payee, page.suggestion.categoryPath))
            actions: [
                Kirigami.Action {
                    icon.name: "view-filter"
                    text: page.i18nContext.i18nc("@action:button", "Create a Rule")
                    onTriggered: {
                        ActionLog.taken("RegisterPage_createRule")
                        page.createRuleFromSuggestion(page.suggestion.payee, page.suggestion.category)
                    }
                },
                Kirigami.Action {
                    text: page.i18nContext.i18nc("@action:button", "No, thanks")
                    onTriggered: {
                        ActionLog.taken("RegisterPage_noThanks")
                        page.rules.dismissSuggestion(page.suggestion.payee, page.suggestion.category)
                    }
                }
            ]
        }
    }

    footer: Controls.ToolBar {
        visible: page.loaded
        contentItem: AmountLabel {
            textFormat: Text.PlainText
            text: page.balanceText(page.register.balance)
            exactText: page.balanceText(page.register.exactBalance)
            font.bold: true
            horizontalAlignment: Text.AlignRight
        }
    }

    ListView {
        id: list

        // Set once the account is read (`page.load`).
        model: page.loaded ? page.register : null
        onHeightChanged: {
            if (height > 0) {
                page.load();
            }
        }
        // The page shows the chosen account; the model also serves the
        // transaction page, which may have shown another one.
        onVisibleChanged: if (visible && page.loaded) { page.register.show(page.accountId); }

        // The book's column titles stay in view while the lines scroll.
        headerPositioning: ListView.OverlayHeader
        header: Controls.ItemDelegate {
            z: 2
            width: ListView.view.width
            visible: page.wide
            height: visible ? implicitHeight : 0
            hoverEnabled: false
            down: false
            // Opaque, so lines scrolling beneath it don't show through.
            background: Rectangle {
                color: Kirigami.Theme.backgroundColor

                Kirigami.Separator {
                    anchors.bottom: parent.bottom
                    width: parent.width
                }
            }
            contentItem: TransactionRow {
                ledger: true
                header: true
                owes: page.register.owes
            }
        }

        Kirigami.PlaceholderMessage {
            anchors.centerIn: parent
            width: parent.width - Kirigami.Units.largeSpacing * 4
            visible: page.loaded && list.count === 0
            readonly property bool filtered: page.register.unfiltered > 0
            text: filtered
                ? page.i18nContext.i18nc("@info nothing fits what was typed in the filter", "No transaction fits")
                : page.i18nContext.i18nc("@info", "Nothing here yet")
            explanation: filtered
                ? page.i18nContext.i18nc("@info", "Try other words, or clear the filter to see all transactions.")
                : page.i18nContext.i18nc("@info", "Add what you paid or received from this account, and it shows up here with what was left afterwards.")
            helpfulAction: filtered ? null : addAction
            property Kirigami.Action addAction: Kirigami.Action {
                icon.name: "list-add"
                text: page.i18nContext.i18nc("@action:button", "Add transaction")
                onTriggered: {
                    ActionLog.taken("RegisterPage_addTransaction_2")
                    page.addTransactionClicked(page.accountId)
                }
            }
        }

        delegate: Controls.ItemDelegate {
            id: delegate

            required property int index
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
            required property string note

            width: ListView.view.width
            Accessible.name: transaction.Accessible.name
            onClicked: {
                ActionLog.taken("RegisterPage_transactionClicked")
                page.transactionClicked(page.accountId, transactionId)
            }
            // Every second line shaded, to follow it across the columns.
            background: Rectangle {
                // The line under the pointer or the keyboard focus is lit.
                readonly property bool lit: delegate.down || delegate.hovered || delegate.visualFocus

                color: lit
                    ? Kirigami.Theme.highlightColor
                    : delegate.index % 2 === 1 ? Kirigami.Theme.alternateBackgroundColor : "transparent"
                opacity: lit ? 0.3 : 1
            }

            contentItem: TransactionRow {
                id: transaction

                ledger: page.wide
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
                note: delegate.note
            }
        }
    }
}
