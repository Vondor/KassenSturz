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

    pageStack.initialPage: StatisticsPage {
        id: page

        saved: saved
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
        return find(page, "TextField", []).filter(f => f.placeholderText !== "" && f.placeholderText.indexOf("01.01") < 0)[0];
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
                find(page, "ComboBox", []).filter(box => box.textRole === "name")[0].activated(page.types.indexOf("kpi.card"));
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
                    const choice = find(page, "PeriodChoice", [])[0];
                    const box = find(choice, "ComboBox", [])[0];
                    window.check(box.count === page.periods.length + 1, "the period box has the quick choices and the range");
                    window.check(!find(choice, "TextField", [])[0].visible, "no field for dates at first");
                    box.activated(page.periods.length);
                }
                break;
            case 7:
                {
                    const choice = find(page, "PeriodChoice", [])[0];
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
                Qt.exit(window.failed ? 1 : 0);
                break;
            }
        }
    }
}
