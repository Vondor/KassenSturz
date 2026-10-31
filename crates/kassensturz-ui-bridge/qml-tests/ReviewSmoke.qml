// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Opens AiChangeReviewPage.qml on stand-ins for the Rust objects (fakes/) with
// a set that holds one new chart (M6a.9, D-282) and one new budget (M6c.11,
// D-358) and checks that the page shows the chart with its title, its period
// and a preview, the budget with how its amount was computed, that nothing
// is confirmed
// before the button is pressed, that the user can correct the chart, leave it
// out and take the AI's version back (M6b.8), and that the button confirms
// the set once.
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
            window.check(page.newBudgets.length === 1, "the page holds the new budget");
            window.check(window.find(page, "AiBudgetChange", []).length === 1, "one budget card");
            window.check(all.includes("Food › Groceries"), "the budget's category is shown");
            window.check(all.includes("Every month, first period 01.11.2026 to 30.11.2026"), "how often and from when");
            window.check(all.includes("Average of the last 6 full periods, 01.05.2026 to 31.10.2026: 387,40 € per period (2.324,40 € in all), rounded up to a multiple of 10,00 €."), "how the amount was computed");

            // The user corrects the chart before confirming (D-293).
            const correction = window.find(page, "AiChangeCorrection", []);
            window.check(correction.length === 2, "one correction row for each change");
            correction[0].start();
            window.check(correction[0].editing, "the form opens");
            window.check(correction[0].fields().title === "Where the money goes", "the form starts from the AI's title");
            window.check(correction[0].fields().period === "this-year", "and its period");
            window.check(!page.editChange("new_views/0", {title: ""}), "an edit the checks refuse is not kept");
            window.check(page.correctionResult.code === "ai-changes.edit-text", "and says why");
            window.check(page.editChange("new_views/0", {title: "Spending", chart: "chart.donut", period: "last-year"}), "an edit is kept");
            window.check(page.kept[0].edited && page.kept[0].title === "Spending", "the page shows the edited change");
            window.check(window.texts(page, []).includes("Changed by you"), "marked as changed by the user");
            window.check(page.correctionResult.code === undefined, "no error left over");
            page.leaveOut("new_views/0");
            window.check(page.kept.length === 1 && page.leftOut.length === 1, "left out");
            window.check(window.texts(page, []).includes("Put it back"), "a left-out change can be put back");
            page.backToAi("new_views/0");
            window.check(page.kept.length === 2 && !page.kept[0].edited && page.kept[0].title === "Where the money goes", "back to the AI's version");
            page.confirm();
            window.check(chat.confirmed.length === 1 && chat.confirmed[0] === "7", "the button confirms the set");
            window.check(page.done, "the page is done");
            console.log(window.failed ? "review: FAILED" : "review: ok");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
