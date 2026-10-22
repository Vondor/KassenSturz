// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Starting a book from a file (M3.12, D-100, D-101, D-103, D-106, D-125): a
// complete export KassenSturz wrote, or a KMyMoney book. The file was read
// before this page shows (as untrusted input), so the user sees what it
// holds, says where it goes, and answers what only they can: which current
// currency replaced one that KassenSturz doesn't know. Nothing is made until
// "Make the book". Every text comes from Rust (D-065); the sentences are
// written here (D-034).

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

FormCard.FormCardPage {
    id: page

    required property BookFromFile bookFromFile
    readonly property var file: bookFromFile.view ?? ({})
    readonly property var unlisted: file.unlisted ?? []

    // The user decided against making the book.
    signal cancelled()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Start a book from a file")
    header: FormError {
        id: error

        forNewBook: true
    }

    function make() {
        const answers = [];
        for (let index = 0; index < currencyQuestions.count; ++index) {
            answers.push(currencyQuestions.itemAt(index).answer());
        }
        error.result = bookFromFile.create(answers);
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "What the file holds")
    }

    FormCard.FormCard {
        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@label", "File")
            description: page.file.fileName ?? ""
            // A file's name could contain anything.
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@label what sort of file it is", "Made by")
            description: page.file.kind === "export"
                ? page.i18nContext.i18nc("@info", "KassenSturz, with \"Save the whole book\"")
                : page.i18nContext.i18nc("@info", "KMyMoney")
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@label", "Accounts")
            description: String(page.file.accounts ?? 0)
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@label", "Categories")
            description: String(page.file.categories ?? 0)
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@label", "Transactions")
            description: String(page.file.transactions ?? 0)
        }
    }

    FormCard.FormSectionText {
        text: page.file.kind === "export"
            ? page.i18nContext.i18nc("@info", "Everything in the file comes back exactly as it was saved. The history of changes isn't part of the file, so Undo starts fresh.")
            : page.i18nContext.i18nc("@info", "KassenSturz reads your KMyMoney file and never changes it. What KassenSturz can't hold yet, such as budgets, is left out, and you will see exactly what.")
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Where it goes")
    }

    FormCard.FormCard {
        FormCard.FormTextDelegate {
            text: page.file.intoOpenBook
                ? page.i18nContext.i18nc("@info", "Into your book")
                : page.i18nContext.i18nc("@info", "Into a new book")
            description: page.file.intoOpenBook
                ? page.i18nContext.i18nc("@info", "Your book is still empty, so the file fills it.")
                : page.i18nContext.i18nc("@info %1 is a book's name", "KassenSturz makes a new book called \"%1\" and opens it. Your current book isn't changed; you can open it again in Settings with \"Open another book…\".", page.file.bookName ?? "")
            descriptionItem.textFormat: Text.PlainText
        }
    }

    FormCard.FormHeader {
        visible: page.unlisted.length > 0
        title: page.i18nContext.i18nc("@title:group", "Currencies KassenSturz doesn't know")
    }

    Repeater {
        id: currencyQuestions

        model: page.unlisted

        delegate: ReplacedCurrency {
            required property var modelData

            info: modelData
            currencies: page.bookFromFile.currencies
        }
    }

    FormCard.FormCard {
        FormCard.FormButtonDelegate {
            icon.name: "document-new"
            text: page.i18nContext.i18nc("@action:button", "Make the book")
            onClicked: page.make()
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            icon.name: "dialog-cancel"
            text: page.i18nContext.i18nc("@action:button", "Don't make a book")
            description: page.i18nContext.i18nc("@info", "The file stays as it is.")
            onClicked: page.cancelled()
        }
    }
}
