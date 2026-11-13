// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust AccountRegister, for `tools/check-panel.sh`
// (M6-B227): one saved transaction, +530.00 into Savings, with a note, and
// one entry in Checking that could be its other side. It records what the
// page saves and which partners it asks for.
//
// For PartsPage.qml (M6-B235): a transaction of 10,00 € in two Parts; the
// preview adds up what the form holds, in cents, and says the form can't be
// read while `unreadable` is set.

import QtQuick

QtObject {
    // What the stored transaction holds; a save changes it.
    property string note: "Algae bars 33"
    // Every form the page saved, in order.
    property var saved: []
    // What the stored note was each time the partners were read.
    property var notesWhenPartnersRead: []
    property string today: "2026-09-01"
    // The Parts page: whether the preview refuses the form, how often it
    // was asked and the last form it got.
    property bool unreadable: false
    property int previews: 0
    property var lastForm: null

    // CategorySmoke.qml (M6-B234): the stored transaction's category, the
    // categories for money in and for money out, and the signal the real
    // model sends when the book was read again (e.g. after the AI changed
    // the transaction).
    property string category: "salary"
    property var incomeChoices: [{ id: "salary", path: "Salary" }]
    property var expenseChoices: []
    signal modelReset()

    // TagSmoke.qml (US4.8): the stored transaction's tags, its parts' own
    // tags and the book's tags to choose from.
    property var tags: [{ id: "garden", name: "Garden party" }]
    property var partTags: []
    property var bookTags: [
        { id: "garden", name: "Garden party", transactions: 1, parts: 0 },
        { id: "tax", name: "Tax", transactions: 0, parts: 2 },
    ]
    function tagChoices() { return bookTags; }

    function details(accountId, transactionId) {
        return ({ kind: "simple", moneyIn: true, amount: "530.00", otherParty: "", date: "2026-09-01", note: note, category: category, tags: tags, partTags: partTags });
    }
    // As the Rust side: the categories of the direction, and `current` even
    // if it is of the other kind, when the book has it.
    function categoryChoices(accountId, moneyIn, current) {
        const own = moneyIn ? incomeChoices : expenseChoices;
        const other = moneyIn ? expenseChoices : incomeChoices;
        const kept = other.filter(choice => choice.id === current);
        return kept.concat(own);
    }
    function amountExample(accountId) { return "12.50"; }
    function noteLength(typed) { return typed.length; }
    function noteLimit(stored) { return Math.max(512, stored.length); }
    function accountChoices() { return [{ id: "savings", name: "Savings" }]; }
    function editTransaction(accountId, transactionId, form) {
        saved = saved.concat([form]);
        category = form.category;
        if (form.tags !== undefined) {
            tags = form.tags;
        }
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
    function moveOffer(a, b) { return ({}); }
    function joinRemembering(a, b) { return ({}); }
    function joinPickedRemembering(a, b) { return ({}); }
    function monthlyMoves() { return []; }
    function removeMove(id) { return ({}); }
    function joinAll(expected) { return ({ joined: expected, left: 0, looksMonthly: [] }); }
    function rememberLooksMonthly(ticked) { return ({}); }

    function cents(text) {
        return text === "" ? 0 : Math.round(Number(text.replace(",", ".")) * 100);
    }
    function euro(value) {
        return (value / 100).toFixed(2).replace(".", ",") + " €";
    }
    function partsDetails(accountId, transactionId) {
        return {
            id: transactionId,
            moneyIn: false,
            total: "10,00",
            otherParty: "Example shop",
            date: "2026-11-02",
            totalFixed: false,
            fixed: [],
            parts: [
                { id: "s1", category: "c1", amount: "4,00", foreignAmount: "" },
                { id: "s2", category: "c2", amount: "6,00", foreignAmount: "" },
            ],
        };
    }
    function partCategoryChoices(accountId, transactionId, moneyIn) {
        return [
            { id: "c1", path: "Food", currency: "" },
            { id: "c2", path: "Household", currency: "" },
        ];
    }
    function partsPreview(accountId, transactionId, form) {
        previews++;
        lastForm = form;
        if (unreadable) {
            return { code: "input.not-a-number" };
        }
        let sum = 0;
        const parts = [];
        for (const part of form.parts) {
            sum += cents(part.amount);
            parts.push({ open: false });
        }
        const difference = cents(form.total) - sum;
        return {
            sum: euro(sum),
            missing: difference > 0 ? euro(difference) : "",
            over: difference < 0 ? euro(-difference) : "",
            parts: parts,
        };
    }
    function saveParts(accountId, transactionId, form) { return ({}); }

    function removalNotice(accountId, transactionId) { return ({}); }
    function removeTransaction(accountId, transactionId) { return ({}); }
}
