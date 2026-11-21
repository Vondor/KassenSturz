// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Offers once to move a book out of a OneDrive folder in the app from the
// Microsoft Store (D-968, B-438, #1349): everything there is uploaded to
// the cloud, and the user never chose that. "Move my book" makes the local
// folder (%USERPROFILE%\KassenSturz) KassenSturz's own folder, copying the
// backups and plugins into it, and opens the move page, which moves the
// book there with a backup first (D-731); "Keep it there" stops asking for
// this book.
//
//     dialog.ask();

import QtQuick
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.PromptDialog {
    id: dialog

    required property BookMove bookMove
    // The user wants to move it: the window opens the move page.
    signal moveChosen()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function ask() {
        open();
    }

    title: i18nContext.i18nc("@title:window", "Keep your book on this computer")
    subtitle: i18nContext.i18nc("@info %1 is a folder", "Your book and its backups are in a OneDrive folder, so they are uploaded to the cloud. KassenSturz can move your book to %1, which stays on this computer, and keeps its new backups there. Before moving, it keeps a copy of your book as it is now with the other backups. Older backups stay in OneDrive until you delete them there. This folder is not uploaded anywhere, so from time to time make a backup copy of your book, for example to a USB stick.", Plain.plain(dialog.bookMove.place?.localFolder ?? ""))
    standardButtons: Kirigami.Dialog.NoButton
    closePolicy: Kirigami.Dialog.NoAutoClose
    showCloseButton: false
    customFooterActions: [
        Kirigami.Action {
            text: dialog.i18nContext.i18nc("@action:button", "Move my book")
            icon.name: "folder-documents"
            onTriggered: {
                ActionLog.taken("BookInSyncFolderDialog_move")
                dialog.close()
                if (dialog.bookMove.prepareLocalFolder())
                    dialog.moveChosen()
            }
        },
        Kirigami.Action {
            text: dialog.i18nContext.i18nc("@action:button keep the book in its folder", "Keep it there")
            icon.name: "dialog-cancel"
            onTriggered: {
                ActionLog.taken("BookInSyncFolderDialog_keep")
                dialog.bookMove.keepInSyncFolder()
                dialog.close()
            }
        }
    ]
}
