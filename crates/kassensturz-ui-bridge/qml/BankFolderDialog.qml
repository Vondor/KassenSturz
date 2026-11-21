// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Offers once to move the bank connection's settings out of
// Documents\KassenSturz in the app from the Microsoft Store (D-967, B-437,
// #1348). Documents is often uploaded to a cloud, and the bank connection's
// settings never belong in a synced folder (T-07). New bank connections
// are kept in the app's private folder already; one an earlier version put
// in Documents moves only when the user agrees. The move runs at the next
// start, before anything uses the folder.
//
//     dialog.ask();

import QtQuick
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.PromptDialog {
    id: dialog

    required property BookMove bookMove

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function ask() {
        open();
    }

    title: i18nContext.i18nc("@title:window", "Keep your bank connection private")
    subtitle: i18nContext.i18nc("@info", "The settings of your bank connection (which bank, your accounts and your login name, never your PIN) are in your Documents folder, which is often uploaded to a cloud such as OneDrive. KassenSturz can move them to a private folder on this computer the next time it starts. That folder is removed when you uninstall KassenSturz; you would then set up the bank connection again. Your book is not affected. The cloud service may keep the old copy in its recycle bin and version history for a while; delete it there if you want it gone.")
    standardButtons: Kirigami.Dialog.NoButton
    closePolicy: Kirigami.Dialog.NoAutoClose
    showCloseButton: false
    customFooterActions: [
        Kirigami.Action {
            text: dialog.i18nContext.i18nc("@action:button", "Move at the next start")
            icon.name: "folder-locked"
            onTriggered: {
                ActionLog.taken("BankFolderDialog_move")
                dialog.bookMove.answerBankMove(true)
                dialog.close()
            }
        },
        Kirigami.Action {
            text: dialog.i18nContext.i18nc("@action:button keep the bank settings in their folder", "Keep it there")
            icon.name: "dialog-cancel"
            onTriggered: {
                ActionLog.taken("BankFolderDialog_keep")
                dialog.bookMove.answerBankMove(false)
                dialog.close()
            }
        }
    ]
}
