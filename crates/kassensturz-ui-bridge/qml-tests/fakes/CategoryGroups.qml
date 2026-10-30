// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object CategoryGroups, with the same calls, for
// `tools/check-panel.sh` (M6b.5c). It keeps the calls it got.

import QtQuick

QtObject {
    id: fake

    property var calls: []
    property var hidden: ["c-food"]
    signal changed()

    function categories() {
        return [
            {id: "c-food", name: "Food", path: "Food", depth: 0, income: false, hidden: hidden.includes("c-food")},
            {id: "c-groceries", name: "Groceries", path: "Food › Groceries", depth: 1, income: false, hidden: hidden.includes("c-groceries")},
            {id: "c-rent", name: "Rent", path: "Rent", depth: 0, income: false, hidden: hidden.includes("c-rent")},
            {id: "c-salary", name: "Salary", path: "Salary", depth: 0, income: true, hidden: hidden.includes("c-salary")}
        ];
    }

    property var view: ({
        groups: [
            {id: "g-fixed", name: "Fixed costs", income: false, members: [{id: "c-rent", name: "Rent", group: false}]},
            {id: "g-pay", name: "Pay", income: true, members: []}
        ],
        categories: categories()
    })

    function refresh() { view = ({groups: view.groups, categories: categories()}); }

    function setHidden(ids) {
        calls.push("setHidden:" + ids);
        hidden = ids === "" ? [] : ids.split(",");
        refresh();
        return {};
    }
    function addGroup(name, income, members) {
        calls.push("addGroup:" + name + ":" + income + ":" + members);
        return name.trim() === "" ? {code: "store.empty-name"} : {};
    }
    function editGroup(id, name, members) {
        calls.push("editGroup:" + id + ":" + name + ":" + members);
        return {};
    }
    function moveGroup(id, position) {
        calls.push("moveGroup:" + id + ":" + position);
        return {};
    }
    function removeGroup(id) {
        calls.push("removeGroup:" + id);
        return {};
    }
}
