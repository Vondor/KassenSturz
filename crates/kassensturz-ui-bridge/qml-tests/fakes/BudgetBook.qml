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
            {id: "b-food", category: "c-food", name: "Food", every: "month", currency: "EUR", amount: "400,00 €", amountInput: "400,00", carryOver: true,
             state: "running", firstDay: "2026-09-01", lastDay: "", canStop: true,
             changes: [{day: "2026-10-01", firstDay: "2026-10-01", lastDay: "2026-10-31", current: true},
                       {day: "2026-09-01", firstDay: "2026-09-01", lastDay: "2026-09-30", current: false}]},
            {id: "b-rent", category: "c-rent", name: "Rent", every: "year", currency: "EUR", amount: "9.600,00 €", amountInput: "9600,00", carryOver: false,
             state: "upcoming", firstDay: "2027-01-01", lastDay: "", canStop: false, changes: []}
        ],
        categories: [
            {id: "c-food", name: "Food", path: "Food", depth: 0, currency: "EUR"},
            {id: "c-rent", name: "Rent", path: "Rent", depth: 0, currency: "EUR"}
        ]
    })

    function startChoices(every) {
        calls.push("startChoices:" + every);
        return [{day: "2026-11-01", firstDay: "2026-11-01", lastDay: "2026-11-30", current: false},
                {day: "2026-10-01", firstDay: "2026-10-01", lastDay: "2026-10-31", current: true}];
    }

    function addBudget(category, every, amount, carryOver, firstDay, lastDay) {
        calls.push("addBudget:" + [category, every, amount, carryOver, firstDay, lastDay].join(":"));
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
