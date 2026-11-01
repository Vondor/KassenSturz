// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Opens the Update all accounts widget (M6d.7, D-446) on a stand-in for
// BankConnection (fakes/) and checks that the grid takes no room for it
// while no account is linked or bank connections can't be used, that only
// the button starts it, what it says while it runs, the summary after a fake
// run with its notices and its failed account, and that Seen clears it.
// `tools/check-panel.sh` runs it.

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 900
    height: 900
    visible: true
    property bool failed: false
    property var opened: []

    BankConnection {
        id: bank
    }

    DashboardLayout {
        id: dashboard
    }

    // A grid without a bank, and one whose bank can't be used: no room for
    // the button in either.
    DashboardGrid {
        id: noBank

        width: 800
        layout: dashboard
    }

    DashboardGrid {
        id: grid

        width: 800
        layout: dashboard
        bank: bank
        bankReady: true
    }

    AddMenu {
        id: withoutBank
    }

    AddMenu {
        id: withBank

        bankReady: true
    }

    pageStack.initialPage: Kirigami.Page {
        id: page

        UpdateAllWidget {
            id: widget

            anchors.fill: parent
            bank: bank
            state: bank.updateAllState()
            onTransactionClicked: (accountId, transactionId) => window.opened.push(accountId + "/" + transactionId)
        }
    }

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    function texts(item, out) {
        const children = item.children ?? [];
        for (let i = 0; i < children.length; ++i) {
            const child = children[i];
            if (child.visible && typeof child.text === "string" && child.text !== "") {
                out.push(child.text);
            }
            texts(child, out);
        }
        return out;
    }

    function buttons(item, out) {
        const children = item.children ?? [];
        for (let i = 0; i < children.length; ++i) {
            const child = children[i];
            if (child.visible && (child.toString().includes("Button") || child.toString().includes("Delegate"))) {
                out.push(child);
            }
            buttons(child, out);
        }
        return out;
    }

    function button(text) {
        return buttons(widget, []).find(found => found.text === text);
    }

    function hasText(part) {
        return texts(widget, []).some(text => text.includes(part));
    }

    Timer {
        interval: 600
        running: true
        onTriggered: {
            const item = {type: "bank.update_all", col: 0, row: 0, cols: 12, rows: 3};
            // Hidden in view mode without a bank, or without a linked account.
            window.check(noBank.isCollapsed(item), "no bank: the widget takes no room");
            window.check(grid.isCollapsed(item), "nothing linked: the widget takes no room");
            window.check(!grid.isCollapsed({type: "kpi.card"}), "other widgets stay");
            // The Bank group is in the Add menu only while banks can be used.
            window.check(!withoutBank.groups.some(group => group.name === "Bank"), "no Bank group without bank support");
            const group = withBank.groups.find(group => group.name === "Bank");
            window.check(group !== undefined && group.rows.length === 1 && group.rows[0].kind === "bank.update_all", "the Bank group offers the button");

            bank.linked = 2;
            window.check(!grid.isCollapsed(item), "a linked account: the widget shows, linked " + grid.updateAll.linked);
            widget.state = bank.updateAllState();
            window.check(bank.calls.length === 0, "nothing starts by itself, calls: " + bank.calls);
            window.check(hasText("Last updated: 2026-10-09"), "it says when the accounts were last updated: " + texts(widget, []));

            const start = window.button("Update all accounts");
            window.check(start !== undefined && start.enabled, "the button is there and can be pressed");
            start.clicked();
            window.check(bank.calls[0] === "updateAll", "the button starts it");
            window.check(ActionLog.mode.actions.includes("UpdateAllWidget_update"), "the button says it ran");
            bank.fetchStarted("a2");
            window.check(hasText("Updating 1 of 2: Card"), "it says which account: " + texts(widget, []));
            window.button("Stop").clicked();
            window.check(bank.calls.includes("stopUpdateAll"), "Stop asks to stop");

            bank.finish({
                updated: 1, booked: 3, toCheck: 2,
                accounts: [
                    {account: "a1", name: "Current account", state: "updated", booked: 3, skipped: "", error: {}, balanceOff: {}},
                    {account: "a2", name: "Card", state: "skipped", booked: 0, skipped: "cancelled", error: {}, balanceOff: {}}
                ],
                notices: [
                    {transaction: "t1", account: "a1", accountName: "Current account", date: "2026-10-01", payee: "Example Employer",
                     amount: {text: "2.500,00 €", exact: "2.500,00 €"}, reason: "unsorted", category: "", usual: {text: "", exact: ""}, size: {text: "", exact: ""}},
                    {transaction: "t2", account: "a1", accountName: "Current account", date: "2026-10-02", payee: "Example Grocer",
                     amount: {text: "-45,23 €", exact: "-45,23 €"}, reason: "unusual", category: "Groceries", usual: {text: "4,50 €", exact: "4,50 €"}, size: {text: "45,23 €", exact: "45,23 €"}}
                ]
            });
            window.check(hasText("1 account updated, 3 transactions booked. 2 to check."), "the summary: " + texts(widget, []));
            window.check(hasText("Unusual for Groceries: 45,23 €, usually about 4,50 €. Do you know this payment?"), "the unusual notice");
            window.check(hasText("No Rule sorted this payment. Do you know it?"), "the unsorted notice");
            window.check(hasText("Skipped: you cancelled the PIN or TAN for its bank."), "the skipped account");
            const notice = buttons(widget, []).find(found => (found.Accessible.name ?? "").includes("Example Grocer"));
            window.check(notice !== undefined, "a notice can be clicked");
            notice?.clicked();
            window.check(window.opened[0] === "a1/t2", "a notice opens its transaction, got " + window.opened);

            window.button("Seen").clicked();
            window.check(bank.updateSummary.accounts === undefined, "Seen clears the summary");
            window.check(window.button("Update all accounts") !== undefined, "the button is back");

            console.log(window.failed ? "update all: FAILED" : "update all: ok");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
