// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The settings group "My book" (D-795): which book is open, its folder and
// tags, opening another book, an example book, a new one or one from a file
// (D-033, M3.12, US4.8).

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

    signal openAnotherClicked()
    signal exampleBookClicked()
    signal newBookClicked()
    signal bookFromFileClicked()
    // The user wants the page of the book's tags (US4.8).
    signal tagsClicked()

    FormCard.FormCard {
        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@label", "Book")
            description: page.session.fileName
            // A file's name could contain anything.
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "tag"
            text: page.i18nContext.i18nc("@action:button", "Tags")
            description: page.i18nContext.i18nc("@info", "Rename, merge or remove your tags.")
            onClicked: {
                ActionLog.taken("SettingsBookPage_tags")
                page.tagsClicked()
            }
        }

        FormCard.FormDelegateSeparator {}

        // Opens the book's folder in the file manager (M5-F2). In the
        // Flatpak this goes through the desktop portal, so it works from the
        // sandbox.
        FormCard.FormButtonDelegate {
            icon.name: "folder-open"
            text: page.i18nContext.i18nc("@label the folder a file is in", "Folder")
            description: page.session.folder
            descriptionItem.textFormat: Text.PlainText
            Controls.ToolTip.visible: hovered
            Controls.ToolTip.text: page.i18nContext.i18nc("@info:tooltip", "Open this folder in the file manager")
            onClicked: {
                ActionLog.taken("SettingsBookPage_folder")
                Qt.openUrlExternally(page.session.folderUrl)
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "document-open"
            text: page.i18nContext.i18nc("@action:button", "Open another book…")
            description: page.i18nContext.i18nc("@info", "KassenSturz opens the book you used last. Most households need only one.")
            onClicked: {
                ActionLog.taken("SettingsBookPage_openAnotherBook")
                page.openAnotherClicked()
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "folder-open"
            text: page.i18nContext.i18nc("@action:button", "Try an example book…")
            description: page.i18nContext.i18nc("@info", "Look around in a made-up household. KassenSturz makes a new book for it, so your own books are never changed.")
            onClicked: {
                ActionLog.taken("SettingsBookPage_tryExampleBook")
                page.exampleBookClicked()
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "document-new"
            text: page.i18nContext.i18nc("@action:button", "Start a new book…")
            description: page.i18nContext.i18nc("@info", "An empty book in a folder you choose, for example to try something out.")
            onClicked: {
                ActionLog.taken("SettingsBookPage_startNewBook")
                page.newBookClicked()
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "document-import"
            text: page.i18nContext.i18nc("@action:button", "Start a book from a file…")
            description: page.i18nContext.i18nc("@info", "From a KMyMoney book, or a whole book you saved with KassenSturz.")
            onClicked: {
                ActionLog.taken("SettingsBookPage_startBookFile")
                page.bookFromFileClicked()
            }
        }
    }

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "My book")
}
