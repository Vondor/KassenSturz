// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: parts page

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

    PlannedPayments {
        id: fakePlans
    }

    Component {
        id: partsComponent

        PartsPage {
            register: fake
            plans: fakePlans
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
        // Opened: 4,00 € + 6,00 € is the whole 10,00 €, with full parity
        // details (deliveredName, purpose, whole note, whole tags, bills,
        // per-part note, and Plan this again…).
        check(rows().length === 2, "two parts at the start, got " + rows().length);
        check(sumLine() === "The parts add up to 10,00 €, the whole transaction.", "whole sum line, got: " + sumLine());

        const delivered = findAll(page, item => item instanceof Controls.Label && item.text === "EXAMPLE SHOP GMBH", []);
        check(delivered.length === 1 && delivered[0].visible, "deliveredName label is shown");

        const purpose = findAll(page, item => item instanceof Controls.Label && item.text === "Card payment 42", []);
        check(purpose.length === 1 && purpose[0].visible, "purpose label is shown");

        const noteAreas = findAll(page, item => item instanceof Controls.TextArea, []);
        check(noteAreas.length === 1 && noteAreas[0].text === "Initial whole note", "whole note loaded");

        const partNoteFields = findAll(page, item => item instanceof Controls.TextField && item.placeholderText === "What this part was for (optional)", []);
        check(partNoteFields.length === 2, "each part has a note field, got " + partNoteFields.length);
        check(partNoteFields[0].text === "Fruit", "first part note loaded");

        // Untouched note and part note are omitted from form() so long memos
        // stay intact; editing a part note or whole note includes them.
        const initialForm = page.form();
        check(initialForm.note === undefined, "untouched whole note omitted from form()");
        check(initialForm.tags === undefined, "untouched whole tags omitted from form()");
        check(initialForm.parts[0].note === undefined, "untouched part note omitted from form()");

        // Linked bill buttons are visible and Remove link removes only the link
        // without discarding unsaved note edits.
        const openBillBtn = findAll(page, item => item instanceof Controls.Button && item.text === "Open bill", []);
        const folderBtn = findAll(page, item => item instanceof Controls.Button && item.text === "Show in folder", []);
        const removeBillBtn = findAll(page, item => item instanceof Controls.Button && item.text === "Remove link", []);
        check(openBillBtn.length === 1 && openBillBtn[0].visible, "Open bill button visible");
        check(folderBtn.length === 1 && folderBtn[0].visible, "Show in folder button visible");
        check(removeBillBtn.length === 1 && removeBillBtn[0].visible, "Remove link button visible");

        noteAreas[0].text = "Updated whole note";
        partNoteFields[1].text = "Soap";
        partNoteFields[1].textEdited();
        removeBillBtn[0].clicked();
        check(page.bills.length === 0, "bill link removed");
        check(noteAreas[0].text === "Updated whole note", "unsaved whole note kept after removing bill link");
        const editedForm = page.form();
        check(editedForm.note === "Updated whole note", "edited whole note sent in form()");
        check(editedForm.parts[0].note === undefined, "still-untouched part 1 note omitted");
        check(editedForm.parts[1].note === "Soap", "edited part 2 note sent in form()");

        // Plan this again… saves unsaved edits first and opens the dialog.
        const planBtn = findAll(page, item => item instanceof Controls.Button && item.text === "Plan this again…", []);
        check(planBtn.length === 1 && planBtn[0].visible, "Plan this again button visible");
        planBtn[0].clicked();
        check(fake.savedParts.length === 1, "Plan this again saved unsaved changes first");
        check(page.planAgainForm.visible, "PlanFormDialog opened");
        page.planAgainForm.close();

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
