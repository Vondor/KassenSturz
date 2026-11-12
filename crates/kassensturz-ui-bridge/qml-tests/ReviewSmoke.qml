// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Opens AiChangeReviewPage.qml on stand-ins for the Rust objects (fakes/) with
// a set that holds one new chart (M6a.9, D-282), one new budget (M6c.11,
// D-358) and one budget that counts other categories (D-690) and checks that the page shows the chart with its title, its period
// and a preview, the budget with how its amount was computed, that nothing
// is confirmed
// before the button is pressed, that the user can correct the chart, leave it
// out and take the AI's version back (M6b.8), and that the button confirms
// the set once, and that the reasons for throwing it away are offered and
// passed on as keywords (US2.17).
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
            window.check(page.newViews.length === 2, "the page holds the new chart and the new card");
            const all = window.texts(page, []);
            window.check(all.includes("Where the money goes"), "the title is shown");
            window.check(all.includes("This year"), "the period is shown in words");
            // What the chart leaves out is said, so a title can't hide it (D-729).
            window.check(all.includes("Shows only: Category is one of Food, Rent; Closed is no"), "the filters in words, got " + all);
            window.check(all.filter(text => text.startsWith("Shows only:")).length === 1, "a chart without filters says nothing");
            window.check(window.find(page, "ComponentPreview", []).length === 2, "one preview each");
            // A formula the AI wrote is shown in words before it is
            // confirmed, over a period that moves with today (US1.17, D-650).
            window.check(all.includes("The salary month before, from day 25"), "a rolling period is named");
            window.check(all.includes("Formula: =Pets - Travel"), "the formula is shown as written");
            window.check(all.includes("Counts Pets (with Vet, Cat food and 2 more), Travel in September 2026"), "what it counts, in words");
            window.check(chat.confirmed.length === 0, "nothing is confirmed before the button");
            window.check(page.newBudgets.length === 1, "the page holds the new budget");
            window.check(page.editedBudgets.length === 1, "the page holds the changed budget");
            window.check(window.find(page, "AiBudgetChange", []).length === 2, "one card for each budget");
            window.check(all.includes("Food › Groceries"), "the budget's category is shown");
            window.check(all.includes("Every salary month, from day 25, first period 25.10.2026 to 24.11.2026"), "how often, from which day (D-651) and from when, got " + all);
            // A changed budget shows what it counts before and after, and
            // that the past keeps its results (D-690).
            window.check(all.includes("Now it counts Food."), "what it counts now, got " + all);
            window.check(all.includes("From this change on it counts Food, Fun, only money paid from Everyday Checking."), "what it counts from the change on");
            window.check(all.includes("Earlier periods keep counting what they counted, so their results don't change. Every amount stays as it was."), "the past stays");
            window.check(!all.includes("Only the name changes. Every amount stays as it was."), "not taken for a new name");
            window.check(all.includes("Every month. The change applies from the period 01.11.2026 to 30.11.2026 on; earlier periods stay as they were."), "from which period");
            window.check(all.includes("Average of the last 6 full periods, 01.05.2026 to 31.10.2026: 387,40 € per period (2.324,40 € in all), rounded up to a multiple of 10,00 €."), "how the amount was computed");

            // The user corrects the chart before confirming (D-293).
            const correction = window.find(page, "AiChangeCorrection", []);
            window.check(correction.length === 4, "one correction row for each change");
            correction[1].start();
            window.check(correction[1].fields().period === "last-salary-month-25", "the form keeps a rolling period");
            correction[1].editing = false;
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
            window.check(page.kept.length === 3 && page.leftOut.length === 1, "left out");
            window.check(window.texts(page, []).includes("Put it back"), "a left-out change can be put back");
            page.backToAi("new_views/0");
            window.check(page.kept.length === 4 && !page.kept[0].edited && page.kept[0].title === "Where the money goes", "back to the AI's version");
            // Optional reasons for throwing the set away (D-678): fixed
            // choices, and only their keywords are passed on.
            const shown = window.texts(page.footer, []);
            window.check(shown.includes("Wrong category") && shown.includes("Not what I asked for"), "the reasons are offered, got " + shown);
            page.toggleReason("wrong_category", true);
            page.toggleReason("too_many_changes", true);
            page.toggleReason("wrong_category", false);
            page.toggleReason("wrong_amount", true);
            window.check(JSON.stringify(page.reasons) === '["too_many_changes","wrong_amount"]', "a reason toggles, got " + JSON.stringify(page.reasons));
            page.throwAway();
            window.check(chat.discardedReasons.length === 1 && chat.discardedReasons[0] === '["too_many_changes","wrong_amount"]', "throwing away passes the keywords");
            page.confirm();
            window.check(chat.confirmed.length === 1 && chat.confirmed[0] === "7", "the button confirms the set");
            window.check(page.done, "the page is done");
            console.log(window.failed ? "review: FAILED" : "review: ok");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
