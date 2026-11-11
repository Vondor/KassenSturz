// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust AccountRegister, for `tools/check-panel.sh`
// (M6-B227): one saved transaction, +530.00 into Savings, with a note, and
// one entry in Checking that could be its other side. It records what the
// page saves and which partners it asks for.

import QtQuick

QtObject {
    // What the stored transaction holds; a save changes it.
    property string note: "Algae bars 33"
    // Every form the page saved, in order.
    property var saved: []
    // What the stored note was each time the partners were read.
    property var notesWhenPartnersRead: []
    property string today: "2026-09-01"

    function details(accountId, transactionId) {
        return ({ kind: "simple", moneyIn: true, amount: "530.00", otherParty: "", date: "2026-09-01", note: note, category: "salary" });
    }
    function categoryChoices(accountId, moneyIn, current) {
        return [{ id: "salary", path: "Salary" }];
    }
    function amountExample(accountId) { return "12.50"; }
    function noteLength(typed) { return typed.length; }
    function noteLimit(stored) { return Math.max(512, stored.length); }
    function accountChoices() { return [{ id: "savings", name: "Savings" }]; }
    function editTransaction(accountId, transactionId, form) {
        saved = saved.concat([form]);
        if (form.note !== undefined) {
            note = form.note;
        }
        return ({});
    }
    function addTransaction(accountId, form) { return ({ id: "new" }); }
    function transferSuggestions(id) { return []; }
    function transferPartners(id) {
        notesWhenPartnersRead = notesWhenPartnersRead.concat([note]);
        return [];
    }
    function joinPicked(a, b) { return ({}); }
    function joinTransfer(a, b) { return ({}); }
    function removalNotice(accountId, transactionId) { return ({}); }
    function removeTransaction(accountId, transactionId) { return ({}); }
}
