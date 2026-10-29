// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object BookCards, for `tools/check-panel.sh`
// (M6a.7): the four cards of one currency in the shape the real one gives.

import QtQuick

QtObject {
    property var view: ({empty: true, sets: []})

    function show(period, compare) {
        const card = (measure, value, direction) => ({measure: measure, currency: "EUR", value: value, previous: "100,00 €", change: "20,00 €", changePercent: "20 %", direction: direction});
        view = {empty: false, sets: [{currency: "EUR", cards: [
            card("income", "120,00 €", "up"), card("expenses", "80,00 €", "down"), card("net", "40,00 €", "up"),
            Object.assign(card("savings-rate", "33,3 %", "up"), {permille: 333})
        ]}]};
    }

    function refresh() { }
}
