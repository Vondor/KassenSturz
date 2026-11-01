// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Asks before removing a transaction that holds what a bank or file
// delivered (M6c.B, D-375). KassenSturz keeps that original unchanged; the
// user's own changes sit on top of it. Removing the transaction removes the
// original too, so this says so in plain words, and for how long Undo can
// still bring it back (D-066), and makes the removal a deliberate click
// (AGENTS.md §1). A transaction the user typed in is removed without asking,
// as before.
//
//     dialog.ask(register.removalNotice(accountId, transactionId));
//
// `removeConfirmed()` follows the click on the remove button only.

import QtQuick
import org.kde.kirigami as Kirigami
import org.kde.ki18n

Kirigami.PromptDialog {
    id: dialog

    // What `AccountRegister.removalNotice()` returned.
    property var notice: ({})

    // The user chose to remove the transaction with its original.
    signal removeConfirmed()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function ask(found: var) {
        notice = found;
        open();
    }

    // How long Undo can bring the transaction back.
    function undoText(): string {
        switch (notice.undo) {
        case "forever":
            return i18nContext.i18nc("@info", "Undo can bring it back later, unless you clean up the history in Settings first.");
        case "until-closed":
            return i18nContext.i18nc("@info", "Undo can bring it back only until you close this book, because your settings keep no history after that.");
        }
        return i18nContext.i18ncp("@info %1 is a number of days", "Undo can bring it back within the next day, unless you clean up the history in Settings first.", "Undo can bring it back within the next %1 days, unless you clean up the history in Settings first.", notice.undoDays ?? 0);
    }

    title: i18nContext.i18nc("@title:window", "Remove what your bank sent?")
    subtitle: i18nContext.i18nc("@info", "This transaction came from your bank or a file. KassenSturz keeps what was delivered for it unchanged, as the original, and your own changes sit on top of it. Removing the transaction removes the original too.") + " " + undoText()
    standardButtons: Kirigami.Dialog.NoButton
    customFooterActions: [
        Kirigami.Action {
            text: dialog.i18nContext.i18nc("@action:button", "Remove it with the original")
            icon.name: "edit-delete"
            onTriggered: {
                ActionLog.taken("RemoveOriginalDialog_removeOriginal")
                dialog.close();
                dialog.removeConfirmed();
            }
        },
        Kirigami.Action {
            text: dialog.i18nContext.i18nc("@action:button keep a transaction", "Keep it")
            icon.name: "go-previous"
            onTriggered: {
                ActionLog.taken("RemoveOriginalDialog_keep")
                dialog.close()
            }
        }
    ]
}
