// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Opens BudgetsPage.qml, BudgetPage.qml and BudgetBars.qml on stand-ins for
// the Rust objects BudgetBook and BookBudgets (fakes/) and checks that the
// page lists the budgets, that Stop and Delete send the budget's id (Delete
// only after the dialog), that the sheet sends what the user chose, that a
// refusal of the book is shown as a sentence, and that the bars say what is
// spent and left (M6c.5). `tools/check-panel.sh` runs it and fails on the
// first FAIL line or on any QML warning about a binding or a type.

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
    property var asked: []

    BudgetBook {
        id: fake
    }

    BookBudgets {
        id: feed
    }

    Component {
        id: sheet

        BudgetPage {
            budgets: fake
        }
    }

    pageStack.initialPage: BudgetsPage {
        id: page

        budgets: fake
        onAddBudgetClicked: window.asked.push("add")
        onChangeBudgetClicked: id => window.asked.push("change:" + id)
    }

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

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

    function all(item, part, out) {
        for (let i = 0; i < item.children.length; ++i) {
            const child = item.children[i];
            if (child.toString().includes(part)) {
                out.push(child);
            }
            all(child, part, out);
        }
        return out;
    }

    Item {
        id: barsHost

        width: 600
        height: 400
        z: -1

        BudgetBars {
            id: bars

            width: parent.width
            shown: feed.view.bars
            budgetCount: feed.view.count
        }
    }

    Timer {
        interval: 600
        running: true
        onTriggered: {
            const listed = window.texts(page, []);
            window.check(listed.includes("Food"), "the first budget is listed");
            window.check(listed.includes("Rent"), "the second budget is listed");
            window.check(listed.some(text => text.startsWith("Starts 2027-01-01")), "the budget that has not started says when it does");

            // Stop is offered for the running budget only.
            const buttons = window.all(page, "ToolButton", []).filter(button => button.visible);
            const stops = buttons.filter(button => button.text === "Stop");
            window.check(stops.length === 1, "one Stop button, found " + stops.length);
            stops[0].clicked();
            window.check(fake.calls[0] === "stopBudget:b-food", "Stop sends the id, got " + fake.calls[0]);
            window.check(ActionLog.mode.actions.includes("BudgetsPage_stop"), "Stop says it ran (M6c.D), got " + ActionLog.mode.actions);
            buttons.filter(button => button.text === "Change")[0].clicked();
            window.check(window.asked.includes("change:b-food"), "Change asks for the sheet");

            // Delete opens the dialog and sends nothing yet.
            buttons.filter(button => button.text === "Delete")[0].clicked();
            window.check(!fake.calls.some(call => call.startsWith("removeBudget")), "Delete waits for the dialog, calls: " + fake.calls);

            // The sheet to add: the sentence of a refusal shows.
            const adding = window.pageStack.push(sheet);
            window.check(adding.adding, "the sheet adds");
            adding.save();
            const added = () => fake.calls.filter(call => call.startsWith("addBudget:"));
            window.check(added()[0] === "addBudget:c-food:month::false:2026-10-01:", "adding sends what was chosen, got " + added()[0]);
            window.check(window.texts(adding, []).some(text => text.includes("amount")), "a refused amount says what to do");
            const amount = window.all(adding, "TextField", []).filter(field => field.placeholderText === "What you plan to spend");
            window.check(amount.length === 1, "one amount field, found " + amount.length);
            amount[0].text = "250,00";
            adding.save();
            window.check(added()[1] === "addBudget:c-food:month:250,00:false:2026-10-01:", "adding sends the amount, got " + added()[1]);

            // The sheet to change a budget sends the period it starts from.
            const changing = window.pageStack.push(sheet, {budgetId: "b-food"});
            window.check(!changing.adding, "the sheet changes");
            window.check(window.all(changing, "TextField", []).some(field => field.text === "400,00"), "the amount is filled in");
            changing.save();
            const changed = fake.calls.filter(call => call.startsWith("changeBudget:"));
            window.check(changed[0] === "changeBudget:b-food:2026-10-01:400,00:true", "changing sends the period, got " + changed[0]);

            // The bars.
            feed.show("this-month", []);
            const drawn = window.texts(bars, []);
            window.check(drawn.includes("Food") && drawn.includes("Fun"), "a bar for each budget, got " + drawn);
            window.check(drawn.some(text => text.includes("150,00 €")), "the spent amount is shown");
            window.check(drawn.some(text => text.includes("50,00 €") && text.includes("over")), "the overspent bar says so, got " + drawn);
            console.log(window.failed ? "budgets: FAILED" : "budgets: ok");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
