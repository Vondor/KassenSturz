// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust BookTags, for `tools/check-panel.sh` (US4.8):
// two tags, one used on the whole of transactions and on parts. It records
// what the page asked for.

import QtQuick

QtObject {
    property var tags: [
        { id: "garden", name: "Garden party", transactions: 2, parts: 1 },
        { id: "tax", name: "Tax", transactions: 0, parts: 0 },
    ]
    // Every change the page asked for, as text.
    property var asked: []
    signal changed()

    function refresh() {}
    function rename(id, name) {
        asked = asked.concat(["rename " + id + " " + name]);
        if (name.trim() === "") {
            return ({ code: "store.empty-name" });
        }
        tags = tags.map(tag => tag.id === id ? ({ id: tag.id, name: name, transactions: tag.transactions, parts: tag.parts }) : tag);
        return ({});
    }
    function remove(id) {
        asked = asked.concat(["remove " + id]);
        tags = tags.filter(tag => tag.id !== id);
        return ({});
    }
    function merge(from, into) {
        asked = asked.concat(["merge " + from + " " + into]);
        tags = tags.filter(tag => tag.id !== from);
        return ({});
    }
}
