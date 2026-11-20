// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Offers, once a start, to move a book out of the folder that Windows
// deletes together with the app from the Microsoft Store (D-856, #1178) or
// the snap's own folder, which removing the snap deletes (B-429).
// Until D-856 the Store app kept its book and the backups there, so
// uninstalling or resetting the app took them with it. "Move my book" opens
// the move page, which moves it into KassenSturz's own folder in Documents
// with a backup first (D-731); "Not now" asks again at the next start;
// "Keep it there" stops asking for this book.
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

    title: i18nContext.i18nc("@title:window", "Keep your book safe")
    subtitle: (dialog.bookMove.place?.inSnap ?? false)
        ? i18nContext.i18nc("@info %1 is a folder", "Your book is in a folder that is deleted, together with all its backups, when the KassenSturz snap is removed. KassenSturz can move it to %1, which stays. Before moving, it keeps a copy of your book as it is now with the other backups.", Plain.plain(dialog.bookMove.place?.ownFolder ?? ""))
        : i18nContext.i18nc("@info %1 is a folder", "Your book is in a folder that Windows deletes, together with all its backups, when KassenSturz is uninstalled or reset. KassenSturz can move it to %1, which Windows leaves alone. Before moving, it keeps a copy of your book as it is now with the other backups.", Plain.plain(dialog.bookMove.place?.ownFolder ?? ""))
    standardButtons: Kirigami.Dialog.NoButton
    closePolicy: Kirigami.Dialog.NoAutoClose
    showCloseButton: false
    customFooterActions: [
        Kirigami.Action {
            text: dialog.i18nContext.i18nc("@action:button", "Move my book")
            icon.name: "folder-documents"
            onTriggered: {
                ActionLog.taken("BookInPackageDialog_move")
                dialog.close()
                dialog.moveChosen()
            }
        },
        Kirigami.Action {
            text: dialog.i18nContext.i18nc("@action:button ask again at the next start", "Not now")
            icon.name: "chronometer"
            onTriggered: {
                ActionLog.taken("BookInPackageDialog_later")
                dialog.close()
            }
        },
        Kirigami.Action {
            text: dialog.i18nContext.i18nc("@action:button keep the book in its folder", "Keep it there")
            icon.name: "dialog-cancel"
            onTriggered: {
                ActionLog.taken("BookInPackageDialog_keep")
                dialog.bookMove.keepInPackage()
                dialog.close()
            }
        }
    ]
}
