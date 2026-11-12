// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object ChartDescription, for
// `tools/check-panel.sh` (M6e.3): the check of a custom chart's description.
// `answer` is what the next check finds; `checked` lists the texts asked
// about, so a smoke test can read what the panel sent.

import QtQuick

QtObject {
    property var view: ({ok: true, dataset: "", canDraw: false, count: 0, unsupported: [], text: "", opened: 0})
    property var answer: ({ok: true, dataset: "income_expense_by_month", canDraw: true, count: 0, unsupported: [], text: "", opened: 0})
    property var checked: []
    property var openedFiles: []

    function check(text) {
        checked.push(text);
        view = answer;
    }

    // Two ready examples, as `examples()` of the Rust object lists them.
    function examples() {
        return [
            {key: "balance-and-flows", dataset: "cash_flow_by_month", text: "{\"data\": {\"name\": \"cash_flow_by_month\"}}"},
            {key: "left-each-month", dataset: "income_expense_by_month", text: "{\"data\": {\"name\": \"income_expense_by_month\"}}"}
        ];
    }

    function openFile(file) {
        openedFiles.push(String(file));
        view = ({ok: true, dataset: "income_expense_by_month", canDraw: true, count: 0, unsupported: [], text: "from the file", opened: (view.opened ?? 0) + 1});
    }
}
