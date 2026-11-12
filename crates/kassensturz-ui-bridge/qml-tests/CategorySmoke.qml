// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: transaction page shows the stored category

// Opens TransactionPage.qml on a stand-in for AccountRegister (fakes/) and
// checks that the category box always shows the category the transaction
// has (M6-B234, #719), with "Other" and "Other › Other" in the book:
//
// - after "I paid money" and back, the box keeps the category by its ID,
//   not by its place in a list that changed;
// - when the book changed while the page was open (the AI changed the
//   category) and nothing was typed, the page shows the new category;
// - a category the list doesn't offer shows as such, and saving keeps it,
//   never "Not sorted yet" in its place.
//
// `tools/check-panel.sh` runs it and fails on the first FAIL line or on any
// QML warning about a binding or a type.

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 640
    height: 900
    visible: true
    property bool failed: false

    AccountRegister {
        id: register

        incomeChoices: [
            { id: "other", path: "Other" },
            { id: "other-other", path: "Other › Other" },
            { id: "salary", path: "Salary" },
        ]
        expenseChoices: [
            { id: "food", path: "Food" },
            { id: "rent", path: "Rent" },
            { id: "fees", path: "Fees" },
        ]
    }

    pageStack.initialPage: TransactionPage {
        id: page

        register: register
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
        for (let i = 0; i < item.children.length; ++i) {
            const child = item.children[i];
            if (test(child)) {
                out.push(child);
            }
            find(child, test, out);
        }
        return out;
    }

    Timer {
        interval: 500
        running: true
        onTriggered: {
            const boxes = window.find(page, child => child instanceof Controls.ComboBox && child.count > 0 && child.textAt(0) === "Not sorted yet", []);
            const radios = window.find(page, child => child instanceof Controls.RadioButton, []);
            const paid = radios.find(radio => radio.text === "I paid money");
            const received = radios.find(radio => radio.text === "I received money");
            window.check(boxes.length === 1, "one category box, got " + boxes.length);
            const box = boxes[0];
            window.check(box.currentText === "Salary", "the stored category shows, got " + box.currentText);

            // The list for money out holds the category in another place.
            paid.toggled();
            window.check(!page.moneyIn, "money out now");
            window.check(box.currentText === "Salary", "the category stays after I paid money, got " + box.currentText);
            received.toggled();
            window.check(box.currentText === "Salary", "the category stays after I received money, got " + box.currentText);
            window.check(!page.unsaved(), "back as loaded, nothing to save");

            // The AI changed the category while the page was open.
            register.category = "other-other";
            register.modelReset();
            window.check(box.currentText === "Other › Other", "the changed category shows, got " + box.currentText);
            window.check(!page.unsaved(), "read again, nothing to save");

            // A category the list doesn't offer, e.g. in another currency.
            register.category = "elsewhere";
            register.modelReset();
            window.check(box.currentText !== "Not sorted yet", "not shown as Not sorted yet");
            window.check(box.currentText.indexOf("not in this list") >= 0, "shown as not in the list, got " + box.currentText);
            window.check(!page.unsaved(), "read again, nothing to save");
            page.save();
            window.check(register.saved.length === 1, "saved once, got " + register.saved.length);
            window.check(register.saved[0]?.category === "elsewhere", "the category was kept, got " + JSON.stringify(register.saved[0]));

            // What the user typed is never replaced by a change elsewhere.
            const amounts = window.find(page, child => child instanceof Controls.TextField && child.text === "530.00", []);
            window.check(amounts.length === 1, "one amount field, got " + amounts.length);
            amounts[0].text = "531.00";
            register.category = "salary";
            register.modelReset();
            window.check(amounts[0].text === "531.00", "the typed amount stays, got " + amounts[0].text);

            console.log(window.failed ? "category box: FAILED" : "category box: ok");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
