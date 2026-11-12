// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: warnings tile

// Opens AlarmList.qml and AlarmRulesField.qml on the stand-in for the Rust
// object BookAlarms (fakes/) and checks that the tile says when it has no
// rule, writes a sentence for each rule that applies, that the settings list
// each rule, refuse an amount that isn't one and send the new list (US4.9).
// `tools/check-panel.sh` runs it and fails on the first FAIL line or on any
// QML warning about a binding or a type.

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
    property string sent: ""

    readonly property string rules: JSON.stringify([
        {reason: "budget_over", budget: "b1", share: 100},
        {reason: "account_below", account: "a1", amount: "500.00", lowest: true},
        {reason: "account_reserve", account: "a1", budget: "b1", times: 2, lowest: false}
    ])

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

    BookAlarms {
        id: feed
    }

    Column {
        width: 600

        AlarmList {
            id: none

            width: 600
            shown: ({rows: [], count: 0, shown: 0})
        }

        AlarmList {
            id: some

            width: 600
            shown: feed.view
        }

        AlarmRulesField {
            id: field

            width: 600
            rulesText: window.rules
            accounts: [{id: "a1", name: "Shared account"}]
            budgets: [{id: "b1", name: "Household"}]
            onRulesEdited: json => window.sent = json
        }
    }

    Timer {
        interval: 600
        running: true
        onTriggered: {
            window.check(window.texts(none, []).some(text => text.startsWith("No warnings set yet")), "a tile without rules says how to add one");

            feed.show(window.rules);
            const lines = window.texts(some, []);
            window.check(lines.some(text => text.startsWith("Household is over by")), "a budget over its amount has its sentence, got " + lines);
            window.check(lines.includes("Shared account went down to 200,00 € this month (on 2026-10-03), under your limit of 500,00 €."), "the lowest point is named with its day, got " + lines);
            window.check(lines.includes("Shared account holds 200,00 €, under its reserve of 500,00 €: 2 months of Household."), "a reserve names its periods and budget, got " + lines);

            const listed = window.texts(field, []);
            window.check(listed.includes("Household used more than 100 %"), "the settings list a budget rule, got " + listed);
            window.check(listed.includes("Shared account under 500,00 at its lowest point this month"), "the settings list an account rule, got " + listed);

            // A new rule: an amount that isn't one keeps Save off.
            const dialog = field.ruleDialog;
            window.check(dialog !== null, "the field has its dialog");
            dialog.edit(-1, {});
            // A budget rule shows the budgets to pick and can't be saved
            // without one (US4 P3, step 7).
            window.check(dialog.reason === "budget_over" && dialog.forBudget && !dialog.forAccount, "a new rule watches a budget");
            window.check(dialog.budgetPicker.visible && dialog.budgetPicker.count === 1, "the budget picker shows the book's budgets");
            window.check(!dialog.accountPicker.visible, "a budget rule has no account picker");
            window.check(!dialog.ready, "a budget rule without its budget can't be saved");
            dialog.budget = "b1";
            window.check(dialog.ready, "a budget rule with its budget can be saved");
            dialog.pick("account_reserve");
            window.check(dialog.accountPicker.visible && dialog.budgetPicker.visible, "a reserve picks an account and a budget");
            window.check(!dialog.ready, "a reserve without its account can't be saved");
            dialog.edit(-1, {});
            dialog.pick("account_below");
            window.check(dialog.accountPicker.visible && !dialog.budgetPicker.visible, "an account rule shows only the account picker");
            window.check(!dialog.ready, "an account rule without its account can't be saved");
            dialog.account = "a1";
            dialog.typed = "five";
            window.check(!dialog.ready, "a wrong amount can't be saved");
            dialog.typed = "500";
            window.check(dialog.ready, "a right amount can be saved");
            dialog.saved(dialog.index, dialog.rule());
            const sent = JSON.parse(window.sent || "[]");
            window.check(sent.length === 4, "the new rule is added to the list, got " + window.sent);
            window.check(sent[3].amount === "500.00" && sent[3].lowest === false, "the rule carries the plain amount, got " + JSON.stringify(sent[3]));
            dialog.close();

            console.log(window.failed ? "alarms: FAILED" : "alarms: ok");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
