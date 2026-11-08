// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One account: its balance, what the user said the bank showed and whether
// the account still holds it (D-117), renaming it, changing its type
// (D-625), comparing it with the bank, and closing it or opening it again
// (D-081).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property AccountList accounts
    // The account's ID.
    required property string accountId
    // What the Rust side says about the account, read again after a change.
    property var account: accounts.details(accountId) ?? ({})

    // "Check against my bank", with what to start the fields with: the
    // stated balance and its day when the account no longer holds it,
    // otherwise empty.
    signal checkClicked(string bank, string date)
    // The account was closed or opened again.
    signal accountClosedOrOpened()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    AccountTypes {
        id: types
    }

    title: Plain.plain(account.name ?? "")
    header: FormError {
        id: error
    }

    function reload() {
        account = accounts.details(accountId) ?? ({});
        typeField.currentIndex = typeIndex();
    }

    // Where the account's type is in the list; -1 for an account without
    // one, e.g. from another program.
    function typeIndex(): int {
        return types.all.findIndex(type => type.key === (page.account.type ?? ""));
    }

    function changeType() {
        error.result = accounts.setAccountType(accountId, types.all[typeField.currentIndex].key);
        if (error.result.code === undefined) {
            reload();
        }
    }

    // A correction made on the check page, or an undo, changes what the
    // account holds on the stated day.
    Connections {
        target: page.accounts

        function onChanged() {
            page.reload();
        }
    }

    // What the bank said, or what KassenSturz has: a debt reads "you owed".
    function said(amount: string, owes: bool): string {
        return owes ? i18nContext.i18nc("@label %1 is an amount of money", "you owed %1", amount) : amount;
    }

    function rename() {
        error.result = accounts.renameAccount(accountId, nameField.text);
        if (error.result.code === undefined) {
            reload();
        }
    }

    function closeAccount() {
        error.result = accounts.closeAccount(accountId);
        if (error.result.code === undefined) {
            accountClosedOrOpened();
        }
    }

    function reopenAccount() {
        error.result = accounts.reopenAccount(accountId);
        if (error.result.code === undefined) {
            accountClosedOrOpened();
        }
    }

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        Controls.Label {
            Layout.fillWidth: true
            visible: page.account.closed ?? false
            text: page.i18nContext.i18nc("@info", "This account is closed. It keeps its history, but takes no new transactions.")
            wrapMode: Text.Wrap
        }

        Controls.Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            text: types.nameOf(page.account.type ?? "")
            visible: text !== ""
            font: Kirigami.Theme.smallFont
        }

        Balance {
            Layout.fillWidth: true
            amount: page.account.balance ?? ""
            exactAmount: page.account.exactBalance ?? ""
            owes: page.account.owes ?? false
            font.bold: true
            font.pointSize: Kirigami.Theme.defaultFont.pointSize * 1.5
        }

        // The stated balance (D-111, D-117): what the user said the bank
        // showed on a day, and whether the account still holds it. Exact
        // amounts, since a rounded one could hide the difference.
        Controls.Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            visible: (page.account.stated ?? null) !== null && (page.account.stated.matches ?? false)
            text: visible
                ? page.i18nContext.i18nc("@info %1 is a date, %2 says how much money there was or was owed", "On %1 your bank said %2, and KassenSturz agrees.", page.account.stated.date, page.said(page.account.stated.exactBalance, page.account.stated.owes))
                : ""
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
        }

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: (page.account.stated ?? null) !== null && !(page.account.stated.matches ?? true)
            type: Kirigami.MessageType.Warning
            text: visible
                ? page.i18nContext.i18nc("@info %1 is a date, %2 and %3 say how much money there was or was owed", "On %1 your bank said %2, but KassenSturz now has %3 for that day. A transaction dated before that day may have changed it. Check against your bank to put it right.", page.account.stated.date, page.said(page.account.stated.exactBalance, page.account.stated.owes), page.said(page.account.stated.exactHeld, page.account.stated.heldOwes))
                : ""
            actions: [
                Kirigami.Action {
                    text: page.i18nContext.i18nc("@action:button", "Check against my bank…")
                    icon.name: "view-financial-account-checking"
                    enabled: !(page.account.closed ?? false)
                    onTriggered: {
                        ActionLog.taken("AccountDetailsPage_checkAgainstBank")
                        page.checkClicked(page.account.stated.exactBalance, page.account.stated.date)
                    }
                }
            ]
        }

        Kirigami.Heading {
            Layout.topMargin: Kirigami.Units.largeSpacing
            text: page.i18nContext.i18nc("@title", "Name")
            level: 2
        }

        RowLayout {
            Layout.fillWidth: true

            Controls.TextField {
                id: nameField

                Layout.fillWidth: true
                text: page.account.name ?? ""
                onAccepted: {
                    ActionLog.taken("AccountDetailsPage_nameAccount")
                    page.rename()
                }
                Accessible.name: page.i18nContext.i18nc("@label:textbox", "Name of the account")
            }

            Controls.Button {
                text: page.i18nContext.i18nc("@action:button", "Rename")
                icon.name: "edit-rename"
                enabled: nameField.text.trim() !== (page.account.name ?? "")
                onClicked: {
                    ActionLog.taken("AccountDetailsPage_rename")
                    page.rename()
                }
            }
        }

        // The type (D-625): only what sort of account it is changes, never
        // its transactions. To or from a credit card, its balance moves
        // between what the user has and what the user owes.
        Kirigami.Heading {
            Layout.topMargin: Kirigami.Units.largeSpacing
            text: page.i18nContext.i18nc("@title", "Type")
            level: 2
        }

        Controls.Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            visible: (page.account.type ?? "") === ""
            text: page.i18nContext.i18nc("@info", "This account has no type yet, for example because it came from another program. It counts as money you have. Choose the type that fits it.")
            wrapMode: Text.Wrap
        }

        RowLayout {
            Layout.fillWidth: true

            Controls.ComboBox {
                id: typeField

                // Picking a type in the list doesn't change it yet: the
                // button does.
                readonly property string chosen: currentIndex >= 0 ? types.all[currentIndex].key : ""

                Layout.fillWidth: true
                model: types.all.map(type => type.name)
                currentIndex: page.typeIndex()
                displayText: currentIndex >= 0 ? currentText : page.i18nContext.i18nc("@item:inlistbox no account type chosen yet", "Choose a type")
                Accessible.name: page.i18nContext.i18nc("@label:listbox", "Type of the account")
            }

            Controls.Button {
                text: page.i18nContext.i18nc("@action:button", "Change type")
                icon.name: "document-edit"
                enabled: typeField.chosen !== "" && typeField.chosen !== (page.account.type ?? "")
                onClicked: {
                    ActionLog.taken("AccountDetailsPage_changeType")
                    page.changeType()
                }
            }
        }

        Controls.Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            visible: typeField.chosen !== "" && typeField.chosen !== (page.account.type ?? "")
            text: !visible
                ? ""
                : (typeField.chosen === "credit-card") === (page.account.debt ?? false)
                    ? page.i18nContext.i18nc("@info", "Only the type changes. Your transactions and the balance stay as they are.")
                    : typeField.chosen === "credit-card"
                        ? page.i18nContext.i18nc("@info", "As a credit card, its balance counts as what you owe instead of what you have. Your transactions, the balance and what you are worth stay the same. You can undo this.")
                        : page.i18nContext.i18nc("@info", "Its balance then counts as what you have instead of what you owe. Your transactions, the balance and what you are worth stay the same. You can undo this.")
            wrapMode: Text.Wrap
        }

        Kirigami.Heading {
            Layout.topMargin: Kirigami.Units.largeSpacing
            text: page.i18nContext.i18nc("@title", "Check against my bank")
            level: 2
        }

        Controls.Label {
            Layout.fillWidth: true
            text: page.i18nContext.i18nc("@info", "Type what your bank says is in the account on a day, and KassenSturz tells you whether it agrees, and if not, where the difference can come from.")
            wrapMode: Text.Wrap
        }

        Controls.Button {
            text: page.i18nContext.i18nc("@action:button", "Check against my bank…")
            icon.name: "view-financial-account-checking"
            enabled: !(page.account.closed ?? false)
            onClicked: {
                ActionLog.taken("AccountDetailsPage_checkAgainstBank_2")
                page.checkClicked("", "")
            }
        }

        Kirigami.Heading {
            textFormat: Text.PlainText
            Layout.topMargin: Kirigami.Units.largeSpacing
            text: page.account.closed ? page.i18nContext.i18nc("@title", "Open again") : page.i18nContext.i18nc("@title", "Close account")
            level: 2
        }

        Controls.Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            text: page.account.closed
                ? page.i18nContext.i18nc("@info", "Open the account again to add transactions to it.")
                : page.account.canClose
                    ? page.i18nContext.i18nc("@info", "A closed account keeps its history, but takes no new transactions and leaves your list. You can open it again any time.")
                    : page.i18nContext.i18nc("@info", "You can close an account when nothing is left in it, so no money is hidden. Move what is left to another account first.")
            wrapMode: Text.Wrap
        }

        Controls.Button {
            text: Plain.plain(page.account.closed ? page.i18nContext.i18nc("@action:button", "Open account again") : page.i18nContext.i18nc("@action:button", "Close account"))
            icon.name: page.account.closed ? "document-open" : "dialog-close"
            enabled: page.account.closed || (page.account.canClose ?? false)
            onClicked: {
                ActionLog.taken("AccountDetailsPage_closeAccount")
                page.account.closed ? page.reopenAccount() : page.closeAccount()
            }
        }
    }
}
