// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust AccountList, for `tools/check-panel.sh` (#1031):
// the accounts a test gives it, read the way the pages read the real one.

import QtQuick

QtObject {
    // The accounts in detail, as `details()` gives them; the list of the
    // accounts screen is made from them.
    property var all: []
    readonly property var view: ({ sections: [{ currency: "EUR", accounts: all.map(account => ({ id: account.id, name: account.name, balance: account.exactBalance, exactBalance: account.exactBalance, owes: account.owes })) }] })
    readonly property string today: "20.11.2026"
    readonly property var currencies: ["EUR"]

    function details(id) {
        return all.find(account => account.id === id);
    }

    function amountExample(currency) {
        return "1.234,56";
    }
}
