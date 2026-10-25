// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// What the changes in the history are called (M3.8, D-034). The Rust side
// names every change with a key (history::action_key); the words are here,
// where KI18n's tools extract them. Each name says what was done, in plain
// language, so the same words serve the history, "Undo: …", "Redo: …" and
// the notice after a change (AGENTS.md §6).

import QtQuick
import org.kde.ki18n

QtObject {
    id: names

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // What was done, by the key of its action.
    function name(action: string): string {
        switch (action) {
        case "set-rounding-mode":
            return i18nContext.i18nc("@info a change you made", "Changed how amounts are rounded");
        case "use-currency":
            return i18nContext.i18nc("@info a change you made", "Started using a currency");
        case "add-account":
            return i18nContext.i18nc("@info a change you made", "Added an account");
        case "rename-account":
            return i18nContext.i18nc("@info a change you made", "Renamed an account");
        case "close-account":
            return i18nContext.i18nc("@info a change you made", "Closed an account");
        case "reopen-account":
            return i18nContext.i18nc("@info a change you made", "Opened an account again");
        case "add-category":
            return i18nContext.i18nc("@info a change you made", "Added a category");
        case "rename-category":
            return i18nContext.i18nc("@info a change you made", "Renamed a category");
        case "move-category":
            return i18nContext.i18nc("@info a change you made", "Moved a category");
        case "add-rate":
            return i18nContext.i18nc("@info a change you made", "Added an exchange rate");
        case "remove-rate":
            return i18nContext.i18nc("@info a change you made", "Removed an exchange rate");
        case "add-transaction":
            return i18nContext.i18nc("@info a change you made", "Added a transaction");
        case "edit-transaction":
            return i18nContext.i18nc("@info a change you made", "Changed a transaction");
        case "remove-transaction":
            return i18nContext.i18nc("@info a change you made", "Removed a transaction");
        case "join-transfer":
            return i18nContext.i18nc("@info a change you made", "Joined two transactions into one move of your own money");
        case "add-rule":
            return i18nContext.i18nc("@info a change you made", "Added a rule");
        case "edit-rule":
            return i18nContext.i18nc("@info a change you made", "Changed a rule");
        case "move-rule":
            return i18nContext.i18nc("@info a change you made", "Moved a rule");
        case "remove-rule":
            return i18nContext.i18nc("@info a change you made", "Removed a rule");
        case "run-rules":
            return i18nContext.i18nc("@info a change you made", "Sorted transactions with your rules");
        case "import-statement":
            return i18nContext.i18nc("@info a change you made", "Imported a bank statement");
        case "correct-starting-balance":
            return i18nContext.i18nc("@info a change you made", "Corrected a starting balance");
        case "add-institution":
            return i18nContext.i18nc("@info a change you made", "Added a bank");
        case "add-payee":
            return i18nContext.i18nc("@info a change you made", "Added someone you pay or get money from");
        case "add-tag":
            return i18nContext.i18nc("@info a change you made", "Added a tag");
        case "link-bank-account":
            return i18nContext.i18nc("@info a change you made", "Linked an account to your bank");
        case "unlink-bank-account":
            return i18nContext.i18nc("@info a change you made", "Removed an account's link to your bank");
        case "fetch-statement":
            return i18nContext.i18nc("@info a change you made", "Fetched new transactions from your bank");
        case "apply-ai-changes":
            // Without the word for AI, which shows only while AI is on
            // (D-013, D-198); the chat panel (M5.8) names the AI there.
            return i18nContext.i18nc("@info a change you made", "Applied suggested changes");
        default:
            // A newer version of KassenSturz knows more changes than this
            // one; the book still opens, and its history says so.
            return i18nContext.i18nc("@info a change you made that this version doesn't know", "Made a change");
        }
    }

    // A change as one line: what was done, and for an import the file it
    // read. `change` is an object of history_model.rs.
    function describe(change: var): string {
        const what = name(change.action);
        return change.fileName
            ? i18nContext.i18nc("@info %1 is what was done, %2 the name of a file", "%1 (%2)", what, change.fileName)
            : what;
    }

    // What the change touched, in a line: how many transactions, in which
    // accounts, on which days. Empty for a change without transactions.
    function summary(change: var): string {
        const summary = change.summary ?? ({});
        const parts = [];
        if (summary.added > 0) {
            parts.push(i18nContext.i18ncp("@info", "%1 transaction added", "%1 transactions added", summary.added));
        }
        if (summary.removed > 0) {
            parts.push(i18nContext.i18ncp("@info", "%1 transaction removed", "%1 transactions removed", summary.removed));
        }
        if (summary.changed > 0) {
            parts.push(i18nContext.i18ncp("@info", "%1 transaction changed", "%1 transactions changed", summary.changed));
        }
        if (parts.length === 0) {
            return "";
        }
        if (summary.accounts.length > 0) {
            parts.push(Array.from(summary.accounts).join(", "));
        }
        if (summary.firstDate) {
            parts.push(summary.firstDate === summary.lastDate
                ? summary.firstDate
                : i18nContext.i18nc("@info %1 and %2 are dates", "%1 to %2", summary.firstDate, summary.lastDate));
        }
        return parts.join(" · ");
    }
}
