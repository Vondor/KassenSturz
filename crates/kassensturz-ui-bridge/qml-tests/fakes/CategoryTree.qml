// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust CategoryTree, for `tools/check-panel.sh` (D-910):
// `addCategory` adds a top-level row, refuses an empty or a repeated name
// with the codes the real one gives, and sends `changed` like it.

import QtQuick

QtObject {
    property var view: ({ rows: [] })
    // Every category the page asked to add, as text.
    property var asked: []
    signal changed()

    function refresh() {}
    function parentChoices(income, id) { return []; }
    function addCategory(name, income, parent, currency) {
        asked = asked.concat([name + " " + (income ? "in" : "out") + " " + parent]);
        if (name.trim() === "") {
            return ({ code: "store.empty-name" });
        }
        if (view.rows.some(row => row.parent === parent && row.name === name.trim())) {
            return ({ code: "store.duplicate-account-name" });
        }
        view = ({ rows: view.rows.concat([{ id: "new-" + name.trim().toLowerCase(), name: name.trim(), path: name.trim(), depth: 0, parent: parent, income: income, currency: "EUR" }]) });
        changed();
        return ({});
    }
}
