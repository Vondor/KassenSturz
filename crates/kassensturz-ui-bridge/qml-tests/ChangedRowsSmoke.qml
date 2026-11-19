// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: changed rows light up and undo names the booking

// Checks the two halves of B-384 (D-857) without the Rust objects: the rows
// of a change light up for a moment (ChangedRows.qml) and go dark again by
// themselves, and the one line for a change (ChangeNames.qml `describe`,
// which "Undone: …" and "Made again: …" use) names the one transaction it
// touched, with its amount as Rust wrote it, and stays as before for a
// change of several transactions or of a file.
// `tools/check-panel.sh` runs it and fails on the first FAIL line or on any
// QML warning about a binding or a type.

import QtQuick
import io.github.vondor.kassensturz

Item {
    id: test

    property bool failed: false

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    ChangeNames {
        id: names
    }

    // A row as the lists draw it: lit while its transaction is.
    Item {
        id: row

        readonly property string transactionId: "t1"
        readonly property bool changed: ChangedRows.lit(transactionId)
    }

    Component.onCompleted: {
        const one = {
            action: "add-transaction",
            summary: { added: 1, transactions: ["t1"], booking: { name: "Corner Grocery", amount: "-23,80 €" } }
        };
        test.check(names.describe(one) === "Added a transaction (Corner Grocery, -23,80 €)", "one booking is named: " + names.describe(one));
        // The screen a change belongs to, for Undo's question (B-395, D-870).
        for (const [action, screen] of [["add-transaction", "accounts"], ["edit-budget", "budgets"], ["edit-view", "statistics"],
                                         ["move-category", "categories"], ["run-rules", "rules"], ["merge-tags", "settings"],
                                         ["apply-ai-changes", ""], ["a-newer-change", "accounts"]]) {
            test.check(names.place(action) === screen, action + " belongs to '" + screen + "', got '" + names.place(action) + "'");
        }
        const noName = { action: "remove-transaction", summary: { removed: 1, transactions: ["t1"], booking: { name: "", amount: "-5,00 €" } } };
        test.check(names.describe(noName) === "Removed a transaction (-5,00 €)", "without a name, the amount: " + names.describe(noName));
        const several = { action: "import-statement", fileName: "statement.sta", summary: { added: 2, transactions: ["t1", "t2"], booking: {} } };
        test.check(names.describe(several) === "Imported a bank statement (statement.sta)", "an import keeps its file: " + names.describe(several));
        const plain = { action: "add-account", summary: {} };
        test.check(names.describe(plain) === "Added an account", "a change without transactions is as before: " + names.describe(plain));

        test.check(!row.changed, "no row is lit at first");
        ChangedRows.light(["t1"]);
        test.check(row.changed, "the changed row is lit");
        ChangedRows.light(undefined);
        test.check(!row.changed, "nothing to light turns it off");
        ChangedRows.light(["t1"]);

        // "New" until the app closes (B-394, D-869): only what is marked.
        test.check(!ChangedRows.isNew("t1"), "nothing is new at first");
        ChangedRows.markNew("t1");
        ChangedRows.markNew("t1");
        ChangedRows.markNew("");
        test.check(ChangedRows.isNew("t1") && !ChangedRows.isNew("t2"), "a marked entry is new, another isn't");
        test.check(ChangedRows.added.length === 1, "marked once, got " + ChangedRows.added.length);
    }

    Timer {
        interval: ChangedRows.duration + 500
        running: true
        onTriggered: {
            test.check(!row.changed, "the row goes dark by itself");
            Qt.exit(test.failed ? 1 : 0);
        }
    }
}
