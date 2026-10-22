// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One account: its balance, renaming it, comparing it with the bank, and
// closing it or opening it again (D-081).

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

    signal checkClicked()
    // The account was closed or opened again.
    signal accountClosedOrOpened()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    AccountTypes {
        id: types
    }

    title: account.name ?? ""
    header: FormError {
        id: error
    }

    function reload() {
        account = accounts.details(accountId) ?? ({});
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
                onAccepted: page.rename()
                Accessible.name: page.i18nContext.i18nc("@label:textbox", "Name of the account")
            }

            Controls.Button {
                text: page.i18nContext.i18nc("@action:button", "Rename")
                icon.name: "edit-rename"
                enabled: nameField.text.trim() !== (page.account.name ?? "")
                onClicked: page.rename()
            }
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
            onClicked: page.checkClicked()
        }

        Kirigami.Heading {
            Layout.topMargin: Kirigami.Units.largeSpacing
            text: page.account.closed ? page.i18nContext.i18nc("@title", "Open again") : page.i18nContext.i18nc("@title", "Close account")
            level: 2
        }

        Controls.Label {
            Layout.fillWidth: true
            text: page.account.closed
                ? page.i18nContext.i18nc("@info", "Open the account again to add transactions to it.")
                : page.account.canClose
                    ? page.i18nContext.i18nc("@info", "A closed account keeps its history, but takes no new transactions and leaves your list. You can open it again any time.")
                    : page.i18nContext.i18nc("@info", "You can close an account when nothing is left in it, so no money is hidden. Move what is left to another account first.")
            wrapMode: Text.Wrap
        }

        Controls.Button {
            text: page.account.closed ? page.i18nContext.i18nc("@action:button", "Open account again") : page.i18nContext.i18nc("@action:button", "Close account")
            icon.name: page.account.closed ? "document-open" : "dialog-close"
            enabled: page.account.closed || (page.account.canClose ?? false)
            onClicked: page.account.closed ? page.reopenAccount() : page.closeAccount()
        }
    }
}
