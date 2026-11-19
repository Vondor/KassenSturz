// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: new category at the end of a category list

// Opens TransactionPage.qml on stand-ins for AccountRegister and
// CategoryTree (fakes/) and checks "New category …" (D-910, #1244):
//
// - the category list ends with "New category …";
// - choosing it opens the dialog and leaves the box on what it showed;
// - a name that is refused (here: a repeated one) is said in the dialog,
//   which stays open;
// - a name that is accepted adds the category, the window is told the book
//   changed, and the box shows the new category.
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
    property int bookChanges: 0

    // As the real window: every screen reads the book again.
    function screenChangedBook() {
        bookChanges += 1;
        register.incomeChoices = register.incomeChoices.concat([{ id: "new-pets", path: "Pets" }]);
        register.modelReset();
    }

    AccountRegister {
        id: register

        incomeChoices: [{ id: "salary", path: "Salary" }]
        expenseChoices: [{ id: "food", path: "Food" }]
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
            window.check(boxes.length === 1, "one category box, got " + boxes.length);
            const box = boxes[0];
            window.check(box.textAt(box.count - 1) === "New category …", "the list ends with New category …, got " + box.textAt(box.count - 1));
            window.check(box.currentText === "Salary", "the stored category shows, got " + box.currentText);

            let dialog;
            for (let i = 0; i < page.data.length; ++i) {
                if (page.data[i].kindFixed !== undefined && page.data[i].created !== undefined) {
                    dialog = page.data[i];
                }
            }
            window.check(dialog !== undefined, "the page has the New category dialog");

            // Choosing the last entry opens the dialog; the box stays.
            box.currentIndex = box.count - 1;
            box.activated(box.count - 1);
            window.check(dialog.visible, "the dialog is open");
            window.check(box.currentText === "Salary", "the box stays on Salary, got " + box.currentText);

            const fields = window.find(dialog.contentItem, child => child instanceof Controls.TextField, []);
            window.check(fields.length === 1, "one name field, got " + fields.length);

            // A repeated name is refused and said; the dialog stays open.
            fields[0].text = "Pets";
            dialog.save();
            window.check(window.bookChanges === 1, "the first Pets changed the book once, got " + window.bookChanges);
            window.check(box.currentText === "Pets", "the box shows the new category, got " + box.currentText);
            window.check(!dialog.visible, "the dialog closed");

            // Again: this dialog's own list already has it.
            dialog.start();
            fields[0].text = "Pets";
            dialog.save();
            window.check(dialog.visible, "a repeated name keeps the dialog open");
            window.check(window.bookChanges === 1, "a refused name changes nothing, got " + window.bookChanges);

            console.log(window.failed ? "new category: FAILED" : "new category: ok");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
