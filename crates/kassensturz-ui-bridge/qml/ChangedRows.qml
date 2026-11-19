// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

pragma Singleton

import QtQuick

// Which transactions a change just touched, so their rows light up for a
// moment after the change, its Undo or its Redo (B-384, D-857): someone who
// added a purchase, or took one back, sees which line it was instead of only
// a total that moved. Main.qml sets the IDs from the history's summary
// (`summary.transactions`); the lists of transactions (RegisterPage.qml,
// the transaction list of a dashboard in GridItem.qml) ask `lit(id)`.
// It also keeps which ones the user added since the start, for their "New"
// badge (`isNew(id)`).
Item {
    id: rows

    visible: false

    // The IDs of the rows lit now; empty most of the time.
    property var ids: []

    // How long the rows stay lit, in milliseconds.
    readonly property int duration: 3000

    // Lights the rows of `transactions` (a list of IDs, or nothing).
    function light(transactions: var) {
        rows.ids = transactions ? Array.from(transactions).map(id => String(id)) : [];
        if (rows.ids.length > 0) {
            fade.restart();
        }
    }

    function lit(id: string): bool {
        return rows.ids.indexOf(id) >= 0;
    }

    // The transactions the user typed in since the app started, which carry
    // a "New" badge until it closes (B-394, D-869). Kept here only, never in
    // the book; an import marks none, its summary says what it added.
    property var added: []

    function markNew(id: string) {
        if (id !== "" && rows.added.indexOf(id) < 0) {
            rows.added = rows.added.concat([id]);
        }
    }

    function isNew(id: string): bool {
        return rows.added.indexOf(id) >= 0;
    }

    Timer {
        id: fade

        interval: rows.duration
        onTriggered: rows.ids = []
    }
}
