// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: a new entry that repeats becomes a plan

// Opens TransactionPage.qml for a new entry on the stand-ins of fakes/ and
// ticks "This repeats" (B-389, D-864): "How often" and "Ends" show, the note
// and tags hide, the main button reads "Create as a plan", and it saves a
// planned payment with what was typed, the date as its first payment, and
// books nothing. `tools/check-panel.sh` runs it and fails on the first FAIL
// line or on any QML warning.

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
    property bool done: false

    AccountRegister {
        id: register
    }

    PlannedPayments {
        id: plans
    }

    pageStack.initialPage: TransactionPage {
        id: page

        register: register
        plans: plans
        accountId: "savings"
        onDone: window.done = true
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

    function isButton(child) {
        return typeof child.toggle === "function" && child.clicked !== undefined;
    }

    function shown(text) {
        return find(page, child => child.visible && isButton(child) && String(child.text) === text, []);
    }

    Timer {
        interval: 300
        running: true
        repeat: true
        onTriggered: {
            window.step += 1;
            switch (window.step) {
            case 1: {
                window.check(window.shown("Add transaction").length === 1, "a new entry is added by default");
                const box = window.shown("This repeats");
                window.check(box.length === 1, "one This repeats box, got " + box.length);
                box[0]?.toggle();
                box[0]?.toggled();
                break;
            }
            case 2: {
                window.check(page.repeating, "the box makes it repeat");
                window.check(window.shown("Create as a plan").length === 1, "the main button makes a plan");
                window.check(window.shown("Add transaction").length === 0, "no button adds an entry");
                const fields = window.find(page, child => child instanceof Controls.TextField && child.visible, []);
                window.check(fields.some(field => field.Accessible.name === "The day it ends"), "Ends shows");
                const spins = window.find(page, child => child instanceof Controls.SpinBox && child.visible, []);
                window.check(spins.length === 1, "How often shows, got " + spins.length);
                const amount = fields.find(field => field.Kirigami.FormData.label === "How much?");
                window.check(amount !== undefined, "the amount field is there");
                if (amount) {
                    amount.text = "29,90";
                }
                const party = fields.find(field => String(field.Kirigami.FormData.label).startsWith("To"));
                window.check(party !== undefined, "the To field is there");
                if (party) {
                    party.text = "Example Gym";
                }
                window.shown("Create as a plan")[0]?.clicked();
                break;
            }
            case 3: {
                window.check(plans.saved.length === 1, "one plan saved, got " + plans.saved.length);
                const saved = plans.saved[0] ?? {};
                window.check(saved.name === "Example Gym" && saved.amount === "29,90" && saved.accountId === "savings" && saved.outgoing === true && saved.unit === "month" && saved.every === "1" && saved.firstDue === "2026-09-01" && saved.ends === "" && saved.fromEntry === "", "the plan as typed, got " + JSON.stringify(saved));
                window.check(window.done, "the page is done after the plan");
                window.check(ActionLog.mode.actions.includes("TransactionPage_repeats"), "the box says it ran, got " + ActionLog.mode.actions);
                console.log(window.failed ? "repeat: FAILED" : "repeat: ok");
                Qt.exit(window.failed ? 1 : 0);
                break;
            }
            }
        }
    }
}
