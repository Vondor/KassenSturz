// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Says at the start, in plain words, what happened to a move of the book
// that was cut short, e.g. by a crash or a power cut (D-731). Before the
// app opened anything, BookMove made sure the right book opens: the old one
// if the new one wasn't remembered yet. A book file is removed only on the
// user's yes: the unused copy, or the old file once the move had finished.
//
//     dialog.ask(bookMove.interrupted);

import QtQuick
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.PromptDialog {
    id: dialog

    required property BookMove bookMove
    // What `BookMove.interrupted` was.
    property var found: ({})

    // Removing failed; `reason` says why and what to do.
    signal settleFailed(string reason)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function ask(interrupted: var) {
        found = interrupted;
        open();
    }

    function folderText(shown: var): string {
        if (!shown)
            return "";
        return shown.own
            ? i18nContext.i18nc("@info the folder KassenSturz keeps its own book in", "KassenSturz's own folder")
            : (shown.text ?? "");
    }

    function settle(remove: bool) {
        const answer = bookMove.settle(remove);
        close();
        if (!answer.ok) {
            switch (answer.code) {
            case "book-move.remove-locked":
                settleFailed(i18nContext.i18nc("@info", "The file wasn't removed: it is open in KassenSturz on another computer. Close it there and remove it yourself."));
                break;
            default:
                settleFailed(i18nContext.i18nc("@info", "The file couldn't be removed. You may remove it yourself; your book is open and safe."));
            }
        }
    }

    function message(): string {
        if (found.after) {
            const done = i18nContext.i18nc("@info %1 is a folder", "KassenSturz stopped right after it moved your book to %1. Your book is open there, and nothing in it changed.", Plain.plain(folderText(found.newFolder)));
            if (!found.offer)
                return done;
            return done + " " + i18nContext.i18nc("@info %1 is a folder, %2 a backup's file name", "The old file is still in %1. Before moving, KassenSturz kept a copy of your book in the backup folder (%2), so nothing is lost if you remove it. Remove the old file?", Plain.plain(folderText(found.offerFolder)), Plain.plain(found.backupName ?? ""));
        }
        const kept = i18nContext.i18nc("@info", "KassenSturz stopped while it was moving your book. Your book is still where it was, and nothing in it changed.");
        if (!found.offer)
            return kept;
        return kept + " " + i18nContext.i18nc("@info %1 is a folder", "An unused copy of it is left in %1. Remove that copy?", Plain.plain(folderText(found.offerFolder)));
    }

    title: Plain.plain(found.after
        ? i18nContext.i18nc("@title:window", "Your book was moved")
        : i18nContext.i18nc("@title:window", "Your book wasn't moved"))
    subtitle: message()
    standardButtons: Kirigami.Dialog.NoButton
    closePolicy: Kirigami.Dialog.NoAutoClose
    showCloseButton: false
    customFooterActions: [
        Kirigami.Action {
            visible: dialog.found.offer ?? false
            text: dialog.found.after
                ? dialog.i18nContext.i18nc("@action:button", "Remove the old file")
                : dialog.i18nContext.i18nc("@action:button", "Remove the copy")
            icon.name: "edit-delete"
            onTriggered: {
                ActionLog.taken("BookMoveInterruptedDialog_remove")
                dialog.settle(true)
            }
        },
        Kirigami.Action {
            text: (dialog.found.offer ?? false)
                ? dialog.i18nContext.i18nc("@action:button keep a file", "Keep it")
                : dialog.i18nContext.i18nc("@action:button", "OK")
            icon.name: "dialog-ok"
            onTriggered: {
                ActionLog.taken("BookMoveInterruptedDialog_keep")
                dialog.settle(false)
            }
        }
    ]
}
