// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object SavedViews, with the same calls, for
// `tools/check-panel.sh` (M6a.7). It keeps the views in a list.

import QtQuick

QtObject {
    id: saved

    property var view: ({views: []})
    property var calls: []
    property int counter: 0
    signal changed()

    function refresh() { view = ({views: store.slice()}); }
    property var store: []
    function find(id) { return store.findIndex(v => v.id === id); }

    function save(name, period, component) {
        calls.push("save:" + name + ":" + period);
        if (name.trim() === "") { return {code: "store.view-invalid", reason: "bad-name"}; }
        if (store.some(v => v.name.toLowerCase() === name.trim().toLowerCase())) {
            return {code: "store.view-name-taken", name: name.trim()};
        }
        const id = "v" + (++counter);
        const parsed = JSON.parse(component);
        store.push({id: id, name: name.trim(), usable: true, kind: parsed.type, dataset: parsed.dataset, component: component, period: period});
        refresh();
        changed();
        return {id: id};
    }

    function open(id) {
        const v = store[find(id)];
        return v ? {id: v.id, name: v.name, period: v.period, component: v.component} : {code: "input.no-such-entry"};
    }

    function update(id, period, component) {
        calls.push("update:" + id + ":" + period);
        const i = find(id);
        if (i < 0) { return {code: "input.no-such-entry"}; }
        store[i].component = component;
        store[i].period = period;
        refresh();
        changed();
        return {};
    }

    function rename(id, name) {
        calls.push("rename:" + id + ":" + name);
        const i = find(id);
        if (i < 0) { return {code: "input.no-such-entry"}; }
        store[i].name = name.trim();
        refresh();
        changed();
        return {};
    }

    function remove(id) {
        calls.push("remove:" + id);
        const i = find(id);
        if (i < 0) { return {code: "input.no-such-entry"}; }
        store.splice(i, 1);
        refresh();
        changed();
        return {};
    }
}
