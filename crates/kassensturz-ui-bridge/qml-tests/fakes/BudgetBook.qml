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
            {id: "b-food", categories: ["c-food"], name: "Food", accounts: "", every: "month", salaryDay: 0, currency: "EUR", amount: "400,00 €", amountInput: "400,00", carryOver: true,
             state: "running", firstDay: "2026-09-01", lastDay: "", canStop: true,
             changes: [{day: "2026-10-01", firstDay: "2026-10-01", lastDay: "2026-10-31", current: true},
                       {day: "2026-09-01", firstDay: "2026-09-01", lastDay: "2026-09-30", current: false}]},
            {id: "b-rent", categories: ["c-rent"], name: "Rent", accounts: "Checking", every: "salary-month", salaryDay: 25, currency: "EUR", amount: "800,00 €", amountInput: "800,00", carryOver: false,
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

    function addBudget(chosen, every, amount, carryOver, firstDay, lastDay) {
        calls.push("addBudget:" + [chosen, every, amount, carryOver, firstDay, lastDay].join(":"));
        return amount === "" ? {code: "store.budget-amount", reason: "not-positive"} : {};
    }

    function changeBudget(id, fromDay, amount, carryOver) {
        calls.push("changeBudget:" + [id, fromDay, amount, carryOver].join(":"));
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
