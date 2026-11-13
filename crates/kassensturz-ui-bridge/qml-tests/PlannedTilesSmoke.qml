// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: left this month and coming up tiles

// Opens LeftThisMonthWidget.qml and ComingUpWidget.qml (US7, D-769, D-770)
// on the shapes `PlannedPayments.view` gives and checks what a person sees:
// the figure, the line under it and what it leaves out; a payment that
// passed is named and not counted, and *Check them…* opens the window; with
// no plan no figure is shown and *Plan a payment…* opens the form; a figure
// below zero says so to the screen reader; a month that starts on a
// salary day says where it ends (#921); and *Coming up* lists the
// payments with the words the Planned payments window uses.
// `tools/check-panel.sh` runs it and fails on the first FAIL line or on any
// QML warning about a binding or a type.

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 600
    height: 700
    visible: true
    property bool failed: false
    property int planned: 0
    property int opened: 0

    readonly property var payment: ({
        planId: "gym", name: "Gym", account: "Checking account", amount: "29,90 €",
        exactAmount: "29,90 €", outgoing: true, due: "2026-10-26", dueOn: "2026-10-25",
        state: "upcoming",
    })
    readonly property var plan: ({
        id: "gym", name: "Gym", account: "Checking account", accountId: "checking",
        amount: "29,90 €", exactAmount: "29,90 €", outgoing: true, unit: "month", every: 1,
        firstDue: "2026-10-25", ends: "", nextDue: "2026-10-26", paused: false,
        accountClosed: false, category: "", party: "",
    })
    readonly property var forecast: ({
        accounts: ["Checking account"], leftOut: 1, hasPlans: true,
        balanceNow: "2340,00 €", exactBalanceNow: "2340,00 €", outDue: "29,90 €",
        exactOutDue: "29,90 €", inDue: "2200,00 €", exactInDue: "2200,00 €",
        left: "2310,10 €", exactLeft: "2310,10 €", belowZero: false,
        expectedEnd: "4510,10 €", exactExpectedEnd: "4510,10 €", overdue: 1,
        status: "to-do", statusCount: 1, months: [],
    })

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    function find(item, test, out) {
        if (!item) {
            return out;
        }
        for (let i = 0; i < item.children.length; ++i) {
            const child = item.children[i];
            if (test(child)) {
                out.push(child);
            }
            find(child, test, out);
        }
        return out;
    }

    function shown(root, starts) {
        return find(root, child => child instanceof Controls.Label && child.visible && String(child.text).startsWith(starts), []);
    }

    function buttons(root, starts) {
        return find(root, child => child instanceof Controls.AbstractButton && child.visible && String(child.text).startsWith(starts), []);
    }

    Column {
        width: window.width

        LeftThisMonthWidget {
            id: left

            width: parent.width
            height: 300
            view: ({ plans: [window.plan], toConfirm: [], comingUp: [window.payment], comingUpStatus: "fine", comingUpCount: 1, forecast: window.forecast })
            onPlanClicked: window.planned += 1
            onOpenClicked: window.opened += 1
        }

        ComingUpWidget {
            id: soon

            width: parent.width
            height: 300
            view: left.view
            onPlanClicked: window.planned += 1
            onOpenClicked: window.opened += 1
        }
    }

    property int step: 0

    Timer {
        interval: 300
        running: true
        repeat: true
        onTriggered: {
            window.step += 1;
            switch (window.step) {
            case 1: {
                const figure = window.find(left, child => child instanceof Kirigami.Heading && child.visible, []);
                window.check(figure.length === 1 && figure[0].text === "2310,10 €", "the figure is what is left, got " + (figure[0]?.text ?? "none"));
                window.check(figure[0]?.Accessible.name === "Left this month: 2310,10 €", "the screen reader hears the figure, got " + figure[0]?.Accessible.name);
                window.check(window.shown(left, "2340,00 € now, minus 29,90 € still to pay this month").length === 1, "the line under the figure says how it is made");
                window.check(window.shown(left, "At month end, with the 2200,00 € you expect: 4510,10 €").length === 1, "the income still to come is a line of its own");
                window.check(window.shown(left, "1 planned payment passed without an entry and is not counted.").length === 1, "the payment that passed is named");
                const notes = window.shown(left, "For Checking account.");
                window.check(notes.length === 1 && notes[0].text.includes("Spending that is not planned is not included.") && notes[0].text.includes("Moves to your other accounts are not counted.") && notes[0].text.includes("1 account in another currency is not included."), "the small print says what is left out, got " + (notes[0]?.text ?? "none"));
                window.check(window.buttons(left, "Plan a payment").length === 0, "with plans there is no Plan a payment button");
                const check = window.buttons(left, "Check them");
                window.check(check.length === 1, "one Check them button, got " + check.length);
                check[0]?.clicked();
                window.check(window.opened === 1, "Check them opens the window");

                window.check(window.shown(soon, "Gym: 29,90 € out, due 2026-10-26").length === 1, "Coming up lists the gym");
                const all = window.buttons(soon, "Planned payments");
                window.check(all.length === 1, "one Planned payments button, got " + all.length);
                all[0]?.clicked();
                window.check(window.opened === 2, "Planned payments opens the window");

                // A month that starts on the salary day says where it ends
                // (#921); the tile's own month wins over the view's.
                left.month = Object.assign({}, window.forecast, { startsOn: 25, likeBudgets: true, ends: "2026-11-24", left: "99,00 €" });
                window.check(window.shown(left, "For Checking account.")[0]?.text.includes("This month runs until 2026-11-24, like your budgets.") ?? false, "a salary month says where it ends");
                window.check(window.find(left, child => child instanceof Kirigami.Heading && child.visible, [])[0]?.text === "99,00 €", "the tile's own month is shown");
                left.month = Object.assign({}, window.forecast, { startsOn: 25, likeBudgets: false, ends: "2026-11-24" });
                window.check(window.shown(left, "For Checking account.")[0]?.text.includes("This month runs until 2026-11-24.") ?? false, "a fixed day says where it ends");
                left.month = ({ code: "store.io" });
                window.check(left.failed, "the tile's own month can fail too");
                left.month = null;
                window.check(!left.failed && window.shown(left, "For Checking account.")[0]?.text.includes("runs until") === false, "back to the calendar month");

                // Below zero, and no income still to come.
                left.view = ({ plans: [window.plan], toConfirm: [], comingUp: [], comingUpStatus: "fine", comingUpCount: 0,
                    forecast: Object.assign({}, window.forecast, { left: "-12,00 €", exactLeft: "-12,00 €", belowZero: true, inDue: "", expectedEnd: "", overdue: 0, leftOut: 0 }) });
                break;
            }
            case 2: {
                const figure = window.find(left, child => child instanceof Kirigami.Heading && child.visible, []);
                window.check(figure[0]?.Accessible.name === "Left this month: -12,00 €, below zero", "below zero is said in words, got " + figure[0]?.Accessible.name);
                window.check(figure.length === 1 && Qt.colorEqual(figure[0].color, Kirigami.Theme.negativeTextColor), "below zero is drawn in the negative colour");
                window.check(window.shown(left, "At month end").length === 0, "no income line without income still to come");
                window.check(window.buttons(left, "Check them").length === 0, "nothing to check, no button");
                window.check(window.shown(soon, "No planned payment in the next 30 days.").length === 1, "Coming up says when nothing comes");

                // No plan at all.
                left.view = ({ plans: [], toConfirm: [], comingUp: [], comingUpStatus: "unknown", comingUpCount: 0,
                    forecast: Object.assign({}, window.forecast, { hasPlans: false, overdue: 0, status: "unknown", statusCount: 0 }) });
                break;
            }
            case 3: {
                window.check(window.find(left, child => child instanceof Kirigami.Heading && child.visible, []).length === 0, "no plan, no figure");
                window.check(window.shown(left, "Nothing planned yet.").length === 1, "the tile says nothing is planned");
                const plan = window.buttons(left, "Plan a payment");
                window.check(plan.length === 1, "one Plan a payment button, got " + plan.length);
                plan[0]?.clicked();
                const again = window.buttons(soon, "Plan a payment");
                window.check(again.length === 1, "Coming up offers it too, got " + again.length);
                again[0]?.clicked();
                window.check(window.planned === 2, "both open the form, got " + window.planned);
                window.check(window.buttons(soon, "Planned payments").length === 0, "no window button without a plan");

                // The view could not be read.
                left.view = ({ code: "store.io" });
                break;
            }
            case 4: {
                window.check(left.failed && soon.failed, "an error object is a failure");
                window.check(window.shown(left, "The planned payments couldn't be read.").length === 1, "the tile says it couldn't read them");
                window.check(window.buttons(left, "Plan a payment").length === 0, "no button on a failure");
                console.log(window.failed ? "planned tiles: FAILED" : "planned tiles: ok");
                Qt.exit(window.failed ? 1 : 0);
                break;
            }
            }
        }
    }
}
