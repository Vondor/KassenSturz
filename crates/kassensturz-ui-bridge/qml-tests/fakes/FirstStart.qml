// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust FirstStart, for `tools/check-panel.sh` (#1031): it
// keeps what the wizard asked it to write, so a test can read it.

import QtQuick

QtObject {
    property bool hasAccount: false
    property bool needsCurrency: true
    property var currencies: ["EUR"]
    property bool bankOffered: false
    // The five strings per account of the last `finish()`.
    property var written: []
    property string writtenCurrency: ""

    function refresh() {}

    function checkAccount(type, name, currency, balance, date) {
        return {};
    }

    function finish(currency, incomeNames, incomeParents, spendingNames, spendingParents, typed) {
        writtenCurrency = currency;
        written = typed;
        hasAccount = true;
        return {};
    }
}
