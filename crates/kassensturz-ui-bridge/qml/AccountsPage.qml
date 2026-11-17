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
    // Any register will do for the Transfers, which belong to the book.
    required property AccountRegister register
    // The planned payments (US7.1, D-769).
    required property PlannedPayments plans
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

    // The accounts list only shows the book, which every undo reads again
    // (`bookChanged`), so it stays open. Without this, an Undo closed the
    // page the user was on, and the first click on "Monthly moves…" right
    // after it hit the page that was just going away (M6-B242).
    function survivesUndo(): bool {
        return true;
    }

    title: i18nContext.i18nc("@title", "Accounts")

    actions: [
        Kirigami.Action {
            icon.name: "list-add"
            text: page.i18nContext.i18nc("@action:button", "Add account")
            onTriggered: {
                ActionLog.taken("AccountsPage_addAccount")
                page.addAccountClicked()
            }
        },
        Kirigami.Action {
            icon.name: "mail-receive"
            visible: page.bankReady && page.sections.length > 0
            text: page.i18nContext.i18nc("@action:button get the accounts' new transactions from the bank over the internet", "Get new transactions")
            onTriggered: {
                ActionLog.taken("AccountsPage_getNewTransactions")
                page.getTransactionsClicked()
            }
        },
        // Every move between the user's accounts the book suggests, open
        // and Rule-sorted sides (D-692), one question at a time.
        Kirigami.Action {
            icon.name: "edit-link"
            visible: page.sections.length > 0
            text: page.i18nContext.i18nc("@action:button", "Find moves between my accounts…")
            onTriggered: {
                ActionLog.taken("AccountsPage_findMoves")
                page.findMoves()
            }
        },
        // The moves KassenSturz joins by itself, to see or stop (US6.3,
        // D-749).
        Kirigami.Action {
            icon.name: "view-calendar-month"
            visible: page.sections.length > 0
            text: page.i18nContext.i18nc("@action:button", "Monthly moves…")
            onTriggered: {
                ActionLog.taken("AccountsPage_monthlyMoves")
                monthlyMoves.show()
            }
        },
        // Payments the user expects again and again (US7.1, D-769), and the
        // ones that didn't come ("Did this happen?").
        Kirigami.Action {
            icon.name: "view-calendar-upcoming-events"
            visible: page.sections.length > 0
            text: (page.plans.view?.toConfirm ?? []).length > 0
                ? page.i18nContext.i18ncp("@action:button %1 is how many planned payments wait for the user", "Planned payments (%1 to check)…", "Planned payments (%1 to check)…", page.plans.view.toConfirm.length)
                : page.i18nContext.i18nc("@action:button", "Planned payments…")
            onTriggered: {
                ActionLog.taken("AccountsPage_plannedPayments")
                plannedPayments.show()
            }
        },
        Kirigami.Action {
            icon.name: "view-visible"
            text: page.i18nContext.i18nc("@action:button", "Show closed accounts")
            checkable: true
            checked: page.showClosed
            visible: page.closedAccounts.length > 0
            onTriggered: {
                ActionLog.taken("AccountsPage_showClosedAccounts")
                page.showClosed = checked
            }
        }
    ]

    function findMoves() {
        const found = register.transferSuggestions("");
        if (found?.length > 1) {
            // Many at once: list them, with "Join all" (D-749).
            joinAllDialog.show(found);
        } else if (found?.length > 0) {
            transferPrompt.ask(found);
        } else {
            applicationWindow().showPassiveNotification(i18nContext.i18nc("@info", "No moves between your accounts to ask about. Moves you sorted yourself aren't looked for: open one of their entries and choose “Join with the other side of a move…”."), "long");
        }
    }

    TransferPrompt {
        id: transferPrompt

        register: page.register
    }

    JoinAllDialog {
        id: joinAllDialog

        register: page.register
        onOneByOne: found => transferPrompt.ask(found)
    }

    MonthlyMovesDialog {
        id: monthlyMoves

        register: page.register
    }

    PlannedPaymentsDialog {
        id: plannedPayments

        plans: page.plans
        register: page.register
    }

    // Every screen reads the book again after an Undo or a Redo
    // (`bookChanged`), and the accounts are read with it: an open Monthly
    // moves window then reads its list again too, so it never keeps saying
    // "No monthly moves yet" for a move that is back (M6-B260).
    onSectionsChanged: {
        if (monthlyMoves.visible) {
            monthlyMoves.load();
        }
    }

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
                onTriggered: {
                    ActionLog.taken("AccountsPage_addAccount_2")
                    page.addAccountClicked()
                }
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

                // The accounts' balances count entries dated after today too,
                // the Overview's figure for this month only those up to today;
                // this says how much of the total is still to come (D-587).
                AmountLabel {
                    Layout.fillWidth: true
                    visible: together.visible && (section.modelData.later ?? "") !== ""
                    text: page.i18nContext.i18nc("@label %1 is an amount of money, with its sign", "Includes entries after today: %1", section.modelData.later ?? "")
                    exactText: page.i18nContext.i18nc("@label %1 is an amount of money, with its sign", "Includes entries after today: %1", section.modelData.exactLater ?? "")
                    wrapMode: Text.Wrap
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
