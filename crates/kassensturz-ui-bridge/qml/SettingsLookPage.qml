// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The settings group "Look and language" (D-795): the language (D-127), how
// amounts look (D-002, D-114) and the colors of the charts (D-589).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

FormCard.FormCardPage {
    id: page

    required property NumberFormatSettings numberFormat
    required property ChartPaletteSettings chartPalette
    required property LanguageSettings language

    // Whether a number format choice couldn't be saved for the next start.
    property bool formatNotSaved: false
    // Whether the language choice couldn't be saved.
    property bool languageNotSaved: false

    header: ColumnLayout {
        spacing: 0

        // of the page, where the other information bars are (D-498, M6-B4),
        // so it is seen wherever the page is scrolled to.
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            position: Kirigami.InlineMessage.Position.Header
            type: Kirigami.MessageType.Information
            visible: page.language.restartNeeded && !page.languageNotSaved
            text: page.i18nContext.i18nc("@info", "Close KassenSturz and open it again to see it in the new language.")
        }
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Language")
    }

    FormCard.FormCard {
        FormCard.FormComboBoxDelegate {
            id: languageBox

            // Shows the saved choice. Set rather than bound: the delegate
            // sets its index itself on phones, which would end a binding.
            function showChoice() {
                currentIndex = indexOfValue(page.language.choice);
            }

            text: page.i18nContext.i18nc("@label:listbox", "Language")
            textRole: "text"
            valueRole: "value"
            // Each language is named in itself, so it is found whatever
            // language the app is in now.
            model: [
                { value: "system", text: page.i18nContext.i18nc("@item:inlistbox", "As in your system") },
                { value: "en", text: "English" },
                { value: "de", text: "Deutsch" },
                { value: "pl", text: "Polski" },
            ]
            Component.onCompleted: showChoice()
            onActivated: {
                ActionLog.taken("SettingsLookPage_language")
                page.languageNotSaved = !page.language.choose(currentValue)
            }
        }

        // A choice that couldn't be saved says so under the box; the
        // restart note is the bar at the top of the page (D-498).
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.largeSpacing
            visible: page.languageNotSaved
            type: Kirigami.MessageType.Error
            text: page.i18nContext.i18nc("@info", "KassenSturz couldn't save this choice. Check that your settings folder can be written to, then choose again.")
        }
    }

    FormCard.FormSectionText {
        textFormat: Text.PlainText
        text: page.i18nContext.i18nc("@info", "Dates and amounts keep following your regional settings; change how amounts look below.")
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
            ActionLog.taken("SettingsLookPage_activated")
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
            descriptionItem.textFormat: Text.PlainText
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
        textFormat: Text.PlainText
        text: page.formatNotSaved
            ? page.i18nContext.i18nc("@info", "KassenSturz couldn't save this choice, so it applies only until you close KassenSturz. Check that your settings folder can be written to, then choose again.")
            : page.i18nContext.i18nc("@info", "This changes only how amounts look on this computer, not the amounts themselves. KassenSturz always calculates with the exact amounts, so rounded amounts may not add up exactly on screen: point at one to see it exactly. You can still type amounts with all their decimal places.")
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Chart colors")
    }

    // The colors of the charts (M7a.10, D-589).
    ChartColorsCard {
        Layout.fillWidth: true
        chartColors: page.chartPalette
    }

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Look and language")
}
