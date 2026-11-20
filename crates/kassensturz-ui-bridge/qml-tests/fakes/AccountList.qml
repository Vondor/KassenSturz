// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object AccountList, for `tools/check-panel.sh`.
// StartBalanceSmoke (B-411) sets what `details` and the check return, as
// accounts_model.rs builds it, and reads back what the page asked for.
// GuidedSmoke (#1031) gives it accounts in `all`, read the way the pages
// read the real one.

import QtQuick

QtObject {
    // The accounts in detail, as `details()` gives them; the list of the
    // accounts screen is made from them unless a test sets `view` itself.
    property var all: []
    property var view: ({ sections: all.length === 0 ? [] : [{ currency: "EUR", accounts: all.map(account => ({ id: account.id, name: account.name, balance: account.exactBalance, exactBalance: account.exactBalance, owes: account.owes })) }], closed: [] })
    property string today: "2026-11-19"
    readonly property var currencies: ["EUR"]
    // What `details` (without `all`) and `checkAgainstBank` return.
    property var account: ({})
    property var check: ({})
    // The last correction the page asked for: "bank|date".
    property string corrected: ""

    function details(id) {
        const found = all.find(account => account.id === id);
        return found === undefined ? account : found;
    }
    function amountExample(currency) { return "12,34"; }
    function checkAgainstBank(id, bank, date) { return check; }
    function correctStartingBalance(id, bank, date) {
        corrected = bank + "|" + date;
        return ({});
    }
}
