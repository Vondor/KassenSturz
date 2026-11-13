// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: planned payments window and form

// Opens PlannedPaymentsDialog.qml on stand-ins for PlannedPayments and
// AccountRegister (fakes/) and clicks through what a person does there
// (US7.1, D-769): "Skip this time", "Book it…" with the amount and date the
// form offers, "Match an entry…" with the one entry offered, and a new
// planned payment saved from the form with its next dates shown. Then
// "Plan this again…" on TransactionPage.qml opens the same form filled from
// the entry. `tools/check-panel.sh` runs it and fails on the first FAIL line
// or on any QML warning about a binding or a type.

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 900
    height: 1000
    visible: true
    property bool failed: false
    property int step: 0

    AccountRegister {
        id: register
    }

    PlannedPayments {
        id: plans
    }

    PlannedPaymentsDialog {
        id: planned

        plans: plans
        register: register
    }

    pageStack.initialPage: TransactionPage {
        id: page

        register: register
        plans: plans
        accountId: "savings"
        transactionId: "into"
    }

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    function find(item, test, out) {
        if (!item) {
            return out;
        }
        for (let i = 0; i < item.children.length; ++i) {
            const child = item.children[i];
            if (test(child)) {
                out.push(child);
            }
            find(child, test, out);
        }
        return out;
    }

    // Any button, by what it does: `instanceof Controls.AbstractButton`
    // is false for the styled buttons with Kubuntu's Qt.
    function isButton(child) {
        return typeof child.toggle === "function" && child.clicked !== undefined;
    }

    function buttons(root, starts) {
        return find(root, child => isButton(child) && String(child.text).startsWith(starts), []);
    }

    Timer {
        interval: 300
        running: true
        repeat: true
        onTriggered: {
            window.step += 1;
            switch (window.step) {
            case 1:
                planned.show();
                break;
            case 2: {
                window.check(planned.visible, "the window opens");
                window.check(planned.toConfirm.length === 1 && planned.planList.length === 1, "one payment to confirm and one plan");
                const skip = window.buttons(window.overlay, "Skip this time");
                window.check(skip.length === 1, "one Skip button, got " + skip.length);
                skip[0]?.clicked();
                break;
            }
            case 3: {
                window.check(plans.skipped.length === 1 && plans.skipped[0] === "rent 2026-10-01", "the payment of 1 October was skipped, got " + JSON.stringify(plans.skipped));
                const book = window.buttons(window.overlay, "Book it…");
                window.check(book.length === 1, "one Book it button, got " + book.length);
                book[0]?.clicked();
                break;
            }
            case 4: {
                const fields = window.find(window.overlay, child => child instanceof Controls.TextField && child.visible && child.text === "750,00", []);
                window.check(fields.length === 1, "the amount to book is offered as 750,00, got " + fields.length);
                const confirm = window.buttons(window.overlay, "Book it").filter(button => String(button.text) === "Book it");
                window.check(confirm.length === 1, "one Book it in the form, got " + confirm.length);
                // A footer button runs its Kirigami.Action; its clicked
                // signal alone doesn't.
                confirm[0]?.action?.trigger();
                break;
            }
            case 5: {
                window.check(plans.booked.length === 1 && plans.booked[0] === "rent 2026-10-01 2026-10-19 750,00", "booked with the offered date and amount, got " + JSON.stringify(plans.booked));
                const match = window.buttons(window.overlay, "Match an entry");
                window.check(match.length === 1, "one Match button, got " + match.length);
                match[0]?.clicked();
                break;
            }
            case 6: {
                const offered = window.buttons(window.overlay, "2026-10-02");
                window.check(offered.length === 1, "the one entry is offered, got " + offered.length);
                offered[0]?.clicked();
                window.check(plans.matched.length === 1 && plans.matched[0] === "rent 2026-10-01 into", "the entry was matched, got " + JSON.stringify(plans.matched));
                const create = window.buttons(window.overlay, "New planned payment");
                window.check(create.length === 1, "one New button, got " + create.length);
                create[0]?.clicked();
                break;
            }
            case 7: {
                const form = [planned.form].filter(item => item.objectName === "planFormDialog" && item.visible);
                window.check(form.length === 1, "the form opens, got " + form.length);
                const fields = window.find(window.overlay, child => child instanceof Controls.TextField && child.visible, []);
                const name = fields.find(field => field.placeholderText === "e.g. Rent");
                window.check(name !== undefined, "the name field is there");
                if (name) {
                    name.text = "Gym";
                }
                const amount = fields.find(field => field.text === "" && field.placeholderText === "");
                window.check(amount !== undefined, "the amount field is there");
                if (amount) {
                    amount.text = "29,90";
                }
                const next = window.find(window.overlay, child => child instanceof Controls.Label && String(child.text).startsWith("Next: 2026-11-02"), []);
                window.check(next.length >= 1, "the next three dates are shown");
                const save = form[0]?.customFooterActions[0];
                window.check(save?.enabled === true, "Save is on once name and amount are typed");
                save?.trigger();
                break;
            }
            case 8: {
                window.check(plans.saved.length === 1, "one plan saved, got " + plans.saved.length);
                const saved = plans.saved[0] ?? {};
                window.check(saved.name === "Gym" && saved.amount === "29,90" && saved.accountId === "savings" && saved.outgoing === true && saved.unit === "month" && saved.every === "1" && saved.fromEntry === "", "the form as typed, got " + JSON.stringify(saved));
                planned.close();
                const again = window.buttons(page, "Plan this again");
                window.check(again.length === 1, "the entry offers Plan this again, got " + again.length);
                again[0]?.clicked();
                break;
            }
            case 9: {
                const form = [page.planAgainForm].filter(item => item.objectName === "planFormDialog" && item.visible);
                window.check(form.length === 1, "Plan this again opens the form, got " + form.length);
                window.check(form[0]?.fromEntry === "into" && form[0]?.outgoing === false && form[0]?.categoryId === "salary", "filled from the entry, got " + JSON.stringify(form[0]?.form()));
                form[0]?.customFooterActions[0].trigger();
                window.check(plans.saved.length === 2 && plans.saved[1].fromEntry === "into" && plans.saved[1].name === "Landlord", "saved with its entry, got " + JSON.stringify(plans.saved[1]));
                console.log(window.failed ? "planned payments: FAILED" : "planned payments: ok");
                Qt.exit(window.failed ? 1 : 0);
                break;
            }
            }
        }
    }
}
