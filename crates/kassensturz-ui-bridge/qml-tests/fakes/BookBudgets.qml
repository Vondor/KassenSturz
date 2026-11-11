// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object BookBudgets, for `tools/check-panel.sh`
// (M6c.5): one running bar, one that is over and one that starts later, in
// the shape the real one gives.

import QtQuick

QtObject {
    property var view: ({bars: [], count: 0})

    function show(period, only) {
        const bar = (name, spent, left, used, permille, over) => ({
            budget: "b-" + name, categories: ["c-" + name], name: name, accounts: name === "Fun" ? "Checking" : "", every: "month", currency: "EUR", firstDay: "2026-10-01", lastDay: "2026-10-31",
            countedTo: "2026-10-19", running: true, planned: "400,00 €", carriedIn: "0,00 €", available: "400,00 €", spent: spent, left: left,
            used: used, permille: permille, over: over, hasShare: true, startsLater: false,
            notes: over ? [{word: "over", amount: "50,00 €", negative: false}] : []
        });
        // A budget that starts later: an empty bar, all of it left (D-710).
        const later = Object.assign(bar("Loan", "0,00 €", "400,00 €", "0,0 %", 0, false), {firstDay: "2026-12-01", lastDay: "2027-11-30", countedTo: "2026-12-01", running: false, startsLater: true});
        view = {bars: [bar("Food", "150,00 €", "250,00 €", "37,5 %", 375, false), bar("Fun", "450,00 €", "0,00 €", "112,5 %", 1125, true), later], count: 3};
    }

    // The filters the next requests are narrowed by (M6c.7); kept to read.
    property string scope: ""
    function setScope(text) { scope = text; }

    // The budgets the next requests show (D-642); kept to read.
    property string budgets: ""
    function setBudgets(text) { budgets = text; }

    function refresh() { }
}
