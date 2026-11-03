// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object ChartPaletteSettings, for
// `tools/check-panel.sh` (M7a.10): the eight colors as text, what was asked
// of it (`calls`), and `warningList`, which `warnings()` hands back.

import QtQuick

QtObject {
    id: fake

    readonly property var standard: ["#0072B2", "#D55E00", "#009E73", "#CC79A7", "#E69F00", "#56B4E9", "#F0E442", "#999999",
        "#882255", "#117733", "#44AA99", "#AA4499", "#DDCC77", "#332288", "#CC6677", "#661100"]
    readonly property int minColors: 2
    readonly property int maxColors: 16
    property var colors: standard.slice(0, 8)
    property bool changed: false
    property var calls: []
    property var warningList: []

    function show(next) {
        colors = next;
        changed = next.length !== 8 || next.some((color, at) => color !== standard[at]);
    }

    function setColor(slot, entry) {
        calls.push("setColor:" + slot + ":" + entry);
        const text = entry.trim();
        const next = colors.slice();
        if (/^#[0-9a-fA-F]{6}$/.test(text)) {
            next[slot] = text.toUpperCase();
        } else {
            const parts = text.split(/[,; ]+/);
            if (parts.length !== 3 || !parts.every(part => /^[0-9]+$/.test(part))) {
                return "format";
            }
            if (parts.some(part => parseInt(part) > 255)) {
                return "part";
            }
            next[slot] = "#" + parts.map(part => ("0" + parseInt(part).toString(16)).slice(-2)).join("").toUpperCase();
        }
        show(next);
        return "ok";
    }

    function reset(slot) {
        calls.push("reset:" + slot);
        const next = colors.slice();
        next[slot] = standard[slot];
        show(next);
        return "ok";
    }

    function resetAll() {
        calls.push("resetAll");
        show(standard.slice(0, 8));
        return "ok";
    }

    function addColor() {
        calls.push("addColor");
        if (colors.length >= maxColors) {
            return "limit";
        }
        show(colors.concat([standard[colors.length]]));
        return "ok";
    }

    function removeColor(slot) {
        calls.push("removeColor:" + slot);
        if (colors.length <= minColors) {
            return "limit";
        }
        const next = colors.slice();
        next.splice(slot, 1);
        show(next);
        return "ok";
    }

    function warnings(background) {
        calls.push("warnings:" + background);
        return warningList;
    }
}
