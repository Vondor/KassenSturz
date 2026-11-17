// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: transaction page note hint stays in the visible width
// check-panel-desktop: transaction page note hint stays in the visible width with the desktop style

// Opens TransactionPage.qml in the smallest window the app allows and fills
// the note to the limit, so its long hint shows (B-318): the hint under the
// note must end before the page's vertical scrollbar. 720 by 720 pixels at
// 200% text is the same as 360 by 360 at the usual size, which is what the
// window has here. `tools/check-panel.sh` runs it and fails on the first
// FAIL line.

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 360
    height: 360
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
            notes[0].text = "x".repeat(600);
            const hint = window.find(page, child => child instanceof Controls.Label && String(child.text).startsWith("A note may be up to"), [])[0];
            window.check(hint !== undefined, "the hint shows when the note is full");
            const flick = page.flickable;
            const right = hint.mapToItem(flick, hint.width, 0).x;
            const bar = flick.Controls.ScrollBar.vertical;
            const room = flick.width - (bar && bar.visible ? bar.width : 0);
            window.check(right <= room, "the hint ends at " + right + ", the room is " + room);
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
