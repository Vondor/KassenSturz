// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Opens PropertiesSheet.qml, the Properties dialog of a component on the
// Overview, on a stand-in for ComponentProperties (fakes/), made when it
// opens as the Overview page makes it (M6-B197). Checks that the dialog shows
// what it holds (the panel's fields and the preview), not only its title and
// Close; that it fits in a window lower than its content; that it opens
// again; and that the Statistic Builder button passes the component on and
// closes the dialog. `tools/check-panel.sh` runs it and fails on the first
// FAIL line or on any QML warning about a binding or a type, so a binding
// loop for the dialog's position while it moves in fails it too.

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Controls.ApplicationWindow {
    id: window

    width: 900
    // Lower than the dialog's content, as on a laptop.
    height: 600
    visible: true
    property bool failed: false
    property int step: 0
    property var sheet: null
    property var asked: []

    ComponentProperties {
        id: fake
    }

    Component {
        id: sheetComponent

        PropertiesSheet {
            model: fake
            period: "this-month"
            builderTypes: ["kpi.card"]
            onBuilderRequested: (component, period) => window.asked.push(JSON.parse(component).type)
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
        for (const child of item.children ?? []) {
            findAll(child, predicate, found);
        }
        return found;
    }

    // Whether `item` and every item it lies in are visible, and it lies
    // inside the window.
    function seen(item): bool {
        if (!item.visible || item.width <= 0 || item.height <= 0) {
            return false;
        }
        const at = item.mapToItem(window.contentItem, 0, 0);
        return at.y >= 0 && at.y < window.height && at.x >= 0 && at.x < window.width;
    }

    function open() {
        sheet = sheetComponent.createObject(window.contentItem);
        sheet.open();
    }

    function checkShown(when: string) {
        check(sheet.opened, when + ": the dialog is open");
        // The part between the title and Close holds the panel.
        check(sheet.contentItem.height > Kirigami.Units.gridUnit * 5, when + ": the dialog shows its content, not only its title and Close (" + sheet.contentItem.height + ")");
        check(sheet.y >= 0 && sheet.y + sheet.height <= window.height, when + ": the dialog fits in the window");
        // The panel with the component's fields is in sight.
        const panels = findAll(sheet.contentItem, item => item.fieldByKey !== undefined && typeof item.placeFieldRows === "function", []);
        check(panels.length === 1 && seen(panels[0]), when + ": the panel is shown");
        check(panels.length === 1 && panels[0].fieldByKey.title !== undefined, when + ": the panel has the title field");
        const previews = findAll(sheet.contentItem, item => item.subject !== undefined && item.period !== undefined, []);
        check(previews.length === 1, when + ": the preview is made");
        const builder = findAll(sheet.contentItem, item => item.clicked !== undefined && String(item.text).includes("Statistic Builder"), []);
        check(builder.length === 1 && seen(builder[0]), when + ": the Statistic Builder button is shown");
    }

    Timer {
        interval: 400
        running: true
        repeat: true
        onTriggered: {
            window.step += 1;
            switch (window.step) {
            case 1:
                fake.open("kpi.card", "account_balances");
                window.open();
                break;
            case 3:
                window.checkShown("first opening");
                window.sheet.close();
                break;
            case 4:
                window.check(window.sheet === null || !window.sheet.visible, "Close closes the dialog");
                window.open();
                break;
            case 6: {
                window.checkShown("second opening");
                const builder = window.findAll(window.sheet.contentItem, item => item.clicked !== undefined && String(item.text).includes("Statistic Builder"), [])[0];
                builder.clicked();
                break;
            }
            case 7:
                window.check(window.asked.length === 1 && window.asked[0] === "kpi.card", "the builder gets the component: " + JSON.stringify(window.asked));
                window.check(window.sheet === null || !window.sheet.visible, "the dialog closes for the builder");
                break;
            case 8:
                console.log(window.failed ? "SheetSmoke: FAILED" : "SheetSmoke: all checks passed");
                Qt.exit(window.failed ? 1 : 0);
                break;
            }
        }
    }
}
