// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The app's settings: which book is open, opening another one or starting a
// new one (D-033, for people who want more than one book), how amounts look
// (D-002, D-114), and where the backups go.

import QtQuick
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

FormCard.FormCardPage {
    id: page

    required property BookSession session
    required property NumberFormatSettings numberFormat

    // Whether a number format choice couldn't be saved for the next start.
    property bool formatNotSaved: false

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
        title: page.i18nContext.i18nc("@title:group", "How amounts look")
    }

    // One number format setting: its choices as `{ text, value }`, `value`
    // being the word the settings file keeps for it (D-114). An inline
    // component doesn't see the page's ids, so it gets the settings object
    // and reports a choice it couldn't save.
    component FormatChoice: FormCard.FormComboBoxDelegate {
        id: choice

        required property string setting
        required property NumberFormatSettings numberFormat

        signal notSaved()

        // Shows the setting's choice, also after choosing one separator
        // changed the other. Set rather than bound: the delegate sets its
        // index itself on phones, which would end a binding.
        function showChoice() {
            currentIndex = indexOfValue(numberFormat.choices[setting]);
        }

        textRole: "text"
        valueRole: "value"
        Component.onCompleted: showChoice()
        onActivated: {
            if (!numberFormat.choose(setting, currentValue)) {
                notSaved();
            }
        }

        Connections {
            target: choice.numberFormat

            function onChoicesChanged() {
                choice.showChoice();
            }
        }
    }

    FormCard.FormCard {
        FormCard.FormTextDelegate {
            text: page.i18nContext.i18nc("@label an example amount", "Example")
            description: page.numberFormat.example + "    " + page.numberFormat.negativeExample
        }

        FormCard.FormDelegateSeparator {}

        FormatChoice {
            setting: "DecimalSeparator"
            numberFormat: page.numberFormat
            onNotSaved: page.formatNotSaved = true
            text: page.i18nContext.i18nc("@label:listbox", "Decimal separator")
            model: [
                { value: "regional", text: page.i18nContext.i18nc("@item:inlistbox", "As in your regional settings") },
                { value: "comma", text: page.i18nContext.i18nc("@item:inlistbox", "Comma: 1,50") },
                { value: "point", text: page.i18nContext.i18nc("@item:inlistbox", "Point: 1.50") },
            ]
        }

        FormCard.FormDelegateSeparator {}

        FormatChoice {
            setting: "DigitGrouping"
            numberFormat: page.numberFormat
            onNotSaved: page.formatNotSaved = true
            text: page.i18nContext.i18nc("@label:listbox", "Thousands separator")
            model: [
                { value: "regional", text: page.i18nContext.i18nc("@item:inlistbox", "As in your regional settings") },
                { value: "point", text: page.i18nContext.i18nc("@item:inlistbox", "Point: 1.000.000") },
                { value: "comma", text: page.i18nContext.i18nc("@item:inlistbox", "Comma: 1,000,000") },
                { value: "space", text: page.i18nContext.i18nc("@item:inlistbox", "Space: 1 000 000") },
                { value: "apostrophe", text: page.i18nContext.i18nc("@item:inlistbox", "Apostrophe: 1'000'000") },
                { value: "none", text: page.i18nContext.i18nc("@item:inlistbox", "None: 1000000") },
            ]
        }

        FormCard.FormDelegateSeparator {}

        FormatChoice {
            setting: "CurrencySymbol"
            numberFormat: page.numberFormat
            onNotSaved: page.formatNotSaved = true
            text: page.i18nContext.i18nc("@label:listbox", "Currency symbol")
            model: [
                { value: "regional", text: page.i18nContext.i18nc("@item:inlistbox", "As in your regional settings") },
                { value: "before", text: page.i18nContext.i18nc("@item:inlistbox", "Before the amount: €5") },
                { value: "after", text: page.i18nContext.i18nc("@item:inlistbox", "After the amount: 5 €") },
            ]
        }

        FormCard.FormDelegateSeparator {}

        FormatChoice {
            setting: "NegativeAmounts"
            numberFormat: page.numberFormat
            onNotSaved: page.formatNotSaved = true
            text: page.i18nContext.i18nc("@label:listbox", "Amounts below zero")
            model: [
                { value: "regional", text: page.i18nContext.i18nc("@item:inlistbox", "As in your regional settings") },
                { value: "minus", text: page.i18nContext.i18nc("@item:inlistbox", "With a minus sign: -5") },
                { value: "parentheses", text: page.i18nContext.i18nc("@item:inlistbox", "In parentheses: (5)") },
            ]
        }

        FormCard.FormDelegateSeparator {}

        FormatChoice {
            setting: "DecimalPlaces"
            numberFormat: page.numberFormat
            onNotSaved: page.formatNotSaved = true
            text: page.i18nContext.i18nc("@label:listbox", "Decimal places")
            model: [
                { value: "all", text: page.i18nContext.i18nc("@item:inlistbox", "All the currency has: 5,00") },
                { value: "none", text: page.i18nContext.i18nc("@item:inlistbox", "None, rounded: 5") },
            ]
        }
    }

    FormCard.FormSectionText {
        text: page.formatNotSaved
            ? page.i18nContext.i18nc("@info", "KassenSturz couldn't save this choice, so it applies only until you close KassenSturz. Check that your settings folder can be written to, then choose again.")
            : page.i18nContext.i18nc("@info", "This changes only how amounts look on this computer, not the amounts themselves. KassenSturz always calculates with the exact amounts, so rounded amounts may not add up exactly on screen: point at one to see it exactly. You can still type amounts with all their decimal places.")
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
