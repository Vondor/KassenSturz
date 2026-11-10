// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Opens AiChangeReviewPage.qml on the stand-ins (fakes/) with the AI's sort
// with all Rules (US2.9, D-682) and checks that the card says what sorts,
// how many, why one stays and how the newest will look, as plain text; that
// the switch "Also sort again what Rules sorted before" is the user's
// correction of the call, sent as `sort_again`; that the card offers no
// other edit; and that nothing is confirmed before the button.
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
        setId: "sort"
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
            window.check(page.sorts.length === 1, "the page holds the sort");
            const cards = window.find(page, "AiRulesSortChange", []);
            window.check(cards.length === 1, "one sort card");
            let all = window.texts(page, []);
            window.check(all.includes("Sort your transactions with all Rules"), "what sorts, got " + all);
            window.check(all.includes("1 transaction will be sorted."), "how many");
            window.check(all.includes("1 transaction looks like money moved between your own accounts. Decide that in the account first; it stays as it is."), "why one stays");
            window.check(all.includes("Pet <b>Shop</b>"), "the example's name as plain text");
            window.check(all.includes("Category: Pets"), "where it goes");
            window.check(all.includes("05.10.2026 · Everyday Checking"), "when and in which account");
            window.check(!all.includes("Change"), "no edit form besides the switch");
            window.check(all.includes("Leave out"), "the sort can be left out");
            window.check(chat.confirmed.length === 0, "nothing is confirmed before the button");

            // The user switches sorting again on (D-631, D-293).
            const boxes = window.find(cards[0], "CheckBox", []);
            window.check(boxes.length === 1 && !boxes[0].checked, "the switch starts as the AI prepared it");
            boxes[0].toggle();
            boxes[0].toggled();
            window.check(chat.edits.length === 1 && chat.edits[0] === '{"sort_again":true}', "the switch edits the call, got " + JSON.stringify(chat.edits));
            window.check(page.sorts.length === 1 && page.sorts[0].sortAgain, "the page shows the run again");
            all = window.texts(page, []);
            window.check(all.includes("2 transactions will be sorted."), "with the new count, got " + all);
            window.check(all.includes("Changed by you"), "marked as the user's correction");

            page.confirm();
            window.check(chat.confirmed.length === 1 && chat.confirmed[0] === "sort", "the button confirms the set");
            console.log(window.failed ? "sort: FAILED" : "sort: ok");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
