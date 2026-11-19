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
    // For AiSettingsPage and the chat panel (B-386, B-396): a provider on
    // this computer is saved with a model. ModelChoiceSmoke sets its own
    // form without one.
    property bool testing: false
    property bool fake: false
    property var form: ({ address: "http://localhost:1234/v1", model: "m", keyUse: "none", place: "this", consent: "not-needed" })
    signal testFinished(var result)
    function load() { return form; }
    function check(address, allowUnencrypted) { return ({ place: "this", network: "this", unencrypted: false }); }
    function withScheme(address) { return address.trim(); }
    function save(address, model, keyUse, key, allowUnencrypted) { return ({ saved: true }); }
    function testConnection(key) {}
    function capabilityRequest() { return ""; }
    function statement() { return []; }
    function forgetKey() { return ({}); }
    function giveConsent() { return ({}); }
    function withdrawConsent() { return ({}); }
    function sample(leaveOut) { return ({}); }
    function notes() { return ""; }
    function notesLimit() { return 2000; }
    function setNotes(text) { return true; }
    function notice(word) { return true; }
    function setNotice(word, value) { return true; }
    function slowAnswer() { return "warn"; }
    function setSlowAnswer(value) { return true; }
    function writeTools() { return false; }
    function setWriteTools(value) { return true; }
    function writeToolsProblem() { return ""; }
    function billFolder() { return ""; }
    function setBillFolder(folder) { return ({}); }
    function removeBillFolder() { return true; }
    function receiptFolder(which) { return ""; }
    function setReceiptFolder(which, folder) { return ({}); }
    function removeReceiptFolder(which) { return true; }

    function locked(key) { return lockedKeys[key] === true; }
    function setLocked(key, value) {
        const next = Object.assign({}, lockedKeys);
        next[key] = value;
        lockedKeys = next;
        lockCalls = lockCalls.concat([[key, value]]);
        return true;
    }
}
