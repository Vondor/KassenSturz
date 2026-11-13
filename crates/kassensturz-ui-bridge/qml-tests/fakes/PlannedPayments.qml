// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object PlannedPayments, for `tools/check-panel.sh`
// (US7.1): one plan, Rent, whose payment of 1 October is waiting for the
// user, in the shapes the real one gives. It records what the screens save,
// skip, book and match.

import QtQuick

QtObject {
    property var view: ({
        plans: [{
            id: "rent", name: "Rent", account: "Savings", accountId: "savings",
            amount: "750,00 €", exactAmount: "750,00 €", outgoing: true, unit: "month",
            every: 1, firstDue: "2026-09-01", ends: "", nextDue: "2026-11-02",
            paused: false, accountClosed: false, category: "", party: "",
        }],
        toConfirm: [{
            planId: "rent", name: "Rent", account: "Savings", amount: "750,00 €",
            exactAmount: "750,00 €", outgoing: true, due: "2026-10-01",
            dueOn: "2026-10-01", state: "overdue",
        }],
        comingUp: [],
    })
    // Every form saved, and every payment skipped, booked or matched.
    property var saved: []
    property var skipped: []
    property var booked: []
    property var matched: []
    signal changed()

    function refresh() {}
    function monthFrom(start) { return view.forecast ?? ({}); }
    function form(id, accountId) {
        return ({
            name: id === "" ? "" : "Rent", accountId: id === "" ? accountId : "savings",
            outgoing: true, amount: id === "" ? "" : "750,00", categoryId: "", unit: "month",
            every: "1", firstDue: "2026-09-01", ends: "", tolerance: "", party: "", fromEntry: "",
        });
    }
    function formFromEntry(transactionId, accountId, fallbackName) {
        return ({
            name: "Landlord", accountId: accountId, outgoing: false, amount: "530,00",
            categoryId: "salary", unit: "month", every: "1", firstDue: "2026-09-01",
            ends: "", tolerance: "", party: "", fromEntry: transactionId,
        });
    }
    function nextDates(form) {
        return form.firstDue === "" ? [] : ["2026-11-02", "2026-12-01", "2027-01-01"];
    }
    function save(id, form) {
        saved = saved.concat([form]);
        return ({ id: id === "" ? "new" : id });
    }
    function setPaused(id, paused) { return ({}); }
    function remove(id) { return ({}); }
    function skip(planId, dueOn) {
        skipped = skipped.concat([planId + " " + dueOn]);
        return ({});
    }
    function bookForm(planId) { return ({ amount: "750,00", date: "2026-10-19" }); }
    function bookIt(planId, dueOn, date, amount) {
        booked = booked.concat([planId + " " + dueOn + " " + date + " " + amount]);
        return ({ id: "booked" });
    }
    function entryChoices(planId, dueOn) {
        return [{ id: "into", date: "2026-10-02", payee: "Landlord", amount: "-750,00 €", exactAmount: "-750,00 €" }];
    }
    function matchEntry(planId, dueOn, transactionId) {
        matched = matched.concat([planId + " " + dueOn + " " + transactionId]);
        return ({});
    }
}
