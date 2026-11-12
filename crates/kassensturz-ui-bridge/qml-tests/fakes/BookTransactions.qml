// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object BookTransactions, for `tools/check-panel.sh`
// (US4.7, D-716): two rows of 30 transactions, from two accounts, in the
// shape the real one gives. Synthetic data only.

import QtQuick

QtObject {
    property var view: undefined
    // What the tile asked for last; kept to read.
    property string period: ""
    property string rows: ""
    property string scope: ""

    function show(asked, count) {
        period = asked;
        rows = count;
        const row = (id, account, other, kind, amount, incoming) => ({
            id: id, accountId: "a-" + account, account: account, date: "19.09.2026", otherParty: id === "t2" ? "" : "Corner shop",
            kind: kind, other: other, amount: amount, exact: amount, incoming: incoming
        });
        view = {
            rows: [row("t1", "Checking", "Groceries", "simple", "-23,40 €", false), row("t2", "Savings", "Checking", "transfer", "100,00 €", true)],
            total: 30, severalAccounts: true,
            filter: {own: [], filtered: [], notes: [], accounts: [], accountsMore: 0, categories: [], categoriesMore: 0, nothing: false}
        };
    }

    function setScope(text) { scope = text; }

    function refresh() { }
}
