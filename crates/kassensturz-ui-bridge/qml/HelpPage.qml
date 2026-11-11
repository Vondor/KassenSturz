// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One help page in plain words on the two things a person types that look
// like a small language: the patterns of a Rule's conditions and the
// formulas of charts and key figures (US3.13, #649, D-700). Every example
// uses invented names (D-699). The rules it states are the ones the code
// keeps: a pattern is a Rust `regex` pattern, capitals are ignored unless
// "Capital letters must match" is ticked, and lookaround and
// back-references are not available (D-015); formulas follow LibreOffice
// Calc (D-510, D-292, D-318, D-649).

import QtQuick
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

FormCard.FormCardPage {
    id: page

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Help: patterns and formulas")

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Patterns in Rules")
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "A Rule looks at the name or the purpose of a transaction. Most of the time \"contains\" is all you need: it finds the words you type anywhere in the text, and capital letters don't matter. A pattern is for the cases where that is not enough. Switch it on with \"Use a pattern (advanced)\" in a condition.")
    }

    FormCard.FormCard {
        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info", "Either of two names")
            description: "fresho|brightmart"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info", "The text starts with a word")
            description: "^FRESHO"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info", "The text ends with four digits")
            description: "\\d{4}$"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info", "A word, then anything, then another word")
            description: "^FRESHO .* THANK YOU$"
            descriptionItem.textFormat: Text.PlainText
        }
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "A pattern can match a part of the text, so you don't need to cover the whole text. ^ means the start of the text and $ means its end. A dot stands for any one character and .* for any amount of any characters. \\d is a digit, and a vertical bar means \"or\". To look for one of these characters itself, put a backslash before it, for example \\. for a dot.")
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "Patterns use the syntax of the Rust \"regex\" library. Capital letters are ignored unless \"Capital letters must match\" is ticked. Lookahead and lookbehind, such as (?=…) or (?<!…), and back-references such as \\1 don't work. If a pattern can't be read, KassenSturz says so and changes nothing.")
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Formulas")
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "A formula works like a formula in a spreadsheet such as LibreOffice Calc. A category or a group stands for its total in the chart's period, as a positive amount. You can start with = or leave it out. KassenSturz works out the formula from the exact amounts of your book and shows what it counted next to the result. A formula only reads your book; it never changes a transaction or a balance.")
    }

    FormCard.FormCard {
        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info", "What is left after one category")
            description: "=Groceries - Pets"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info", "Two categories added up")
            description: "=SUM(Groceries; Pets)"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info", "A name with spaces goes in single quotes")
            description: "='Eating out' + Travel"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info", "One category in a period of its own")
            description: "=Groceries@\"last month\""
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info", "An average per month over the last three whole months")
            description: "=Groceries@\"last 3 complete months\" / 3"
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@info", "Count Pets only when it is above 100")
            description: "=IF(Pets > 100; Pets; 0)"
            descriptionItem.textFormat: Text.PlainText
        }
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "You can use + - * / and brackets, % for a percentage, and the comparisons = <> < > <= >=. Between the parts of a function, ; works everywhere, as in LibreOffice Calc. If your number format uses a decimal point, a comma works as well. A period word such as \"last month\", \"this year\" or \"last 12 months\" in quotes after @ gives a name its own period. The formula field lists the functions with a short line each and completes the names of your categories while you type. A mistake is described in words, with the place marked.")
    }
}
