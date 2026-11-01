// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The *Update all accounts* widget (M6d.7, D-446, D-449): one button that
// fetches every linked open account from its bank, one bank after another,
// and books what came at once. Only a click on the button starts it: no
// timer, nothing when a book or the app opens (T-55). While it runs it shows
// which account is fetched and *Stop*; afterwards the summary stays until the
// user presses *Seen* or the app closes. The PIN, TAN and approval dialogs
// are the app's own (BankDialogs.qml). Text from the bank is shown as plain
// text (D-143).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Flickable {
    id: widget

    // A `BankConnection`, or null where there is none.
    property var bank: null
    // Whether bank connections can be used (D-054); when not, `bankStatus`
    // is what `BankConnection.status()` said, with the error's `code`.
    property bool bankReady: false
    property var bankStatus: ({})
    // What `BankConnection.updateAllState()` said: `linked`, `lastUpdated`,
    // `never`.
    property var state: ({})
    // The title the user gave it; empty shows the button's own words.
    property string title: ""
    // The layout is being arranged: the button is a picture of itself.
    property bool arranging: false

    readonly property bool failed: false
    readonly property int linked: state.linked ?? 0
    // Nothing to update: no account is linked to a bank, or bank connections
    // can't be used here. The widget then says how to connect a bank, or
    // why that can't be done (D-497), instead of taking no room.
    readonly property bool cannotUpdate: !bankReady || linked === 0
    readonly property bool running: bank?.updating ?? false
    readonly property var summary: bank?.updateSummary ?? ({})
    readonly property bool hasSummary: !running && summary.accounts !== undefined
    readonly property bool hasError: !running && summary.code !== undefined
    // The accounts of the run, `id` and `name`, and how many were begun.
    property var accounts: []
    property int begun: 0
    property string current: ""
    property bool showAll: false

    // A notice was clicked: open that transaction in the register.
    signal transactionClicked(string accountId, string transactionId)
    // *Connect a bank* was pressed: open the bank setup.
    signal connectBankClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    contentHeight: column.implicitHeight
    clip: true

    function start() {
        const result = bank.updateAll();
        if (result.code !== undefined) {
            startError.result = result;
            return;
        }
        startError.result = ({});
        accounts = result.accounts;
        begun = 0;
        current = "";
        showAll = false;
    }

    function noticeText(notice: var): string {
        if (notice.reason === "unusual") {
            return i18nContext.i18nc("@info %1 is a category, %2 and %3 amounts", "Unusual for %1: %2, usually about %3. Do you know this payment?", notice.category, notice.size.text, notice.usual.text);
        }
        return i18nContext.i18nc("@info", "No Rule sorted this payment. Do you know it?");
    }

    function skippedText(reason: string): string {
        switch (reason) {
        case "cancelled":
            return i18nContext.i18nc("@info", "Skipped: you cancelled the PIN or TAN for its bank.");
        case "bank-failed":
            return i18nContext.i18nc("@info", "Skipped: its bank failed for an account before it. Try again later.");
        case "stopped":
            return i18nContext.i18nc("@info", "Skipped: you pressed Stop.");
        }
        return i18nContext.i18nc("@info", "Skipped, because the one before failed. Try again.");
    }

    Connections {
        target: widget.bank
        enabled: widget.running

        function onFetchStarted(account: string) {
            widget.begun += 1;
            const found = widget.accounts.find(row => row.id === account);
            widget.current = found ? found.name : "";
        }
    }

    ColumnLayout {
        id: column

        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        // Nothing linked yet, or no bank connections here (D-497).
        Controls.Label {
            Layout.fillWidth: true
            visible: widget.cannotUpdate && !widget.running && !widget.hasSummary
            wrapMode: Text.Wrap
            text: widget.i18nContext.i18nc("@info", "Connect a bank to update your accounts here.")
        }

        Controls.Button {
            visible: widget.cannotUpdate && widget.bankReady && !widget.running && !widget.hasSummary
            enabled: !widget.arranging
            icon.name: "list-add"
            text: widget.i18nContext.i18nc("@action:button", "Connect a bank")
            Accessible.description: widget.i18nContext.i18nc("@info", "Sets up your bank's login, so your accounts can be updated from it.")
            onClicked: {
                ActionLog.taken("UpdateAllWidget_connectBank")
                widget.connectBankClicked()
            }
        }

        // Why bank connections can't be used here, and what to do instead.
        FormError {
            Layout.fillWidth: true
            result: widget.cannotUpdate && !widget.bankReady && !widget.running ? widget.bankStatus : ({})
        }

        Controls.Button {
            id: button

            visible: !widget.running && !widget.cannotUpdate
            enabled: !widget.arranging && !(widget.bank?.busy ?? true)
            icon.name: "view-refresh"
            text: widget.title !== "" ? widget.title : widget.i18nContext.i18nc("@action:button", "Update all accounts")
            Accessible.description: widget.i18nContext.i18nc("@info", "Asks your banks for the new transactions of every linked account and books them.")
            onClicked: {
                ActionLog.taken("UpdateAllWidget_update")
                widget.start()
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            textFormat: Text.PlainText
            visible: !widget.running && !widget.hasSummary && widget.linked > 0
            wrapMode: Text.Wrap
            color: Kirigami.Theme.disabledTextColor
            text: (widget.state.never ?? false) || (widget.state.lastUpdated ?? "") === ""
                ? widget.i18nContext.i18nc("@info", "Not every account has been updated yet.")
                : widget.i18nContext.i18nc("@info %1 is a date", "Last updated: %1", widget.state.lastUpdated)
        }

        FormError {
            id: startError

            Layout.fillWidth: true
        }

        RowLayout {
            Layout.fillWidth: true
            visible: widget.running

            Controls.BusyIndicator {
                running: widget.running
                Layout.preferredHeight: Kirigami.Units.iconSizes.medium
                Layout.preferredWidth: Kirigami.Units.iconSizes.medium
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: widget.begun > 0
                    ? widget.i18nContext.i18nc("@info %1 and %2 are counts, %3 the name of an account", "Updating %1 of %2: %3", widget.begun, widget.accounts.length, widget.current)
                    : widget.i18nContext.i18nc("@info", "Asking your banks…")
            }

            Controls.Button {
                text: widget.i18nContext.i18nc("@action:button", "Stop")
                icon.name: "process-stop"
                Accessible.description: widget.i18nContext.i18nc("@info", "Stops after the account being updated now.")
                onClicked: {
                    ActionLog.taken("UpdateAllWidget_stop")
                    widget.bank.stopUpdateAll()
                }
            }
        }

        FormError {
            Layout.fillWidth: true
            result: widget.hasError ? widget.summary : ({})
        }

        // The summary (D-449): what was booked, then what needs a look.
        Controls.Label {
            Layout.fillWidth: true
            textFormat: Text.PlainText
            visible: widget.hasSummary
            wrapMode: Text.Wrap
            font.bold: true
            text: widget.hasSummary
                ? widget.i18nContext.i18ncp("@info", "%1 account updated", "%1 accounts updated", widget.summary.updated)
                    + ", " + widget.i18nContext.i18ncp("@info", "%1 transaction booked.", "%1 transactions booked.", widget.summary.booked)
                    + (widget.summary.toCheck > 0 ? " " + widget.i18nContext.i18ncp("@info", "%1 to check.", "%1 to check.", widget.summary.toCheck) : "")
                : ""
        }

        Repeater {
            model: widget.hasSummary ? (widget.showAll ? widget.summary.notices : widget.summary.notices.slice(0, 20)) : []

            delegate: Controls.ItemDelegate {
                id: noticeRow

                required property var modelData

                Layout.fillWidth: true
                Accessible.name: noticeLines.text
                onClicked: {
                    ActionLog.taken("UpdateAllWidget_openTransaction")
                    widget.transactionClicked(modelData.account, modelData.transaction)
                }

                contentItem: ColumnLayout {
                    spacing: 0

                    Controls.Label {
                        id: noticeLines

                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: [noticeRow.modelData.date, noticeRow.modelData.payee, noticeRow.modelData.amount.text, noticeRow.modelData.accountName]
                            .filter(part => part !== "").join(" · ")
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        color: Kirigami.Theme.neutralTextColor
                        text: widget.noticeText(noticeRow.modelData)
                    }
                }
            }
        }

        Controls.Button {
            visible: widget.hasSummary && !widget.showAll && widget.summary.notices.length > 20
            flat: true
            text: widget.i18nContext.i18nc("@action:button", "Show all")
            onClicked: {
                ActionLog.taken("UpdateAllWidget_showAll")
                widget.showAll = true
            }
        }

        // An account that failed, was skipped or whose balance differs.
        Repeater {
            model: widget.hasSummary ? widget.summary.accounts.filter(line => line.state !== "updated" || line.balanceOff.date !== undefined) : []

            delegate: ColumnLayout {
                id: accountRow

                required property var modelData

                Layout.fillWidth: true
                spacing: 0

                Controls.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    font.bold: true
                    text: accountRow.modelData.name
                }

                FormError {
                    Layout.fillWidth: true
                    result: accountRow.modelData.state === "failed" ? accountRow.modelData.error : ({})
                }

                Controls.Label {
                    Layout.fillWidth: true
                    textFormat: Text.PlainText
                    visible: accountRow.modelData.state === "skipped"
                    wrapMode: Text.Wrap
                    text: widget.skippedText(accountRow.modelData.skipped)
                }

                Controls.Label {
                    Layout.fillWidth: true
                    textFormat: Text.PlainText
                    visible: accountRow.modelData.balanceOff.date !== undefined
                    wrapMode: Text.Wrap
                    text: visible
                        ? widget.i18nContext.i18nc("@info %1 is an amount, %2 a date, %3 the difference", "Your bank says %1 on %2; KassenSturz differs by %3. Check the account against your bank.", accountRow.modelData.balanceOff.bank.text, accountRow.modelData.balanceOff.date, accountRow.modelData.balanceOff.difference.text)
                        : ""
                }
            }
        }

        Controls.Button {
            visible: widget.hasSummary || widget.hasError
            text: widget.i18nContext.i18nc("@action:button the user has read the summary", "Seen")
            icon.name: "dialog-ok"
            onClicked: {
                ActionLog.taken("UpdateAllWidget_seen")
                widget.bank.updateSummary = ({})
            }
        }
    }
}
