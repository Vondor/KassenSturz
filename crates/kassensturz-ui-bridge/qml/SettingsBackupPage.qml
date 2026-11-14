// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The settings group "Backups and undo" (D-795): where the backups go,
// copies in a folder the user chose and restoring one (#806), and how long
// the history of changes is kept (D-066, M3.8).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

FormCard.FormCardPage {
    id: page

    required property BookSession session
    required property ChangeHistory history

    // Why the folder for backup copies wasn't taken, or "" (#806).
    property string copiesChoiceProblem: ""

    signal showHistoryClicked()
    // The user wants the list of backups to restore one (#806).
    signal restoreBackupClicked()

    // The history setting the user chose and the dialog asks about, with how
    // many changes it would delete, and why the last change of the history
    // settings failed.
    property string pendingRetention: ""
    property int pendingCount: 0
    property var historyResult: ({})

    // How long the history can be kept, as the keys history_model.rs knows:
    // a number of days, 0 for only while the book is open, or "forever". A
    // number the screen doesn't offer, e.g. one set with the command-line
    // tool, is added.
    readonly property var keepChoices: {
        const choices = [
            { value: "0", text: i18nContext.i18nc("@item:inlistbox keep the history", "Only while the book is open") },
            { value: "7", text: i18nContext.i18nc("@item:inlistbox keep the history", "1 week") },
            { value: "30", text: i18nContext.i18nc("@item:inlistbox keep the history", "30 days") },
            { value: "90", text: i18nContext.i18nc("@item:inlistbox keep the history", "3 months") },
            { value: "365", text: i18nContext.i18nc("@item:inlistbox keep the history", "1 year") },
            { value: "forever", text: i18nContext.i18nc("@item:inlistbox keep the history", "As long as the book exists") },
        ];
        if (choices.some(choice => choice.value === history.retention)) {
            return choices;
        }
        return choices.concat([{
            value: history.retention,
            text: i18nContext.i18ncp("@item:inlistbox keep the history for this many days", "%1 day", "%1 days", parseInt(history.retention)),
        }]);
    }

    // Keeps the history for `key`, or asks first if that deletes changes.
    function chooseRetention(key: string) {
        const count = key === "0" ? history.total : history.expiring(key);
        if (count > 0) {
            pendingRetention = key;
            pendingCount = count;
            shorterDialog.open();
        } else {
            saveRetention(key);
        }
    }

    function saveRetention(key: string) {
        historyResult = history.setRetention(key);
        keepFor.showChoice();
    }

    header: ColumnLayout {
        spacing: 0

        // Why the last change to the history settings failed, if it did.
        FormError {
            Layout.fillWidth: true
            result: page.historyResult
        }
    }


    // What the folder for copies says, from `backupCopies.status` (#806).
    function copiesStatusText(status: var): string {
        switch (status.state) {
        case "kept":
            return i18nContext.i18nc("@info %1 is when the copy was saved, e.g. 21.10.2026 14:02", "Newest copy there: %1", status.when);
        case "problem":
            return copiesChoiceText(status.code, i18nContext.i18nc("@info", "Your book and the backups on this computer are fine."));
        default:
            return i18nContext.i18nc("@info", "KassenSturz saves a copy there when it opens your book.");
        }
    }

    // The words for a code of the folder for copies; `fine` is added where
    // the book itself is not affected.
    function copiesChoiceText(code: string, fine: string): string {
        const sentences = (first, next) => fine === "" ? first + " " + next : first + " " + fine + " " + next;
        switch (code) {
        case "backup-copy.missing":
            return sentences(i18nContext.i18nc("@info", "The folder isn't there right now, so no copy was saved there."), i18nContext.i18nc("@info", "If it is on a USB stick or another drive, plug it in and choose Try again, or choose another folder."));
        case "backup-copy.full":
            return sentences(i18nContext.i18nc("@info", "The disk with that folder is full, so no copy was saved there."), i18nContext.i18nc("@info", "Make room there and choose Try again, or choose another folder."));
        case "backup-copy.read-only":
            return sentences(i18nContext.i18nc("@info", "KassenSturz may not write to that folder, so no copy was saved there."), i18nContext.i18nc("@info", "Please choose another folder."));
        case "backup-copy.same-folder":
            return i18nContext.i18nc("@info", "That is KassenSturz's own backup folder. Please choose a folder on another drive, for example a USB stick.");
        case "backup-copy.not-saved":
            return i18nContext.i18nc("@info", "KassenSturz couldn't save this choice. Check that your settings folder can be written to, then choose again.");
        default:
            return sentences(i18nContext.i18nc("@info", "KassenSturz couldn't save a copy there."), i18nContext.i18nc("@info", "Choose Try again, or choose another folder."));
        }
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Backups")
    }

    // Copies in a folder the user chose, e.g. on a USB stick, and restoring
    // a backup (#806). The folder belongs to this computer, not to the book.
    BackupCopies {
        id: backupCopies
    }

    FolderChooser {
        id: copiesChooser

        title: page.i18nContext.i18nc("@title:window", "Choose a folder for copies of your book")
        onGranted: grant => {
            const result = backupCopies.choose(grant.path);
            page.copiesChoiceProblem = result.ok ? "" : page.copiesChoiceText(result.code, "");
        }
        onRefused: (code, reason) => {
            page.copiesChoiceProblem = reason;
        }
    }

    FormCard.FormCard {
        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@label", "Backup folder")
            description: page.session.backupFolder
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {
            visible: copiesButton.visible
        }

        FormCard.FormButtonDelegate {
            id: copiesButton

            visible: copiesChooser.available || backupCopies.chosen
            icon.name: "folder-open"
            text: Plain.plain(backupCopies.chosen
                ? page.i18nContext.i18nc("@action:button %1 is the folder", "Also keep copies in: %1", backupCopies.shownAs)
                : page.i18nContext.i18nc("@action:button", "Also keep copies in another folder…"))
            description: backupCopies.chosen
                ? page.i18nContext.i18nc("@info", "Choose another folder")
                : page.i18nContext.i18nc("@info", "For example on a USB stick, or in a folder your cloud app uploads")
            // The folder's path holds the user's own names.
            descriptionItem.textFormat: Text.PlainText
            onClicked: {
                ActionLog.taken("SettingsBackupPage_chooseCopiesFolder")
                page.copiesChoiceProblem = "";
                copiesChooser.open();
            }
        }

        FormCard.FormDelegateSeparator {
            visible: backupCopies.chosen
        }

        FormCard.FormComboBoxDelegate {
            id: copiesKeepChoice

            // Shows the saved choice, also after a choice couldn't be saved.
            function showChoice() {
                currentIndex = indexOfValue(backupCopies.keep);
            }

            visible: backupCopies.chosen
            text: page.i18nContext.i18nc("@label:listbox", "Copies of each book to keep there")
            textRole: "text"
            valueRole: "value"
            model: [
                { value: "3", text: page.i18nContext.i18nc("@item:inlistbox how many copies", "3") },
                { value: "5", text: page.i18nContext.i18nc("@item:inlistbox how many copies", "5") },
                { value: "10", text: page.i18nContext.i18nc("@item:inlistbox how many copies", "10") },
                { value: "20", text: page.i18nContext.i18nc("@item:inlistbox how many copies", "20") },
            ]
            Component.onCompleted: showChoice()
            onActivated: {
                ActionLog.taken("SettingsBackupPage_copiesToKeep")
                page.copiesChoiceProblem = backupCopies.chooseKeep(currentValue)
                    ? ""
                    : page.copiesChoiceText("backup-copy.not-saved", "");
                showChoice();
            }
        }

        FormCard.FormDelegateSeparator {
            visible: backupCopies.chosen
        }

        FormCard.FormTextDelegate {
            visible: backupCopies.chosen
            text: page.copiesStatusText(backupCopies.status)
            textItem.textFormat: Text.PlainText
            textItem.wrapMode: Text.Wrap
            Accessible.role: backupCopies.status.state === "problem" ? Accessible.AlertMessage : Accessible.StaticText
        }

        // A row of its own: as a trailing button of the line above it didn't
        // show in the VM, while the text tells the user to choose it
        // (M6-B241).
        FormCard.FormDelegateSeparator {
            visible: copiesTryAgain.visible
        }

        FormCard.FormButtonDelegate {
            id: copiesTryAgain

            visible: backupCopies.chosen && backupCopies.status.state === "problem"
            icon.name: "view-refresh"
            text: page.i18nContext.i18nc("@action:button make a copy again", "Try again")
            onClicked: {
                ActionLog.taken("SettingsBackupPage_copyAgain")
                backupCopies.tryAgain()
            }
        }

        FormCard.FormDelegateSeparator {
            visible: backupCopies.chosen
        }

        FormCard.FormButtonDelegate {
            visible: backupCopies.chosen
            icon.name: "edit-delete-remove"
            text: page.i18nContext.i18nc("@action:button", "Stop keeping copies there")
            description: page.i18nContext.i18nc("@info", "The copies already there stay")
            onClicked: {
                ActionLog.taken("SettingsBackupPage_stopCopies")
                page.copiesChoiceProblem = backupCopies.forget()
                    ? ""
                    : page.copiesChoiceText("backup-copy.not-saved", "");
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "document-revert"
            text: page.i18nContext.i18nc("@action:button", "Restore from backup…")
            description: page.i18nContext.i18nc("@info", "Go back to an earlier copy of your book")
            onClicked: {
                ActionLog.taken("SettingsBackupPage_restoreFromBackup")
                page.restoreBackupClicked()
            }
        }
    }

    FormCard.FormSectionText {
        visible: page.copiesChoiceProblem !== ""
        textFormat: Text.PlainText
        text: page.copiesChoiceProblem
        Accessible.role: Accessible.AlertMessage
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "Before KassenSturz updates a book to work with a new version, it saves a copy of it here. It keeps the last 10 copies of each book.")
    }

    FormCard.FormSectionText {
        visible: copiesButton.visible
        text: page.i18nContext.i18nc("@info", "These copies are on the same disk as your book. If you choose another folder, for example on a USB stick or in a folder your cloud app uploads, KassenSturz also keeps a copy there once a day when you open your book, and a copy of every backup.")
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Undo and history")
    }

    FormCard.FormCard {
        FormCard.FormComboBoxDelegate {
            id: keepFor

            // Shows the history's setting, also after the dialog turned the
            // choice down. Set rather than bound: the delegate sets its
            // index itself on phones, which would end a binding.
            function showChoice() {
                currentIndex = indexOfValue(page.history.retention);
            }

            text: page.i18nContext.i18nc("@label:listbox", "Keep the history for")
            textRole: "text"
            valueRole: "value"
            model: page.keepChoices
            Component.onCompleted: showChoice()
            onModelChanged: Qt.callLater(showChoice)
            onActivated: {
                ActionLog.taken("SettingsBackupPage_keepHistory")
                page.chooseRetention(currentValue)
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            descriptionItem.textFormat: Text.PlainText
            icon.name: "view-history"
            text: page.i18nContext.i18nc("@action:button", "Show history")
            description: page.i18nContext.i18ncp("@info %1 is how many changes there are", "%1 change so far.", "%1 changes so far.", page.history.total)
            onClicked: {
                ActionLog.taken("SettingsBackupPage_showHistory")
                page.showHistoryClicked()
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "edit-clear-history"
            text: page.i18nContext.i18nc("@action:button", "Clean up history…")
            description: page.i18nContext.i18nc("@info", "Deletes the whole history. Your accounts and transactions stay as they are.")
            enabled: page.history.total > 0
            onClicked: {
                ActionLog.taken("SettingsBackupPage_cleanUpHistory")
                cleanUpDialog.open()
            }
        }
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "Undo takes back your changes, newest first, also after you close and open the book again. The longer the history is kept, the further back Undo goes. With \"Only while the book is open\", nothing is saved in the book: Undo works until you close it.")
    }

    Kirigami.PromptDialog {
        id: shorterDialog

        title: page.i18nContext.i18nc("@title:window", "Delete part of the history?")
        subtitle: page.pendingRetention === "0"
            ? page.i18nContext.i18ncp("@info %1 is how many changes there are", "Undo will then work only until you close the book. The %1 change made so far will be gone by then. Your accounts and transactions are not affected.", "Undo will then work only until you close the book. The %1 changes made so far will be gone by then. Your accounts and transactions are not affected.", page.pendingCount)
            : page.i18nContext.i18ncp("@info %1 is how many changes there are", "%1 change is older than this and will be deleted for good. You won't be able to undo it or make it again. Your accounts and transactions are not affected.", "%1 changes are older than this and will be deleted for good. You won't be able to undo them or make them again. Your accounts and transactions are not affected.", page.pendingCount)
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Delete them")
                icon.name: "edit-delete"
                onTriggered: {
                    ActionLog.taken("SettingsBackupPage_deleteThem")
                    page.saveRetention(page.pendingRetention);
                    shorterDialog.close();
                }
            },
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Keep the history as it is")
                icon.name: "dialog-cancel"
                onTriggered: {
                    ActionLog.taken("SettingsBackupPage_keepHistory_2")
                    shorterDialog.close()
                }
            }
        ]
        onClosed: keepFor.showChoice()
    }

    Kirigami.PromptDialog {
        id: cleanUpDialog

        title: page.i18nContext.i18nc("@title:window", "Clean up the history?")
        subtitle: page.i18nContext.i18nc("@info", "This deletes the whole history for good, so nothing you did so far can be undone or made again. Your accounts, transactions and everything else stay exactly as they are.\n\nCopies of the book that KassenSturz saved before an update (backups) keep their history; cleaning up does not touch them.")
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Delete the history")
                icon.name: "edit-clear-history"
                onTriggered: {
                    ActionLog.taken("SettingsBackupPage_deleteHistory")
                    page.historyResult = page.history.cleanUp();
                    cleanUpDialog.close();
                }
            },
            Kirigami.Action {
                text: page.i18nContext.i18nc("@action:button", "Keep the history")
                icon.name: "dialog-cancel"
                onTriggered: {
                    ActionLog.taken("SettingsBackupPage_keepHistory_3")
                    cleanUpDialog.close()
                }
            }
        ]
    }

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Backups and undo")
}
