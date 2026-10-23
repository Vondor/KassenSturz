// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// What starting a book from a file did (M3.12, D-029, D-101): what came in,
// what was changed to make it fit, and exactly what the file held that
// KassenSturz can't hold yet. Nothing is left out without being said. Every
// text comes from Rust (D-065); the sentences are written here (D-034).

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

FormCard.FormCardPage {
    id: page

    required property BookFromFile bookFromFile
    // For the questions about Transfers, which belong to the book.
    required property AccountRegister register
    readonly property var report: bookFromFile.view ?? ({})

    // The user is done reading and wants to go on with the book.
    signal doneClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // What KassenSturz left out of a file, by the keyword the Rust side
    // gives each kind, in plain words (D-029).
    function skippedName(what: string, count: int): string {
        switch (what) {
        case "investments":
            return i18nContext.i18ncp("@item what was left out, %1 is how many", "%1 investment account", "%1 investment accounts", count);
        case "schedules":
            return i18nContext.i18ncp("@item what was left out, %1 is how many", "%1 scheduled or recurring transaction", "%1 scheduled or recurring transactions", count);
        case "budgets":
            return i18nContext.i18ncp("@item what was left out, %1 is how many", "%1 budget", "%1 budgets", count);
        case "reports":
            return i18nContext.i18ncp("@item what was left out, %1 is how many", "%1 saved report", "%1 saved reports", count);
        case "tags":
            return i18nContext.i18ncp("@item what was left out, %1 is how many", "%1 tag that no transaction could be given", "%1 tags that no transaction could be given", count);
        case "rates":
            return i18nContext.i18ncp("@item what was left out, %1 is how many", "%1 exchange rate or price that can't be kept exactly", "%1 exchange rates or prices that can't be kept exactly", count);
        case "online-banking":
            return i18nContext.i18ncp("@item what was left out, %1 is how many", "%1 online banking setting", "%1 online banking settings", count);
        case "owner":
            return i18nContext.i18nc("@item what was left out", "your name and address");
        default:
            return i18nContext.i18ncp("@item what was left out, %1 is how many", "%1 other entry", "%1 other entries", count);
        }
    }

    // A plain list of names for a sentence: a name could be anything, so
    // it is shown as plain text.
    function joined(names: var): string {
        return names.join(", ");
    }

    title: i18nContext.i18nc("@title", "Your book is ready")

    TransferPrompt {
        id: transferPrompt

        register: page.register
    }

    // Not an InlineMessage: the book's names could contain anything, so they
    // are shown as plain text.
    FormCard.FormCard {
        FormCard.FormTextDelegate {
            textItem.textFormat: Text.PlainText
            icon.name: "dialog-ok"
            text: page.report.newBook
                ? page.i18nContext.i18nc("@info", "A new book was made and is open")
                : page.i18nContext.i18nc("@info", "Your book now holds what the file held")
            description: !page.report.newBook
                ? ""
                : (page.report.previousFileName ?? "") === ""
                    ? page.i18nContext.i18nc("@info %1 is a book's name", "It is called \"%1\".", page.report.fileName ?? "")
                    : page.i18nContext.i18nc("@info %1 and %2 are book names", "It is called \"%1\". Your other book, \"%2\", is still where it was.", page.report.fileName ?? "", page.report.previousFileName)
            descriptionItem.textFormat: Text.PlainText
        }
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "What came in")
    }

    FormCard.FormCard {
        FormCard.FormTextDelegate {
            descriptionItem.textFormat: Text.PlainText
            text: page.i18nContext.i18nc("@label", "Accounts")
            description: String(page.report.accounts ?? 0)
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            descriptionItem.textFormat: Text.PlainText
            text: page.i18nContext.i18nc("@label", "Categories")
            description: String(page.report.categories ?? 0)
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            descriptionItem.textFormat: Text.PlainText
            text: page.i18nContext.i18nc("@label", "Transactions")
            description: String(page.report.transactions ?? 0)
        }

        FormCard.FormDelegateSeparator {
            visible: (page.report.rates ?? 0) > 0
        }

        FormCard.FormTextDelegate {
            descriptionItem.textFormat: Text.PlainText
            visible: (page.report.rates ?? 0) > 0
            text: page.i18nContext.i18nc("@label", "Exchange rates")
            description: String(page.report.rates ?? 0)
        }
    }

    FormCard.FormHeader {
        visible: notes.visible
        title: page.i18nContext.i18nc("@title:group", "Worth a look")
    }

    FormCard.FormCard {
        id: notes

        visible: (page.report.uncategorized ?? 0) > 0 || (page.report.completed ?? 0) > 0
            || (page.report.renamed ?? []).length > 0 || (page.report.movedToTop ?? []).length > 0
            || (page.report.leftOpen ?? []).length > 0 || (page.report.converted ?? []).length > 0
            || (page.report.transfers ?? 0) > 0

        FormCard.FormTextDelegate {
            textItem.textFormat: Text.PlainText
            visible: (page.report.uncategorized ?? 0) > 0
            text: page.i18nContext.i18ncp("@info %1 is how many", "%1 transaction has no category yet", "%1 transactions have no category yet", page.report.uncategorized ?? 0)
            description: page.i18nContext.i18nc("@info", "That is how they are in the file, as often happens with transactions that came from a bank. Give them a category when you like.")
        }

        FormCard.FormDelegateSeparator {
            visible: (page.report.uncategorized ?? 0) > 0 && (page.report.completed ?? 0) > 0
        }

        FormCard.FormTextDelegate {
            textItem.textFormat: Text.PlainText
            visible: (page.report.completed ?? 0) > 0
            text: page.i18nContext.i18ncp("@info %1 is how many", "%1 transaction didn't add up in the file", "%1 transactions didn't add up in the file", page.report.completed ?? 0)
            description: page.i18nContext.i18nc("@info", "KassenSturz added a part without a category for the difference, so every amount still adds up. Look at them and give the difference a category.")
        }

        FormCard.FormDelegateSeparator {
            visible: ((page.report.uncategorized ?? 0) > 0 || (page.report.completed ?? 0) > 0) && (page.report.renamed ?? []).length > 0
        }

        FormCard.FormTextDelegate {
            visible: (page.report.renamed ?? []).length > 0
            text: page.i18nContext.i18nc("@info", "Some names were taken")
            description: page.i18nContext.i18nc("@info %1 is a list of names", "These got a number after their name, so you can tell them apart: %1", page.joined(page.report.renamed ?? []))
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {
            visible: (page.report.movedToTop ?? []).length > 0 && (((page.report.uncategorized ?? 0) > 0 || (page.report.completed ?? 0) > 0) || (page.report.renamed ?? []).length > 0)
        }

        FormCard.FormTextDelegate {
            visible: (page.report.movedToTop ?? []).length > 0
            text: page.i18nContext.i18nc("@info", "Some categories moved to the top")
            description: page.i18nContext.i18nc("@info %1 is a list of names", "These were inside a category in another currency, which KassenSturz can't add up, so they are top-level categories now: %1", page.joined(page.report.movedToTop ?? []))
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {
            visible: (page.report.leftOpen ?? []).length > 0 && (((page.report.uncategorized ?? 0) > 0 || (page.report.completed ?? 0) > 0) || (page.report.renamed ?? []).length > 0 || (page.report.movedToTop ?? []).length > 0)
        }

        FormCard.FormTextDelegate {
            visible: (page.report.leftOpen ?? []).length > 0
            text: page.i18nContext.i18nc("@info", "Some closed accounts are open")
            description: page.i18nContext.i18nc("@info %1 is a list of names", "These were closed in the file, but money is still in them, so they stay open and nothing is hidden: %1", page.joined(page.report.leftOpen ?? []))
            descriptionItem.textFormat: Text.PlainText
        }

        Repeater {
            model: page.report.converted ?? []

            delegate: FormCard.FormTextDelegate {
                descriptionItem.textFormat: Text.PlainText
                textItem.textFormat: Text.PlainText
                required property var modelData

                text: page.i18nContext.i18nc("@info %1 and %2 are currency codes", "Amounts in %1 are counted in %2", modelData.old, modelData.new)
                description: page.i18nContext.i18nc("@info %1 and %2 are currency codes, %3 is how many, %4 a date", "1 %2 was worth %3 %1 from %4. Every converted transaction keeps its original amount in %1.", modelData.old, modelData.new, modelData.rate, modelData.date)
            }
        }

        FormCard.FormDelegateSeparator {
            visible: (page.report.transfers ?? 0) > 0
        }

        FormCard.FormButtonDelegate {
            visible: (page.report.transfers ?? 0) > 0
            icon.name: "transform-move"
            text: page.i18nContext.i18ncp("@action:button %1 is how many", "%1 pair of transactions may be money you moved between your accounts", "%1 pairs of transactions may be money you moved between your accounts", page.report.transfers ?? 0)
            description: page.i18nContext.i18nc("@info", "KassenSturz asks about each pair, one at a time. Nothing is joined without your click.")
            onClicked: transferPrompt.ask(page.register.transferSuggestions(""))
        }
    }

    FormCard.FormHeader {
        visible: (page.report.skipped ?? []).length > 0
        title: page.i18nContext.i18nc("@title:group", "Left out")
    }

    FormCard.FormCard {
        visible: (page.report.skipped ?? []).length > 0

        Repeater {
            model: page.report.skipped ?? []

            delegate: FormCard.FormTextDelegate {
                textItem.textFormat: Text.PlainText
                required property var modelData

                text: page.skippedName(modelData.what, modelData.count)
            }
        }
    }

    FormCard.FormSectionText {
        visible: (page.report.skipped ?? []).length > 0
        text: page.i18nContext.i18nc("@info", "KassenSturz can't hold these yet, so they weren't brought into the book. Your KMyMoney file still has them: it was not changed.")
    }

    FormCard.FormCard {
        FormCard.FormButtonDelegate {
            icon.name: "go-next"
            text: page.i18nContext.i18nc("@action:button", "Go on with the book")
            onClicked: page.doneClicked()
        }
    }
}
