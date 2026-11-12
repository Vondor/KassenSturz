// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Opens AiPermissionGrid.qml on stand-ins for the Rust objects (fakes/) and
// checks that a switch means "allowed" in the field box as in the grid
// (#631, D-684): a locked field's switch is off, an unlocked one's on, and
// switching a field on unlocks it; and that the cell for joining Transfers
// keeps Transactions › Read on, and that chart descriptions have a row.
// `tools/check-panel.sh` runs it and fails on the first FAIL line or on any
// QML warning about a binding or a type.

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 900
    height: 1200
    visible: true
    property bool failed: false

    AiSwitch {
        id: ai
    }

    AiSettings {
        id: settings
    }

    pageStack.initialPage: Kirigami.ScrollablePage {
        AiPermissionGrid {
            id: grid

            ai: ai
            settings: settings
        }
    }

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    // The switch of the field box whose accessible name names `label`.
    function fieldSwitch(item, label) {
        for (let i = 0; i < item.children.length; ++i) {
            const child = item.children[i];
            if (child instanceof Controls.Switch && String(child.Accessible.name) === "The AI may write " + label) {
                return child;
            }
            const found = fieldSwitch(child, label);
            if (found) {
                return found;
            }
        }
        return null;
    }

    // The labels of cells that aren't offered, as drawn.
    function absentLabels(item, found) {
        for (let i = 0; i < item.children.length; ++i) {
            const child = item.children[i];
            if (child instanceof Controls.Label && child.visible && child.wrapMode === Text.Wrap && child.horizontalAlignment === Text.AlignHCenter) {
                found.push(child);
            }
            absentLabels(child, found);
        }
        return found;
    }

    Timer {
        interval: 500
        running: true
        onTriggered: {
            const amount = window.fieldSwitch(grid, "Amount");
            const note = window.fieldSwitch(grid, "Note");
            window.check(amount !== null && note !== null, "the field box shows a switch per field");
            if (amount !== null && note !== null) {
                window.check(!amount.checked, "a locked field's switch is off");
                window.check(note.checked, "a field the AI may write has its switch on");
                // The user switches Amount on: the AI may now write it.
                amount.checked = true;
                amount.toggled();
                window.check(settings.lockCalls.length === 1 && settings.lockCalls[0][0] === "LockAmount" && settings.lockCalls[0][1] === false, "switching a field on unlocks it, got " + JSON.stringify(settings.lockCalls));
                window.check(amount.checked, "the switch stays on");
                // And off again: locked.
                amount.checked = false;
                amount.toggled();
                window.check(settings.locked("LockAmount"), "switching it off locks it again");
                window.check(!amount.checked, "the switch stays off");
            }
            // Joining two sides is its own row (D-693), and it keeps
            // Transactions › Read on, as a change of that row does.
            settings.allowed = ({ AllowJoinTransfers: true });
            const transactions = grid.rows.find(row => row.key === "Transactions");
            window.check(transactions !== undefined, "the grid has its Transactions row");
            window.check(grid.readForced(transactions, 0), "joining keeps the transactions readable");
            window.check(grid.historyAtRisk(0), "joining warns about the history");
            settings.allowed = ({});
            window.check(!grid.readForced(transactions, 0), "without it Read is free again");
            // Chart descriptions are their own row with one cell (D-729);
            // they read nothing and don't touch the history.
            const descriptions = grid.rows.find(row => row.cells.some(cell => cell.key === "AllowCustomCharts"));
            window.check(descriptions !== undefined, "the grid has a row for chart descriptions");
            settings.allowed = ({ AllowCustomCharts: true });
            window.check(grid.isOn("AllowCustomCharts", 0), "the cell reads its setting");
            window.check(!grid.historyAtRisk(0), "chart descriptions don't warn about the history");
            const charts = grid.rows.find(row => row.cells.some(cell => cell.key === "AllowReadViews"));
            window.check(!grid.readForced(charts, 0), "chart descriptions don't force reading the charts");
            // Budgets › Add reads only the budgets, not the other rows: the
            // grid shows what the AI is really given (US4 P3, R5).
            settings.allowed = ({ AllowNewBudgets: true });
            const named = name => grid.rows.find(row => row.cells.some(cell => cell.key === name));
            window.check(grid.readForced(named("AllowReadBudgets"), 0), "adding budgets keeps the budgets readable");
            ["AllowReadCategories", "AllowReadRules", "AllowReadAccounts", "AllowReadTransactions", "AllowReadViews"].forEach(key => window.check(!grid.readForced(named(key), 0), "adding budgets leaves " + key + " free"));
            settings.allowed = ({ AllowEditAccounts: true });
            window.check(!grid.readForced(named("AllowReadCategories"), 0), "changing accounts leaves the categories free");
            window.check(grid.readForced(named("AllowReadAccounts"), 0), "changing accounts keeps the accounts readable");
            settings.allowed = ({});
            // A cell's word stays within its own column (R8).
            const words = window.absentLabels(grid, []);
            window.check(words.length > 0, "the grid has cells that aren't offered");
            words.forEach(label => window.check(label.width > 0 && label.width <= label.parent.width + 0.5 && label.contentWidth <= label.width + 0.5, "\"" + label.text + "\" fits its cell: " + label.contentWidth + " in " + label.width));
            console.log(window.failed ? "permissions: FAILED" : "permissions: ok");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
