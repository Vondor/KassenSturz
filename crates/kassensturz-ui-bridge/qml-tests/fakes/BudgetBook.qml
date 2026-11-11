// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object BudgetBook, for `tools/check-panel.sh`
// (M6c.5): the same `view` shape and calls, recording each call in `calls`.

import QtQuick

QtObject {
    id: fake

    property var calls: []
    signal changed()

    property var view: ({
        budgets: [
            {id: "b-food", categories: ["c-food"], name: "Food", ownName: "", categoryNames: "Food", accounts: "", every: "month", salaryDay: 0, currency: "EUR", amount: "400,00 €", amountInput: "400,00", carryOver: true,
             accountIds: [], countsFrom: "2026-10-01",
             history: [{earlier: true, day: "2026-09-30", categoryNames: "Rent", accounts: "Checking"}],
             state: "running", firstDay: "2026-09-01", lastDay: "", canStop: true,
             changes: [{day: "2026-10-01", firstDay: "2026-10-01", lastDay: "2026-10-31", current: true},
                       {day: "2026-09-01", firstDay: "2026-09-01", lastDay: "2026-09-30", current: false}]},
            {id: "b-rent", categories: ["c-rent"], name: "Flat", ownName: "Flat", categoryNames: "Rent", accounts: "Checking", every: "salary-month", salaryDay: 25, currency: "EUR", amount: "800,00 €", amountInput: "800,00", carryOver: false,
             accountIds: ["a-checking"], countsFrom: "2027-01-25", history: [],
             state: "upcoming", firstDay: "2027-01-25", lastDay: "", canStop: false, changes: []}
        ],
        categories: [
            {id: "c-food", name: "Food", path: "Food", depth: 0, currency: "EUR"},
            {id: "c-rent", name: "Rent", path: "Rent", depth: 0, currency: "EUR"}
        ],
        accounts: [
            {id: "a-checking", name: "Checking"},
            {id: "a-savings", name: "Savings"}
        ]
    })

    function startChoices(every) {
        calls.push("startChoices:" + every);
        return [{day: "2026-11-01", firstDay: "2026-11-01", lastDay: "2026-11-30", current: false},
                {day: "2026-10-01", firstDay: "2026-10-01", lastDay: "2026-10-31", current: true}];
    }

    // `days` is the first day, a line break and the last day; the name the
    // user typed comes last in the record (D-664).
    function addBudget(name, chosen, every, amount, carryOver, days) {
        const [firstDay, lastDay] = days.split("\n");
        calls.push("addBudget:" + [chosen, every, amount, carryOver, firstDay, lastDay ?? "", name].join(":"));
        return amount === "" ? {code: "store.budget-amount", reason: "not-positive"} : {};
    }

    // `fromDay` may go on after a line break with what the budget counts
    // from then on (D-689).
    function changeBudget(id, name, changesAmount, fromDay, amount, carryOver) {
        calls.push("changeBudget:" + [id, name, changesAmount, fromDay, amount, carryOver].join(":"));
        return {};
    }

    function stopBudget(id) {
        calls.push("stopBudget:" + id);
        return {};
    }

    function removeBudget(id) {
        calls.push("removeBudget:" + id);
        return {};
    }

    function refresh() { }
}
