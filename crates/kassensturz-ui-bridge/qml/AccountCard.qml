// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One account as a card: its name, its kind and its balance. Clicking it
// opens the account.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n

Kirigami.AbstractCard {
    id: card

    // The account from the Rust side: id, name, type, balance and
    // exactBalance (both text), owes, and later and exactLater: what the
    // entries dated after today add to the balance, empty if none (D-587).
    required property var modelData

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

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
            textFormat: Text.PlainText
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

        // The balance counts entries dated after today too; the Overview's
        // figure for this month only those up to today (D-587).
        AmountLabel {
            Layout.fillWidth: true
            visible: (card.modelData.later ?? "") !== ""
            text: card.i18nContext.i18nc("@label %1 is an amount of money, with its sign", "Includes entries after today: %1", card.modelData.later ?? "")
            exactText: card.i18nContext.i18nc("@label %1 is an amount of money, with its sign", "Includes entries after today: %1", card.modelData.exactLater ?? "")
            horizontalAlignment: Text.AlignRight
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
        }
    }
}
