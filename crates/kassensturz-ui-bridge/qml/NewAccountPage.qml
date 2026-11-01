// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Adds an account: what kind it is, its name, and how much is in it on a day
// the user names, today unless changed (D-111). Every field has a sensible
// default, so a new user can just press the button. The currency is the
// household's usual one; another one is a deliberate step away. At the first
// start it is the first account, and leads on to the first transaction
// (D-116).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property AccountList accounts
    // Part of the first start rather than added from a screen.
    property bool firstRun: false
    property int typeIndex: 0

    signal added()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    AccountTypes {
        id: types
    }

    readonly property var type: types.all[typeIndex]
    readonly property bool owes: type.key === "credit-card"
    // The household's usual currency is the first; others are chosen on request.
    readonly property string currency: otherCurrency.checked ? currencyBox.currentText : accounts.currencies[0]

    title: firstRun
        ? i18nContext.i18nc("@title", "Your first account")
        : i18nContext.i18nc("@title", "Add an account")
    header: ColumnLayout {
        spacing: 0

        MockNotice {
            Layout.fillWidth: true
            visible: page.firstRun
        }

        FormError {
            id: error
            Layout.fillWidth: true
        }
    }

    function save() {
        const name = nameField.text.trim() === "" ? type.name : nameField.text;
        const result = accounts.addAccount(type.key, name, currency, balanceField.text, dateField.text);
        error.result = result;
        if (result.code === undefined) {
            added();
        }
    }

    Kirigami.FormLayout {
        Controls.ButtonGroup {
            id: typeGroup
        }

        Repeater {
            model: types.all

            delegate: Controls.RadioButton {
                required property var modelData
                required property int index

                Kirigami.FormData.label: index === 0 ? page.i18nContext.i18nc("@label", "What kind of account is it?") : ""
                text: modelData.name
                checked: index === page.typeIndex
                Controls.ButtonGroup.group: typeGroup
                onToggled: {
                    ActionLog.taken("NewAccountPage_typeIndex")
                    page.typeIndex = index
                }
            }
        }

        Controls.Label {
            textFormat: Text.PlainText
            Layout.maximumWidth: Kirigami.Units.gridUnit * 20
            text: page.type.hint
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
        }

        Kirigami.Separator {
            Kirigami.FormData.isSection: true
        }

        Controls.TextField {
            id: nameField
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "Name:")
            // Left empty, the account is named after its kind.
            placeholderText: page.type.name
            onAccepted: {
                ActionLog.taken("NewAccountPage_save")
                page.save()
            }
        }

        Controls.TextField {
            id: balanceField
            Kirigami.FormData.label: page.owes
                ? page.i18nContext.i18nc("@label:textbox", "How much do you owe on it?")
                : page.i18nContext.i18nc("@label:textbox", "How much is in it?")
            placeholderText: page.i18nContext.i18nc("@info:placeholder %1 is an example amount", "For example %1", accounts.amountExample(page.currency))
            inputMethodHints: Qt.ImhFormattedNumbersOnly
            onAccepted: {
                ActionLog.taken("NewAccountPage_save_2")
                page.save()
            }
        }

        Controls.TextField {
            id: dateField
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "On which day?")
            placeholderText: page.i18nContext.i18nc("@info:placeholder %1 is today's date", "Today, %1", accounts.today)
            onAccepted: {
                ActionLog.taken("NewAccountPage_save_3")
                page.save()
            }
        }

        Controls.Label {
            Layout.maximumWidth: Kirigami.Units.gridUnit * 20
            text: page.i18nContext.i18nc("@info", "The day your balance is for. Leave the amount empty if there's nothing in it. If you type a day in the past, KassenSturz counts the money from then on.")
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
        }

        Controls.Label {
            Layout.maximumWidth: Kirigami.Units.gridUnit * 20
            visible: page.firstRun
            text: page.i18nContext.i18nc("@info", "Take the balance from your bank's statement or app, so KassenSturz starts from what you really have. It can check against your bank at any time.")
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
        }

        Controls.CheckBox {
            id: otherCurrency
            Kirigami.FormData.label: page.i18nContext.i18nc("@label", "Currency:")
            text: page.i18nContext.i18nc("@option:check %1 is a currency code such as EUR", "This account is not in %1", accounts.currencies[0])
        }

        Controls.ComboBox {
            id: currencyBox
            visible: otherCurrency.checked
            // The list is long and a ComboBox builds all of it up front, so
            // it is handed over only when the box is wanted: a page that is
            // only opened should not wait for it.
            model: otherCurrency.checked ? page.accounts.currencies : []
        }

        Controls.Button {
            text: page.i18nContext.i18nc("@action:button", "Add account")
            icon.name: "list-add"
            onClicked: {
                ActionLog.taken("NewAccountPage_addAccount")
                page.save()
            }
        }
    }
}
