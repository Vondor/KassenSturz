// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: guided setup, step 3 (sort by name)

// Opens step 3 of the guided setup (#1031, docs/GUIDED_SETUP.md) on
// stand-ins for GuidedSort and AccountRegister (fakes/) and checks:
//
// - one box per name, with how many transactions and their sum as the Rust
//   side wrote it (D-065);
// - choosing a category sorts the whole group and the page then offers a
//   Rule for that name, filled from the group's newest transaction (D-911);
//   "No, thanks" closes the offer;
// - the list of a group ends with "New category …" (D-910);
// - a refused sort shows a message and no offer;
// - nothing has to be sorted: Skip, Continue and "End the setup" are there
//   (D-342), and with nothing left the page says so.
//
// `tools/check-panel.sh` runs it and fails on the first FAIL line or on any
// QML warning about a binding or a type.

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
    property var seen: []

    GuidedSort {
        id: fake

        more: 2
        groups: [
            { key: "out|EUR|Corner Shop", name: "Corner Shop", moneyIn: false, count: 3, total: "-36 €", exactTotal: "-35,75 €", accountId: "giro", transactionId: "t-9" },
            { key: "out|EUR|", name: "", moneyIn: false, count: 1, total: "-3 €", exactTotal: "-3,00 €", accountId: "giro", transactionId: "t-4" }
        ]
    }

    AccountRegister {
        id: accountRegister

        expenseChoices: [{ id: "food", path: "Food" }, { id: "rent", path: "Rent" }]
    }

    Component {
        id: sortPage

        GuidedSortPage {
            sorter: fake
            register: accountRegister
            step: 3
            count: 3
            onCreateRuleClicked: (accountId, transactionId) => window.seen.push("rule:" + accountId + ":" + transactionId)
            onNextClicked: window.seen.push("next")
            onSkipClicked: window.seen.push("skip")
            onEndClicked: window.seen.push("end")
        }
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

    function buttons(page, text) {
        return find(page, child => typeof child.clicked === "function" && child.down !== undefined && child.text === text, []);
    }

    function texts(page) {
        return find(page, child => child.text !== undefined && child.visible, []).map(child => child.text);
    }

    function boxes(page) {
        return find(page, child => child.currentIndex !== undefined && typeof child.activated === "function" && child.popup !== undefined, []);
    }

    property var page

    Component.onCompleted: page = pageStack.push(sortPage)

    Timer {
        interval: 1000
        running: true
        onTriggered: {
            const page = window.page;
            let shown = window.texts(page);
            window.check(shown.indexOf("Corner Shop") >= 0, "the name of a group is shown");
            window.check(shown.indexOf("Without a name") >= 0, "a group without a name says so");
            window.check(shown.indexOf("3 transactions, together -36 €") >= 0, "count and sum are shown as written, got " + shown.filter(text => text.indexOf("transaction") >= 0));
            window.check(shown.indexOf("1 transaction, -3 €") >= 0, "one transaction reads in the singular");
            window.check(shown.some(text => text.indexOf("4 transactions have none yet") >= 0), "the intro says how many have no category");
            window.check(shown.some(text => text.indexOf("2 more names") >= 0), "names not listed are counted");
            window.check(shown.indexOf("Step 3 of 3") >= 0, "the page says Step 3 of 3");

            const lists = window.boxes(page);
            window.check(lists.length === 2, "one category list per group, got " + lists.length);
            const entries = lists[0].model;
            window.check(entries[0] === "Choose a category …" && entries[entries.length - 1] === "New category …", "the list starts with the prompt and ends with New category …, got " + entries);
            window.check(entries.length === accountRegister.expenseChoices.length + 2, "the list holds the spending categories");

            // A refused sort: a message, no offer, the group stays.
            fake.refuse = "input.no-such-entry";
            lists[0].activated(1);
            window.check(page.offer === null, "a refused sort offers no Rule");
            window.check(page.shown.groups.length === 2, "a refused sort leaves the group");
            fake.refuse = "";

            // Sorting the first group: asked once, with the chosen category.
            fake.asked = [];
            window.boxes(page)[0].activated(1);
            window.check(fake.asked.length === 1 && fake.asked[0] === "out|EUR|Corner Shop>" + accountRegister.expenseChoices[0].id, "the group is sorted into the chosen category, got " + fake.asked);
            window.check(page.shown.groups.length === 1, "the sorted group is gone, got " + page.shown.groups.length);
            shown = window.texts(page);
            window.check(page.offer !== null && page.offer.sorted === 3 && page.offer.name === "Corner Shop", "the Rule offer names the group");

            // The offer: a Rule filled from the newest transaction, or not.
            page.createRuleClicked(page.offer.accountId, page.offer.transactionId);
            window.check(window.seen.indexOf("rule:giro:t-9") >= 0, "the Rule is filled from the group's newest transaction, got " + window.seen);

            // Choosing the prompt itself sorts nothing.
            fake.asked = [];
            window.boxes(page)[0].activated(0);
            window.check(fake.asked.length === 0, "the prompt sorts nothing");

            window.buttons(page, "Skip")[0].clicked();
            window.buttons(page, "Continue")[0].clicked();
            window.buttons(page, "End the setup")[0].clicked();
            window.check(["skip", "next", "end"].every(what => window.seen.indexOf(what) >= 0), "step 3 has Skip, Continue and End the setup, got " + window.seen);

            // Nothing left to sort.
            fake.groups = [];
            fake.more = 0;
            page.reload();
            window.check(window.texts(page).indexOf("Every transaction has a category. There is nothing to sort.") >= 0, "with nothing left the page says so");
            window.check(window.boxes(page).length === 0, "with nothing left there is no list");

            console.log(window.failed ? "guided sort: FAILED" : "guided sort: ok");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
