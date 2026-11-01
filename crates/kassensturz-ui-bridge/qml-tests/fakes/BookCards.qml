// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object BookCards, for `tools/check-panel.sh`
// (M6a.7): the four cards of one currency in the shape the real one gives.

import QtQuick

QtObject {
    property var view: ({empty: true, sets: []})
    property var asked: []

    function show(period, compare) {
        const card = (measure, value, direction) => ({measure: measure, currency: "EUR", value: value, previous: "100,00 €", change: "20,00 €", changePercent: "20 %", direction: direction, exactValue: value, negative: false});
        view = {empty: false, sets: [{currency: "EUR", cards: [
            card("income", "120,00 €", "up"), card("expenses", "80,00 €", "down"), card("net", "40,00 €", "up"),
            Object.assign(card("savings-rate", "33,3 %", "up"), {permille: 333})
        ]}]};
    }

    // What you have, owe and are worth (M6c.2), one set in the shape the real
    // one gives.
    function showDataset(dataset, period, compare) {
        if (dataset !== "account_balances") {
            show(period, compare);
            return;
        }
        const card = (measure, value, negative) => ({measure: measure, currency: "EUR", value: value, previous: "900,00 €", change: "100,00 €", changePercent: "11,1 %", direction: "up", exactValue: value, negative: negative, magnitude: negative ? value.replace("-", "") : value, exactMagnitude: negative ? value.replace("-", "") : value});
        asked.push([dataset, period, compare].join(" "));
        view = {empty: false, sets: [{currency: "EUR", cards: [
            card("net_worth", "1.000,00 €", false), card("assets", "1.250,00 €", false), card("liabilities", "250,00 €", false)
        ]}]};
    }

    // The filters the next requests are narrowed by (M6c.7); kept to read.
    property string scope: ""
    function setScope(text) { scope = text; }

    function refresh() { }
}
