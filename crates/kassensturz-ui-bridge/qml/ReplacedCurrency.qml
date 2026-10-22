// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The question about one currency a file uses that KassenSturz doesn't know
// (M3.12, D-106): most likely one that another currency replaced long ago.
// The user says which one replaced it, how many of the old one a unit of the
// new one was worth, and from which day. The file's own rates are only a
// suggestion to check; they are filled in only when the user presses the
// button, never by themselves. Every text comes from Rust (D-065); the
// sentences are written here (D-034).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n

FormCard.FormCard {
    id: card

    // One entry of the file's `unlisted` list: `currency`, and the
    // `suggestion` the file's rates give, if they do.
    required property var info
    // The current currencies to choose from.
    required property var currencies

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // A labelled text field. Not FormTextFieldDelegate, which asks for the
    // engine-wide KI18n context that the app doesn't use (D-074).
    component TextRow: FormCard.AbstractFormDelegate {
        id: row

        property string label
        property alias field: input

        // The delegate is a button; typing in it must not click it.
        hoverEnabled: false
        contentItem: ColumnLayout {
            spacing: Kirigami.Units.smallSpacing

            Controls.Label {
                Layout.fillWidth: true
                text: row.label
                wrapMode: Text.Wrap
            }

            Controls.TextField {
                id: input

                Layout.fillWidth: true
            }
        }
    }

    // What the user chose and typed, as the book reads it.
    function answer(): var {
        return {
            currency: info.currency,
            successor: successor.currentIndex >= 0 ? successor.currentText : "",
            rate: rate.field.text,
            date: since.field.text,
        };
    }

    // Fills in what the file's own rates suggest. The user checks it.
    function useSuggestion() {
        successor.currentIndex = successor.find(info.suggestion.successor);
        rate.field.text = info.suggestion.rate;
        since.field.text = info.suggestion.date;
    }

    FormCard.FormTextDelegate {
        text: card.i18nContext.i18nc("@label %1 is a currency code", "The file has amounts in %1", card.info.currency)
        description: card.i18nContext.i18nc("@info %1 is a currency code", "KassenSturz doesn't know %1. It was most likely replaced by another currency a long time ago. Say which one, so the amounts can be counted in it. Every amount keeps its original value in %1.", card.info.currency)
    }

    FormCard.FormDelegateSeparator {
        visible: card.info.hasSuggestion
    }

    FormCard.FormButtonDelegate {
        visible: card.info.hasSuggestion
        icon.name: "dialog-information"
        text: card.i18nContext.i18nc("@action:button %1 is a currency code", "Use what the file suggests for %1", card.info.currency)
        description: visible
            ? card.i18nContext.i18nc("@info %1 and %2 are currency codes, %3 is how many, %4 a date", "The file's own rates say 1 %1 was worth %3 %2 from %4. Another program's rates can be wrong, so check this before you use it.", card.info.suggestion.successor, card.info.currency, card.info.suggestion.rate, card.info.suggestion.date)
            : ""
        onClicked: card.useSuggestion()
    }

    FormCard.FormDelegateSeparator {}

    FormCard.FormComboBoxDelegate {
        id: successor

        text: card.i18nContext.i18nc("@label:listbox the currency that replaced the old one", "Replaced by")
        model: card.currencies
        Component.onCompleted: currentIndex = -1
    }

    FormCard.FormDelegateSeparator {}

    TextRow {
        id: rate

        label: card.i18nContext.i18nc("@label:textbox %1 is a currency code", "How many %1 was one of the new currency worth?", card.info.currency)
        field.inputMethodHints: Qt.ImhFormattedNumbersOnly
    }

    FormCard.FormDelegateSeparator {}

    TextRow {
        id: since

        label: card.i18nContext.i18nc("@label:textbox a date", "From which day?")
    }
}
