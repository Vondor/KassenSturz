// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One Part on the Parts page (M3.6, D-017): what it was for, how much of the
// total, and for a category in another currency the amount in that currency
// with the rate behind it. The amount left empty is worked out by the Rust
// side and shown in its place (D-065).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n

ColumnLayout {
    id: row

    // Which Part this is, counted from 1.
    required property int number
    // The categories to choose from: objects with id, path and currency.
    required property var choices
    // What the form holds for this Part.
    required property string category
    required property string amount
    required property string foreignAmount
    // What the Rust side worked out for this Part (`partsPreview`).
    required property var preview
    // An amount written the way the user types it.
    required property string amountExample
    // Whether the page's money went in rather than out, for the labels.
    required property bool moneyIn

    signal categoryChosen(string categoryId)
    signal amountEdited(string text)
    signal foreignAmountEdited(string text)
    signal removeClicked()
    signal accepted()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The category's currency, if it isn't the account's.
    readonly property string currency: (choices.find(choice => choice.id === category) ?? ({})).currency ?? ""
    readonly property bool open: preview.open ?? false

    function choiceLabel(choice: var): string {
        return choice.currency === ""
            ? choice.path
            : i18nContext.i18nc("@item:inlistbox %1 is a category, %2 a currency code", "%1 (%2)", choice.path, choice.currency);
    }

    // The place of the category with `id` in the list; 0 is "Not sorted yet".
    function categoryIndex(id: string): int {
        return choices.findIndex(choice => choice.id === id) + 1;
    }

    spacing: Kirigami.Units.smallSpacing
    onCategoryChanged: categoryBox.sync()

    RowLayout {
        spacing: Kirigami.Units.smallSpacing

        Controls.ComboBox {
            id: categoryBox
            Layout.fillWidth: true
            model: [row.i18nContext.i18nc("@item:inlistbox the part gets no category yet", "Not sorted yet")]
                .concat(row.choices.map(choice => row.choiceLabel(choice)))
            Accessible.name: row.i18nContext.i18nc("@label:listbox %1 is the number of the part", "What part %1 was for", row.number)
            onActivated: index => {
                ActionLog.taken("PartRow_whatPartWas")
                row.categoryChosen(index <= 0 ? "" : row.choices[index - 1].id)
            }

            // Shows the Part's category: again after the list changed,
            // since a new list resets the choice.
            function sync() {
                currentIndex = row.categoryIndex(row.category);
            }
            onModelChanged: sync()
            Component.onCompleted: sync()
        }

        Controls.TextField {
            id: amountField
            Layout.preferredWidth: Kirigami.Units.gridUnit * 7
            text: row.amount
            // The Part left empty shows what is left for it.
            placeholderText: row.open ? row.preview.amount : row.amountExample
            inputMethodHints: Qt.ImhFormattedNumbersOnly
            Accessible.name: row.i18nContext.i18nc("@label:textbox %1 is the number of the part", "How much of it part %1 was", row.number)
            onTextEdited: row.amountEdited(text)
            onAccepted: {
                ActionLog.taken("PartRow_howMuchPart")
                row.accepted()
            }
        }

        Controls.ToolButton {
            icon.name: "list-remove"
            text: row.i18nContext.i18nc("@action:button", "Remove this part")
            display: Controls.AbstractButton.IconOnly
            Controls.ToolTip.text: text
            Controls.ToolTip.visible: hovered
            onClicked: {
                ActionLog.taken("PartRow_removePart")
                row.removeClicked()
            }
        }
    }

    // What is left for the open Part, written out for screen readers and
    // when the field is focused and the placeholder goes away.
    Controls.Label {
        textFormat: Text.PlainText
        visible: row.open && row.preview.amount !== undefined
        text: row.i18nContext.i18nc("@info %1 is an amount of money without its currency, %2 is the number of the part", "Part %2 gets what is left: %1", row.preview.amount ?? "", row.number)
        font: Kirigami.Theme.smallFont
    }

    RowLayout {
        visible: row.currency !== ""
        spacing: Kirigami.Units.smallSpacing

        Controls.Label {
            textFormat: Text.PlainText
            text: row.i18nContext.i18nc("@label:textbox %1 is a currency code", "In %1:", row.currency)
        }

        Controls.TextField {
            id: foreignField
            Layout.preferredWidth: Kirigami.Units.gridUnit * 7
            text: row.foreignAmount
            // Worked out from the book's exchange rates, if it has one.
            placeholderText: (row.preview.suggested ?? false) ? row.preview.foreignAmount : ""
            inputMethodHints: Qt.ImhFormattedNumbersOnly
            Accessible.name: row.i18nContext.i18nc("@label:textbox %1 is the number of the part, %2 a currency code", "What part %1 was in %2", row.number, row.currency)
            onTextEdited: row.foreignAmountEdited(text)
            onAccepted: {
                ActionLog.taken("PartRow_whatPartWas_2")
                row.accepted()
            }
        }

        Controls.Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            visible: (row.preview.rate ?? "") !== ""
            text: (row.preview.rateDate ?? "") === ""
                ? row.i18nContext.i18nc("@info %1 and %3 are currency codes, %2 an exchange rate", "1 %1 = %2 %3", row.preview.rateFrom ?? "", row.preview.rate ?? "", row.preview.rateTo ?? "")
                : row.i18nContext.i18nc("@info %1 and %3 are currency codes, %2 an exchange rate, %4 a date", "1 %1 = %2 %3, your rate of %4", row.preview.rateFrom ?? "", row.preview.rate ?? "", row.preview.rateTo ?? "", row.preview.rateDate ?? "")
            font: Kirigami.Theme.smallFont
            wrapMode: Text.Wrap
        }
    }

    Controls.Label {
        textFormat: Text.PlainText
        visible: row.currency !== "" && !(row.preview.suggested ?? false) && (row.preview.foreignAmount ?? "") === ""
        text: row.i18nContext.i18nc("@info %1 is a currency code", "Your book has no exchange rate for %1 yet, so type what this part was in %1, e.g. from the receipt.", row.currency)
        font: Kirigami.Theme.smallFont
        wrapMode: Text.Wrap
        Layout.fillWidth: true
    }
}
