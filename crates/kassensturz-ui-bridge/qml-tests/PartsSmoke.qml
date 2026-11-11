// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Opens PartsPage.qml, the page that breaks a transaction down into Parts, on
// a stand-in for AccountRegister (fakes/), and checks the line that says what
// the Parts add up to and the button that removes a Part (M6-B235). Before
// that fix the line asked KI18n for its message with no amount whenever
// there was no sum yet ("0 instead of 1 arguments"), and removing a Part
// went on in the destroyed row ("ReferenceError: page is not defined").
// `tools/check-panel.sh` runs it and fails on the first FAIL line or on
// either warning.

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 900
    height: 800
    visible: true
    property bool failed: false
    property var page: null

    AccountRegister {
        id: fake
    }

    Component {
        id: partsComponent

        PartsPage {
            register: fake
            accountId: "a1"
            transactionId: "t1"
        }
    }

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    function findAll(item, predicate, found) {
        if (predicate(item)) {
            found.push(item);
        }
        for (const child of item.children ?? []) {
            findAll(child, predicate, found);
        }
        return found;
    }

    function sumLine() {
        const found = findAll(page, item => item instanceof Controls.Label && item.text.startsWith("The parts add up to"), []);
        return found.length === 1 ? found[0].text : "";
    }

    function rows() {
        return findAll(page, item => item instanceof PartRow, []);
    }

    function removeButtons() {
        return findAll(page, item => item instanceof Controls.ToolButton && item.text === "Remove this part", []);
    }

    Timer {
        interval: 200
        running: true
        onTriggered: {
            page = pageStack.push(partsComponent);
            Qt.callLater(window.run);
        }
    }

    function run() {
        // Opened: 4,00 € + 6,00 € is the whole 10,00 €.
        check(rows().length === 2, "two parts at the start, got " + rows().length);
        check(sumLine() === "The parts add up to 10,00 €, the whole transaction.", "whole sum line, got: " + sumLine());

        // A form that can't be read has no sum: the line goes away and
        // asks KI18n for nothing.
        fake.unreadable = true;
        page.refresh();
        check(sumLine() === "", "no sum line for a form that can't be read, got: " + sumLine());
        fake.unreadable = false;
        page.refresh();

        // Removing the second Part: the row goes, the page works it out
        // again and says what is still to sort.
        const before = fake.previews;
        const buttons = removeButtons();
        check(buttons.length === 2, "a remove button per part, got " + buttons.length);
        buttons[1].clicked();
        check(page.partsModel.count === 1, "one part after removing, got " + page.partsModel.count);
        check(fake.previews === before + 1, "worked out again after removing");
        check(fake.lastForm.parts.length === 1 && fake.lastForm.parts[0].id === "s1", "the first part is kept");
        check(sumLine() === "The parts add up to 4,00 €. 6,00 € is still to be sorted into a part.", "missing sum line, got: " + sumLine());

        // Removing the last one too, through its row's button.
        removeButtons()[0].clicked();
        check(page.partsModel.count === 0, "no parts after removing both");
        check(sumLine() === "The parts add up to 0,00 €. 10,00 € is still to be sorted into a part.", "empty sum line, got: " + sumLine());

        // Parts over the total.
        page.addPart();
        page.partsModel.setProperty(0, "amount", "12,00");
        page.refresh();
        check(sumLine() === "The parts add up to 12,00 €, which is 2,00 € more than the total.", "over sum line, got: " + sumLine());

        console.log(failed ? "parts page: FAILED" : "parts page: all checks passed");
        Qt.exit(failed ? 1 : 0);
    }
}
