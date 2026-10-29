// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Opens AiChangeReviewPage.qml on stand-ins for the Rust objects (fakes/) with
// a set that holds one new chart (M6a.9, D-282) and checks that the page shows
// it with its title, its period and a preview, that nothing is confirmed
// before the button is pressed, and that the button confirms the set once.
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

    AiSwitch {
        id: ai
    }

    AiChat {
        id: chat
    }

    pageStack.initialPage: AiChangeReviewPage {
        id: page

        ai: ai
        chat: chat
        setId: "7"
    }

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    function find(item, type, out) {
        for (let i = 0; i < item.children.length; ++i) {
            const child = item.children[i];
            if (child.toString().startsWith(type + "_") || child.toString().startsWith(type + "(")) {
                out.push(child);
            }
            find(child, type, out);
        }
        return out;
    }

    // The text of every visible item below `item` that has one.
    function texts(item, out) {
        for (let i = 0; i < item.children.length; ++i) {
            const child = item.children[i];
            if (child.visible && typeof child.text === "string") {
                out.push(child.text);
            }
            texts(child, out);
        }
        return out;
    }

    Timer {
        interval: 500
        running: true
        onTriggered: {
            window.check(page.newViews.length === 1, "the page holds the new chart");
            const all = window.texts(page, []);
            window.check(all.includes("Where the money goes"), "the title is shown");
            window.check(all.includes("This year"), "the period is shown in words");
            window.check(window.find(page, "ComponentPreview", []).length === 1, "one preview");
            window.check(chat.confirmed.length === 0, "nothing is confirmed before the button");
            page.confirm();
            window.check(chat.confirmed.length === 1 && chat.confirmed[0] === "7", "the button confirms the set");
            window.check(page.done, "the page is done");
            console.log(window.failed ? "review: FAILED" : "review: ok");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
