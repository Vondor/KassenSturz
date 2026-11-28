// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object AiChat, for `tools/check-panel.sh`
// (M6a.9): one prepared set, with a new chart and a new budget as the bridge
// writes them (`ChangeView::NewView` and `ChangeView::NewBudget` in a
// `ChangeRow`, M6c.11), and the calls the review page
// makes, among them the user's corrections (M6b.8, D-293), which the real
// object checks in Rust.

import QtQuick

QtObject {
    // For the chat panel (SuggestionSmoke, B-386): no question runs, and
    // the questions asked are kept.
    property bool busy: false
    property bool available: false
    property var asked: []
    signal finished(var result)
    signal reviewing(var request)
    signal logged(var entry)
    function ask(question, key) {
        asked = asked.concat([question]);
        return { started: true };
    }
    function askFeature(feature, accountId, key) { return {}; }
    function checkRequests() {}
    function reset() {}
    function stop() {}
    function issueUsed(how) {}

    property var confirmed: []
    property var discarded: []
    property var edits: []
    readonly property var aiView: ({
        title: "Where the money goes",
        chart: "chart.pie",
        period: "this-year",
        dataset: "spending_by_category"
    })
    property var view: aiView
    property bool edited: false
    property bool leftOut: false
    // The set "sort": the AI's sort with all Rules (US2.9, D-682), as
    // `ChangeView::SortWithRules` writes it.
    property bool sortAgain: false

    // The set "correction": exercises inline Change on editTransaction and
    // splitTransaction cards (US11.2, D-1016).
    property bool correctionNewCatLeftOut: false
    property bool correctionEditEdited: false
    property bool correctionSplitEdited: false
    property bool correctionSplitNoteUnlocked: true
    property var correctionEditForm: ({ transaction_id: "tx-1", note: "Order 104 · Pens" })
    property var correctionSplitForm: ({
        transaction_id: "tx-2",
        parts: [
            { new_category: "gadgets", amount: "-12.50", name: "Mouse" },
            { category_id: "cat-food", amount: "-7.49", name: "Snack" }
        ]
    })

    function correctionChanges() {
        const partLabel = part => {
            if (part.category_id === "cat-food") return "Food";
            if (part.category_id === "cat-office") return "Office supplies";
            if (part.new_category === "gadgets") return "Gadgets";
            return "";
        };
        const fmt = amount => amount.replace(".", ",") + " €";
        const editLines = [];
        if (correctionEditForm.payee !== undefined) {
            editLines.push({ field: "payee", before: "Stationery Shop", after: correctionEditForm.payee });
        }
        if (correctionEditForm.note !== undefined) {
            editLines.push({ field: "note", before: "Order 104", after: correctionEditForm.note });
        }
        return [{
            kind: "newCategory",
            slot: "new_categories/0",
            edited: false,
            leftOut: correctionNewCatLeftOut,
            fields: correctionNewCatLeftOut ? [] : ["name", "parent_id"],
            form: correctionNewCatLeftOut ? "" : JSON.stringify({ key: "gadgets", name: "Gadgets" }),
            name: "Gadgets",
            path: "Gadgets",
            income: false,
            currency: "EUR"
        }, {
            kind: "editTransaction",
            slot: "edit_transactions/0",
            edited: correctionEditEdited,
            leftOut: false,
            fields: ["note"],
            form: JSON.stringify(correctionEditForm),
            date: "04.11.2026",
            payee: "Stationery Shop",
            note: "Order 104",
            account: "Everyday Checking",
            amount: "-9,50 €",
            lines: editLines
        }, {
            kind: "splitTransaction",
            slot: "split_transactions/0",
            edited: correctionSplitEdited,
            leftOut: false,
            fields: ["parts"],
            form: JSON.stringify(correctionSplitForm),
            date: "05.11.2026",
            payee: "General Store",
            account: "Everyday Checking",
            total: "-19,99 €",
            currency: "EUR",
            noteUnlocked: correctionSplitNoteUnlocked,
            before: [{ category: "", amount: "-19,99 €", name: "" }],
            parts: correctionSplitForm.parts.map(p => ({
                category: partLabel(p),
                amount: fmt(p.amount),
                name: p.name ?? ""
            }))
        }];
    }

    function sortChanges() {
        return [{
            kind: "sortWithRules",
            slot: "sort_with_rules/0",
            edited: sortAgain,
            leftOut: false,
            fields: ["sort_again"],
            form: JSON.stringify({sort_again: sortAgain}),
            ruleName: "",
            sortAgain: sortAgain,
            changed: sortAgain ? 2 : 1,
            transfers: 1,
            noRate: 0,
            closed: 0,
            other: 0,
            examples: [{
                accountId: "a1",
                account: "Everyday Checking",
                id: "t1",
                date: "05.10.2026",
                otherParty: "Pet <b>Shop</b>",
                category: "Pets",
                amount: "-19,99 €",
                exactAmount: "-19,99 €",
                incoming: false,
                kind: "simple"
            }]
        }];
    }

    function changes(setId) {
        if (setId === "sort") {
            return sortChanges();
        }
        if (setId === "correction") {
            return correctionChanges();
        }
        return [{
            kind: "newView",
            slot: "new_views/0",
            edited: edited,
            leftOut: leftOut,
            fields: leftOut ? [] : ["title", "chart", "period"],
            form: leftOut ? "" : JSON.stringify(view),
            title: view.title,
            period: view.period,
            charts: ["chart.pie", "chart.donut", "chart.bar"],
            // What a chart's own description leaves out (D-729).
            filters: [{field: "category", test: "one-of", values: ["Food", "Rent"], yesNo: false}, {field: "closed", test: "equal", values: ["no"], yesNo: true}],
            component: JSON.stringify({type: view.chart, dataset: "spending_by_category", params: {grouping: "category"}, props: {title: view.title}})
        }, {
            // A number card with a formula the AI wrote, over a salary
            // month (US1.17, D-650).
            kind: "newView",
            slot: "new_views/1",
            edited: false,
            leftOut: false,
            fields: ["title", "chart", "period"],
            form: JSON.stringify({title: "Food less travel", chart: "kpi.card", period: "last-salary-month-25", dataset: "income_expense_by_month"}),
            title: "Food less travel",
            period: "last-salary-month-25",
            charts: ["kpi.card", "progress.bar"],
            filters: [],
            component: JSON.stringify({type: "kpi.card", dataset: "income_expense_by_month", params: {measure: "formula"}, props: {title: "Food less travel", formula: "=Pets - Travel"}})
        }, {
            kind: "newBudget",
            slot: "new_budgets/0",
            edited: false,
            leftOut: false,
            fields: [],
            form: "",
            category: "Food › Groceries",
            every: "salary-month",
            salaryDay: 25,
            from: "25.10.2026",
            to: "24.11.2026",
            amount: "390,00 €",
            carryOver: true,
            basisPeriods: 6,
            basisFrom: "01.05.2026",
            basisTo: "31.10.2026",
            basisSpent: "2.324,40 €",
            basisAverage: "387,40 €",
            basisStep: "10,00 €",
            basisLeftOutFrom: "",
            basisLeftOutTo: ""
        }, {
            // A budget that counts other categories from now on (D-690).
            kind: "editBudget",
            slot: "edit_budgets/0",
            edited: false,
            leftOut: false,
            fields: [],
            form: "",
            name: "",
            renames: false,
            newName: "",
            changesAmount: false,
            changesCounts: true,
            category: "Food",
            accounts: "",
            newCategory: "Food, Fun",
            newAccounts: "Everyday Checking",
            every: "month",
            salaryDay: 0,
            from: "01.11.2026",
            to: "30.11.2026",
            before: "300,00 €",
            amount: "300,00 €",
            carriedOver: false,
            carryOver: false,
            basisPeriods: 0,
            basisFrom: "",
            basisTo: "",
            basisSpent: "",
            basisAverage: "",
            basisStep: "",
            basisLeftOutFrom: "",
            basisLeftOutTo: ""
        }];
    }

    function changeChoices(setId) {
        if (setId === "correction") {
            const list = [
                {id: "cat-food", key: "", path: "Food", income: false, currency: "EUR"},
                {id: "cat-office", key: "", path: "Office supplies", income: false, currency: "EUR"},
                {id: "cat-usd", key: "", path: "Travel USD", income: false, currency: "USD"}
            ];
            if (!correctionNewCatLeftOut) {
                list.push({id: "", key: "gadgets", path: "Gadgets", income: false, currency: "EUR"});
            }
            return list;
        }
        return [{id: "1b0c", key: "", path: "Food", income: false, currency: "EUR"}];
    }

    function editChange(setId, slot, fields) {
        const changed = JSON.parse(fields);
        edits = edits.concat([fields]);
        if (setId === "sort") {
            sortAgain = changed.sort_again === true;
            return changes(setId);
        }
        if (setId === "correction") {
            if (slot === "edit_transactions/0") {
                if (typeof changed.note === "string" && changed.note.indexOf("\n") >= 0) {
                    return {code: "ai-changes.edit-text"};
                }
                const next = Object.assign({}, correctionEditForm);
                if (changed.note === null) {
                    delete next.note;
                } else if (changed.note !== undefined) {
                    next.note = changed.note;
                }
                if (changed.payee === null) {
                    delete next.payee;
                } else if (changed.payee !== undefined) {
                    next.payee = changed.payee;
                }
                correctionEditForm = next;
                correctionEditEdited = true;
                return changes(setId);
            }
            if (slot === "split_transactions/0") {
                correctionSplitForm = { transaction_id: "tx-2", parts: changed.parts };
                correctionSplitEdited = true;
                return changes(setId);
            }
            return {code: "ai-changes.not-editable"};
        }
        if (changed.title === "") {
            return {code: "ai-changes.edit-text"};
        }
        view = Object.assign({}, view, changed);
        edited = true;
        return changes(setId);
    }

    function leaveOutChange(setId, slot) {
        if (setId === "correction" && slot === "new_categories/0") {
            const stillUsed = (correctionSplitForm.parts ?? []).some(p => p.new_category === "gadgets");
            if (stillUsed) {
                return {code: "ai-changes.edit-in-use"};
            }
            correctionNewCatLeftOut = true;
            return changes(setId);
        }
        leftOut = true;
        return changes(setId);
    }

    function backToAi(setId, slot) {
        if (setId === "correction") {
            if (slot === "edit_transactions/0") {
                correctionEditForm = { transaction_id: "tx-1", note: "Order 104 · Pens" };
                correctionEditEdited = false;
                return changes(setId);
            }
            if (slot === "split_transactions/0") {
                if (correctionNewCatLeftOut) {
                    return {code: "ai-changes.edit-in-use"};
                }
                correctionSplitForm = {
                    transaction_id: "tx-2",
                    parts: [
                        { new_category: "gadgets", amount: "-12.50", name: "Mouse" },
                        { category_id: "cat-food", amount: "-7.49", name: "Snack" }
                    ]
                };
                correctionSplitEdited = false;
                return changes(setId);
            }
            if (slot === "new_categories/0") {
                correctionNewCatLeftOut = false;
                return changes(setId);
            }
        }
        view = aiView;
        edited = false;
        leftOut = false;
        return changes(setId);
    }

    // The danger line of a set (B-248): the fake sets take nothing away
    // and touch only a few transactions, so none has one.
    function danger(setId) {
        return {};
    }

    // Whether the user's own AI tool prepared a set (B-371): the fake sets
    // all come from the chat.
    function isFromOwnTool(setId) {
        return false;
    }

    function confirm(setId) {
        confirmed = confirmed.concat([setId]);
        return {ok: true};
    }

    function discard(setId) { discarded = discarded.concat([setId]); }

    property var discardedReasons: []

    function discardBecause(setId, reasons) {
        discarded = discarded.concat([setId]);
        discardedReasons = discardedReasons.concat([reasons]);
    }

    // The answers to a request shown for a look (AiRequestReview.qml, B-375).
    property var reviewAnswers: []

    function reviewSend() { reviewAnswers = reviewAnswers.concat(["send"]); }
    function reviewSendAndStopAsking() { reviewAnswers = reviewAnswers.concat(["send and stop asking"]); }
    function reviewStop() { reviewAnswers = reviewAnswers.concat(["stop"]); }
    function reviewLeaveOut(phrase) { reviewAnswers = reviewAnswers.concat(["leave out " + phrase]); }
}
