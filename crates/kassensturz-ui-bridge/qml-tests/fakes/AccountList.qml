// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object AccountList, for `tools/check-panel.sh`
// (StartBalanceSmoke, B-411): the test sets what the check returns, as
// accounts_model.rs builds it, and reads back what the page asked for.

import QtQuick

QtObject {
    property var view: ({ sections: [], closed: [] })
    property string today: "2026-11-19"
    // What `details` and `checkAgainstBank` return.
    property var account: ({})
    property var check: ({})
    // The last correction the page asked for: "bank|date".
    property string corrected: ""

    function details(id) { return account; }
    function amountExample(currency) { return "12,34"; }
    function checkAgainstBank(id, bank, date) { return check; }
    function correctStartingBalance(id, bank, date) {
        corrected = bank + "|" + date;
        return ({});
    }
}
