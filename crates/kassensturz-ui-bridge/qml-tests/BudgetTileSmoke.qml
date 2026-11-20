// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: budget tile

// Opens a dashboard with the budget bars of a book that has no budget
// (B-412, D-904) on the stand-ins of fakes/ and checks that the empty tile
// takes its room while the page is only looked at, says how to fill it and
// asks for the Set a budget page; that the same button still gets the
// pointer while the layout is arranged, through a hole in the overlay that
// otherwise selects and drags the tile; that the picked tile's content
// starts below its row of buttons, so they hide none of its text; and that
// a preview, which has no page to open, shows the note without a button.
// `tools/check-panel.sh` runs it.

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
    property int asked: 0

    DashboardLayout {
        id: dashboard

        Component.onCompleted: {
            const tile = item("budgets", "budget.progress", 0, 0, 12, 4);
            // The stand-in for BookBudgets reads this as "no budget at all".
            tile.subject.preview = {shows: "budget-progress", only: "no-budgets", budgets: ""};
            view = Object.assign({}, view, {rows: 4, items: [tile]});
        }
    }

    pageStack.initialPage: Kirigami.Page {
        id: page

        title: "Budget tile"

        DashboardGrid {
            id: grid

            width: page.width - 2 * Kirigami.Units.largeSpacing
            layout: dashboard
            monthKey: "2026-09"
            onSetBudgetClicked: window.asked += 1
        }

        // The same bars in a preview: nothing listens there.
        BudgetBars {
            id: previewBars

            y: 600
            width: 400
            budgetCount: 0
            offersSet: false
        }
    }

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    // Every visible item below `item` that has a text.
    function withText(item, out) {
        const children = item.children ?? [];
        for (let i = 0; i < children.length; ++i) {
            const child = children[i];
            if (child.visible && typeof child.text === "string" && child.text !== "") {
                out.push(child);
            }
            withText(child, out);
        }
        return out;
    }

    // The item that catches the pointer while the layout is arranged: the
    // one below `item` with a mask.
    function catcher(item) {
        const children = item.children ?? [];
        for (let i = 0; i < children.length; ++i) {
            const child = children[i];
            if (child.containmentMask) {
                return child;
            }
            const found = catcher(child);
            if (found) {
                return found;
            }
        }
        return null;
    }

    Timer {
        interval: 500
        running: true
        repeat: true
        onTriggered: {
            const cell = grid.itemFor("budgets");
            switch (window.step++) {
            case 0: {
                window.check(cell !== null && cell.contentMade, "the budget tile is made");
                window.check(!grid.isCollapsed(cell.info) && cell.visible && cell.height > 0, "with no budget the tile still takes its room while the page is looked at");
                const shown = window.withText(cell, []);
                window.check(shown.some(found => found.text === "Your budgets appear here once you set one."), "it says how to fill it, got " + shown.map(found => found.text));
                const button = shown.find(found => found.text === "Set a budget");
                window.check(button !== undefined && button.enabled, "it offers Set a budget");
                button?.clicked();
                window.check(window.asked === 1, "Set a budget asks for the page, asked " + window.asked);
                window.check(ActionLog.mode.actions.includes("BudgetBars_setBudget"), "Set a budget says it ran");
                window.check(cell.barRoom === 0, "a tile that is not picked leaves no room for buttons");

                const inPreview = window.withText(previewBars, []).map(found => found.text);
                window.check(inPreview.includes("Your budgets appear here once you set one.") && !inPreview.includes("Set a budget"), "a preview has the note and no button, got " + inPreview);
                window.check(!previewBars.liveAt(Qt.point(5, 5)), "a preview has nothing for the pointer");
                dashboard.edit();
                break;
            }
            case 1: {
                window.check(grid.editing && cell.pointerOverlay, "arranging puts the overlay on the tile");
                const overlay = window.catcher(cell);
                const button = window.withText(cell, []).find(found => found.text === "Set a budget");
                window.check(overlay !== null && button !== undefined && button.enabled, "arranging keeps the button, and it can be pressed");
                if (overlay && button) {
                    const middle = button.mapToItem(overlay, button.width / 2, button.height / 2);
                    window.check(!overlay.containmentMask.contains(middle), "the overlay lets the pointer through to Set a budget");
                    window.check(overlay.containmentMask.contains(Qt.point(overlay.width - 4, overlay.height - 4)), "the rest of the tile still selects and drags it");
                    button.clicked();
                    window.check(window.asked === 2, "Set a budget asks for the page while arranging too, asked " + window.asked);
                }
                grid.select("budgets");
                break;
            }
            case 2: {
                window.check(cell.selected && cell.barHeight > 0 && cell.barRoom > cell.barHeight, "the picked tile leaves room for its buttons, room " + cell.barRoom + " for " + cell.barHeight);
                const note = window.withText(cell, []).find(found => found.text === "Your budgets appear here once you set one.");
                const top = note ? note.mapToItem(cell, 0, 0).y : -1;
                window.check(top >= cell.barRoom, "the note starts below the buttons, at " + top + " with room " + cell.barRoom);
                dashboard.done();
                break;
            }
            default: {
                window.check(!grid.editing && !grid.isCollapsed(cell.info) && cell.visible, "after Done the empty tile is still there");
                console.log(window.failed ? "budget tile: FAILED" : "budget tile: ok");
                Qt.exit(window.failed ? 1 : 0);
            }
            }
        }
    }
}
