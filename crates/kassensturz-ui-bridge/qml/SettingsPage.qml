// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The app's settings. For now: which book is open, opening another one or
// starting a new one (D-033, for people who want more than one book), and
// where the backups go. The number format comes with M3.2 (D-002).

import QtQuick
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

FormCard.FormCardPage {
    id: page

    required property BookSession session

    signal openAnotherClicked()
    signal newBookClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Settings")

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Your book")
    }

    FormCard.FormCard {
        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@label", "Book")
            description: page.session.fileName
            // A file's name could contain anything.
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@label the folder a file is in", "Folder")
            description: page.session.folder
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "document-open"
            text: page.i18nContext.i18nc("@action:button", "Open another book…")
            description: page.i18nContext.i18nc("@info", "KassenSturz opens the book you used last. Most households need only one.")
            onClicked: page.openAnotherClicked()
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "document-new"
            text: page.i18nContext.i18nc("@action:button", "Start a new book…")
            description: page.i18nContext.i18nc("@info", "An empty book in a folder you choose, for example to try something out.")
            onClicked: page.newBookClicked()
        }
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Backups")
    }

    FormCard.FormCard {
        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@label", "Backup folder")
            description: page.session.backupFolder
            descriptionItem.textFormat: Text.PlainText
        }
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "Before KassenSturz updates a book to work with a new version, it saves a copy of it here. It keeps the last 10 copies of each book.")
    }
}
