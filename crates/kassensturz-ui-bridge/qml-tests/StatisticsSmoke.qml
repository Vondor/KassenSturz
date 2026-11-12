// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: statistics page

// Opens StatisticsPage.qml on stand-ins for the Rust objects (fakes/) and
// clicks through it (M6a.7): a chart shows, a view is saved, a second one
// with the same name is refused with a message, a key figure replaces the
// chart when the type changes, and deleting the view that is open ends
// "changing" it; a new one without a name gets "Statistic 1", the saved views
// stay in view in the wizard and each can be changed step by step, an
// unfinished statistic is reported as a draft and comes back, and a chart of
// the Overview is changed and handed back (M6-B164). `tools/check-panel.sh` runs it and fails on the first FAIL
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
    // What the page last said was left unfinished (M6-B164).
    property var lastDraft: ({})
    // A page opened on a chart of the Overview, and what it handed back.
    property var overviewPage: null
    property var handedBack: ({})
    property bool overviewCancelled: false
    readonly property var store: saved

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
        onDraftLeft: left => window.lastDraft = left
    }

    Component {
        id: overviewBuilder

        StatisticsPage {
            // Not `saved: saved`: inside the page that names its own property.
            saved: window.store
            groupsSource: groups
            overviewComponentId: "w7"
            initialComponent: JSON.stringify({type: "chart.line", dataset: "spending_by_category", params: {}, props: {title: "On the page"}})
            initialPeriod: "page"
            onOverviewChangeFinished: (id, json) => window.handedBack = {id: id, json: json}
            onOverviewChangeCancelled: window.overviewCancelled = true
            onDraftLeft: left => window.lastDraft = left
        }
    }

    Component {
        id: draftBuilder

        StatisticsPage {
            // Not `saved: saved`: inside the page that names its own property.
            saved: window.store
            groupsSource: groups
            draft: ({
                component: JSON.stringify({type: "chart.donut", dataset: "spending_by_category", params: {}, props: {title: "Half done"}}),
                period: "last-year",
                name: "Kept name",
                step: 2
            })
        }
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
                // The type box, not a period box: those name a period `word`
                // in each entry, and may have as many entries.
                find(page, "ComboBox", []).filter(box => box.textRole === "name" && box.model.length === page.types.length && Array.from(box.model).every(entry => entry.word === undefined))[0].activated(page.types.indexOf("kpi.card"));
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
                // The row's own Delete button: removing the view destroys the
                // row, and what the button did after that gave
                // "ReferenceError: page is not defined" (M6-B199).
                find(page, "ToolButton", []).filter(button => button.icon.name === "edit-delete" && button.text === "Delete")[0].clicked();
                break;
            case 6:
                window.check(page.openId === "", "a deleted view is no longer the one being changed");
                window.check(page.views.length === 0, "and the list is empty");
                {
                    // After the quick choices come the last months, the last
                    // days (D-717), a salary month (D-644, D-645) and a range
                    // of dates the user types (D-332).
                    const choice = find(page, "PeriodChoice", []).filter(c => c.options.length === page.periods.length).pop();
                    const box = find(choice, "ComboBox", [])[0];
                    window.check(box.count === page.periods.length + 4, "the period box has the quick choices, the rolling ones and the range");
                    window.check(!find(choice, "TextField", [])[0].visible, "no field for dates at first");
                    window.check(find(choice, "SpinBox", []).every(spin => !spin.visible), "no numbers to choose at first");
                    box.activated(page.periods.length);
                    window.check(page.period === "last-3-complete-months", "the last months start as three complete ones: " + page.period);
                    window.check(box.currentIndex === page.periods.length, "and the box shows that choice");
                    const spins = find(choice, "SpinBox", []);
                    window.check(spins[0].visible && spins[0].value === 3 && !spins[1].visible && !spins[2].visible, "the number of months can be chosen");
                    const complete = find(choice, "CheckBox", [])[0];
                    window.check(complete.visible && complete.checked, "complete months only, at first");
                    complete.toggle();
                    complete.toggled();
                    window.check(page.period === "last-3-months", "this month can count too: " + page.period);
                    box.activated(page.periods.length + 1);
                    window.check(page.period === "last-7-days", "the last days start as a week: " + page.period);
                    window.check(box.currentIndex === page.periods.length + 1, "and the box shows that choice");
                    window.check(spins[1].visible && spins[1].value === 7 && !spins[0].visible, "the number of days can be chosen");
                    spins[1].value = 30;
                    spins[1].valueModified();
                    window.check(page.period === "last-30-days", "a number of days moves it: " + page.period);
                    box.activated(page.periods.length + 2);
                    window.check(page.period === "this-salary-month-1", "a salary month starts on day 1: " + page.period);
                    window.check(spins[2].visible && !spins[0].visible && !spins[1].visible, "and its day can be chosen");
                    spins[2].value = 28;
                    spins[2].valueModified();
                    window.check(page.period === "this-salary-month-28", "a day moves it: " + page.period);
                    const which = find(choice, "ComboBox", [])[1];
                    which.activated(1);
                    window.check(page.period === "last-salary-month-28", "and the one before can be chosen: " + page.period);
                    box.activated(page.periods.indexOf("last-12-months"));
                    window.check(page.period === "last-12-months" && !spins[2].visible, "a quick choice replaces it");
                    box.activated(page.periods.length + 3);
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
                    window.check(window.lastDraft.component !== undefined && JSON.parse(window.lastDraft.component).type === "chart.pie" && window.lastDraft.step === 2, "the unfinished statistic is reported as a draft: " + JSON.stringify(window.lastDraft));
                    const headings = find(page, "Heading", []).filter(heading => heading.visible && heading.text === "Saved views");
                    window.check(headings.length === 1, "the saved views stay in view in the wizard");
                    window.check(find(page, "Button", []).filter(button => button.visible && button.text === "Edit palette…").length === 1, "the last step offers Edit palette…");
                    wizard.step = 0;
                    wizard.finish();
                }
                break;
            case 11:
                window.check(page.openId !== "" && page.openName === "Statistic 1", "Finish without a name keeps the view as Statistic 1: " + page.openName);
                window.check(page.defaultName() === "Statistic 2", "the next one would be Statistic 2: " + page.defaultName());
                window.check(window.lastDraft.component === undefined, "and nothing is left unfinished");
                window.check(!page.wizard, "and the page shows every setting from then on");
                window.check(find(page, "Button", []).filter(button => button.visible && button.text === "Put it on the Overview").length === 1, "the kept view can be put on the Overview");
                window.check(find(page, "ToolButton", []).filter(button => button.visible && button.text === "Change step by step").length === 1, "each saved view can be changed step by step");
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
                    // A card can start a column, so its menu is never empty.
                    const noColumn = board.choicesFor("c-groceries", -1);
                    window.check(noColumn.length === 1 && noColumn[0].create === true, "the menu of a card offers a new column when there is none yet");
                    window.check(board.freeName("") === "New column" && board.freeName("Pets") === "Pets", "a column made from a card is named by what was typed, or New column");
                    board.moveToNew("c-groceries", -1, "");
                    window.check(board.columns.length === 1 && board.columns[0].members.join() === "c-groceries", "a card makes a new column of its own");
                    window.check(board.freeName("") === "New column 2", "the next one is named New column 2");
                    board.removeColumn(0);
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
                    const board = find(wizard, "ColumnBoard", [])[0];
                    board.totals = [{column: "Fast food", amounts: ["12,00 €", "3,00 $"]}];
                    window.check(board.totalOf("Fast food") === "12,00 € + 3,00 $" && board.totalOf("Home") === "", "a card shows what its column adds up to, and nothing for a column with no figure");
                    // A column worked out by a formula (D-341): only an empty one can
                    // become that, it takes no cards, and it keeps what was typed.
                    window.check(board.addColumn("Both"), "a column is added for a formula");
                    const both = board.columns.length - 1;
                    board.toggleFormula(both);
                    window.check(board.isFormula(board.columns[both]), "an empty column can become a formula");
                    board.moveCard("c-food", -1, both);
                    window.check(board.problem !== "" && board.columns[both].members.length === 0, "a card is not put into a formula column: " + board.problem);
                    board.setColumnFormula(both, " Fast food + 1 ");
                    window.check(JSON.parse(wizard.workbench.view.columns)[both].formula === "Fast food + 1", "the formula is kept: " + wizard.workbench.view.columns);
                    window.check(board.choicesFor("c-rent", -1).every(entry => entry.to !== both), "the menu does not offer a formula column");
                    board.removeColumn(both);
                    // The Formula card is never used up and makes formula columns.
                    const unusedBefore = board.unused.length;
                    const countBefore = board.columns.length;
                    board.moveToNew(board.formulaCard, -1, "");
                    window.check(board.columns.length === countBefore + 1 && board.isFormula(board.columns[countBefore]) && board.columns[countBefore].members.length === 0, "the Formula card makes a formula column");
                    window.check(board.unused.length === unusedBefore, "the Formula card is not a category and is not used up");
                    board.moveCard(board.formulaCard, -1, 0);
                    window.check(board.problem !== "" && !board.isFormula(board.columns[0]), "the Formula card is refused by a column that holds categories: " + board.problem);
                    window.check(board.addColumn("Empty formula"), "an empty column");
                    const emptyOne = board.columns.length - 1;
                    board.moveCard(board.formulaCard, -1, emptyOne);
                    window.check(board.isFormula(board.columns[emptyOne]), "the Formula card makes an empty column a formula");
                    window.check(board.choicesFor(board.formulaCard, -1).some(entry => entry.create), "the Formula card's menu can make a new formula column");
                    window.check(board.addColumn("First plain") && board.addColumn("Second plain"), "two plain columns for the examples");
                    window.check(board.examples(0).length === 3, "two other columns give three ready-made formulas: " + board.examples(0).length);
                    window.check(board.suggest(0, "Empty", 5).some(entry => entry.label === "Empty formula"), "typing suggests the name of another column");
                    window.check(board.suggest(0, "SU", 2).some(entry => entry.insert === "SUM("), "typing suggests a function");
                    const inCall = board.suggest(0, "SUM(", 4);
                    window.check(inCall.length > 1 && inCall[0].label === "SUM(…)" && inCall[0].insert === "", "inside a call the syntax and names are offered: " + JSON.stringify(inCall.map(entry => entry.label)));
                    window.check(board.formulaProblem(0, "SUM(Empty formula").indexOf(")") >= 0, "a missing bracket is named");
                    window.check(board.formulaProblem(0, "SUM(1))").indexOf(")") >= 0, "an extra bracket is named");
                    window.check(board.formulaProblem(0, "Zzzqq + 1").indexOf("Zzzqq") >= 0, "an unknown name is named");
                    window.check(board.formulaProblem(0, "SUM(1)") === "", "a good formula has no problem");
                    board.removeColumn(board.columns.length - 1);
                    board.removeColumn(board.columns.length - 1);
                    board.removeColumn(emptyOne);
                    board.removeColumn(countBefore);
                    // A parent category makes a column for each subcategory.
                    window.check(board.choicesFor("c-food", -1).some(entry => entry.split), "a parent category's menu can split it");
                    window.check(!board.choicesFor("c-groceries", -1).some(entry => entry.split), "a category with no subcategory has no split");
                    const splitBefore = board.columns.length;
                    board.splitCard("c-food", -1);
                    window.check(board.columns.length === splitBefore + 1 && board.columns[splitBefore].name === "Groceries" && board.columns[splitBefore].members.join() === "c-groceries", "a column for the subcategory, named after it");
                    window.check(board.leftOut.length === 1, "the board says money booked straight to the parent has no column: " + board.leftOut);
                    board.removeColumn(splitBefore);
                    // Copying a card (Ctrl while dragging) keeps it in its column too.
                    window.check(board.addColumn("Copy"), "a column to copy into");
                    const copy = board.columns.length - 1;
                    board.copyCard("c-rent", 0, copy);
                    window.check(board.columns[0].members.includes("c-rent") && board.columns[copy].members.includes("c-rent"), "a copied card is in both columns");
                    board.removeColumn(copy);
                    window.check(board.columns.length === 1, "and the column goes again");
                    // Looking at a kind without columns (one figure) and coming back
                    // does not lose the columns.
                    wizard.step = 2;
                    wizard.workbench.setType("kpi.card");
                    window.check(wizard.workbench.view.columns === undefined, "one figure has no columns");
                    wizard.workbench.setType("chart.bar");
                    window.check(JSON.parse(wizard.workbench.view.columns).length === 1, "the columns come back with the chart: " + wizard.workbench.view.columns);
                    wizard.step = 1;
                    board.saveAsGroup(0);
                    window.check(groups.calls.includes("addGroup:Fast food:false:c-rent") && board.saved !== "" && board.groupResult.code === undefined, "a column is kept as a group of its categories: " + groups.calls);
                    window.check(page.saveChanges(), "the changes of the open view are saved with its columns");
                    wizard.cancelled();
                }
                break;
            case 15:
                {
                    const wizard = find(page, "StatisticWizard", [])[0];
                    window.check(JSON.parse(wizard.workbench.view.columns).length === 1, "the saved view comes back with its column");
                    // Change step by step from the list opens the wizard at its start.
                    page.wizard = false;
                    page.changeView(page.openId);
                    window.check(page.wizard && wizard.step === 0, "Change step by step opens the saved view in the wizard");
                    window.overviewPage = window.pageStack.push(overviewBuilder);
                }
                break;
            case 16:
                {
                    const other = window.overviewPage;
                    const wizard = find(other, "StatisticWizard", [])[0];
                    window.check(other.wizard && wizard.visible && wizard.onOverview && !wizard.isNew && wizard.step === 0, "a chart of the Overview opens in the wizard");
                    window.check(other.workbench.view.type === "chart.line", "with its own chart: " + other.workbench.view.type);
                    window.check(find(other, "Heading", []).filter(heading => heading.visible && heading.text === "Saved views").length === 0, "and without the saved views");
                    window.check(other.period === "last-12-months" && other.startPeriod === other.period, "a chart following the page's period is shown for the last 12 months");
                    other.workbench.setType("chart.area");
                    wizard.finish();
                    window.check(window.handedBack.id === "w7" && JSON.parse(window.handedBack.json).type === "chart.area", "Finish hands the changed chart back: " + JSON.stringify(window.handedBack));
                    window.check(saved.view.views.length === 1, "and saves no view");
                    wizard.cancelled();
                    window.check(window.overviewCancelled, "Cancel hands nothing back");
                    window.pageStack.pop();
                    window.overviewPage = window.pageStack.push(draftBuilder);
                }
                break;
            case 17:
                {
                    const other = window.overviewPage;
                    const wizard = find(other, "StatisticWizard", [])[0];
                    window.check(other.shownDraft && other.wizard && wizard.step === 2, "an unfinished statistic comes back at its step");
                    window.check(other.workbench.view.type === "chart.donut" && other.period === "last-year" && wizard.nameText === "Kept name", "with its chart, period and name");
                    window.check(other.defaultName() === "Half done", "its title is its default name");
                    Qt.exit(window.failed ? 1 : 0);
                }
                break;
            }
        }
    }
}
