// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object BookAlarms, for `tools/check-panel.sh`
// (US4.9): one row per rule in the shape the real one gives, each rule
// triggered, so the tile has a line for every one.

import QtQuick

QtObject {
    property var view: ({rows: [], count: 0, shown: 0})
    // The rules last asked for, kept to read.
    property string asked: ""

    function show(rules) {
        asked = rules;
        let list = [];
        try {
            list = JSON.parse(rules);
        } catch (error) {
            list = [];
        }
        const rows = list.map((rule, index) => ({
            index: index, reason: rule.reason, state: "triggered", why: "",
            account: rule.account ?? "", accountName: rule.account ? "Shared account" : "",
            budget: rule.budget ?? "", budgetName: rule.budget ? "Household" : "",
            every: rule.budget ? "month" : "", ended: rule.reason === "budget_under",
            balance: "200,00 €", balanceDay: "2026-10-03", lowest: rule.lowest ?? false,
            limit: "500,00 €", amountInput: rule.amount ? "500,00" : "",
            planned: "900,00 €", available: "900,00 €", spent: "800,00 €", over: "",
            nothingToSpend: false, share: rule.share ?? 0, times: rule.times ?? 0
        }));
        view = {rows: rows, count: rows.length, shown: rows.length};
    }

    function refresh() { }

    // A typed amount in the tile's plain form; "five" is no amount.
    function readAmount(typed, account) {
        return typed === "five" || account === "" ? "" : "500.00";
    }
}
