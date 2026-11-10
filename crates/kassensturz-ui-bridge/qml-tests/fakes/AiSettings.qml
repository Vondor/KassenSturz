// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object AiSettings, for `tools/check-panel.sh`
// (#631): the AI may change transactions, and amount, date and payee are
// locked, as at the first start (D-252).

import QtQuick

QtObject {
    property var allowed: ({ AllowSortTransactions: true })
    property var lockedKeys: ({ LockAmount: true, LockDate: true, LockPayee: true })
    // Every call of setLocked, as [key, value].
    property var lockCalls: []

    function allows(key) { return allowed[key] === true; }
    function setAllows(key, value) {
        const next = Object.assign({}, allowed);
        next[key] = value;
        allowed = next;
        return true;
    }
    function locked(key) { return lockedKeys[key] === true; }
    function setLocked(key, value) {
        const next = Object.assign({}, lockedKeys);
        next[key] = value;
        lockedKeys = next;
        lockCalls = lockCalls.concat([[key, value]]);
        return true;
    }
}
