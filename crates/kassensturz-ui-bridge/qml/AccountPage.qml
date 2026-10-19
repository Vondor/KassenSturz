// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Adds an account: what kind it is, its name and how much is in it today
// (M1.M). Every field has a sensible default, so a new user can just press
// the button.

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
    property int typeIndex: 0

    signal added()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The keys are the ones the Rust side knows (first_run::type_key).
    readonly property var types: [
        {
            key: "checking",
            name: i18nContext.i18nc("@item:inlistbox an account type", "Checking account"),
            hint: i18nContext.i18nc("@info", "The account your pay goes into and your bills are paid from."),
        },
        {
            key: "savings",
            name: i18nContext.i18nc("@item:inlistbox an account type", "Savings account"),
            hint: i18nContext.i18nc("@info", "Money you put aside."),
        },
        {
            key: "cash",
            name: i18nContext.i18nc("@item:inlistbox an account type", "Cash"),
            hint: i18nContext.i18nc("@info", "The money in your wallet."),
        },
        {
            key: "credit-card",
            name: i18nContext.i18nc("@item:inlistbox an account type", "Credit card"),
            hint: i18nContext.i18nc("@info", "What you buy with it, you pay back later. KassenSturz shows what you owe on it."),
        },
    ]
    readonly property var type: types[typeIndex]
    readonly property bool owes: type.key === "credit-card"

    title: firstRun
        ? i18nContext.i18nc("@title", "Your first account")
        : i18nContext.i18nc("@title", "Add an account")

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
        const name = nameField.text.trim() === "" ? type.name : nameField.text;
        const result = mock.addAccount(type.key, name, balanceField.text);
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
            model: page.types

            delegate: Controls.RadioButton {
                required property var modelData
                required property int index

                Kirigami.FormData.label: index === 0 ? page.i18nContext.i18nc("@label", "What kind of account is it?") : ""
                text: modelData.name
                checked: index === page.typeIndex
                Controls.ButtonGroup.group: typeGroup
                onToggled: page.typeIndex = index
            }
        }

        Controls.Label {
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
            onAccepted: page.save()
        }

        Controls.TextField {
            id: balanceField
            Kirigami.FormData.label: page.owes
                ? page.i18nContext.i18nc("@label:textbox", "How much do you owe on it today?")
                : page.i18nContext.i18nc("@label:textbox", "How much is in it today?")
            placeholderText: page.i18nContext.i18nc("@info:placeholder %1 is an example amount", "For example %1", page.mock.amountExample)
            inputMethodHints: Qt.ImhFormattedNumbersOnly
            onAccepted: page.save()
        }

        Controls.Label {
            Layout.maximumWidth: Kirigami.Units.gridUnit * 20
            text: page.i18nContext.i18nc("@info", "Leave it empty if there's nothing in it.")
            visible: !page.owes
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
        }

        Controls.Button {
            text: page.i18nContext.i18nc("@action:button", "Add account")
            icon.name: "list-add"
            onClicked: page.save()
        }
    }
}
