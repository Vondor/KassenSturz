// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Opens DashboardGrid.qml on a stand-in for DashboardLayout (fakes/) and
// checks what the grid draws and what its keys ask the layout for (M6b.3):
// one item per component placed on the 12 columns, the same components in
// one column when the window is narrow, a problem counted for a component of
// a type the app does not know, a toolbar and a resize corner only on the
// selected component, and every key of the keyboard map naming its call.
// `tools/check-panel.sh` runs it and fails on the first FAIL line or on any
// QML warning about a binding or a type.

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 1100
    height: 900
    visible: true
    property bool failed: false
    property int step: 0
    property var refused: []
    property var asked: []

    DashboardLayout {
        id: dashboard
    }

    // The menu that adds a widget (D-337).
    AddMenu {
        id: addMenu

        savedList: [{id: "b", name: "Zeta"}, {id: "a", name: "Alpha"}]
        onKindChosen: kind => window.asked.push("add:" + kind)
        onSavedChosen: id => window.asked.push("saved:" + id)
    }

    pageStack.initialPage: Kirigami.Page {
        id: page

        title: "Grid"

        DashboardGrid {
            id: grid

            width: page.width - 2 * Kirigami.Units.largeSpacing
            layout: dashboard
            monthKey: "2026-09"
            onPropertiesRequested: id => window.asked.push("properties:" + id)
            onFailed: result => window.refused.push(result.code)
        }
    }

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    function key(code, modifiers, id) {
        const event = {key: code, modifiers: modifiers, accepted: false};
        grid.keyPressed(grid.itemFor(id), event);
        return event.accepted;
    }

    function last() {
        return dashboard.calls[dashboard.calls.length - 1];
    }

    Timer {
        interval: 500
        running: true
        repeat: true
        onTriggered: {
            switch (window.step++) {
            case 0: {
                window.check(grid.items.length === 12, "twelve components are given: " + grid.items.length);
                window.check(grid.ids.length === 12, "one item for each");
                const income = grid.itemFor("income");
                const spending = grid.itemFor("spending");
                window.check(income !== null && spending !== null, "the items exist");
                const step = grid.cell + grid.gap;
                window.check(Math.abs(income.x) < 1 && Math.abs(income.y) < 1, "the first card is at the top left: " + income.x + "," + income.y);
                window.check(Math.abs(spending.x - 3 * step) < 1, "the donut starts at column 4: " + spending.x);
                window.check(Math.abs(spending.width - (6 * grid.cell + 5 * grid.gap)) < 1, "the donut is six columns wide: " + spending.width);
                window.check(Math.abs(spending.height - (4 * grid.rowHeight + 3 * grid.gap)) < 1, "the donut is four rows high: " + spending.height);
                window.check(!grid.narrow, "a wide window is not narrow");
                window.check(grid.problemCount === 1 && grid.problemIds[0] === "future", "the unknown component is the one problem: " + JSON.stringify(grid.problemIds));
                window.check(!income.selected, "nothing is selected in view mode");
                window.check(grid.itemFor("title").heading === "Spending", "a heading shows its text");
                // The separator and the notices draw from the page's own data
                // (D-328, D-334) and none of them is a problem.
                grid.pageView = {
                    fetchGaps: [{days: 20, account: "Checking", lastFetched: "2026-10-01", accountId: "a1"}], transfers: 2,
                    sections: [{currency: "EUR", together: "1.000,00 €", exactTogether: "1.000,00 €", togetherOwes: false}],
                    totals: [{currency: "EUR", income: [{name: "Salary", amount: "2.000,00 €", exactAmount: "2.000,00 €", unsorted: false}]}]
                };
                window.check(!grid.itemFor("worth").hasProblem && !grid.itemFor("sources").hasProblem, "the worth card and the income list are not problems");
                window.check(grid.itemFor("line").bare && grid.itemFor("gaps").bare && grid.itemFor("moved").bare, "the line and the notices have no card in view mode");
                window.check(!grid.itemFor("income").bare, "a figure keeps its card");
                // A widget inside a period box takes the box's period unless
                // it has one of its own; one outside the box follows the page.
                window.check(grid.periodFor(grid.byId["inner"]) === "this-year", "a widget in a period box takes its period: " + grid.periodFor(grid.byId["inner"]));
                window.check(grid.periodFor(grid.byId["income"]) === "2026-09", "a widget outside every box follows the page");
                window.check(grid.itemFor("zone").isBox && grid.itemFor("zone").z < grid.itemFor("inner").z, "the box lies under its widgets");
                window.check(!grid.itemFor("gaps").hasProblem && !grid.itemFor("moved").hasProblem && !grid.itemFor("line").hasProblem, "the notices are not problems");
                window.check(grid.periodFor(grid.items[0]) === "2026-09", "a component on the page's period asks for the page's month: " + grid.periodFor(grid.items[0]));
                dashboard.edit();
                break;
            }
            case 1: {
                window.check(grid.editing, "edit mode follows the layout");
                window.check(grid.implicitHeight > grid.frameHeight(9), "edit mode leaves rows below the lowest component");
                grid.select("spending");
                break;
            }
            case 2: {
                window.check(grid.itemFor("spending").selected && !grid.itemFor("income").selected, "only the picked component is selected");
                const Shift = Qt.ShiftModifier;
                window.check(window.key(Qt.Key_Right, 0, "spending") && window.last() === "nudge:spending:1:0", "Right moves one cell: " + window.last());
                window.key(Qt.Key_Up, 0, "spending");
                window.check(window.last() === "nudge:spending:0:-1", "Up moves one row up: " + window.last());
                window.key(Qt.Key_Right, Shift, "spending");
                window.check(window.last() === "grow:spending:1:0", "Shift+Right makes it one cell wider: " + window.last());
                window.key(Qt.Key_Up, Shift, "spending");
                window.check(window.last() === "grow:spending:0:-1", "Shift+Up makes it one row lower: " + window.last());
                window.key(Qt.Key_D, Qt.ControlModifier, "spending");
                window.check(window.last() === "duplicate:spending", "Ctrl+D copies it: " + window.last());
                window.check(!window.key(Qt.Key_D, 0, "spending"), "D alone is left to others");
                window.key(Qt.Key_Return, 0, "spending");
                window.check(window.asked[window.asked.length - 1] === "properties:spending", "Enter opens the properties");
                window.key(Qt.Key_Delete, 0, "spending");
                window.check(window.last() === "remove:spending", "Delete removes it: " + window.last());
                window.check(grid.selectedId === "", "nothing is selected after a removal");
                const event = {accepted: true};
                grid.focusNeighbour("income", 1, event);
                window.check(event.accepted && grid.selectedId === "spending", "Tab goes to the next in reading order: " + grid.selectedId);
                const end = {accepted: true};
                grid.focusNeighbour("future", 1, end);
                window.check(!end.accepted, "Tab leaves the grid after the last one");
                grid.add("text.heading");
                window.check(window.last() === "add:text.heading", "the library adds by kind");
                grid.width = 300;
                break;
            }
            case 3: {
                window.check(grid.narrow, "a narrow window shows one column");
                const income = grid.itemFor("income");
                const spending = grid.itemFor("spending");
                window.check(Math.abs(income.width - grid.width) < 1 && Math.abs(spending.x) < 1, "the components fill the width");
                window.check(spending.y > income.y && spending.y >= income.height, "the donut is under the card: " + spending.y + " vs " + income.height);
                grid.select("spending");
                window.key(Qt.Key_Down, 0, "spending");
                window.check(window.last() === "later:spending", "Down moves it later in a narrow window: " + window.last());
                window.key(Qt.Key_Up, 0, "spending");
                window.check(window.last() === "earlier:spending", "Up moves it earlier in a narrow window: " + window.last());
                window.key(Qt.Key_Right, 0, "spending");
                window.check(window.last() === "earlier:spending", "Right does nothing in a narrow window");
                dashboard.cancel();
                break;
            }
            default:
                const groupNames = addMenu.groups.map(group => group.name);
                window.check(groupNames.length === 5, "five groups with a saved view: " + groupNames);
                window.check(groupNames.join() === groupNames.slice().sort((x, y) => x.localeCompare(y)).join(), "the groups are in alphabetical order: " + groupNames);
                for (const group of addMenu.groups) {
                    const rowNames = group.rows.map(row => row.name);
                    window.check(rowNames.join() === rowNames.slice().sort((x, y) => x.localeCompare(y)).join(), "the rows of " + group.name + " are in alphabetical order: " + rowNames);
                }
                const saved = addMenu.groups.find(group => group.rows[0].saved);
                window.check(saved && saved.rows.map(row => row.id).join() === "a,b", "the saved views are sorted by name");
                const kinds = addMenu.groups.filter(group => !group.rows[0].saved).reduce((all, group) => all.concat(group.rows), []).length;
                window.check(kinds === 16, "every kind of widget is in a group: " + kinds);
                window.check(addMenu.count === 5, "one submenu for each group: " + addMenu.count);
                // Choosing an item of a submenu reaches the menu's signals (M6-B9:
                // `menu` inside a MenuItem is the MenuItem's own property).
                const before = window.asked.length;
                for (let g = 0; g < addMenu.count; ++g) {
                    addMenu.menuAt(g).itemAt(0).triggered();
                }
                const added = window.asked.slice(before);
                window.check(added.length === 5 && added.filter(entry => entry.startsWith("add:")).length === 4 && added.filter(entry => entry === "saved:a").length === 1, "choosing the first item of every submenu says so: " + added);
                // The period box is one of them (M6-B12).
                const layoutGroup = addMenu.groups.findIndex(group => group.rows.some(row => row.id === "group.period"));
                const boxRow = addMenu.groups[layoutGroup].rows.findIndex(row => row.id === "group.period");
                const beforeBox = window.asked.length;
                addMenu.menuAt(layoutGroup).itemAt(boxRow).triggered();
                window.check(window.asked.slice(beforeBox).join() === "add:group.period", "choosing the period box says so: " + window.asked.slice(beforeBox));
                window.check(!grid.editing && grid.selectedId === "", "leaving edit mode selects nothing");
                if (window.failed) {
                    console.log("FAIL: the grid smoke test");
                }
                Qt.exit(window.failed ? 1 : 0);
            }
        }
    }
}
