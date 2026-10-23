// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Breaks a transaction down into Parts (M3.6, D-017, D-119): what one
// purchase was for, piece by piece. One Part's amount can be left empty, and
// KassenSturz works it out from the rest, so nobody has to do the
// subtraction; saving is refused unless the Parts add up to the total. A
// Part booked to a category in another currency also gets its amount in
// that currency (D-043). Money moved to another of the user's accounts in
// the same transaction is shown and kept as it is. Every amount is text
// from the Rust side (D-065).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property AccountRegister register
    required property string accountId
    required property string transactionId

    // What the Rust side says about the transaction.
    property var entry: ({})
    property bool moneyIn: false
    // The categories to choose from, read again when the direction changes.
    property var categoryChoices: []
    // What the Rust side worked out for the form as it stands.
    property var preview: ({ parts: [] })
    property string amountExample: ""
    // The Parts as the form holds them.
    readonly property ListModel partsModel: ListModel {}

    // Saved or removed.
    signal done()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Break down this transaction")

    header: FormError {
        id: error
    }

    Component.onCompleted: {
        entry = register.partsDetails(accountId, transactionId);
        if (entry.code !== undefined) {
            error.result = entry;
            return;
        }
        moneyIn = entry.moneyIn;
        totalField.text = entry.total;
        otherPartyField.text = entry.otherParty;
        dateField.text = entry.date;
        amountExample = register.amountExample(accountId);
        for (const part of entry.parts) {
            page.partsModel.append({ splitId: part.id, category: part.category, amount: part.amount, foreignAmount: part.foreignAmount });
        }
        reloadChoices();
        refresh();
    }

    onMoneyInChanged: {
        if (entry.id !== undefined) {
            reloadChoices();
            refresh();
        }
    }

    function reloadChoices() {
        const found = register.partCategoryChoices(accountId, transactionId, moneyIn);
        categoryChoices = found.code === undefined ? found : [];
    }

    // What the user typed, as the Rust side takes it.
    function form(): var {
        const parts = [];
        for (let i = 0; i < page.partsModel.count; i++) {
            const part = page.partsModel.get(i);
            parts.push({ id: part.splitId, category: part.category, amount: part.amount, foreignAmount: part.foreignAmount });
        }
        return {
            moneyIn: moneyIn,
            total: totalField.text,
            otherParty: otherPartyField.text,
            date: dateField.text,
            parts: parts,
        };
    }

    // Asks the Rust side what the form adds up to. A form it can't read yet
    // shows nothing worked out; saving says what is wrong.
    function refresh() {
        const result = register.partsPreview(accountId, transactionId, form());
        preview = result.code === undefined ? result : ({ parts: [] });
    }

    function previewOf(index: int): var {
        return (preview.parts ?? [])[index] ?? ({});
    }

    function addPart() {
        page.partsModel.append({ splitId: "", category: "", amount: "", foreignAmount: "" });
        refresh();
    }

    function save() {
        const result = register.saveParts(accountId, transactionId, form());
        error.result = result;
        if (result.code === undefined) {
            done();
        }
    }

    function remove() {
        const result = register.removeTransaction(accountId, transactionId);
        error.result = result;
        if (result.code === undefined) {
            done();
        }
    }

    Kirigami.FormLayout {
        id: layout

        Controls.Label {
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
            text: page.i18nContext.i18nc("@info", "Say what each part of this transaction was for. Leave one part's amount empty, and KassenSturz works it out from the rest.")
            wrapMode: Text.Wrap
        }

        Controls.ButtonGroup {
            id: directionGroup
        }

        Controls.RadioButton {
            Kirigami.FormData.label: page.i18nContext.i18nc("@label", "What happened?")
            text: page.i18nContext.i18nc("@option:radio", "I paid money")
            checked: !page.moneyIn
            Controls.ButtonGroup.group: directionGroup
            onToggled: page.moneyIn = false
        }

        Controls.RadioButton {
            text: page.i18nContext.i18nc("@option:radio", "I received money")
            checked: page.moneyIn
            Controls.ButtonGroup.group: directionGroup
            onToggled: page.moneyIn = true
        }

        Controls.TextField {
            id: totalField
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "How much in all?")
            placeholderText: page.i18nContext.i18nc("@info:placeholder %1 is an example amount", "For example %1", page.amountExample)
            inputMethodHints: Qt.ImhFormattedNumbersOnly
            readOnly: page.entry.totalFixed ?? false
            onTextEdited: page.refresh()
            onAccepted: page.save()
        }

        Controls.Label {
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
            visible: page.entry.totalFixed ?? false
            text: page.i18nContext.i18nc("@info", "This transaction came from your bank, so the total stays as the bank sent it. You can change the parts.")
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
        }

        Controls.TextField {
            id: otherPartyField
            Kirigami.FormData.label: page.moneyIn
                ? page.i18nContext.i18nc("@label:textbox who the money came from", "From:")
                : page.i18nContext.i18nc("@label:textbox who the money went to", "To:")
            onAccepted: page.save()
        }

        Controls.TextField {
            id: dateField
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "Date:")
            onTextEdited: page.refresh()
            onAccepted: page.save()
        }

        Kirigami.Separator {
            Kirigami.FormData.isSection: true
            Kirigami.FormData.label: page.i18nContext.i18nc("@title:group", "The parts")
        }

        // The rows sit in a column of their own, so a Part added later
        // joins the others and not the end of the form.
        ColumnLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.largeSpacing

            // Money moved to or from another of the user's accounts in the
            // same transaction stays as it is (M3.7).
            Repeater {
                model: page.entry.fixed ?? []

                Controls.Label {
                    textFormat: Text.PlainText
                    required property var modelData

                    text: modelData.incoming
                        ? page.i18nContext.i18nc("@label %1 is the name of an account, %2 an amount of money", "Moved from %1: %2", modelData.name, modelData.amount)
                        : page.i18nContext.i18nc("@label %1 is the name of an account, %2 an amount of money", "Moved to %1: %2", modelData.name, modelData.amount)
                }
            }

            Repeater {
                model: page.partsModel

                PartRow {
                    required property int index
                    required property var model

                    Layout.fillWidth: true
                    number: index + 1
                    choices: page.categoryChoices
                    category: model.category
                    amount: model.amount
                    foreignAmount: model.foreignAmount
                    preview: page.previewOf(index)
                    amountExample: page.amountExample
                    moneyIn: page.moneyIn
                    onCategoryChosen: categoryId => {
                        model.category = categoryId;
                        page.refresh();
                    }
                    onAmountEdited: text => {
                        model.amount = text;
                        page.refresh();
                    }
                    onForeignAmountEdited: text => {
                        model.foreignAmount = text;
                        page.refresh();
                    }
                    onRemoveClicked: {
                        page.partsModel.remove(index);
                        page.refresh();
                    }
                    onAccepted: page.save()
                }
            }
        }

        Controls.Button {
            text: page.i18nContext.i18nc("@action:button", "Add a part")
            icon.name: "list-add"
            onClicked: page.addPart()
        }

        // Whether it all adds up, from the exact amounts (D-065).
        Controls.Label {
            textFormat: Text.PlainText
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
            visible: (page.preview.sum ?? "") !== ""
            text: (page.preview.missing ?? "") !== ""
                ? page.i18nContext.i18nc("@info %1 and %2 are amounts of money", "The parts add up to %1. %2 is still to be sorted into a part.", page.preview.sum, page.preview.missing)
                : (page.preview.over ?? "") !== ""
                    ? page.i18nContext.i18nc("@info %1 and %2 are amounts of money", "The parts add up to %1, which is %2 more than the total.", page.preview.sum, page.preview.over)
                    : page.i18nContext.i18nc("@info %1 is an amount of money", "The parts add up to %1, the whole transaction.", page.preview.sum)
            wrapMode: Text.Wrap
        }

        RowLayout {
            Controls.Button {
                text: page.i18nContext.i18nc("@action:button", "Save")
                icon.name: "document-save"
                enabled: page.entry.id !== undefined
                onClicked: page.save()
            }

            Controls.Button {
                text: page.i18nContext.i18nc("@action:button", "Remove")
                icon.name: "edit-delete"
                enabled: page.entry.id !== undefined
                onClicked: page.remove()
            }
        }

        Controls.Label {
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
            text: page.i18nContext.i18nc("@info", "Removing takes the transaction out of this account's list and balance. Undo brings it back.")
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
        }
    }
}
