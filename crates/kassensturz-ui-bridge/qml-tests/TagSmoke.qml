// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Opens TransactionPage.qml on a stand-in for AccountRegister (fakes/) and
// checks the Tags field (US4.8, docs/TAGS.md §1):
//
// - the stored tag shows as a chip, and the parts' own tags on a line of
//   their own;
// - a save with the tags as loaded sends no `tags` key, so the Rust side
//   leaves them as they are;
// - typing offers the book's tags that fit and "Create …" for a new name;
//   the functions Enter, Down and Backspace call take, move and take away;
// - a save then sends the tags, a new one with an empty ID.
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

        partTags: [{ id: "tax", name: "Tax" }]
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

    function chips() {
        return window.find(page, child => child instanceof Kirigami.Chip && child.visible, []).map(chip => chip.text);
    }

    Timer {
        interval: 500
        running: true
        onTriggered: {
            const fields = window.find(page, child => child instanceof TagField, []);
            window.check(fields.length === 1, "one tags field, got " + fields.length);
            const field = fields[0];
            window.check(JSON.stringify(window.chips()) === JSON.stringify(["Garden party"]), "the stored tag shows, got " + JSON.stringify(window.chips()));
            const partLine = window.find(page, child => child instanceof Controls.Label && child.visible && child.text === "Some parts also have the tags: Tax", []);
            window.check(partLine.length === 1, "the parts' own tags show on their own line");
            window.check(!page.unsaved(), "nothing typed yet");

            // Saving as loaded leaves the tags to the Rust side.
            page.save();
            window.check(register.saved.length === 1, "saved once, got " + register.saved.length);
            window.check(register.saved[0].tags === undefined, "no tags key when nothing changed, got " + JSON.stringify(register.saved[0]));

            // Typing offers the book's tag that fits and a new one.
            const boxes = window.find(field, child => child instanceof Controls.TextField, []);
            window.check(boxes.length === 1, "one box to type a tag, got " + boxes.length);
            const box = boxes[0];
            box.forceActiveFocus();
            box.text = "ta";
            window.check(field.offered.length === 2 && field.offered[0].id === "tax" && field.offered[1].create, "offers Tax and Create, got " + JSON.stringify(field.offered));
            field.takeChosen();
            window.check(JSON.stringify(window.chips()) === JSON.stringify(["Garden party", "Tax"]), "Enter took Tax, got " + JSON.stringify(window.chips()));
            window.check(box.text === "", "the box is empty again");
            window.check(page.unsaved(), "a tag added is something to save");

            // A name the book doesn't have is made; the same name in other
            // capitals isn't offered twice.
            box.text = "Holiday 2026";
            window.check(field.offered.length === 1 && field.offered[0].create, "offers only Create, got " + JSON.stringify(field.offered));
            field.moveChoice(1);
            field.takeChosen();
            box.text = "holiday 2026";
            window.check(field.offered.length === 0, "a chosen name isn't offered again, got " + JSON.stringify(field.offered));
            box.text = "";

            // Backspace in the empty box takes away the last chip.
            field.removeAt(field.tags.length - 1);
            window.check(JSON.stringify(window.chips()) === JSON.stringify(["Garden party", "Tax"]), "Backspace took away the last tag, got " + JSON.stringify(window.chips()));
            box.text = "Holiday 2026";
            field.takeChosen();
            window.check(JSON.stringify(window.chips()) === JSON.stringify(["Garden party", "Tax", "Holiday 2026"]), "Holiday 2026 was taken, got " + JSON.stringify(window.chips()));

            page.save();
            const sent = register.saved[register.saved.length - 1].tags;
            window.check(JSON.stringify(sent) === JSON.stringify([{ id: "garden", name: "Garden party" }, { id: "tax", name: "Tax" }, { id: "", name: "Holiday 2026" }]), "the save sends the tags, got " + JSON.stringify(sent));

            console.log(window.failed ? "tags field: FAILED" : "tags field: ok");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
