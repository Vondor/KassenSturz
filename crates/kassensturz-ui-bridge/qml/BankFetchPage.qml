// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// "Get new transactions" (M4.7, D-153): the accounts that are linked to a
// bank account are fetched from the bank, one after the other, and the
// result of each is looked at like a file's: the preview shows what would
// happen, and nothing is added until the user says so (D-030, D-092). An
// account that isn't linked yet can be linked here to one of the bank
// accounts the logins list (D-148). While the bank is asked, it may want a
// PIN, a TAN or an approval; the dialogs for that are in BankDialogs.qml.
// Everything the bank says is data, shown as plain text (D-143). Rust
// returns codes and text; every sentence is written here (D-034).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property BankConnection bank
    // The account whose page sent the user here, which is ticked to begin
    // with; empty ticks every linked account.
    property string initialAccountId: ""

    // The book's open accounts with their links, from bank.fetchAccounts().
    property var rows: []
    // Which accounts are ticked, by ID.
    property var ticked: ({})
    // What the last fetch came to, by account ID.
    property var outcomes: ({})
    // The accounts whose statement the user has opened since.
    property var looked: ({})
    // The account the bank is being asked about, by ID.
    property string current: ""
    // Whether bank connections can be used at all, and why not.
    property var bankStatus: ({})
    readonly property bool usable: bankStatus.ready ?? false
    readonly property bool fetching: bank.busy
    readonly property bool anyTicked: Object.keys(ticked).some(id => ticked[id] === true)

    // The book changed: an account was linked.
    signal linked()
    // The user wants to connect a bank first.
    signal connectBankClicked()
    // The user wants to look at the statement fetched for this account.
    signal lookAtClicked(string accountId)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    BankAccountNames {
        id: accountNames
    }

    title: i18nContext.i18nc("@title", "Get new transactions")
    header: ColumnLayout {
        spacing: 0

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            position: Kirigami.InlineMessage.Position.Header
            type: Kirigami.MessageType.Information
            visible: page.bank.fake
            text: page.i18nContext.i18nc("@info", "Trying it out: a made-up bank stands in for yours. Its PIN is 12345 and its TAN 123456; nothing reaches a real bank.")
        }

        FormError {
            id: error
            Layout.fillWidth: true
        }
    }

    // Writes error codes as messages, for the result of one account.
    FormError {
        id: texts

        visible: false
    }

    function refresh() {
        const result = bank.fetchAccounts();
        if (result.code !== undefined) {
            error.result = result;
            rows = [];
            return;
        }
        rows = Array.from(result.accounts);
    }

    // Ticks the linked accounts to begin with: the one whose page sent the
    // user here, or all.
    function tickToBegin() {
        const next = {};
        for (const row of Array.from(rows)) {
            if (row.linked && (initialAccountId === "" || row.id === initialAccountId)) {
                next[row.id] = true;
            }
        }
        ticked = next;
    }

    function setTicked(id: string, on: bool) {
        const next = Object.assign({}, ticked);
        next[id] = on;
        ticked = next;
    }

    function nameOf(id: string): string {
        const row = Array.from(rows).find(row => row.id === id);
        return row ? row.name : "";
    }

    function fetchTicked() {
        error.result = ({});
        const ids = Array.from(rows).filter(row => row.linked && ticked[row.id] === true).map(row => row.id);
        outcomes = ({});
        looked = ({});
        error.result = bank.fetch(ids);
    }

    function lookAt(id: string) {
        const next = Object.assign({}, looked);
        next[id] = true;
        looked = next;
        lookAtClicked(id);
    }

    Component.onCompleted: {
        bankStatus = bank.status();
        error.result = bankStatus.ready === false ? bankStatus : ({});
        refresh();
        tickToBegin();
    }

    // Back from a statement the user looked at: the last day fetched may
    // have changed.
    onIsCurrentPageChanged: {
        if (isCurrentPage && rows.length > 0) {
            refresh();
        }
    }

    Connections {
        target: page.bank

        function onFetchStarted(account: string) {
            page.current = account;
        }

        function onFetchFinished(result: var) {
            page.current = "";
            if (result.code !== undefined) {
                error.result = result;
                return;
            }
            const next = {};
            for (const outcome of result.outcomes) {
                next[outcome.account] = outcome;
            }
            page.outcomes = next;
        }
    }

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        Controls.Label {
            Layout.fillWidth: true
            Layout.maximumWidth: Kirigami.Units.gridUnit * 36
            wrapMode: Text.Wrap
            text: page.i18nContext.i18nc("@info", "KassenSturz asks your bank for the transactions since the last time and shows them to you first. Nothing is added to your book until you say so. It only reads from your bank; it never sends money.")
        }

        Kirigami.PlaceholderMessage {
            Layout.fillWidth: true
            visible: page.rows.length === 0
            text: page.i18nContext.i18nc("@info", "You have no accounts yet")
            explanation: page.i18nContext.i18nc("@info", "Add an account first, then link it to your bank account here.")
        }

        Repeater {
            model: page.rows

            delegate: ColumnLayout {
                id: row

                required property var modelData
                readonly property var outcome: page.outcomes[modelData.id]
                readonly property bool seen: page.looked[modelData.id] === true

                Layout.fillWidth: true
                Layout.maximumWidth: Kirigami.Units.gridUnit * 36
                spacing: Kirigami.Units.smallSpacing

                Kirigami.Separator {
                    Layout.fillWidth: true
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Kirigami.Units.largeSpacing

                    Controls.CheckBox {
                        id: tick

                        visible: row.modelData.linked
                        enabled: page.usable && !page.fetching
                        checked: page.ticked[row.modelData.id] === true
                        Accessible.name: row.modelData.name
                        onToggled: page.setTicked(row.modelData.id, checked)
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0

                        // The account's name is the user's text; the bank
                        // account is the bank's: plain text (D-143).
                        Controls.Label {
                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            font.bold: true
                            text: row.modelData.name
                        }

                        Controls.Label {
                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            font: Kirigami.Theme.smallFont
                            opacity: 0.7
                            text: {
                                if (!row.modelData.linked) {
                                    return page.i18nContext.i18nc("@info", "Not linked to a bank account yet");
                                }
                                return row.modelData.lastFetched === ""
                                    ? page.i18nContext.i18nc("@info %1 is a bank account number", "Linked to %1 · not fetched yet", row.modelData.bankAccount)
                                    : page.i18nContext.i18nc("@info %1 is a bank account number, %2 a date", "Linked to %1 · last fetched on %2", row.modelData.bankAccount, row.modelData.lastFetched);
                            }
                        }
                    }

                    Controls.BusyIndicator {
                        running: visible
                        visible: page.current === row.modelData.id
                    }

                    Controls.Button {
                        visible: !row.modelData.linked
                        enabled: page.usable && !page.fetching
                        text: page.i18nContext.i18nc("@action:button", "Link to a bank account…")
                        icon.name: "link"
                        onClicked: linkDialog.show(row.modelData)
                    }
                }

                // What the last fetch of this account came to.
                FormError {
                    Layout.fillWidth: true
                    result: row.outcome && row.outcome.state === "failed" ? row.outcome.error : ({})
                }

                Controls.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    visible: row.outcome !== undefined && row.outcome.state === "skipped"
                    text: page.i18nContext.i18nc("@info", "Not fetched, because the one before failed. Try again.")
                }

                RowLayout {
                    Layout.fillWidth: true
                    visible: row.outcome !== undefined && row.outcome.state === "ready"

                    Controls.Button {
                        icon.name: "view-list-details"
                        enabled: !row.seen
                        text: row.outcome && row.outcome.transactions > 0
                            ? page.i18nContext.i18ncp("@action:button", "Look at %1 transaction from your bank", "Look at %1 transactions from your bank", row.outcome.transactions)
                            : page.i18nContext.i18nc("@action:button", "Look at what your bank sent")
                        onClicked: page.lookAt(row.modelData.id)
                    }

                    Controls.Label {
                        textFormat: Text.PlainText
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        text: row.seen
                            ? page.i18nContext.i18nc("@info", "You looked at it. Get the transactions again to see them once more.")
                            : page.i18nContext.i18nc("@info", "Nothing is added to your book before you say so.")
                    }
                }
            }
        }

        Kirigami.Separator {
            Layout.fillWidth: true
            Layout.maximumWidth: Kirigami.Units.gridUnit * 36
            visible: page.rows.length > 0
        }

        RowLayout {
            Layout.fillWidth: true
            visible: page.rows.length > 0
            spacing: Kirigami.Units.largeSpacing

            Controls.Button {
                text: page.i18nContext.i18nc("@action:button", "Get new transactions")
                icon.name: "mail-receive"
                enabled: page.usable && page.anyTicked && !page.fetching
                onClicked: page.fetchTicked()
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                visible: page.fetching
                text: page.current !== ""
                    ? page.i18nContext.i18nc("@info %1 is the name of an account", "Getting the new transactions for %1 from your bank… Your bank may ask for a TAN, or for your approval in its app.", page.nameOf(page.current))
                    : page.i18nContext.i18nc("@info", "Talking to your bank…")
            }
        }

        Controls.Button {
            visible: page.usable && page.rows.length > 0 && !Array.from(page.rows).some(row => row.linked)
            text: page.i18nContext.i18nc("@action:button", "Connect a bank…")
            icon.name: "network-connect"
            onClicked: page.connectBankClicked()
        }
    }

    // Links an account of the book to one of the bank accounts the logins
    // list (D-148).
    Kirigami.Dialog {
        id: linkDialog

        // The account to link, as the page lists it.
        property var account: ({})
        property var choices: []
        property string chosen: ""

        function show(account: var) {
            const result = page.bank.bankAccountChoices(account.id);
            linkError.result = result.code !== undefined ? result : ({});
            linkDialog.account = account;
            linkDialog.choices = Array.from(result.choices ?? []);
            linkDialog.chosen = result.suggested ?? "";
            open();
        }

        function link() {
            const result = page.bank.linkAccount(account.id, chosen);
            if (result.code !== undefined) {
                linkError.result = result;
                return;
            }
            close();
            page.linked();
            page.refresh();
            page.setTicked(account.id, true);
        }

        title: page.i18nContext.i18nc("@title:window", "Link to a bank account")
        padding: Kirigami.Units.largeSpacing
        preferredWidth: Kirigami.Units.gridUnit * 28
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Link")
                icon.name: "dialog-ok"
                enabled: linkDialog.chosen !== ""
                onTriggered: linkDialog.link()
            },
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Cancel")
                icon.name: "dialog-cancel"
                onTriggered: linkDialog.close()
            }
        ]

        ColumnLayout {
            spacing: Kirigami.Units.largeSpacing

            FormError {
                id: linkError
                Layout.fillWidth: true
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: page.i18nContext.i18nc("@info %1 is the name of an account", "Which bank account fills %1? Its new transactions are fetched into it.", linkDialog.account.name ?? "")
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                visible: linkDialog.choices.length === 0 && linkError.result.code === undefined
                text: page.i18nContext.i18nc("@info", "Your bank logins list no bank account that isn't linked yet. Connect your bank first, or check that the bank account isn't linked to another account already.")
            }

            Controls.Button {
                visible: linkDialog.choices.length === 0
                text: page.i18nContext.i18nc("@action:button", "Connect a bank…")
                icon.name: "network-connect"
                onClicked: {
                    linkDialog.close();
                    page.connectBankClicked();
                }
            }

            Controls.ButtonGroup {
                id: group
            }

            // One row for each bank account: the bank's own words, plain
            // text (D-143).
            Repeater {
                model: linkDialog.choices

                delegate: RowLayout {
                    id: choice

                    required property var modelData

                    Layout.fillWidth: true
                    spacing: Kirigami.Units.largeSpacing

                    Controls.RadioButton {
                        Controls.ButtonGroup.group: group
                        checked: choice.modelData.key === linkDialog.chosen
                        Accessible.name: accountNames.name(choice.modelData, page.bank.fake)
                        onToggled: linkDialog.chosen = choice.modelData.key
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0

                        Controls.Label {
                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            font.bold: true
                            text: accountNames.name(choice.modelData, page.bank.fake)
                        }

                        Controls.Label {
                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            font: Kirigami.Theme.smallFont
                            opacity: 0.7
                            text: page.i18nContext.i18nc("@info %1 is an IBAN or account number, %2 the name of a bank", "%1 · %2", choice.modelData.iban !== "" ? choice.modelData.iban : choice.modelData.number, choice.modelData.bank)
                        }
                    }
                }
            }
        }
    }
}
