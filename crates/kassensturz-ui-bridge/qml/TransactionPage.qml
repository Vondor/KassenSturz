// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Adds a transaction: money paid or received, how much, to or from whom,
// what for and when (M1.M). The other party reads "To" or "From" (D-034).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property FirstRunMock mock
    // Part of the first start rather than added from the overview.
    property bool firstRun: false
    property bool moneyIn: false

    // Added, or skipped during the first start.
    signal done()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: firstRun
        ? i18nContext.i18nc("@title", "Your first transaction")
        : i18nContext.i18nc("@title", "Add a transaction")

    header: ColumnLayout {
        spacing: 0

        MockNotice {
            Layout.fillWidth: true
        }

        FormError {
            id: error
            Layout.fillWidth: true
        }
    }

    function save() {
        const result = mock.addTransaction(accountBox.currentIndex, moneyIn, amountField.text,
            otherPartyField.text, categoryBox.currentIndex, dateField.text);
        error.result = result;
        if (result.code === undefined) {
            done();
        }
    }

    Kirigami.FormLayout {
        Controls.Label {
            Layout.maximumWidth: Kirigami.Units.gridUnit * 20
            visible: page.firstRun
            text: page.i18nContext.i18nc("@info", "Now add something you paid or received, for example your last shopping trip.")
            wrapMode: Text.Wrap
        }

        Controls.ComboBox {
            id: accountBox
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:listbox", "Account:")
            model: page.mock.accounts
            // The account just added comes last; with one account there's
            // nothing to choose.
            currentIndex: count - 1
            visible: count > 1
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
            id: amountField
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "How much?")
            placeholderText: page.i18nContext.i18nc("@info:placeholder %1 is an example amount", "For example %1", page.mock.amountExample)
            inputMethodHints: Qt.ImhFormattedNumbersOnly
            onAccepted: page.save()
        }

        Controls.TextField {
            id: otherPartyField
            Kirigami.FormData.label: page.moneyIn
                ? page.i18nContext.i18nc("@label:textbox who the money came from", "From:")
                : page.i18nContext.i18nc("@label:textbox who the money went to", "To:")
            placeholderText: page.moneyIn
                ? page.i18nContext.i18nc("@info:placeholder", "For example your employer")
                : page.i18nContext.i18nc("@info:placeholder", "For example the shop")
            onAccepted: page.save()
        }

        Controls.ComboBox {
            id: categoryBox
            Kirigami.FormData.label: page.moneyIn
                ? page.i18nContext.i18nc("@label:listbox", "What kind of income?")
                : page.i18nContext.i18nc("@label:listbox", "What was it for?")
            model: page.mock.categories(page.moneyIn)
        }

        Controls.TextField {
            id: dateField
            Kirigami.FormData.label: page.i18nContext.i18nc("@label:textbox", "Date:")
            text: page.mock.today
            onAccepted: page.save()
        }

        RowLayout {
            Controls.Button {
                text: page.i18nContext.i18nc("@action:button", "Add transaction")
                icon.name: "list-add"
                onClicked: page.save()
            }

            Controls.Button {
                visible: page.firstRun
                text: page.i18nContext.i18nc("@action:button", "Skip for now")
                flat: true
                onClicked: page.done()
            }
        }
    }
}
