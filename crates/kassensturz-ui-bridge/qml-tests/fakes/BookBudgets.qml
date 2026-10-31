// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object BookBudgets, for `tools/check-panel.sh`
// (M6c.5): one running bar and one that is over, in the shape the real one
// gives.

import QtQuick

QtObject {
    property var view: ({bars: [], count: 0})

    function show(period, only) {
        const bar = (name, spent, left, used, permille, over) => ({
            budget: "b-" + name, category: "c-" + name, name: name, every: "month", currency: "EUR", firstDay: "2026-10-01", lastDay: "2026-10-31",
            countedTo: "2026-10-19", running: true, planned: "400,00 €", carriedIn: "0,00 €", available: "400,00 €", spent: spent, left: left,
            used: used, permille: permille, over: over, hasShare: true,
            notes: over ? [{word: "over", amount: "50,00 €", negative: false}] : []
        });
        view = {bars: [bar("Food", "150,00 €", "250,00 €", "37,5 %", 375, false), bar("Fun", "450,00 €", "0,00 €", "112,5 %", 1125, true)], count: 2};
    }

    function refresh() { }
}
