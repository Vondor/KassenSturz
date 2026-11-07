// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Opens DashboardGrid.qml with the period header, the filter bar (M6d.8,
// D-474) and the dashboard tabs (M6d.9, D-529) as widgets on stand-ins for
// DashboardLayout, BookOverview and CategoryGroups, and checks that each is
// drawn in its cell without a frame and without counting as a problem, that
// the header shows the month and its buttons reach the overview, that the
// bar's buttons are there, that the tabs switch the dashboard, take no room
// while there is one dashboard and say so while arranging, and that the Add
// menu always offers each (D-532: a page may hold more than one; a second set
// of tabs works like the first).
// `tools/check-panel.sh` runs it and fails on the first FAIL line or on any
// QML warning about a binding or a type.

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 1100
    height: 700
    visible: true
    property bool failed: false
    property int step: 0
    property var opened: []
    // What the page knows besides the figures: the whole book's transactions
    // without a category and the newest of them (US1.9, #535).
    readonly property var pageData: ({
        unsorted: 12,
        unsortedRows: [
            {accountId: "a1", account: "Giro", id: "t1", date: "13.10.2026", otherParty: "Kiosk", category: "", amount: "-12,50 €", exactAmount: "-12,50 €", incoming: false, kind: "simple"},
            {accountId: "a2", account: "Cash", id: "t2", date: "02.03.2026", otherParty: "", category: "", amount: "-3,00 €", exactAmount: "-3,00 €", incoming: false, kind: "simple"}
        ]
    })

    DashboardLayout {
        id: dashboard

        view: ({
            editing: false, damaged: false, hasChanges: false, canUndo: false, canRedo: false,
            undoEdit: "", redoEdit: "", builtIn: true, columns: 12, maxRows: 24, rows: 8,
            library: [],
            filters: {period: "", accounts: [], categories: [], forNow: false},
            dashboards: [
                {id: "d1", name: "Overview", isDefault: true, shown: true},
                {id: "d2", name: "Holidays", isDefault: false, shown: false}
            ],
            items: [
                dashboard.item("tabs", "page.tabs", 0, 0, 12, 1),
                dashboard.item("month", "page.period", 0, 1, 12, 1),
                dashboard.item("bar", "page.filters", 0, 2, 12, 1),
                dashboard.item("income", "kpi.card", 0, 3, 3, 2),
                dashboard.item("tabs2", "page.tabs", 0, 5, 12, 1),
                dashboard.item("note", "notice.unsorted", 0, 6, 12, 2)
            ]
        })
    }

    // A stand-in for BookOverview: the month and the three calls.
    QtObject {
        id: overview

        property var calls: []
        property var view: ({month: "September 2026", monthKey: "2026-09", hasEarlier: true, hasLater: true, thisMonth: false, empty: true})

        function showEarlierMonth() {
            calls = calls.concat(["earlier"]);
        }

        function showLaterMonth() {
            calls = calls.concat(["later"]);
        }

        function showThisMonth() {
            calls = calls.concat(["this"]);
        }
    }

    CategoryGroups {
        id: book
    }

    AddMenu {
        id: addMenu
    }

    pageStack.initialPage: Kirigami.Page {
        id: page

        title: "Page widgets"

        DashboardGrid {
            id: grid

            width: page.width - 2 * Kirigami.Units.largeSpacing
            layout: dashboard
            monthKey: "2026-09"
            overview: overview
            choices: book
            pageView: window.pageData
            onTransactionClicked: (account, transaction) => window.opened.push(account + ":" + transaction)
        }
    }

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    function findAll(item, predicate, found) {
        if (predicate(item)) {
            found.push(item);
        }
        for (const child of item.data) {
            if (child && child.data !== undefined) {
                findAll(child, predicate, found);
            }
        }
        return found;
    }

    Timer {
        interval: 500
        running: true
        repeat: true
        onTriggered: {
            switch (window.step++) {
            case 0: {
                const month = grid.itemFor("month");
                const bar = grid.itemFor("bar");
                window.check(month !== null && bar !== null, "both widgets have an item");
                window.check(month.isPageWidget && bar.isPageWidget && month.bare && bar.bare, "both are drawn without a card");
                window.check(!month.hasProblem && !bar.hasProblem && grid.problemCount === 0, "neither is a problem: " + JSON.stringify(grid.problemIds));
                window.check(!bar.takesPointer, "the bar takes the pointer only when picked while arranging");
                break;
            }
            case 1: {
                const month = grid.itemFor("month");
                const texts = window.findAll(month, item => item.text !== undefined && item.visible, []).map(item => item.text);
                window.check(texts.includes("September 2026"), "the header shows the month: " + JSON.stringify(texts));
                window.check(texts.some(text => text.includes("Back to this month")), "a month that is not this one offers the way back");
                window.check(texts.some(text => text.includes("Nothing came in")), "an empty month says so");
                const buttons = window.findAll(month, item => item.clicked !== undefined && item.text !== undefined && item.visible, []);
                const earlier = buttons.find(button => button.text === "Earlier month");
                window.check(earlier !== undefined && earlier.enabled, "the earlier button is there");
                earlier.clicked();
                window.check(overview.calls[overview.calls.length - 1] === "earlier", "it asks the overview: " + overview.calls);
                const bar = grid.itemFor("bar");
                const barButtons = window.findAll(bar, item => item.clicked !== undefined && item.text !== undefined && item.visible, []).map(button => button.text);
                window.check(barButtons.length === 3 && barButtons[0].includes("Period"), "the bar shows its three buttons: " + JSON.stringify(barButtons));
                break;
            }
            case 2: {
                const layoutGroup = addMenu.groups.find(group => group.rows.some(row => row.kind === "text.heading"));
                const kinds = layoutGroup.rows.map(row => row.kind);
                // The page already has each of the three; the user may add more (D-532).
                window.check(kinds.includes("page.filters") && kinds.includes("page.period") && kinds.includes("page.tabs"), "the menu always offers the tabs, the header and the bar: " + kinds);
                window.check(layoutGroup.rows.every(row => row.name !== "Unknown"), "each has a name");
                break;
            }
            case 3: {
                const tabs = grid.itemFor("tabs");
                window.check(tabs !== null && tabs.isPageWidget && tabs.bare, "the tabs are a widget drawn without a card");
                window.check(!tabs.hasProblem && grid.problemCount === 0, "the tabs are no problem: " + JSON.stringify(grid.problemIds));
                window.check(!grid.isCollapsed(tabs.info), "with two dashboards the tabs take their room");
                const buttons = window.findAll(tabs, item => item.checked !== undefined && item.text !== undefined && item.visible, []).map(button => button.text);
                window.check(buttons.length === 2 && buttons[0] === "Overview" && buttons[1] === "Holidays", "one tab per dashboard: " + JSON.stringify(buttons));
                const second = window.findAll(tabs, item => item.checked !== undefined && item.text === "Holidays", [])[0];
                second.clicked();
                window.check(dashboard.calls.includes("showDashboard:d2"), "a tab switches the dashboard: " + dashboard.calls);
                const again = grid.itemFor("tabs2");
                const buttonsAgain = window.findAll(again, item => item.checked !== undefined && item.text !== undefined && item.visible, []).map(button => button.text);
                window.check(buttonsAgain.length === 2, "a second set of tabs shows the dashboards too: " + JSON.stringify(buttonsAgain));
                break;
            }
            case 4: {
                // One dashboard: no tabs to show, and no room taken.
                const one = [{id: "d1", name: "Overview", isDefault: true, shown: true}];
                dashboard.view = Object.assign({}, dashboard.view, {dashboards: one});
                const tabs = grid.itemFor("tabs");
                window.check(grid.isCollapsed(tabs.info) && grid.isCollapsed(grid.itemFor("tabs2").info), "with one dashboard every set of tabs takes no room");
                window.check(!tabs.visible, "and are not drawn");
                const shown = window.findAll(tabs, item => item.checked !== undefined && item.text !== undefined && item.visible, []);
                window.check(shown.length === 0, "no tab button is visible: " + shown.length);
                // Arranging: the widget is there to be moved, and says why it is empty.
                dashboard.setEditing(true);
                window.check(!grid.isCollapsed(tabs.info), "while arranging the tabs have their cell");
                break;
            }
            case 5: {
                const tabs = grid.itemFor("tabs");
                const texts = window.findAll(tabs, item => item.text !== undefined && item.visible, []).map(item => item.text);
                window.check(texts.some(text => text.includes("once you have two or more")), "arranging says what the tabs will show: " + JSON.stringify(texts));
                break;
            }
            case 6: {
                // A bar that is not picked is picked by a drag that starts on
                // it, and the drag is not cut off by that (US1.8, #520).
                const bar = grid.itemFor("bar");
                grid.select("income");
                window.check(!bar.selected && bar.pointerOverlay && !bar.takesPointer, "a bar that is not picked has the overlay that picks and drags it");
                bar.dragStarted();
                window.check(bar.moving && bar.selected && bar.takesPointer, "a drag from an unpicked bar picks it");
                window.check(bar.pointerOverlay, "the overlay under the drag stays until it is let go");
                bar.dragEnded();
                window.check(!bar.moving && dashboard.calls.some(call => call.startsWith("moveTo:bar:")), "letting go moves the bar: " + dashboard.calls);
                window.check(bar.selected && !bar.pointerOverlay, "a picked bar gives the pointer back to its buttons");
                window.check(!bar.hasSettings && !grid.itemFor("month").hasSettings && grid.itemFor("income").hasSettings, "the page widgets have nothing to set, a card has");
                break;
            }
            case 7: {
                // The note about the transactions without a category (US1.9).
                const note = grid.itemFor("note");
                window.check(note !== null && note.bare && !note.hasProblem, "the note is drawn without a card and is no problem");
                const texts = window.findAll(note, item => item.text !== undefined && item.visible, []).map(item => item.text);
                window.check(texts.some(text => text.includes("12 transactions have no category yet")), "it says how many, the whole book's: " + JSON.stringify(texts));
                window.check(texts.some(text => text.includes("The newest 2 are listed here.")), "it says when only the newest are listed: " + JSON.stringify(texts));
                const rows = window.findAll(note, item => item.clicked !== undefined && item.modelData !== undefined && item.visible, []);
                window.check(rows.length === 2, "one row for each listed transaction: " + rows.length);
                rows[0].clicked();
                window.check(window.opened.join() === "a1:t1", "a row opens that transaction in its account: " + window.opened);
                break;
            }
            default:
                console.log(window.failed ? "page widgets smoke: FAILED" : "page widgets smoke: all checks passed");
                Qt.exit(window.failed ? 1 : 0);
            }
        }
    }
}
