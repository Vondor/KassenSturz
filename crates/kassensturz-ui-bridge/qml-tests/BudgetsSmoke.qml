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
            window.check(listed.includes("Flat"), "the second budget is listed by its own name");
            window.check(listed.some(text => text.startsWith("Starts 2027-01-25")), "the budget that has not started says when it does");
            window.check(listed.includes("Every salary month, from day 25, 800,00 €"), "a salary month names its day (D-651), got " + listed);

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
            window.check(added()[0] === "addBudget:c-food;:month::false:2026-10-01::", "adding sends what was chosen, got " + added()[0]);
            window.check(window.texts(adding, []).some(text => text.includes("amount")), "a refused amount says what to do");
            const amount = window.all(adding, "TextField", []).filter(field => field.placeholderText === "What you plan to spend");
            window.check(amount.length === 1, "one amount field, found " + amount.length);
            amount[0].text = "250,00";
            adding.save();
            window.check(added()[1] === "addBudget:c-food;:month:250,00:false:2026-10-01::", "adding sends the amount, got " + added()[1]);

            // More categories and some accounts are one deliberate tick away
            // (D-639); nothing else is sent before they are ticked.
            const boxes = () => window.all(adding, "CheckBox", []).filter(box => box.visible);
            const box = text => boxes().find(candidate => candidate.text === text);
            window.check(box("Rent") === undefined, "more categories wait for their tick");
            box("Count more categories with it").toggle();
            box("Rent").toggle();
            box("Rent").toggled();
            box("Only count money paid from some accounts").toggle();
            box("Checking").toggle();
            box("Checking").toggled();
            adding.save();
            window.check(added()[2] === "addBudget:c-food,c-rent;a-checking:month:250,00:false:2026-10-01::", "adding sends the categories and accounts, got " + added()[2]);

            // A salary month asks for its day and sends it, also for the
            // periods to start with (D-651).
            const spins = () => window.all(adding, "SpinBox", []).filter(spin => spin.visible);
            window.check(spins().length === 0, "the day waits for a salary month");
            const every = window.all(adding, "ComboBox", []).find(combo => (combo.model ?? []).includes?.("Every salary month"));
            window.check(every !== undefined, "the sheet offers a salary month");
            every.currentIndex = 1;
            window.check(spins().length === 1, "a salary month shows its day, found " + spins().length);
            spins()[0].value = 25;
            window.check(fake.calls.includes("startChoices:salary-month-25"), "the periods follow the day, calls: " + fake.calls);
            adding.save();
            window.check(added()[3] === "addBudget:c-food,c-rent;a-checking:salary-month-25:250,00:false:2026-10-01::", "adding sends the salary day, got " + added()[3]);

            // A name of the user's own is sent with it (D-664).
            const nameField = () => window.all(adding, "TextField", []).find(field => field.placeholderText === "Optional, for example Holiday");
            window.check(nameField() !== undefined, "the sheet has a name field");
            nameField().text = "Holiday";
            adding.save();
            window.check(added()[4] === "addBudget:c-food,c-rent;a-checking:salary-month-25:250,00:false:2026-10-01::Holiday", "adding sends the name, got " + added()[4]);

            // The sheet to change a budget sends the period it starts from.
            const changing = window.pageStack.push(sheet, {budgetId: "b-food"});
            window.check(!changing.adding, "the sheet changes");
            window.check(window.all(changing, "TextField", []).some(field => field.text === "400,00"), "the amount is filled in");
            // Only a new name: no amount is sent as changed, so amounts
            // planned for later periods stay (D-664, D-345).
            const changingName = window.all(changing, "TextField", []).find(field => field.placeholderText === "Optional, for example Holiday");
            changingName.text = "Weekly shop";
            changing.save();
            const changed = () => fake.calls.filter(call => call.startsWith("changeBudget:"));
            window.check(changed()[0] === "changeBudget:b-food:Weekly shop:false:2026-10-01:400,00:true", "a new name alone changes no amount, got " + changed()[0]);
            window.all(changing, "TextField", []).find(field => field.text === "400,00").text = "380,00";
            changing.save();
            window.check(changed()[1] === "changeBudget:b-food:Weekly shop:true:2026-10-01:380,00:true", "changing sends the period, got " + changed()[1]);
            // A budget with a name shows it, and its categories under it.
            const listedNames = window.texts(page, []);
            window.check(listedNames.includes("Flat") && listedNames.some(text => text.includes("Rent")), "a named budget shows its name and categories, got " + listedNames);

            // The bars.
            feed.show("this-month", []);
            const drawn = window.texts(bars, []);
            window.check(drawn.includes("Food") && drawn.includes("Fun"), "a bar for each budget, got " + drawn);
            window.check(drawn.some(text => text.includes("150,00 €")), "the spent amount is shown");
            window.check(drawn.some(text => text.includes("50,00 €") && text.includes("over")), "the overspent bar says so, got " + drawn);
            window.check(drawn.includes("Only money paid from Checking"), "a bar names the accounts it counts, got " + drawn);
            console.log(window.failed ? "budgets: FAILED" : "budgets: ok");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
