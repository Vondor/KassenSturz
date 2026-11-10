// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Opens AiPermissionGrid.qml on stand-ins for the Rust objects (fakes/) and
// checks that a switch means "allowed" in the field box as in the grid
// (#631, D-684): a locked field's switch is off, an unlocked one's on, and
// switching a field on unlocks it.
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
            console.log(window.failed ? "permissions: FAILED" : "permissions: ok");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
