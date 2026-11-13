// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Restore from backup (#806): the backups of the open book, newest first,
// from KassenSturz's own backup folder and from the folder the user chose
// for copies. Restoring asks first, saves the book as it is now as a new
// backup, then puts the chosen copy in its place and opens it. Paths are
// never shown; each backup says when it was made and where it is kept.

import QtQuick
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

FormCard.FormCardPage {
    id: page

    required property BookSession session

    // What copies.backups() returned, read again when the page is shown.
    property var found: ({ items: [], problem: "" })
    // The backup the user picked, while the dialog asks.
    property var picked: ({})
    // Why the last restore didn't happen, as a code, or "".
    property string failedCode: ""

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function reload() {
        copies.refresh();
        found = copies.backups();
    }

    function failureText(code: string): string {
        switch (code) {
        case "restore.not-usable":
            return i18nContext.i18nc("@info", "This backup can't be used: it is damaged, or a newer KassenSturz made it. Your book wasn't changed. Choose another backup.");
        case "restore.no-backup-first":
            return i18nContext.i18nc("@info", "KassenSturz couldn't save your book as it is now, so it didn't restore anything. Check that there is free space on this computer, then try again.");
        case "restore.not-listed":
            return i18nContext.i18nc("@info", "This backup isn't there any more. Your book wasn't changed. Choose another one from the list.");
        default:
            return i18nContext.i18nc("@info", "KassenSturz couldn't put the backup in place, so your book is as it was. Check that the folder of your book can be written to, then try again.");
        }
    }

    function folderProblemText(code: string): string {
        switch (code) {
        case "":
            return "";
        case "backup-copy.missing":
            return i18nContext.i18nc("@info", "The folder you chose for copies isn't there right now, so its copies aren't listed. If it is on a USB stick or another drive, plug it in and open this page again.");
        default:
            return i18nContext.i18nc("@info", "KassenSturz can't read the folder you chose for copies, so its copies aren't listed.");
        }
    }

    title: i18nContext.i18nc("@title", "Restore from backup")

    Component.onCompleted: reload()

    BackupCopies {
        id: copies
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "Pick the copy of your book to go back to. Changes you made after it was saved won't be in it. Before KassenSturz restores it, it saves your book as it is now as another backup, so you can always come back.")
    }

    FormCard.FormSectionText {
        visible: page.failedCode !== ""
        textFormat: Text.PlainText
        text: page.failedCode === "" ? "" : page.failureText(page.failedCode)
        Accessible.role: Accessible.AlertMessage
    }

    FormCard.FormSectionText {
        visible: page.found.problem !== ""
        textFormat: Text.PlainText
        text: page.folderProblemText(page.found.problem)
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Backups of this book")
    }

    FormCard.FormCard {
        FormCard.FormTextDelegate {
            visible: page.found.items.length === 0
            text: page.i18nContext.i18nc("@info", "There are no backups of this book yet.")
            descriptionItem.textFormat: Text.PlainText
            description: copies.chosen
                ? page.i18nContext.i18nc("@info", "KassenSturz saves one before it updates your book and, in the folder you chose, once a day when you open the book.")
                : page.i18nContext.i18nc("@info", "KassenSturz saves one before it updates your book. In Settings you can choose a folder where it also keeps a copy once a day, for example on a USB stick.")
        }

        Repeater {
            model: page.found.items

            delegate: FormCard.FormButtonDelegate {
                required property var modelData

                icon.name: "document-revert"
                text: Plain.plain(modelData.when)
                description: modelData.inChosen
                    ? page.i18nContext.i18nc("@info where a backup is kept", "In the folder you chose for copies")
                    : page.i18nContext.i18nc("@info where a backup is kept", "In KassenSturz's backup folder")
                descriptionItem.textFormat: Text.PlainText
                Accessible.name: text + ", " + description
                onClicked: {
                    ActionLog.taken("RestoreBackupPage_pick")
                    page.picked = modelData;
                    confirmDialog.open();
                }
            }
        }
    }

    Kirigami.PromptDialog {
        id: confirmDialog

        title: page.i18nContext.i18nc("@title:window", "Restore this backup?")
        subtitle: page.i18nContext.i18nc("@info %1 is when the backup was saved, e.g. 21.10.2026 14:02", "Your book goes back to how it was on %1. Changes made after that won't be in it.\n\nKassenSturz first saves your book as it is now as a backup, so you can come back to it from this list.", page.picked.when ?? "")
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Restore it")
                icon.name: "document-revert"
                onTriggered: {
                    ActionLog.taken("RestoreBackupPage_restore")
                    confirmDialog.close();
                    // The page leaves when the book opens again.
                    const window = applicationWindow();
                    const result = page.session.restoreBackup(page.picked.path);
                    if (result.ok) {
                        window.showPassiveNotification(page.i18nContext.i18nc("@info", "Your book was restored. The book as it was before is the newest backup."), "long");
                    } else {
                        page.failedCode = result.code;
                        page.reload();
                    }
                }
            },
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Keep my book as it is")
                icon.name: "dialog-cancel"
                onTriggered: {
                    ActionLog.taken("RestoreBackupPage_keep")
                    confirmDialog.close()
                }
            }
        ]
    }
}
