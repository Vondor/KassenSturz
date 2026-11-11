// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Opens TransactionPage.qml on a stand-in for AccountRegister (fakes/) and
// checks that a note typed but not saved is saved before "Join with the
// other side of a move…" lists the partners (M6-B227): the page closes after
// a join, and the note was lost without a word. A second click with nothing
// new typed saves nothing again. `tools/check-panel.sh` runs it and fails on
// the first FAIL line or on any QML warning about a binding or a type.

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
            const notes = window.find(page, child => child instanceof Controls.TextArea, []);
            const joins = window.find(page, child => child instanceof Controls.Button && String(child.text).startsWith("Join with the other side"), []);
            window.check(notes.length === 1, "one note field, got " + notes.length);
            window.check(joins.length === 1, "one join button, got " + joins.length);
            window.check(notes[0].text === "Algae bars 33", "the stored note shows, got " + notes[0].text);
            window.check(!page.unsaved(), "nothing typed yet");

            notes[0].text = "Algae bars 33\nFor the dome";
            window.check(page.unsaved(), "the typed note counts as not saved");
            joins[0].clicked();
            window.check(register.saved.length === 1, "the typed note was saved first, saves: " + register.saved.length);
            window.check(register.saved[0]?.note === "Algae bars 33\nFor the dome", "with the typed note, got " + JSON.stringify(register.saved[0]));
            window.check(register.notesWhenPartnersRead[0] === "Algae bars 33\nFor the dome", "the partners were read after the save, got " + register.notesWhenPartnersRead);
            window.check(!page.unsaved(), "saved, nothing left to save");

            joins[0].clicked();
            window.check(register.saved.length === 1, "nothing new typed, nothing saved again");
            window.check(register.notesWhenPartnersRead.length === 2, "the partners were read again");

            console.log(window.failed ? "transaction page: FAILED" : "transaction page: ok");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
