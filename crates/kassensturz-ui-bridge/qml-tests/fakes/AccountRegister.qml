// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the AccountRegister object, with only what PartsPage.qml
// calls, for `tools/check-panel.sh` (M6-B235). A synthetic transaction of
// 10,00 € in two Parts; the preview adds up what the form holds, in cents,
// and says the form can't be read while `unreadable` is set.

import QtQuick

QtObject {
    property bool unreadable: false
    property int previews: 0
    property var lastForm: null

    function cents(text) {
        return text === "" ? 0 : Math.round(Number(text.replace(",", ".")) * 100);
    }

    function euro(value) {
        return (value / 100).toFixed(2).replace(".", ",") + " €";
    }

    function amountExample(accountId) { return "12,34"; }

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

    function saveParts(accountId, transactionId, form) { return {}; }
    function removalNotice(accountId, transactionId) { return { holdsOriginal: false }; }
    function removeTransaction(accountId, transactionId) { return {}; }
}
