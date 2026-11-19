// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: Plans page of the menu

// Shows PlansPage.qml, the Plans screen of the menu (B-388, D-863), on the
// stand-ins of fakes/: it says what a plan is, lists the plans of
// PlannedPaymentsView.qml without its own "New planned payment…" button, and
// "Add a plan" in the header opens the plan form. `tools/check-panel.sh`
// runs it and fails on the first FAIL line or on any QML warning.

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 900
    height: 900
    visible: true
    property bool failed: false
    property int step: 0

    AccountRegister {
        id: register
    }

    PlannedPayments {
        id: plans
    }

    pageStack.initialPage: PlansPage {
        id: page

        plans: plans
        register: register
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

    Timer {
        interval: 300
        running: true
        repeat: true
        onTriggered: {
            window.step += 1;
            switch (window.step) {
            case 1: {
                const shown = window.texts(page, []);
                window.check(page.title === "Plans", "the page is called Plans, got " + page.title);
                window.check(shown.includes("What is a plan?"), "the page says what a plan is");
                window.check(page.planList.length === 1, "the plans are read, got " + page.planList.length);
                window.check(!shown.some(text => text.startsWith("New planned payment")), "no second button to add a plan");
                const add = page.actions.filter(action => action.text === "Add a plan");
                window.check(add.length === 1, "one Add a plan action, got " + add.length);
                add[0]?.trigger();
                break;
            }
            case 2: {
                window.check(page.form.visible, "Add a plan opens the form");
                window.check(ActionLog.mode.actions.includes("PlansPage_addPlan"), "Add a plan says it ran, got " + ActionLog.mode.actions);
                page.form.close();
                console.log(window.failed ? "plans page: FAILED" : "plans page: ok");
                Qt.exit(window.failed ? 1 : 0);
                break;
            }
            }
        }
    }
}
