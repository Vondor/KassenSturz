// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Opens StatisticsPage.qml on stand-ins for the Rust objects (fakes/) and
// clicks through it (M6a.7): a chart shows, a view is saved, a second one
// with the same name is refused with a message, a key figure replaces the
// chart when the type changes, and deleting the view that is open ends
// "changing" it. `tools/check-panel.sh` runs it and fails on the first FAIL
// line or on any QML warning about a binding or a type.

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
    property int step: 0

    SavedViews {
        id: saved
    }

    CategoryGroups {
        id: groups
    }

    pageStack.initialPage: StatisticsPage {
        id: page

        saved: saved
        groupsSource: groups
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

    function shown(type) {
        const all = find(page, type, []);

        return all.filter(item => item.visible).length;
    }

    function nameField() {
        // The last one: the saved views' own box comes after the wizard's.
        return find(page, "TextField", []).filter(f => f.placeholderText !== "" && f.placeholderText.indexOf("01.01") < 0).pop();
    }

    Timer {
        interval: 700
        running: true
        repeat: true
        onTriggered: {
            switch (window.step++) {
            case 0:
                window.check(window.shown("ChartView") === 1, "a chart is shown at first: " + window.shown("ChartView"));
                window.check(page.views.length === 0, "no saved views at first");
                window.nameField().text = "  Rent chart ";
                page.saveNew();
                break;
            case 1:
                window.check(page.views.length === 1 && page.views[0].name === "Rent chart", "the view is saved with its name trimmed");
                window.check(page.openId === "v1", "the saved view is the one being changed");
                window.nameField().text = "rent CHART";
                page.saveNew();
                break;
            case 2:
                window.check(page.header.result.code === "store.view-name-taken", "a second view with the same name is refused");
                window.check(page.header.visible && page.header.text.indexOf("rent CHART") >= 0 || page.header.text.length > 0, "and the message is shown");
                window.check(page.views.length === 1, "and nothing was added");
                // Change the type: the chart gives way to a key figure.
                find(page, "ComboBox", []).filter(box => box.textRole === "name" && box.model.length === page.types.length)[0].activated(page.types.indexOf("kpi.card"));
                break;
            case 3:
                window.check(window.shown("ChartView") === 0, "the chart is gone");
                window.check(window.shown("KpiCard") === 1, "a key figure is shown: " + window.shown("KpiCard"));
                page.saveChanges();
                break;
            case 4:
                window.check(saved.calls.indexOf("update:v1:last-12-months") >= 0, "the changes are saved to the open view");
                page.openView("v1");
                break;
            case 5:
                window.check(window.shown("KpiCard") === 1, "opening it shows the key figure again");
                saved.remove("v1");
                break;
            case 6:
                window.check(page.openId === "", "a deleted view is no longer the one being changed");
                window.check(page.views.length === 0, "and the list is empty");
                {
                    // The last choice of the period is a range of dates the
                    // user types; the quick choices stay (D-332).
                    const choice = find(page, "PeriodChoice", []).filter(c => c.options.length === page.periods.length).pop();
                    const box = find(choice, "ComboBox", [])[0];
                    window.check(box.count === page.periods.length + 1, "the period box has the quick choices and the range");
                    window.check(!find(choice, "TextField", [])[0].visible, "no field for dates at first");
                    box.activated(page.periods.length);
                }
                break;
            case 7:
                {
                    const choice = find(page, "PeriodChoice", []).filter(c => c.options.length === page.periods.length).pop();
                    const field = find(choice, "TextField", [])[0];
                    window.check(field.visible, "the field for the dates appears");
                    field.text = "nonsense";
                    field.editingFinished();
                    window.check(choice.problem.code === "input.bad-range-text" && page.period === "last-12-months", "text that is no range changes nothing");
                    field.text = "01.01-02.02";
                    field.editingFinished();
                    window.check(page.period === "01.01..02.02", "a range sets the period: " + page.period);
                    window.check(field.text === "01.01-02.02" && choice.problem.code === undefined, "and is shown as typed");
                    find(choice, "ComboBox", [])[0].activated(0);
                }
                break;
            case 8:
                window.check(page.period === "this-month", "a quick choice replaces the range: " + page.period);
                // A new view starts in the wizard (D-335).
                page.startNew();
                break;
            case 9:
                {
                    const wizard = find(page, "StatisticWizard", [])[0];
                    window.check(page.wizard && wizard.visible && wizard.step === 0, "a new view starts in the wizard at its first step");
                    const types = find(wizard, "AbstractButton", []).filter(button => button.checkable);
                    window.check(types.length === 2, "the first kind offers its two ways to show: " + types.length);
                    const entries = find(wizard, "ItemDelegate", []);
                    window.check(entries.length === wizard.steps + wizard.kinds.length, "a list of steps and a list of kinds: " + entries.length);
                    entries[wizard.steps + 1].clicked();
                }
                break;
            case 10:
                {
                    const wizard = find(page, "StatisticWizard", [])[0];
                    window.check(wizard.workbench.view.type === "chart.pie", "choosing a kind sets its first type: " + wizard.workbench.view.type);
                    window.check(window.shown("ChartView") === 1, "and the preview shows it");
                    wizard.step = 0;
                    wizard.next();
                    window.check(wizard.step === (wizard.usable(1) ? 1 : 2), "Next goes to the categories, or past them when there is nothing to choose: " + wizard.step);
                    wizard.goTo(2);
                    window.check(wizard.step === 2, "the last step is reachable by its list");
                    wizard.step = 0;
                    wizard.finish();
                    window.check(wizard.step === 2 && page.openId === "", "Finish without a name asks for it first");
                    wizard.finishRequested("Wizard view");
                }
                break;
            case 11:
                window.check(page.openId !== "" && page.openName === "Wizard view", "the wizard saves the view under its name");
                window.check(!page.wizard, "and the page shows every setting from then on");
                // The board of columns (D-340): reopen the view at the step.
                page.changeStep(1);
                break;
            case 12:
                {
                    const wizard = find(page, "StatisticWizard", [])[0];
                    const board = find(wizard, "ColumnBoard", [])[0];
                    window.check(page.wizard && wizard.step === 1 && board !== undefined && board.visible, "the saved view reopens the wizard at the board");
                    // Food and Groceries inside it, Rent, and one group of money spent.
                    window.check(board.unused.length === 4, "everything starts in Unused: " + board.unused.length);
                    window.check(!board.addColumn("  "), "a column needs a name");
                    window.check(board.addColumn("Fast food") && board.addColumn("Home"), "two columns are added");
                    window.check(!board.addColumn("fast FOOD") && board.problem !== "", "a name another column has is refused");
                    board.moveCard("c-food", -1, 0);
                    board.moveCard("c-rent", -1, 1);
                    board.alsoPut("c-rent", 0);
                }
                break;
            case 13:
                {
                    const wizard = find(page, "StatisticWizard", [])[0];
                    const board = find(wizard, "ColumnBoard", [])[0];
                    const columns = JSON.parse(wizard.workbench.view.columns);
                    window.check(columns.length === 2 && columns[0].name === "Fast food", "the workbench holds the columns: " + wizard.workbench.view.columns);
                    window.check(columns[0].members.join() === "c-food,c-rent" && columns[1].members.join() === "c-rent", "a card moves, and may also sit in a second column");
                    // Groceries lies inside Food, so it is counted and not offered again.
                    window.check(board.unused.map(card => card.id).join() === "g-fixed", "what is counted leaves Unused: " + board.unused.map(card => card.id));
                    board.moveCard("c-food", 0, -1);
                    board.removeColumn(1);
                }
                break;
            case 14:
                {
                    const wizard = find(page, "StatisticWizard", [])[0];
                    const columns = JSON.parse(wizard.workbench.view.columns);
                    window.check(columns.length === 1 && columns[0].members.join() === "c-rent", "a card goes back to Unused and a column can be removed");
                    window.check(page.saveChanges(), "the changes of the open view are saved with its columns");
                    wizard.cancelled();
                }
                break;
            case 15:
                {
                    const wizard = find(page, "StatisticWizard", [])[0];
                    window.check(JSON.parse(wizard.workbench.view.columns).length === 1, "the saved view comes back with its column");
                    Qt.exit(window.failed ? 1 : 0);
                }
                break;
            }
        }
    }
}
