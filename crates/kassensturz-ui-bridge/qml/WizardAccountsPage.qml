// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Page 3b of the setup wizard (M7a.9, docs/SETUP_WIZARD.md): the user's
// accounts, one row each, with how much is in them today. Nothing is saved
// while they type. "Create my household book" writes the currency, the
// default categories and every account in one user action (FirstStart.finish),
// so the book holds all of it or none of it. Each row is checked in Rust when
// its amount is left, and amounts reach Rust only as typed text (D-065).
//
// In the guided setup (#1031, docs/GUIDED_SETUP.md, step 1) the page asks
// only for the kind and the name: the balance comes from the bank or from a
// file afterwards (D-895), so nobody types it. The book is written the same
// way, with every account starting empty.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property FirstStart firstStart
    required property AccountList accounts
    // The household's currency, chosen on the first page.
    required property string currency

    // Part of the guided setup: no amounts, and its own words.
    property bool guided: false
    // The guided setup's step of this page and how many steps there are.
    property int guidedStep: 0
    property int guidedCount: 0

    // The book is written.
    signal finished()
    // "End the setup" before anything is written: back to the choices.
    signal guidedEnded()
    signal exampleClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    AccountTypes {
        id: types
    }

    DefaultCategories {
        id: defaults
    }

    // The rows as typed. `error` is the row's message, empty if none.
    ListModel {
        id: rows
    }
    // For click-through drivers and tests.
    readonly property alias accountRows: rows

    Component.onCompleted: addRow()

    // A row of the next kind not yet used: checking, savings, cash, credit card.
    function addRow() {
        let typeIndex = 0;
        for (let t = 0; t < types.all.length; ++t) {
            let used = false;
            for (let r = 0; r < rows.count; ++r) {
                if (rows.get(r).typeIndex === t) {
                    used = true;
                }
            }
            if (!used) {
                typeIndex = t;
                break;
            }
        }
        rows.append({ typeIndex: typeIndex, name: "", balance: "", date: "", rowCurrency: "", more: false, error: "" });
    }

    function nameOf(row: var): string {
        return row.name.trim() === "" ? types.all[row.typeIndex].name : row.name;
    }

    function currencyOf(row: var): string {
        return row.rowCurrency === "" ? page.currency : row.rowCurrency;
    }

    function check(index: int) {
        const row = rows.get(index);
        const result = page.firstStart.checkAccount(types.all[row.typeIndex].key, nameOf(row), currencyOf(row), row.balance, row.date);
        rows.setProperty(index, "error", result.code === undefined ? "" : errors.messageFor(result));
    }

    function create() {
        // Five strings per account, one after the other.
        const typed = [];
        for (let r = 0; r < rows.count; ++r) {
            const row = rows.get(r);
            typed.push(types.all[row.typeIndex].key, nameOf(row), currencyOf(row), row.balance, row.date);
        }
        const result = page.firstStart.finish(page.currency, defaults.incomeNames, defaults.incomeParents,
            defaults.spendingNames, defaults.spendingParents, typed);
        if (result.code === undefined) {
            page.finished();
            return;
        }
        if (result.row >= 0 && result.row < rows.count) {
            rows.setProperty(result.row, "error", errors.messageFor(result));
            createError.text = Plain.plain(page.i18nContext.i18nc("@info %1 is the account's name", "Please check the account “%1”. Nothing was saved yet.", nameOf(rows.get(result.row))));
        } else {
            createError.text = Plain.plain(errors.messageFor(result));
        }
    }

    // Writes error codes as messages.
    FormError {
        id: errors

        visible: false
    }

    title: Plain.plain(guided
        ? i18nContext.i18nc("@title a step of the guided setup", "Which accounts do you have?")
        : i18nContext.i18nc("@title", "Your accounts"))
    header: ColumnLayout {
        spacing: 0

        MockNotice {
            Layout.fillWidth: true
        }

        GuidedBar {
            Layout.fillWidth: true
            step: page.guided ? page.guidedStep : 0
            count: page.guidedCount
            onEndClicked: page.guidedEnded()
        }
    }

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        ColumnLayout {
            Layout.alignment: Qt.AlignHCenter
            Layout.maximumWidth: Kirigami.Units.gridUnit * 40
            Layout.preferredWidth: Kirigami.Units.gridUnit * 40
            spacing: Kirigami.Units.largeSpacing

            Controls.Label {
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignHCenter
                textFormat: Text.PlainText
                text: page.guided
                    ? page.i18nContext.i18nc("@info", "Name the accounts you want to see in KassenSturz. You don't need to type how much is in them: that comes from your bank or from a file in a moment. Nothing is saved until you click “Continue”.")
                    : page.i18nContext.i18nc("@info", "Type how much is in each account today. Nothing is saved until you click “Create my household book”.")
                wrapMode: Text.Wrap
            }

            Repeater {
                model: rows

                delegate: ColumnLayout {
                    id: rowItem

                    required property int index
                    required property int typeIndex
                    required property string name
                    required property string balance
                    required property string date
                    required property string rowCurrency
                    required property bool more
                    required property string error

                    readonly property bool owes: types.all[typeIndex].key === "credit-card"

                    Layout.fillWidth: true
                    spacing: Kirigami.Units.smallSpacing

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing

                        Controls.ComboBox {
                            Layout.preferredWidth: Kirigami.Units.gridUnit * 9
                            Accessible.name: page.i18nContext.i18nc("@label:listbox", "What kind of account is it?")
                            model: types.all.map(type => type.name)
                            currentIndex: rowItem.typeIndex
                            onActivated: newIndex => {
                                ActionLog.taken("WizardAccountsPage_kind")
                                rows.setProperty(rowItem.index, "typeIndex", newIndex)
                            }
                        }

                        Controls.TextField {
                            Layout.fillWidth: true
                            Accessible.name: page.i18nContext.i18nc("@label:textbox", "Name:")
                            // Left empty, the account is named after its kind.
                            placeholderText: types.all[rowItem.typeIndex].name
                            text: rowItem.name
                            onTextEdited: rows.setProperty(rowItem.index, "name", text)
                        }

                        Controls.Label {
                            visible: !page.guided
                            textFormat: Text.PlainText
                            text: rowItem.owes
                                ? page.i18nContext.i18nc("@label:textbox", "You owe:")
                                : page.i18nContext.i18nc("@label:textbox", "Today:")
                        }

                        Controls.TextField {
                            visible: !page.guided
                            Layout.preferredWidth: Kirigami.Units.gridUnit * 8
                            Accessible.name: rowItem.owes
                                ? page.i18nContext.i18nc("@label:textbox", "How much do you owe on it today?")
                                : page.i18nContext.i18nc("@label:textbox", "How much is in it today?")
                            horizontalAlignment: Text.AlignRight
                            inputMethodHints: Qt.ImhFormattedNumbersOnly
                            placeholderText: page.accounts.amountExample(page.currencyOf(rows.get(rowItem.index)))
                            text: rowItem.balance
                            onTextEdited: rows.setProperty(rowItem.index, "balance", text)
                            onEditingFinished: page.check(rowItem.index)
                        }

                        Controls.ToolButton {
                            visible: !page.guided
                            icon.name: rowItem.more ? "arrow-up" : "overflow-menu"
                            text: page.i18nContext.i18nc("@action:button another day or currency for this account", "More")
                            display: Controls.AbstractButton.IconOnly
                            Controls.ToolTip.text: text
                            Controls.ToolTip.visible: hovered
                            onClicked: {
                                ActionLog.taken("WizardAccountsPage_more")
                                rows.setProperty(rowItem.index, "more", !rowItem.more)
                            }
                        }

                        Controls.ToolButton {
                            icon.name: "edit-delete-remove"
                            text: page.i18nContext.i18nc("@action:button", "Remove this account")
                            display: Controls.AbstractButton.IconOnly
                            Controls.ToolTip.text: text
                            Controls.ToolTip.visible: hovered
                            // The wizard always keeps one account.
                            enabled: rows.count > 1
                            onClicked: {
                                ActionLog.taken("WizardAccountsPage_remove")
                                rows.remove(rowItem.index)
                            }
                        }
                    }

                    // One deliberate step away: another day, another currency.
                    Kirigami.FormLayout {
                        Layout.fillWidth: true
                        visible: rowItem.more

                        Controls.TextField {
                            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "On which day?")
                            placeholderText: page.i18nContext.i18nc("@info:placeholder %1 is today's date", "Today, %1", page.accounts.today)
                            text: rowItem.date
                            onTextEdited: rows.setProperty(rowItem.index, "date", text)
                            onEditingFinished: page.check(rowItem.index)
                        }

                        Controls.ComboBox {
                            Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Currency:")
                            // The long list is built only once it is wanted.
                            model: rowItem.more ? page.accounts.currencies : []
                            currentIndex: Math.max(0, page.accounts.currencies.indexOf(page.currencyOf(rows.get(rowItem.index))))
                            onActivated: newIndex => {
                                ActionLog.taken("WizardAccountsPage_currency")
                                rows.setProperty(rowItem.index, "rowCurrency", page.accounts.currencies[newIndex])
                                page.check(rowItem.index)
                            }
                        }
                    }

                    Kirigami.InlineMessage {
                        Layout.fillWidth: true
                        visible: rowItem.error !== ""
                        type: Kirigami.MessageType.Error
                        text: Plain.plain(rowItem.error)
                    }
                }
            }

            Controls.Button {
                Layout.alignment: Qt.AlignLeft
                flat: true
                icon.name: "list-add"
                text: page.i18nContext.i18nc("@action:button", "Add another account")
                onClicked: {
                    ActionLog.taken("WizardAccountsPage_addRow")
                    page.addRow()
                }
            }

            Kirigami.InlineMessage {
                Layout.fillWidth: true
                // The guided setup asks for no amount.
                visible: !page.guided
                type: Kirigami.MessageType.Information
                text: page.i18nContext.i18nc("@info", "You find the amount in your banking app or on your statement. Just trying KassenSturz out? A made-up amount is fine, or try an example book instead.")
                actions: [
                    Kirigami.Action {
                        text: page.i18nContext.i18nc("@action:button", "Try an example book")
                        icon.name: "view-preview"
                        onTriggered: {
                            ActionLog.taken("WizardAccountsPage_example")
                            page.exampleClicked()
                        }
                    }
                ]
            }

            Kirigami.InlineMessage {
                id: createError

                Layout.fillWidth: true
                visible: text !== ""
                type: Kirigami.MessageType.Error
            }

            Controls.Button {
                Layout.alignment: Qt.AlignHCenter
                highlighted: true
                icon.name: page.guided ? "go-next" : "dialog-ok-apply"
                text: Plain.plain(page.guided
                    ? page.i18nContext.i18nc("@action:button", "Continue")
                    : page.i18nContext.i18nc("@action:button", "Create my household book"))
                onClicked: {
                    ActionLog.taken("WizardAccountsPage_create")
                    page.create()
                }
            }
        }
    }
}
