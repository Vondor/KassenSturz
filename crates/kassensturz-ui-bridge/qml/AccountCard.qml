// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One account as a card: its name, its kind and its balance. Clicking it
// opens the account.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Kirigami.AbstractCard {
    id: card

    // The account from the Rust side: id, name, type, balance and
    // exactBalance (both text) and owes.
    required property var modelData

    AccountTypes {
        id: types
    }

    showClickFeedback: true
    Accessible.name: [modelData.name, types.nameOf(modelData.type), balance.phrase(modelData.exactBalance)]
        .filter(text => text !== "").join(", ")

    contentItem: ColumnLayout {
        spacing: 0

        Controls.Label {
            Layout.fillWidth: true
            text: card.modelData.name
            elide: Text.ElideRight
            font.bold: true
            // A name could contain anything.
            textFormat: Text.PlainText
        }

        Controls.Label {
            Layout.fillWidth: true
            text: types.nameOf(card.modelData.type)
            // An account named after its type needn't say it twice.
            visible: text !== "" && text !== card.modelData.name
            font: Kirigami.Theme.smallFont
        }

        Balance {
            id: balance

            Layout.fillWidth: true
            Layout.topMargin: Kirigami.Units.smallSpacing
            amount: card.modelData.balance
            exactAmount: card.modelData.exactBalance
            owes: card.modelData.owes
            horizontalAlignment: Text.AlignRight
            font.bold: true
        }
    }
}
