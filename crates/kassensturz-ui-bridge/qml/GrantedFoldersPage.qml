// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The folders KassenSturz may open, and taking one back (#873, D-730,
// docs/BOOK_FOLDER.md §3). Only in the Flatpak, where the user gives access
// to a folder through the system's folder chooser and the access stays until
// it is taken back. "Forget" ends the access; it never deletes or changes a
// file. A folder the open book or the backup copies still use says so and
// can't be forgotten. Folders are shown as plain text only (D-730); every
// text comes from Rust, the sentences are written here (D-034).

import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

FormCard.FormCardPage {
    id: page

    required property BookMove bookMove

    // The folders, as `grantedFolders` answers.
    property var folders: []
    // What the last attempt to forget a folder said, as a sentence; empty if
    // it worked or nothing was tried.
    property string problem: ""
    // Whether the last attempt worked.
    property bool forgotten: false

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Folders KassenSturz may open")

    function reload() {
        folders = bookMove.grantedFolders();
    }

    // What still uses a folder, in words.
    function usedText(used: var): string {
        if (!used || used.length === 0)
            return "";
        const book = used.indexOf("book") >= 0;
        const copies = used.indexOf("backup-copies") >= 0;
        if (book && copies)
            return i18nContext.i18nc("@info", "Your book is kept here and your backup copies are written here. Move them elsewhere first to forget this folder.");
        if (book)
            return i18nContext.i18nc("@info", "Your book is kept here. Keep it in another folder first to forget this folder.");
        return i18nContext.i18nc("@info", "Your backup copies are written here. Choose another folder for them first to forget this folder.");
    }

    // What a code from BookMove means, and what to do next.
    function reasonFor(code: string): string {
        switch (code) {
        case "granted-folders.in-use-by-book":
            return i18nContext.i18nc("@info", "Your book is kept in this folder, so it wasn't forgotten. Keep your book in another folder first.");
        case "granted-folders.in-use-by-backup-copies":
            return i18nContext.i18nc("@info", "Your backup copies are written to this folder, so it wasn't forgotten. Choose another folder for the copies first.");
        case "granted-folders.not-there":
            return i18nContext.i18nc("@info", "This folder isn't in the list any more. Nothing was changed.");
        default:
            return i18nContext.i18nc("@info", "Your computer didn't take the access back. Nothing was changed and no file was touched. Try again, or take it back in your system's settings for apps.");
        }
    }

    Component.onCompleted: reload()

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "What you gave access to")
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "When you choose a folder for KassenSturz, it keeps the right to open that folder until you take it back here. Forgetting a folder only ends this right: no file is deleted, moved or changed. If you need the folder again, you choose it again.")
    }

    Kirigami.InlineMessage {
        Layout.fillWidth: true
        Layout.margins: Kirigami.Units.largeSpacing
        type: Kirigami.MessageType.Warning
        visible: page.problem !== ""
        text: page.problem
    }

    Kirigami.InlineMessage {
        Layout.fillWidth: true
        Layout.margins: Kirigami.Units.largeSpacing
        type: Kirigami.MessageType.Positive
        visible: page.forgotten
        text: page.i18nContext.i18nc("@info", "KassenSturz can't open that folder any more. Its files are as they were.")
    }

    FormCard.FormCard {
        visible: page.folders.length > 0

        Repeater {
            model: page.folders

            delegate: ColumnLayout {
                id: row

                required property var modelData
                required property int index

                Layout.fillWidth: true
                spacing: 0

                FormCard.FormTextDelegate {
                    Layout.fillWidth: true
                    // A path usually holds the user's name: plain text.
                    text: row.modelData.shownAs
                    textItem.textFormat: Text.PlainText
                    description: page.usedText(row.modelData.usedBy)
                    descriptionItem.textFormat: Text.PlainText
                }

                FormCard.FormButtonDelegate {
                    Layout.fillWidth: true
                    visible: row.modelData.usedBy.length === 0
                    icon.name: "edit-delete-remove"
                    text: page.i18nContext.i18nc("@action:button", "Forget this folder")
                    description: page.i18nContext.i18nc("@info", "No file is deleted.")
                    onClicked: {
                        ActionLog.taken("GrantedFoldersPage_forget")
                        const answer = page.bookMove.forgetFolder(row.modelData.id);
                        page.forgotten = answer.ok === true;
                        page.problem = answer.ok === true ? "" : page.reasonFor(answer.code ?? "");
                        page.reload();
                    }
                }

                FormCard.FormDelegateSeparator {
                    visible: row.index < page.folders.length - 1
                }
            }
        }
    }

    FormCard.FormCard {
        visible: page.folders.length === 0

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info", "You have not given KassenSturz access to any folder.")
            description: page.i18nContext.i18nc("@info", "Folders you choose, for example for your book or for backup copies, appear here.")
        }
    }
}
